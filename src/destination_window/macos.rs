//! macOS Destination Window Implementation
//!
//! Renders captured frames to a transparent overlay window using NSWindow and CoreGraphics.
//! Supports profile-based configuration for different screen sharing applications:
//! - Google Meet: Requires normal window level + specific collection behaviors
//! - Discord: May require different settings
//! - Zoom: Similar to Meet

use crate::traits::PreviewWindow;
use cocoa::appkit::{NSBackingStoreType, NSColor, NSView, NSWindow, NSWindowStyleMask};
use cocoa::base::{id, nil, BOOL, NO, YES};
use cocoa::foundation::{NSAutoreleasePool, NSPoint, NSRect, NSSize};
use core_graphics::color_space::CGColorSpace;
use core_graphics::data_provider::CGDataProvider;
use core_graphics::geometry::CGRect as CG_CGRect;
use core_graphics::image::CGImage;
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use objc::{class, msg_send, sel, sel_impl};
use std::sync::{Arc, Mutex, Once};

extern "C" {
    static _dispatch_main_q: std::ffi::c_void;
    fn dispatch_sync_f(
        queue: *const std::ffi::c_void,
        context: *mut std::ffi::c_void,
        work: extern "C" fn(*mut std::ffi::c_void),
    );
    fn pthread_main_np() -> i32; // Returns non-zero if on main thread

    // IOSurface APIs for GPU-accelerated rendering
    fn IOSurfaceLookup(iosurface_id: u32) -> *mut std::ffi::c_void;
    fn IOSurfaceGetWidth(iosurface: *mut std::ffi::c_void) -> usize;
    fn IOSurfaceGetHeight(iosurface: *mut std::ffi::c_void) -> usize;
    fn IOSurfaceGetBytesPerRow(iosurface: *mut std::ffi::c_void) -> usize;
    fn IOSurfaceGetBaseAddress(iosurface: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    fn IOSurfaceLock(iosurface: *mut std::ffi::c_void, options: u32, seed: *mut u32) -> i32;
    fn IOSurfaceUnlock(iosurface: *mut std::ffi::c_void, options: u32, seed: *mut u32) -> i32;

    // CoreFoundation memory management
    fn CFRetain(cf: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    fn CFRelease(cf: *mut std::ffi::c_void);
}

// NSWindow constants for screen sharing and window management
const NS_NORMAL_WINDOW_LEVEL: i32 = 0;
const NS_FLOATING_WINDOW_LEVEL: i32 = 3;
const NS_SCREEN_SAVER_WINDOW_LEVEL: i32 = 1000;

// NSWindowSharingType - controls screen capture behavior
const NS_WINDOW_SHARING_NONE: u64 = 0;
const NS_WINDOW_SHARING_READ_ONLY: u64 = 1;
const NS_WINDOW_SHARING_READ_WRITE: u64 = 2;

// NSWindowCollectionBehavior - controls window grouping and spaces
const NS_WINDOW_COLLECTION_BEHAVIOR_DEFAULT: u64 = 0;
const NS_WINDOW_COLLECTION_BEHAVIOR_CAN_JOIN_ALL_SPACES: u64 = 1 << 0;
const NS_WINDOW_COLLECTION_BEHAVIOR_MOVE_TO_ACTIVE_SPACE: u64 = 1 << 1;
const NS_WINDOW_COLLECTION_BEHAVIOR_MANAGED: u64 = 1 << 2;
const NS_WINDOW_COLLECTION_BEHAVIOR_TRANSIENT: u64 = 1 << 3;
const NS_WINDOW_COLLECTION_BEHAVIOR_STATIONARY: u64 = 1 << 4; // Stays behind user windows like desktop
const NS_WINDOW_COLLECTION_BEHAVIOR_PARTICIPATES_IN_CYCLE: u64 = 1 << 5;
const NS_WINDOW_COLLECTION_BEHAVIOR_IGNORES_CYCLE: u64 = 1 << 6;
const NS_WINDOW_COLLECTION_BEHAVIOR_FULL_SCREEN_PRIMARY: u64 = 1 << 7;
const NS_WINDOW_COLLECTION_BEHAVIOR_FULL_SCREEN_AUXILIARY: u64 = 1 << 8;
const NS_WINDOW_COLLECTION_BEHAVIOR_FULL_SCREEN_ALLOWS_TILING: u64 = 1 << 11;

// No global frame buffer needed - we render directly on update_frame

// Note: We used to have a custom NSView for rendering, but it caused black screen issues
// when window was off-screen. Now using simpler approach with direct content view updates.

pub struct DestinationWindow {
    window: id,
    view: id,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DestinationWindowConfig {
    pub alpha: Option<u8>,
    pub topmost: Option<bool>,
    pub click_through: Option<bool>,

    // macOS-specific options
    /// Window level: None = auto (normal for screen sharing), Some(true) = floating, Some(false) = normal
    pub macos_floating_level: Option<bool>,

    /// Window sharing type: None = read-only (default), Some(0) = none, Some(1) = read-only, Some(2) = read-write
    pub macos_sharing_type: Option<u64>,

    /// Collection behavior: None = default for screen sharing
    /// Set to Some(value) to override with custom NSWindowCollectionBehavior flags
    pub macos_collection_behavior: Option<u64>,

    /// Whether to participate in Mission Control and window cycling
    /// Default: true (visible in screen sharing pickers)
    pub macos_participates_in_cycle: Option<bool>,

    // Legacy Windows fields (ignored on macOS but kept for compatibility)
    pub toolwindow: Option<bool>,
    pub layered: Option<bool>,
    pub appwindow: Option<bool>,
    pub noactivate: Option<bool>,
    pub overlapped: Option<bool>,
}

unsafe impl Send for DestinationWindow {}
unsafe impl Sync for DestinationWindow {}

// Context struct for main thread creation
struct CreateDestWindowContext {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    config: DestinationWindowConfig,
    result_window: *mut id,
    result_view: *mut id,
}

unsafe impl Send for CreateDestWindowContext {}

extern "C" fn create_dest_window_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
    let ctx = unsafe { &mut *(ctx_ptr as *mut CreateDestWindowContext) };

    unsafe {
        let _pool = NSAutoreleasePool::new(nil);

        // Create window style mask
        let style_mask = NSWindowStyleMask::NSBorderlessWindowMask;

        // IMPORTANT: Frontend sends dimensions in POINTS (not pixels)
        // On macOS, NSWindow uses points, and the hollow border also uses points
        // No conversion needed - use the values directly
        let main_screen: id = msg_send![class!(NSScreen), mainScreen];
        let backing_scale: f64 = if !main_screen.is_null() {
            msg_send![main_screen, backingScaleFactor]
        } else {
            2.0 // Default to 2x Retina if we can't get screen
        };

        log::info!("[DestWindow] Display backing scale: {}", backing_scale);

        // Create window frame at requested position (already in points)
        // CRITICAL: Convert from top-left to bottom-left origin (Cocoa coordinates)
        // Use main screen frame (not visibleFrame) to get full screen height including menu bar
        let screen_frame: NSRect = msg_send![main_screen, frame];
        let screen_height = screen_frame.size.height;
        let x_pos = ctx.x as f64;
        let y_pos_top_left = ctx.y as f64;
        let y_pos = screen_height - y_pos_top_left - (ctx.height as f64);

        // Use dimensions directly as they're already in POINTS
        let width = ctx.width as f64;
        let height = ctx.height as f64;

        log::info!(
            "[DestWindow] Window size: {}x{} points @ {}x scale ({}x{} pixels)",
            width,
            height,
            backing_scale,
            (width * backing_scale) as u32,
            (height * backing_scale) as u32
        );
        log::info!("[DestWindow] Window position: ({}, {})", x_pos, y_pos);

        let frame = NSRect::new(NSPoint::new(x_pos, y_pos), NSSize::new(width, height));

        // Create the window
        let window = NSWindow::alloc(nil).initWithContentRect_styleMask_backing_defer_(
            frame,
            style_mask,
            NSBackingStoreType::NSBackingStoreBuffered,
            NO,
        );

        if window == nil {
            log::error!("Failed to create destination NSWindow");
            return;
        }

        log::info!("[DestWindow] Window created successfully");

        // Configure window properties
        // IMPORTANT: Keep opaque=YES and use solid background for CGWindowList
        // Window transparency is controlled via setAlphaValue instead
        // If we use clearColor + opaque=NO, CGWindowList captures a black window
        window.setOpaque_(YES);
        let black_color: id = msg_send![class!(NSColor), blackColor];
        window.setBackgroundColor_(black_color);

        // Set window title (optional, mainly for debugging)
        // Using CFSTR to avoid NSString lifecycle issues
        use core_foundation::base::TCFType;
        use core_foundation::string::CFString;
        let title_cf = CFString::new("Rust - Share This Window");
        let _: () = msg_send![window, setTitle: title_cf.as_concrete_TypeRef()];

        // Configure window level based on config
        // CRITICAL: Use NORMAL window level (0) for screen sharing visibility
        // Desktop level windows are filtered out by Meet/Zoom/Teams screen capture
        // Z-order control is done via orderWindow and collection behavior instead
        let use_floating = ctx.config.macos_floating_level.unwrap_or(false);
        let window_level = if use_floating {
            NS_FLOATING_WINDOW_LEVEL
        } else {
            NS_NORMAL_WINDOW_LEVEL
        };

        log::info!(
            "Setting window level to {} (floating: {}, normal for screen sharing)",
            window_level,
            use_floating
        );
        let _: () = msg_send![window, setLevel: window_level];

        // Configure NSWindowSharingType for screen capture
        // CRITICAL: READ_ONLY (1) so destination window IS visible in Meet/Zoom picker
        // Separation layer uses NONE (0) to stay hidden
        let sharing_type = ctx
            .config
            .macos_sharing_type
            .unwrap_or(NS_WINDOW_SHARING_READ_ONLY);
        log::info!("Setting window sharing type to {} (READ_ONLY for screen sharing visibility)", sharing_type);
        let _: () = msg_send![window, setSharingType: sharing_type];

        // Configure NSWindowCollectionBehavior for window management
        // Key behaviors for screen sharing visibility:
        // - Managed: Participates in Exposé and window management
        // - MoveToActiveSpace: Moves with active space (hides on desktop view) - CRITICAL!
        // - ParticipatesInCycle / IgnoresCycle: Window cycling visibility
        // - FullScreenAuxiliary: Can be shown alongside fullscreen windows
        // CRITICAL: Do NOT use CAN_JOIN_ALL_SPACES - it keeps window visible on desktop view!
        let collection_behavior =
            if let Some(custom_behavior) = ctx.config.macos_collection_behavior {
                custom_behavior
            } else {
                // Default: optimal for screen sharing apps (Meet, Zoom, Discord)
                // CRITICAL: Do NOT use STATIONARY - that hides window from screen sharing pickers
                // Use MOVE_TO_ACTIVE_SPACE instead of CAN_JOIN_ALL_SPACES for proper desktop hiding
                let mut behavior = NS_WINDOW_COLLECTION_BEHAVIOR_MANAGED
                    | NS_WINDOW_COLLECTION_BEHAVIOR_MOVE_TO_ACTIVE_SPACE
                    | NS_WINDOW_COLLECTION_BEHAVIOR_FULL_SCREEN_AUXILIARY;

                // ParticipatesInCycle vs IgnoresCycle
                // In release mode: always use IgnoresCycle so preview window never appears in Dock/Cmd+Tab
                // In debug mode: respect config for easier debugging
                let participates = if cfg!(debug_assertions) {
                    ctx.config.macos_participates_in_cycle.unwrap_or(true)
                } else {
                    false // Always ignore cycle in release
                };

                if participates {
                    behavior |= NS_WINDOW_COLLECTION_BEHAVIOR_PARTICIPATES_IN_CYCLE;
                } else {
                    behavior |= NS_WINDOW_COLLECTION_BEHAVIOR_IGNORES_CYCLE;
                }

                behavior
            };

        log::info!("Setting collection behavior to {:#x}", collection_behavior);
        let _: () = msg_send![window, setCollectionBehavior: collection_behavior];

        // Configure click-through behavior (default: true in release)
        let click_through = ctx.config.click_through.unwrap_or(!cfg!(debug_assertions));
        if click_through {
            window.setIgnoresMouseEvents_(YES);
        }

        // Use standard content view
        let content_view = window.contentView();

        // CRITICAL: Enable layer-backing and disable animations ONCE at window creation
        // Doing this on every frame causes performance issues and may re-enable animations
        let _: () = msg_send![content_view, setWantsLayer: YES];
        let layer: id = msg_send![content_view, layer];

        if !layer.is_null() {
            // CRITICAL: Permanently disable ALL implicit animations
            // Setting layer.actions to NSNull for all animated properties
            let null_class = class!(NSNull);
            let ns_null: id = msg_send![null_class, null];
            let dict_class = class!(NSMutableDictionary);
            let actions_dict: id = msg_send![dict_class, dictionary];

            // Disable animations for ALL layer properties that might animate
            let properties = [
                "contents",
                "contentsRect",
                "contentsScale",
                "bounds",
                "position",
                "frame",
                "opacity",
                "backgroundColor",
            ];

            for property in &properties {
                let key = cocoa::foundation::NSString::alloc(nil);
                let key = cocoa::foundation::NSString::init_str(key, property);
                let _: () = msg_send![actions_dict, setObject:ns_null forKey:key];
            }

            let _: () = msg_send![layer, setActions: actions_dict];
            log::info!("[DestWindow] Layer animations permanently disabled");

            // Make layer opaque for performance
            let _: () = msg_send![layer, setOpaque: YES];
        } else {
            log::warn!("[DestWindow] Failed to get layer from content view");
        }

        // Set alpha: Full opacity for proper CGWindowList capture
        // Window is invisible due to level=-1 (below desktop), not alpha
        let window_alpha = ctx.config.alpha.unwrap_or(255);
        window.setAlphaValue_((window_alpha as f64) / 255.0);

        // Show the window
        // CRITICAL: Place window at absolute back (behind EVERYTHING including desktop)
        // orderOut first to ensure clean state, then orderBack
        let _: () = msg_send![window, orderOut: nil];
        let _: () = msg_send![window, orderBack: nil];

        // Log window visibility and position
        let is_visible: BOOL = msg_send![window, isVisible];
        let final_frame: NSRect = msg_send![window, frame];
        log::info!(
            "Destination window created at top-left ({}, {}) -> Cocoa ({}, {}) size {}x{} alpha={}",
            ctx.x,
            ctx.y,
            x_pos,
            y_pos,
            width,
            height,
            window_alpha
        );
        log::info!(
            "[DestWindow] Window visibility: {}, Actual frame: origin=({}, {}), size={}x{}",
            is_visible == YES,
            final_frame.origin.x,
            final_frame.origin.y,
            final_frame.size.width,
            final_frame.size.height
        );

        // Store results
        *ctx.result_window = window;
        *ctx.result_view = content_view;
    }
}

impl DestinationWindow {
    pub fn new(x: i32, y: i32, width: u32, height: u32, config: DestinationWindowConfig) -> Option<Self> {
        log::info!("Creating destination window at ({}, {}) {}x{}", x, y, width, height);

        let is_main = unsafe { pthread_main_np() } != 0;

        let mut result_window: id = nil;
        let mut result_view: id = nil;

        let mut context = CreateDestWindowContext {
            x,
            y,
            width,
            height,
            config,
            result_window: &mut result_window,
            result_view: &mut result_view,
        };

        if !is_main {
            unsafe {
                dispatch_sync_f(
                    &_dispatch_main_q,
                    &mut context as *mut _ as *mut std::ffi::c_void,
                    create_dest_window_on_main_thread,
                );
            }
        } else {
            create_dest_window_on_main_thread(&mut context as *mut _ as *mut std::ffi::c_void);
        }

        if result_window == nil {
            log::error!("Failed to create destination window");
            return None;
        }
        Some(Self {
            window: result_window,
            view: result_view,
            width,
            height,
        })
    }

    pub fn hwnd_value(&self) -> isize {
        self.window as isize
    }

    /// Get raw NSWindow pointer for z-order operations
    pub fn get_window(&self) -> id {
        self.window
    }

    pub fn update_frame(&self, data: Vec<u8>, width: u32, height: u32) {
        extern "C" fn update_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
            #[repr(C)]
            struct UpdateContext {
                view: id,
                data: *const Vec<u8>,
                width: u32,
                height: u32,
            }

            let ctx = unsafe { &*(ctx_ptr as *const UpdateContext) };
            let data = unsafe { &*ctx.data };

            unsafe {
                // Create CGImage from RGBA data
                let data_arc = Arc::new(data.clone());
                let data_provider = CGDataProvider::from_buffer(data_arc);
                let color_space = CGColorSpace::create_device_rgb();

                let cg_image = CGImage::new(
                    ctx.width as usize,
                    ctx.height as usize,
                    8,                      // bits per component
                    32,                     // bits per pixel (RGBA)
                    ctx.width as usize * 4, // bytes per row
                    &color_space,
                    core_graphics::base::kCGImageAlphaLast
                        | core_graphics::base::kCGBitmapByteOrderDefault,
                    &data_provider,
                    false, // should interpolate
                    core_graphics::base::kCGRenderingIntentDefault,
                );

                // Get layer (already created and configured at window creation)
                let layer: id = msg_send![ctx.view, layer];
                if layer.is_null() {
                    log::error!("[DestWindow CPU] Layer is null - should have been created at window creation!");
                    return;
                }

                // CRITICAL: Set contentsScale on EVERY frame for Retina displays
                let window: id = msg_send![ctx.view, window];
                if !window.is_null() {
                    let backing_scale: f64 = msg_send![window, backingScaleFactor];
                    let _: () = msg_send![layer, setContentsScale: backing_scale];
                }

                // Set contentsGravity to resize (not resizeAspect) for pixel-perfect display
                let resize_gravity = cocoa::foundation::NSString::alloc(nil);
                let resize_gravity =
                    cocoa::foundation::NSString::init_str(resize_gravity, "resize");
                let _: () = msg_send![layer, setContentsGravity: resize_gravity];

                // Disable magnification filter for sharp pixels
                let nearest = cocoa::foundation::NSString::alloc(nil);
                let nearest = cocoa::foundation::NSString::init_str(nearest, "nearest");
                let _: () = msg_send![layer, setMagnificationFilter: nearest];
                let _: () = msg_send![layer, setMinificationFilter: nearest];

                // CRITICAL: Reset contentsRect to full (0,0,1,1) for CPU rendering
                // GPU path uses cropped contentsRect, but CPU path provides already-cropped CGImage
                // If we don't reset, layer will apply GPU's crop to CPU's already-cropped image!
                use core_graphics::geometry::CGRect;
                let full_rect = CGRect::new(
                    &core_graphics::geometry::CGPoint::new(0.0, 0.0),
                    &core_graphics::geometry::CGSize::new(1.0, 1.0),
                );
                let _: () = msg_send![layer, setContentsRect: full_rect];

                // CRITICAL: Wrap CALayer property changes in CATransaction with disabled animations
                let transaction_class = class!(CATransaction);
                let _: () = msg_send![transaction_class, begin];
                let _: () = msg_send![transaction_class, setDisableActions: YES];

                // Set CGImage as layer contents
                use foreign_types_shared::ForeignType;
                let cg_image_ref = cg_image.as_ptr() as *const std::ffi::c_void;
                let _: () = msg_send![layer, setContents: cg_image_ref];

                let _: () = msg_send![transaction_class, commit];
            }
        }

        unsafe {
            let is_main = pthread_main_np() != 0;

            struct UpdateContext {
                view: id,
                data: *const Vec<u8>,
                width: u32,
                height: u32,
            }

            let context = UpdateContext {
                view: self.view,
                data: &data as *const Vec<u8>,
                width,
                height,
            };

            if !is_main {
                dispatch_sync_f(
                    &_dispatch_main_q,
                    &context as *const _ as *mut std::ffi::c_void,
                    update_on_main_thread,
                );
            } else {
                update_on_main_thread(&context as *const _ as *mut std::ffi::c_void);
            }
        }
    }

    pub fn render(&mut self, pixels: &[u8], width: u32, height: u32) {
        // Update stored dimensions
        self.width = width;
        self.height = height;
        self.update_frame(pixels.to_vec(), width, height);
    }

    /// GPU-accelerated rendering from retained IOSurface pointer with cropping
    /// This is significantly faster than CPU-based rendering for large regions
    /// Uses retained pointer directly - no lookup needed!
    pub fn update_frame_from_iosurface_ptr(
        &self,
        iosurface_ptr: *mut std::ffi::c_void,
        crop_x: i64,
        crop_y: i64,
        crop_w: i64,
        crop_h: i64,
    ) {
        // GPU rendering with CALayer contentsRect for cropping
        // No need for CPU fallback - CALayer handles cropping on GPU!

        extern "C" fn update_from_iosurface_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
            #[repr(C)]
            struct UpdateContext {
                view: id,
                iosurface_ptr: *mut std::ffi::c_void,
                crop_x: i64,
                crop_y: i64,
                crop_w: i64,
                crop_h: i64,
            }

            let ctx = unsafe { &*(ctx_ptr as *const UpdateContext) };

            unsafe {
                // Get IOSurface pointer (already retained by get_iosurface(), we own this reference)
                let iosurface = ctx.iosurface_ptr;
                if iosurface.is_null() {
                    tracing::warn!("IOSurface pointer is null");
                    return;
                }

                // NOTE: Pointer already retained by get_iosurface() - we MUST CFRelease when done!
                // NO LOCK/UNLOCK needed - CALayer can access GPU memory directly!

                // Get or create CALayer
                // Get layer from view (layer-backing already enabled at window creation)
                let layer: id = msg_send![ctx.view, layer];
                if layer.is_null() {
                    tracing::warn!("Failed to get layer from view");
                    CFRelease(iosurface);
                    return;
                }

                // CRITICAL: Set contentsScale for Retina displays
                let window: id = msg_send![ctx.view, window];
                if !window.is_null() {
                    let backing_scale: f64 = msg_send![window, backingScaleFactor];
                    let _: () = msg_send![layer, setContentsScale: backing_scale];
                }

                // Get IOSurface dimensions to calculate normalized crop rect
                let surface_width = IOSurfaceGetWidth(iosurface);
                let surface_height = IOSurfaceGetHeight(iosurface);

                // Calculate normalized contentsRect for GPU-accelerated cropping
                // contentsRect uses normalized coordinates (0.0 - 1.0)
                // CRITICAL: CALayer contentsRect has bottom-left origin, but IOSurface is top-left
                // We MUST flip Y coordinate: Y_bottom_left = 1.0 - (Y_top_left + Height) / Total_Height

                // Wrap contentsRect changes in CATransaction to prevent animation
                let transaction_class = class!(CATransaction);
                let _: () = msg_send![transaction_class, begin];
                let _: () = msg_send![transaction_class, setDisableActions: YES];

                if ctx.crop_x != 0
                    || ctx.crop_y != 0
                    || (ctx.crop_w > 0 && ctx.crop_w != surface_width as i64)
                    || (ctx.crop_h > 0 && ctx.crop_h != surface_height as i64)
                {
                    let x_norm = (ctx.crop_x as f64) / (surface_width as f64);
                    let w_norm = (ctx.crop_w as f64) / (surface_width as f64);
                    let h_norm = (ctx.crop_h as f64) / (surface_height as f64);

                    // Flip Y coordinate: CALayer uses bottom-left, IOSurface uses top-left
                    let y_norm =
                        1.0 - ((ctx.crop_y as f64 + ctx.crop_h as f64) / (surface_height as f64));

                    use core_graphics::geometry::CGRect;
                    let contents_rect = CGRect::new(
                        &core_graphics::geometry::CGPoint::new(x_norm, y_norm),
                        &core_graphics::geometry::CGSize::new(w_norm, h_norm),
                    );
                    let _: () = msg_send![layer, setContentsRect: contents_rect];

                    tracing::debug!("🚀 GPU crop: IOSurface {}x{}, crop pixels ({}, {}, {}, {}), normalized ({:.4}, {:.4}, {:.4}, {:.4}) [Y-flipped for CALayer]",
                        surface_width, surface_height, ctx.crop_x, ctx.crop_y, ctx.crop_w, ctx.crop_h, 
                        x_norm, y_norm, w_norm, h_norm);
                } else {
                    // No crop - use full surface
                    use core_graphics::geometry::CGRect;
                    let full_rect = CGRect::new(
                        &core_graphics::geometry::CGPoint::new(0.0, 0.0),
                        &core_graphics::geometry::CGSize::new(1.0, 1.0),
                    );
                    let _: () = msg_send![layer, setContentsRect: full_rect];
                    tracing::debug!(
                        "🚀 GPU full: IOSurface {}x{}, no crop",
                        surface_width,
                        surface_height
                    );
                }

                let _: () = msg_send![transaction_class, commit];

                // Set contentsGravity to resize (not resizeAspect) for pixel-perfect display
                let resize_gravity = cocoa::foundation::NSString::alloc(nil);
                let resize_gravity =
                    cocoa::foundation::NSString::init_str(resize_gravity, "resize");
                let _: () = msg_send![layer, setContentsGravity: resize_gravity];

                // Disable magnification filter for sharp pixels
                let nearest = cocoa::foundation::NSString::alloc(nil);
                let nearest = cocoa::foundation::NSString::init_str(nearest, "nearest");
                let _: () = msg_send![layer, setMagnificationFilter: nearest];
                let _: () = msg_send![layer, setMinificationFilter: nearest];

                // CRITICAL: Wrap CALayer property changes in CATransaction with disabled animations
                // This is a backup to layer.actions - ensures no implicit animations
                let transaction_class = class!(CATransaction);
                let _: () = msg_send![transaction_class, begin];
                let _: () = msg_send![transaction_class, setDisableActions: YES];

                // 🚀 GPU ACCELERATION: Set IOSurface directly as layer contents!
                // Zero-copy, all rendering happens on GPU. No CPU involved!
                let _: () = msg_send![layer, setContents: iosurface];

                let _: () = msg_send![transaction_class, commit];

                // Release our retain from get_iosurface() - CALayer now owns a reference
                CFRelease(iosurface);
            }
        }

        unsafe {
            let is_main = pthread_main_np() != 0;

            #[repr(C)]
            struct UpdateContext {
                view: id,
                iosurface_ptr: *mut std::ffi::c_void,
                crop_x: i64,
                crop_y: i64,
                crop_w: i64,
                crop_h: i64,
            }

            let context = UpdateContext {
                view: self.view,
                iosurface_ptr,
                crop_x,
                crop_y,
                crop_w,
                crop_h,
            };

            if !is_main {
                dispatch_sync_f(
                    &_dispatch_main_q,
                    &context as *const _ as *mut std::ffi::c_void,
                    update_from_iosurface_on_main_thread,
                );
            } else {
                update_from_iosurface_on_main_thread(&context as *const _ as *mut std::ffi::c_void);
            }
        }
    }

    /// Resize the destination window (called when border is resized)
    /// This is more efficient than checking every frame
    /// NOTE: width/height are already in POINTS (from frontend/border)
    pub fn resize(&mut self, width: u32, height: u32) {
        extern "C" fn resize_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
            #[repr(C)]
            struct ResizeContext {
                window: id,
                width_points: u32,
                height_points: u32,
            }

            let ctx = unsafe { &*(ctx_ptr as *const ResizeContext) };

            unsafe {
                // Get backing scale factor for logging purposes
                let backing_scale: f64 = msg_send![ctx.window, backingScaleFactor];
                let width_points = ctx.width_points as f64;
                let height_points = ctx.height_points as f64;

                log::info!(
                    "[DestWindow] Resize: {}x{} points @ {}x scale ({}x{} pixels)",
                    width_points,
                    height_points,
                    backing_scale,
                    (width_points * backing_scale) as u32,
                    (height_points * backing_scale) as u32
                );

                let current_frame: NSRect = msg_send![ctx.window, frame];
                log::info!(
                    "[DestWindow] Current frame BEFORE resize: origin=({}, {}), size={}x{}",
                    current_frame.origin.x,
                    current_frame.origin.y,
                    current_frame.size.width,
                    current_frame.size.height
                );

                let new_frame = NSRect::new(
                    current_frame.origin,
                    NSSize::new(width_points, height_points),
                );
                // CRITICAL: Use animate:NO to disable resize animation for instant update
                let _: () = msg_send![ctx.window, setFrame:new_frame display:YES animate:NO];

                // Verify the resize actually happened
                let final_frame: NSRect = msg_send![ctx.window, frame];
                let is_visible: BOOL = msg_send![ctx.window, isVisible];
                log::info!(
                    "[DestWindow] Final frame AFTER resize: origin=({}, {}), size={}x{}",
                    final_frame.origin.x,
                    final_frame.origin.y,
                    final_frame.size.width,
                    final_frame.size.height
                );
                log::info!("[DestWindow] Window visibility: {}", is_visible == YES);

                // Check content view bounds
                let content_view: id = msg_send![ctx.window, contentView];
                if !content_view.is_null() {
                    let view_bounds: NSRect = msg_send![content_view, bounds];
                    log::info!(
                        "[DestWindow] Content view bounds: origin=({}, {}), size={}x{}",
                        view_bounds.origin.x,
                        view_bounds.origin.y,
                        view_bounds.size.width,
                        view_bounds.size.height
                    );
                }
            }
        }

        self.width = width;
        self.height = height;

        unsafe {
            let is_main = pthread_main_np() != 0;

            struct ResizeContext {
                window: id,
                width_points: u32,
                height_points: u32,
            }

            let context = ResizeContext {
                window: self.window,
                width_points: width,
                height_points: height,
            };

            if !is_main {
            } else {
                resize_on_main_thread(&context as *const _ as *mut std::ffi::c_void);
            }
        }
    }

    pub fn set_pos(&mut self, x: i32, y: i32) {
        unsafe {
            // macOS uses bottom-left origin, need to convert from top-left
            let screen_frame: NSRect = msg_send![self.window, screen];
            let screen_height = screen_frame.size.height;

            let origin = NSPoint::new(x as f64, screen_height - (y as f64) - (self.height as f64));

            let _: () = msg_send![self.window, setFrameOrigin: origin];
        }
    }

    /// Get current window position and size (x, y, width, height)
    pub fn get_rect(&self) -> Option<(i32, i32, i32, i32)> {
        unsafe {
            let frame: NSRect = msg_send![self.window, frame];
            let screen_frame: NSRect = msg_send![self.window, screen];
            let screen_height = screen_frame.size.height;

            // Convert from bottom-left origin to top-left origin
            let x = frame.origin.x as i32;
            let y = (screen_height - frame.origin.y - frame.size.height) as i32;
            let width = frame.size.width as i32;
            let height = frame.size.height as i32;

            Some((x, y, width, height))
        }
    }

    /// Update window position and size - CRITICAL for keeping all windows synchronized
    pub fn update_position(&self, x: i32, y: i32, width: u32, height: u32) {
        #[repr(C)]
        struct PosCtx {
            window: id,
            x: i32,
            y: i32,
            width: u32,
            height: u32,
        }

        extern "C" fn move_on_main(ctx_ptr: *mut std::ffi::c_void) {
            unsafe {
                let ctx = &*(ctx_ptr as *const PosCtx);
                
                // macOS uses bottom-left origin, need to convert from top-left
                let screen: id = msg_send![class!(NSScreen), mainScreen];
                let screen_frame: NSRect = msg_send![screen, frame];
                let screen_height = screen_frame.size.height;
                
                // Convert y from top-left to bottom-left origin
                let cocoa_y = screen_height - (ctx.y as f64) - (ctx.height as f64);
                
                let new_frame = NSRect::new(
                    NSPoint::new(ctx.x as f64, cocoa_y),
                    NSSize::new(ctx.width as f64, ctx.height as f64),
                );
                let _: () = msg_send![ctx.window, setFrame:new_frame display:YES animate:NO];
                
                // CRITICAL: Keep preview window at absolute back (behind EVERYTHING)
                // orderOut first to remove from any existing order
                let _: () = msg_send![ctx.window, orderOut: nil];
                let _: () = msg_send![ctx.window, orderBack: nil];
            }
        }

        let mut ctx = PosCtx {
            window: self.window,
            x,
            y,
            width,
            height,
        };

        unsafe {
            if pthread_main_np() != 0 {
                move_on_main(&mut ctx as *mut _ as *mut std::ffi::c_void);
            } else {
                dispatch_sync_f(
                    &_dispatch_main_q,
                    &mut ctx as *mut _ as *mut std::ffi::c_void,
                    move_on_main,
                );
            }
        }
    }

    /// Get the macOS CGWindowID for this window (used for filtering in capture engine)
    pub fn get_window_id(&self) -> u32 {
        extern "C" fn get_window_id_on_main_thread(ctx_ptr: *mut std::ffi::c_void) -> u32 {
            let window = ctx_ptr as id;
            unsafe {
                let window_number: u32 = msg_send![window, windowNumber];
                window_number
            }
        }

        unsafe {
            let is_main = pthread_main_np() != 0;

            if !is_main {
                // Can't directly get window ID from background thread
                // Must dispatch to main thread synchronously
                let mut result: u32 = 0;

                struct IdContext {
                    window: id,
                    result: *mut u32,
                }

                extern "C" fn get_id_on_main(ctx_ptr: *mut std::ffi::c_void) {
                    let ctx = ctx_ptr as *const IdContext;
                    let ctx = unsafe { &*ctx };
                    let window_number: u32 = unsafe { msg_send![ctx.window, windowNumber] };
                    unsafe {
                        *ctx.result = window_number;
                    }
                }

                let ctx = IdContext {
                    window: self.window,
                    result: &mut result,
                };

                dispatch_sync_f(
                    &_dispatch_main_q,
                    &ctx as *const _ as *mut std::ffi::c_void,
                    get_id_on_main,
                );

                result
            } else {
                let window_number: u32 = msg_send![self.window, windowNumber];
                window_number
            }
        }
    }
}

