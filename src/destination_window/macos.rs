//! macOS Destination Window Implementation
//!
//! Renders captured frames to a transparent overlay window using NSWindow and CoreGraphics.
//! Supports profile-based configuration for different screen sharing applications:
//! - Google Meet: Requires normal window level + specific collection behaviors
//! - Discord: May require different settings
//! - Zoom: Similar to Meet

use crate::traits::PreviewWindow;
use cocoa::appkit::{NSWindow, NSWindowStyleMask, NSBackingStoreType, NSColor, NSView};
use cocoa::base::{id, nil, YES, NO};
use cocoa::foundation::{NSRect, NSPoint, NSSize, NSAutoreleasePool};
use objc::{msg_send, sel, sel_impl, class};
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use std::sync::{Arc, Mutex, Once};
use core_graphics::geometry::CGRect as CG_CGRect;
use core_graphics::color_space::CGColorSpace;
use core_graphics::data_provider::CGDataProvider;
use core_graphics::image::CGImage;

extern "C" {
    static _dispatch_main_q: std::ffi::c_void;
    fn dispatch_sync_f(
        queue: *const std::ffi::c_void,
        context: *mut std::ffi::c_void,
        work: extern "C" fn(*mut std::ffi::c_void),
    );
    fn pthread_main_np() -> i32; // Returns non-zero if on main thread
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
const NS_WINDOW_COLLECTION_BEHAVIOR_STATIONARY: u64 = 1 << 4;
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
        
        // Create window frame
        // Position strategy:
        // - Debug mode: On-screen at (100, 100) for debugging
        // - Release mode: Off-screen to keep it completely hidden from user
        //   Note: Off-screen windows are NOT visible in screen sharing pickers (Meet/Zoom)
        //   but user experience (complete invisibility) is more important
        let (x_pos, y_pos) = if cfg!(debug_assertions) {
            (100.0, 100.0)
        } else {
            (-10000.0, -10000.0)
        };
        let width = ctx.width as f64;
        let height = ctx.height as f64;
        
        let frame = NSRect::new(
            NSPoint::new(x_pos, y_pos),
            NSSize::new(width, height)
        );
        
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
        
        // Configure window properties
        // IMPORTANT: Keep opaque=YES and use solid background for CGWindowList
        // Window transparency is controlled via setAlphaValue instead
        // If we use clearColor + opaque=NO, CGWindowList captures a black window
        window.setOpaque_(YES);
        let black_color: id = msg_send![class!(NSColor), blackColor];
        window.setBackgroundColor_(black_color);
        
        // Set window title (optional, mainly for debugging)
        // Using CFSTR to avoid NSString lifecycle issues
        use core_foundation::string::CFString;
        use core_foundation::base::TCFType;
        let title_cf = CFString::new("RustFrame Preview");
        let _: () = msg_send![window, setTitle: title_cf.as_concrete_TypeRef()];
        
        // Configure window level based on config
        // For screen sharing (Meet, Zoom, etc.): Use NORMAL level (0)
        // Level -1 doesn't work - CGWindowList filters it out
        let use_floating = ctx.config.macos_floating_level.unwrap_or(false);
        let window_level = if use_floating {
            NS_FLOATING_WINDOW_LEVEL
        } else {
            NS_NORMAL_WINDOW_LEVEL
        };
        
        log::info!("Setting window level to {} (floating: {})", window_level, use_floating);
        let _: () = msg_send![window, setLevel: window_level];
        
        // Configure NSWindowSharingType for screen capture
        // NSWindowSharingReadOnly (1) = window can be captured but not controlled
        let sharing_type = ctx.config.macos_sharing_type.unwrap_or(NS_WINDOW_SHARING_READ_ONLY);
        log::info!("Setting window sharing type to {}", sharing_type);
        let _: () = msg_send![window, setSharingType: sharing_type];
        
        // Configure NSWindowCollectionBehavior for window management
        // Key behaviors for screen sharing visibility:
        // - Managed: Participates in Exposé and window management
        // - CanJoinAllSpaces: Available in all Mission Control spaces
        // - ParticipatesInCycle: Visible in window cycling (Cmd+Tab, etc.)
        // - FullScreenAuxiliary: Can be shown alongside fullscreen windows
        let collection_behavior = if let Some(custom_behavior) = ctx.config.macos_collection_behavior {
            custom_behavior
        } else {
            // Default: optimal for screen sharing apps (Meet, Zoom, Discord)
            let mut behavior = NS_WINDOW_COLLECTION_BEHAVIOR_MANAGED
                | NS_WINDOW_COLLECTION_BEHAVIOR_CAN_JOIN_ALL_SPACES
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
        
        // Set alpha: Full opacity for proper CGWindowList capture
        // Window is invisible due to level=-1 (below desktop), not alpha
        let window_alpha = ctx.config.alpha.unwrap_or(255);
        window.setAlphaValue_((window_alpha as f64) / 255.0);
        
        // Show the window
        if cfg!(debug_assertions) {
            window.makeKeyAndOrderFront_(nil);
        } else {
            let _: () = msg_send![window, orderFront: nil];
        }
        
        log::info!("Destination window created at ({}, {}) size {}x{} alpha={}", 
                   x_pos, y_pos, width, height, window_alpha);
        
        // Store results
        *ctx.result_window = window;
        *ctx.result_view = content_view;
    }
}

impl DestinationWindow {
    pub fn new(width: u32, height: u32, config: DestinationWindowConfig) -> Option<Self> {
        log::info!("Creating destination window {}x{}", width, height);
        
        let is_main = unsafe { pthread_main_np() } != 0;
        
        let mut result_window: id = nil;
        let mut result_view: id = nil;
        
        let mut context = CreateDestWindowContext {
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
                    8,  // bits per component
                    32, // bits per pixel (RGBA)
                    ctx.width as usize * 4, // bytes per row
                    &color_space,
                    core_graphics::base::kCGImageAlphaLast | core_graphics::base::kCGBitmapByteOrderDefault,
                    &data_provider,
                    false, // should interpolate
                    core_graphics::base::kCGRenderingIntentDefault,
                );
                
                // Get or create CALayer
                let layer: id = msg_send![ctx.view, layer];
                if layer.is_null() {
                    // Enable layer-backing
                    let _: () = msg_send![ctx.view, setWantsLayer: YES];
                    let layer: id = msg_send![ctx.view, layer];
                    if layer.is_null() {
                        return;
                    }
                }
                
                let layer: id = msg_send![ctx.view, layer];
                if layer.is_null() {
                    return;
                }
                
                // Make layer opaque for proper rendering
                let _: () = msg_send![layer, setOpaque: YES];
                
                // CRITICAL: Set contentsScale on EVERY frame for Retina displays
                let window: id = msg_send![ctx.view, window];
                if !window.is_null() {
                    let backing_scale: f64 = msg_send![window, backingScaleFactor];
                    let _: () = msg_send![layer, setContentsScale: backing_scale];
                }
                
                // Set contentsGravity to resize (not resizeAspect) for pixel-perfect display
                let resize_gravity = cocoa::foundation::NSString::alloc(nil);
                let resize_gravity = cocoa::foundation::NSString::init_str(resize_gravity, "resize");
                let _: () = msg_send![layer, setContentsGravity: resize_gravity];
                
                // Disable magnification filter for sharp pixels
                let nearest = cocoa::foundation::NSString::alloc(nil);
                let nearest = cocoa::foundation::NSString::init_str(nearest, "nearest");
                let _: () = msg_send![layer, setMagnificationFilter: nearest];
                let _: () = msg_send![layer, setMinificationFilter: nearest];
                
                // Set CGImage as layer contents
                use foreign_types_shared::ForeignType;
                let cg_image_ref = cg_image.as_ptr() as *const std::ffi::c_void;
                let _: () = msg_send![layer, setContents: cg_image_ref];
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
    
    /// Resize the destination window (called when border is resized)
    /// This is more efficient than checking every frame
    pub fn resize(&mut self, width: u32, height: u32) {
        extern "C" fn resize_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
            #[repr(C)]
            struct ResizeContext {
                window: id,
                width: u32,
                height: u32,
            }
            
            let ctx = unsafe { &*(ctx_ptr as *const ResizeContext) };
            
            unsafe {
                let current_frame: NSRect = msg_send![ctx.window, frame];
                let new_frame = NSRect::new(
                    current_frame.origin,
                    NSSize::new(ctx.width as f64, ctx.height as f64),
                );
                let _: () = msg_send![ctx.window, setFrame:new_frame display:YES];
            }
        }
        
        self.width = width;
        self.height = height;
        
        unsafe {
            let is_main = pthread_main_np() != 0;
            
            struct ResizeContext {
                window: id,
                width: u32,
                height: u32,
            }
            
            let context = ResizeContext {
                window: self.window,
                width,
                height,
            };
            
            if !is_main {
                dispatch_sync_f(
                    &_dispatch_main_q,
                    &context as *const _ as *mut std::ffi::c_void,
                    resize_on_main_thread,
                );
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
            
            let origin = NSPoint::new(
                x as f64,
                screen_height - (y as f64) - (self.height as f64)
            );
            
            let _: () = msg_send![self.window, setFrameOrigin: origin];
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
    
    fn new(width: u32, height: u32, config: Self::Config) -> Option<Self> where Self: Sized {
        DestinationWindow::new(width, height, config)
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
}