impl Drop for DestinationWindow {
    fn drop(&mut self) {
        tracing::debug!(window_ptr = ?self.window, "Dropping destination window");

        extern "C" fn close_window_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
            let window = ctx_ptr as id;
            unsafe {
                tracing::debug!("Hiding and closing window on main thread");
                let _: () = msg_send![window, orderOut: nil];
                let _: () = msg_send![window, close];
            }
        }

        unsafe {
            let is_main = pthread_main_np() != 0;
            tracing::debug!(is_main_thread = is_main, "Drop on main thread check");

            if !is_main {
                tracing::debug!("Dispatching window close to main thread");
                dispatch_sync_f(
                    &_dispatch_main_q,
                    self.window as *mut std::ffi::c_void,
                    close_window_on_main_thread,
                );
            } else {
                tracing::debug!("Closing window directly on main thread");
                let _: () = msg_send![self.window, orderOut: nil];
                let _: () = msg_send![self.window, close];
            }
        }

        tracing::debug!("Destination window drop completed");
    }
}

impl PreviewWindow for DestinationWindow {
    type Config = DestinationWindowConfig;

    fn new(x: i32, y: i32, width: u32, height: u32, config: Self::Config) -> Option<Self>
    where
        Self: Sized,
    {
        DestinationWindow::new(x, y, width, height, config)
    }

    fn hwnd_value(&self) -> isize {
        self.hwnd_value()
    }

    fn update_frame(&self, data: Vec<u8>, width: u32, height: u32) {
        self.update_frame(data, width, height);
    }

    fn render(&mut self, pixels: &[u8], width: u32, height: u32) {
        self.render(pixels, width, height);
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.resize(width, height);
    }

    fn set_pos(&mut self, x: i32, y: i32) {
        self.set_pos(x, y);
    }

    fn send_to_back(&self) {
        // macOS: orderBack moves window behind all others at the same level
        unsafe {
            let _: () = msg_send![self.window, orderBack: nil];
        }
        tracing::debug!("macOS destination window sent to back");
    }

    fn bring_to_front(&self) {
        // macOS: orderFront brings window to front
        unsafe {
            let _: () = msg_send![self.window, orderFront: nil];
        }
        tracing::debug!("macOS destination window brought to front");
    }

    fn exclude_from_capture(&self) {
        // macOS: Use NSWindow sharingType to exclude from screen capture
        // kCGWindowSharingNone = 0 prevents window from being captured
        unsafe {
            let _: () = msg_send![self.window, setSharingType: 0]; // NSWindowSharingNone
        }
        tracing::info!("✅ macOS destination window excluded from screen capture");
    }
}
