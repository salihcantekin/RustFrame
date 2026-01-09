// capture/macos.rs - macOS Screen Capture Implementation
//
// Supports multiple capture APIs based on macOS version:
// - macOS 15.0+: Uses CGDisplayStream with improved privacy handling
// - macOS 12.3+: Can use ScreenCaptureKit (modern, system picker)
// - macOS 12.2-: Uses legacy CGWindowListCreateImage
// ALL CoreGraphics operations must happen on main thread to avoid ObjC exceptions

use super::{CaptureEngine, CaptureFrame, CaptureRect};
use anyhow::{anyhow, Result};
use core_graphics::image::CGImage;
use core_graphics::geometry::{CGRect, CGPoint, CGSize};
use core_graphics::window::{kCGWindowListOptionOnScreenOnly, kCGNullWindowID};
use std::sync::Arc;
use log::info;
use foreign_types_shared::ForeignType;

#[cfg(target_os = "macos")]
#[path = "macos_sck.rs"]
mod macos_sck;

#[cfg(target_os = "macos")]
use macos_sck::ScreenCaptureKitCapture;

#[cfg(target_os = "macos")]
use cocoa::base::nil;

#[cfg(target_os = "macos")]
use cocoa::foundation::{NSPoint, NSRect, NSSize};

#[cfg(target_os = "macos")]
use objc::rc::autoreleasepool;

// macOS version detection structures
#[cfg(target_os = "macos")]
#[repr(C)]
struct NSOperatingSystemVersion {
    major: isize,
    minor: isize,
    patch: isize,
}

#[cfg(target_os = "macos")]
#[link(name = "Foundation", kind = "framework")]
extern "C" {
    fn NSProcessInfo_operatingSystemVersion() -> NSOperatingSystemVersion;
}

/// Get current macOS version
#[cfg(target_os = "macos")]
fn get_macos_version() -> (isize, isize, isize) {
    use objc::{class, msg_send, sel, sel_impl};
    use objc::runtime::Object;
    
    unsafe {
        let process_info: *mut Object = msg_send![class!(NSProcessInfo), processInfo];
        let version: NSOperatingSystemVersion = msg_send![process_info, operatingSystemVersion];
        (version.major, version.minor, version.patch)
    }
}

/// Check if we should use legacy CGWindowListCreateImage
/// or modern CGDisplayCreateImageForRect (macOS 15+/Sequoia)
/// 
/// BOTH APIs require Screen Recording permission (one-time setup).
/// After permission is granted once, no more prompts appear.
/// 
/// Note: macOS internal versioning:
/// - macOS 11 Big Sur = 20.x
/// - macOS 12 Monterey = 21.x
/// - macOS 13 Ventura = 22.x
/// - macOS 14 Sonoma = 23.x
/// - macOS 15 Sequoia = 24.x, 25.x, 26.x (depending on build)
#[cfg(target_os = "macos")]
fn should_use_legacy_capture() -> bool {
    let (major, _minor, _patch) = get_macos_version();
    // Use legacy for anything before Sequoia (macOS 15)
    // Sequoia uses internal version 24, 25, or 26
    major < 24
}

/// Log the capture method being used
#[cfg(target_os = "macos")]
fn log_capture_method() {
    let (major, minor, patch) = get_macos_version();
    if should_use_legacy_capture() {
        info!("macOS {}.{}.{}: Using legacy CGWindowListCreateImage (one-time permission required)", major, minor, patch);
    } else {
        info!("macOS {}.{}.{}: Using modern CGDisplayCreateImageForRect (one-time permission required)", major, minor, patch);
    }
}

#[cfg(target_os = "macos")]
fn supports_screen_capture_kit() -> bool {
    let (major, minor, patch) = get_macos_version();
    (major > 12)
        || (major == 12 && minor > 3)
        || (major == 12 && minor == 3 && patch >= 0)
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
    fn CGWindowListCreateImage(
        screenBounds: CGRect,
        listOption: u32,
        windowID: u32,
        imageOption: u32,
    ) -> *mut core_graphics::sys::CGImage;
    
    // Modern API for macOS 15+ (less intrusive)
    fn CGDisplayCreateImage(display: u32) -> *mut core_graphics::sys::CGImage;
    fn CGDisplayCreateImageForRect(display: u32, rect: CGRect) -> *mut core_graphics::sys::CGImage;
    fn CGMainDisplayID() -> u32;
}

#[cfg(target_os = "macos")]
#[link(name = "ImageIO", kind = "framework")]
extern "C" {
    fn CGImageSourceCreateWithData(
        data: *const std::ffi::c_void,
        options: *const std::ffi::c_void,
    ) -> *mut std::ffi::c_void;

    fn CGImageSourceCreateImageAtIndex(
        isrc: *mut std::ffi::c_void,
        index: usize,
        options: *const std::ffi::c_void,
    ) -> *mut core_graphics::sys::CGImage;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const std::ffi::c_void);
}

#[cfg(target_os = "macos")]
fn cursor_rgba_premultiplied() -> Option<(Vec<u8>, u32, u32, i32, i32)> {
    // Returns (rgba_premultiplied, width_px, height_px, hotspot_x_px, hotspot_y_px)
    // Hotspot is assumed to be in image coordinates with origin at top-left (AppKit behavior).
    use cocoa::appkit::NSCursor;
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let cursor: *mut objc::runtime::Object = msg_send![class!(NSCursor), currentCursor];
        if cursor == nil {
            return None;
        }

        let image: *mut objc::runtime::Object = msg_send![cursor, image];
        if image == nil {
            return None;
        }

        let hot_spot_points: NSPoint = msg_send![cursor, hotSpot];
        let size_points: NSSize = msg_send![image, size];

        // Use TIFFRepresentation + ImageIO to avoid fragile CGImageForProposedRect paths.
        let tiff: *mut objc::runtime::Object = msg_send![image, TIFFRepresentation];
        if tiff == nil {
            return None;
        }

        let image_source = CGImageSourceCreateWithData(tiff as *const std::ffi::c_void, std::ptr::null());
        if image_source.is_null() {
            return None;
        }

        let cursor_ptr = CGImageSourceCreateImageAtIndex(image_source, 0, std::ptr::null());
        CFRelease(image_source as *const std::ffi::c_void);

        if cursor_ptr.is_null() {
            return None;
        }

        let cursor_image: CGImage = CGImage::from_ptr(cursor_ptr);
        let cw = cursor_image.width() as u32;
        let ch = cursor_image.height() as u32;
        if cw == 0 || ch == 0 {
            return None;
        }

        let scale_x = if size_points.width > 0.0 {
            cw as f64 / size_points.width
        } else {
            1.0
        };
        let scale_y = if size_points.height > 0.0 {
            ch as f64 / size_points.height
        } else {
            1.0
        };

        let hotspot_x = (hot_spot_points.x * scale_x).round() as i32;
        let hotspot_y = (hot_spot_points.y * scale_y).round() as i32;

        // Convert cursor image to RGBA premultiplied.
        let bytes_per_row = (cw as usize) * 4;
        let color_space = core_graphics::color_space::CGColorSpace::create_device_rgb();
        let mut rgba = vec![0u8; bytes_per_row * (ch as usize)];
        let ctx = core_graphics::context::CGContext::create_bitmap_context(
            Some(rgba.as_mut_ptr() as *mut _),
            cw as usize,
            ch as usize,
            8,
            bytes_per_row,
            &color_space,
            core_graphics::base::kCGImageAlphaPremultipliedLast,
        );

        ctx.draw_image(
            CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: CGSize {
                    width: cw as f64,
                    height: ch as f64,
                },
            },
            &cursor_image,
        );

        Some((rgba, cw, ch, hotspot_x, hotspot_y))
    }
}

#[cfg(target_os = "macos")]
fn overlay_cursor_rgba_premultiplied(
    dst: &mut [u8],
    dst_width: u32,
    dst_height: u32,
    src: &[u8],
    src_width: u32,
    src_height: u32,
    dst_x: i32,
    dst_y: i32,
) {
    if dst_width == 0 || dst_height == 0 || src_width == 0 || src_height == 0 {
        return;
    }

    let dst_stride = (dst_width as usize) * 4;
    let src_stride = (src_width as usize) * 4;

    for sy in 0..(src_height as i32) {
        let dy = dst_y + sy;
        if dy < 0 || dy >= dst_height as i32 {
            continue;
        }
        let src_row = (sy as usize) * src_stride;
        let dst_row = (dy as usize) * dst_stride;

        for sx in 0..(src_width as i32) {
            let dx = dst_x + sx;
            if dx < 0 || dx >= dst_width as i32 {
                continue;
            }
            let si = src_row + (sx as usize) * 4;
            let di = dst_row + (dx as usize) * 4;
            if si + 3 >= src.len() || di + 3 >= dst.len() {
                continue;
            }

            let sa = src[si + 3] as u16;
            if sa == 0 {
                continue;
            }
            let inv = 255u16 - sa;

            // Premultiplied alpha-over: out = src + dst*(1-sa)
            dst[di] = (src[si] as u16 + (dst[di] as u16 * inv + 127) / 255) as u8;
            dst[di + 1] = (src[si + 1] as u16 + (dst[di + 1] as u16 * inv + 127) / 255) as u8;
            dst[di + 2] = (src[si + 2] as u16 + (dst[di + 2] as u16 * inv + 127) / 255) as u8;
            dst[di + 3] = (sa + (dst[di + 3] as u16 * inv + 127) / 255) as u8;
        }
    }
}

#[cfg(target_os = "macos")]
fn overlay_software_cursor_dot_rgba(
    pixel_data: &mut [u8],
    width: u32,
    height: u32,
    center_x: i32,
    center_y_from_top: i32,
) {
    if width == 0 || height == 0 {
        return;
    }

    let radius: i32 = 6;
    let border: i32 = 1;
    let r2 = radius * radius;
    let inner_r2 = (radius - border).max(0);
    let inner_r2 = inner_r2 * inner_r2;

    let stride = (width as usize) * 4;

    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let dist2 = dx * dx + dy * dy;
            if dist2 > r2 {
                continue;
            }

            let x = center_x + dx;
            let y = center_y_from_top + dy;

            if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                continue;
            }

            let is_border = dist2 >= inner_r2;
            let (r, g, b, a) = if is_border {
                (0u8, 0u8, 0u8, 255u8)
            } else {
                (255u8, 255u8, 255u8, 255u8)
            };

            let idx = (y as usize) * stride + (x as usize) * 4;
            if idx + 3 < pixel_data.len() {
                pixel_data[idx] = r;
                pixel_data[idx + 1] = g;
                pixel_data[idx + 2] = b;
                pixel_data[idx + 3] = a;
            }
        }
    }
}

// dispatch_sync to main queue - required for CoreGraphics calls from background threads
#[cfg(target_os = "macos")]
extern "C" {
    static _dispatch_main_q: std::ffi::c_void;
    
    fn dispatch_sync_f(
        queue: *const std::ffi::c_void,
        context: *mut std::ffi::c_void,
        work: extern "C" fn(*mut std::ffi::c_void),
    );
    
    fn pthread_main_np() -> i32;
}

// CGWindowImageOption values
const kCGWindowImageDefault: u32 = 0;
const kCGWindowImageNominalResolution: u32 = 1 << 4;
const kCGWindowImageBestResolution: u32 = 1 << 5;

/// macOS capture engine using CoreGraphics
pub struct MacOSCaptureEngine {
    is_active: bool,
    region: Option<CaptureRect>,
    show_cursor: bool,
    last_frame: Option<Arc<Vec<u8>>>,
    frame_width: u32,
    frame_height: u32,
    
    // Monitor tracking for multi-monitor support
    monitor_origin: (i32, i32),

    // ScreenCaptureKit backend (macOS 12.3+). When active, this captures frames
    // with the system cursor composited by the OS.
    sck: Option<ScreenCaptureKitCapture>,
    sck_last_seq: u64,
    using_sck: bool,
}

/// Context for dispatch_sync callback - returns raw pixel data, not CGImage
#[cfg(target_os = "macos")]
struct CaptureContext {
    region: CaptureRect,
    show_cursor: bool,
    // Results - all processing done on main thread
    pixel_data: Option<Vec<u8>>,
    result_width: u32,
    result_height: u32,
    error: Option<String>,
}

#[cfg(target_os = "macos")]
fn ensure_screen_recording_permission() -> Result<()> {
    // Avoid calling CGRequestScreenCaptureAccess here.
    // In practice, the user must enable Screen Recording in System Settings
    // and restart the app; requesting during a synchronous main-thread capture
    // can contribute to UI stalls.
    let has_perm = unsafe { CGPreflightScreenCaptureAccess() };
    if has_perm {
        return Ok(());
    }

    Err(anyhow!(
        "Screen Recording permission is not granted. Enable it in System Settings > Privacy & Security > Screen Recording, then restart the app.\n\nTip: if you're running via `cargo run`/VS Code, macOS may require granting Screen Recording to the launcher (Terminal / Visual Studio Code) rather than showing a stable 'rustframe' entry."
    ))
}

/// Callback function executed on main thread
/// Does ALL CoreGraphics work here to avoid any CG calls on worker threads
/// Supports both legacy and modern capture APIs based on macOS version
#[cfg(target_os = "macos")]
extern "C" fn capture_on_main_thread(context: *mut std::ffi::c_void) {
    let ctx = unsafe { &mut *(context as *mut CaptureContext) };
    
    autoreleasepool(|| {
        // Permission is checked before dispatching to the main thread.
        
        let capture_rect = CGRect {
            origin: CGPoint {
                x: ctx.region.x as f64,
                y: ctx.region.y as f64,
            },
            size: CGSize {
                width: ctx.region.width as f64,
                height: ctx.region.height as f64,
            },
        };
        
        // Choose capture method based on macOS version
        let image_ptr = if should_use_legacy_capture() {
            unsafe {
                CGWindowListCreateImage(
                    capture_rect,
                    kCGWindowListOptionOnScreenOnly,
                    kCGNullWindowID,
                    kCGWindowImageDefault | kCGWindowImageBestResolution,
                )
            }
        } else {
            unsafe {
                let display_id = CGMainDisplayID();
                CGDisplayCreateImageForRect(display_id, capture_rect)
            }
        };
        
        if image_ptr.is_null() {
            ctx.error = Some("Screen capture returned NULL - screen recording permission may be denied or region may be invalid".to_string());
            return;
        }
        
        // Take ownership of the image
        let screen_image: CGImage = unsafe { CGImage::from_ptr(image_ptr) };

        // Convert to RGBA8 - ALL on main thread
        let img_width = screen_image.width();
        let img_height = screen_image.height();
        if img_width == 0 || img_height == 0 {
            ctx.error = Some("Captured image has zero dimensions".to_string());
            return;
        }
        
        let width = img_width as u32;
        let height = img_height as u32;
        let bytes_per_row = width as usize * 4;
        
        let color_space = core_graphics::color_space::CGColorSpace::create_device_rgb();
        let mut pixel_data = vec![0u8; bytes_per_row * height as usize];
        let cg_context = core_graphics::context::CGContext::create_bitmap_context(
            Some(pixel_data.as_mut_ptr() as *mut _),
            width as usize,
            height as usize,
            8,
            bytes_per_row,
            &color_space,
            core_graphics::base::kCGImageAlphaPremultipliedLast,
        );

        cg_context.draw_image(
            core_graphics::geometry::CGRect {
                origin: core_graphics::geometry::CGPoint { x: 0.0, y: 0.0 },
                size: core_graphics::geometry::CGSize {
                    width: width as f64,
                    height: height as f64,
                },
            },
            &screen_image,
        );

        // Optional: overlay cursor into the captured frame.
        // CoreGraphics screen capture does not include the cursor by default, so we overlay it.
        // We prefer the real system cursor image; if we can't fetch it reliably, we fall back to
        // a software dot.
        if ctx.show_cursor {
            // Use NSEvent mouseLocation (global, bottom-left origin in Cocoa screen coords)
            // and convert to our capture-space which uses top-left origin (see hollow_border cache).
            use cocoa::appkit::{NSEvent, NSScreen};
            use objc::{class, msg_send, sel, sel_impl};

            unsafe {
                let screen: *mut objc::runtime::Object = msg_send![class!(NSScreen), mainScreen];
                if screen != nil {
                    let screen_frame: NSRect = msg_send![screen, frame];
                    let screen_height_points = screen_frame.size.height;

                    let mouse: NSPoint = msg_send![class!(NSEvent), mouseLocation];

                    // Convert to top-left origin (points)
                    let cursor_x_points = mouse.x;
                    let cursor_y_points_tl = screen_height_points - mouse.y;

                    let rel_x_points = cursor_x_points - ctx.region.x as f64;
                    let rel_y_points = cursor_y_points_tl - ctx.region.y as f64;

                    if ctx.region.width > 0
                        && ctx.region.height > 0
                        && rel_x_points >= 0.0
                        && rel_y_points >= 0.0
                        && rel_x_points <= ctx.region.width as f64
                        && rel_y_points <= ctx.region.height as f64
                    {
                        // The capture region is specified in points; the captured image size may
                        // be scaled (Retina), so compute a point->pixel scale.
                        let scale_x = width as f64 / ctx.region.width as f64;
                        let scale_y = height as f64 / ctx.region.height as f64;

                        let rel_x_px = rel_x_points * scale_x;
                        let rel_y_px_from_top = rel_y_points * scale_y;

                        let cursor_x_px = rel_x_px.round() as i32;
                        let cursor_y_px = rel_y_px_from_top.round() as i32;

                        if let Some((cursor_rgba, cw, ch, hot_x, hot_y)) = cursor_rgba_premultiplied() {
                            let draw_x = cursor_x_px - hot_x;
                            let draw_y = cursor_y_px - hot_y;
                            overlay_cursor_rgba_premultiplied(
                                &mut pixel_data,
                                width,
                                height,
                                &cursor_rgba,
                                cw,
                                ch,
                                draw_x,
                                draw_y,
                            );
                        } else {
                            overlay_software_cursor_dot_rgba(
                                &mut pixel_data,
                                width,
                                height,
                                cursor_x_px,
                                cursor_y_px,
                            );
                        }
                    }
                }
            }
        }
        
        // Store results
        ctx.pixel_data = Some(pixel_data);
        ctx.result_width = width;
        ctx.result_height = height;
    });
}

impl MacOSCaptureEngine {
    pub fn new() -> Result<Self> {
        log::info!("[MACOS_ENGINE] new() called");
        
        // Log which capture method will be used
        #[cfg(target_os = "macos")]
        log_capture_method();
        
        log::info!("[MACOS_ENGINE] Checking SCK availability...");
        log::info!("[MACOS_ENGINE]   cfg!(target_os = \"macos\"): {}", cfg!(target_os = "macos"));
        log::info!("[MACOS_ENGINE]   supports_screen_capture_kit(): {}", supports_screen_capture_kit());
        log::info!("[MACOS_ENGINE]   ScreenCaptureKitCapture::is_available(): {}", ScreenCaptureKitCapture::is_available());
        
        let sck = if cfg!(target_os = "macos") && supports_screen_capture_kit() && ScreenCaptureKitCapture::is_available() {
            log::info!("[MACOS_ENGINE] Creating ScreenCaptureKit instance...");
            Some(ScreenCaptureKitCapture::new())
        } else {
            log::warn!("[MACOS_ENGINE] ScreenCaptureKit NOT available, will use CoreGraphics fallback");
            None
        };
        
        log::info!("[MACOS_ENGINE] sck.is_some() = {}", sck.is_some());

        Ok(Self {
            is_active: false,
            region: None,
            show_cursor: true,
            last_frame: None,
            frame_width: 0,
            frame_height: 0,
            monitor_origin: (0, 0),

            sck,
            sck_last_seq: 0,
            using_sck: false,
        })
    }

    /// Capture a region of the screen - dispatches ALL work to main thread
    #[cfg(target_os = "macos")]
    fn capture_region(&mut self, region: CaptureRect) -> Result<()> {
        ensure_screen_recording_permission()?;

        // Create context for the callback
        let mut ctx = CaptureContext {
            region,
            show_cursor: self.show_cursor,
            pixel_data: None,
            result_width: 0,
            result_height: 0,
            error: None,
        };
        
        // Dispatch ALL capture work to main thread synchronously.
        // IMPORTANT: dispatching synchronously to the main queue from the main thread will deadlock.
        unsafe {
            let is_main = pthread_main_np() != 0;
            if is_main {
                capture_on_main_thread(&mut ctx as *mut CaptureContext as *mut std::ffi::c_void);
            } else {
                let main_queue = &_dispatch_main_q as *const std::ffi::c_void;
                dispatch_sync_f(
                    main_queue,
                    &mut ctx as *mut CaptureContext as *mut std::ffi::c_void,
                    capture_on_main_thread,
                );
            }
        }
        
        // Check for errors from the callback
        if let Some(err) = ctx.error {
            log::error!("Capture error: {}", err);
            return Err(anyhow!(err));
        }
        
        // Get the pixel data (already converted on main thread)
        let pixel_data = ctx.pixel_data.ok_or_else(|| anyhow!("No pixel data captured"))?;
        
        self.frame_width = ctx.result_width;
        self.frame_height = ctx.result_height;
        self.last_frame = Some(Arc::new(pixel_data));
        self.region = Some(region);
        
        log::info!("Capture region completed successfully: {}x{}", self.frame_width, self.frame_height);
        Ok(())
    }
    
    #[cfg(not(target_os = "macos"))]
    fn capture_region(&mut self, _region: CaptureRect) -> Result<()> {
        Err(anyhow!("macOS capture engine called on non-macOS build"))
    }
}

impl CaptureEngine for MacOSCaptureEngine {
    fn start(&mut self, region: CaptureRect, show_cursor: bool) -> Result<()> {
        log::info!("[MACOS_ENGINE] start() called with region: {:?}, cursor: {}", region, show_cursor);

        // IMPORTANT: If Screen Recording permission is not granted, calling into
        // ScreenCaptureKit/CoreGraphics can fail in non-obvious ways. Preflight
        // here and fail gracefully (no crash) so the user can grant permission.
        #[cfg(target_os = "macos")]
        {
            log::info!("[MACOS_ENGINE] Checking Screen Recording permission...");
            if let Err(e) = ensure_screen_recording_permission() {
                log::error!("[MACOS_ENGINE] Permission check failed: {}", e);
                self.is_active = false;
                self.using_sck = false;
                return Err(e);
            }
            log::info!("[MACOS_ENGINE] Permission check passed");
        }

        self.region = Some(region);
        self.show_cursor = show_cursor;
        self.is_active = true;

        // Detect which monitor contains this region
        let center_x = region.x + (region.width as i32 / 2);
        let center_y = region.y + (region.height as i32 / 2);
        
        if let Some((origin_x, origin_y, display_width, display_height)) = Self::get_display_for_point(center_x, center_y) {
            self.monitor_origin = (origin_x, origin_y);
            log::info!("[MACOS_ENGINE] Capture region on monitor at origin ({}, {}), size: {}x{}", 
                      origin_x, origin_y, display_width, display_height);
        } else {
            log::warn!("[MACOS_ENGINE] Could not determine display for point ({}, {})", center_x, center_y);
            self.monitor_origin = (0, 0);
        }

        // Prefer ScreenCaptureKit when available; it includes the real system cursor.
        log::info!("[MACOS_ENGINE] Checking if SCK is available... self.sck.is_some()={}", self.sck.is_some());
        if let Some(ref mut sck) = self.sck {
            log::info!("[MACOS_ENGINE] Checking supports_screen_capture_kit()...");
            if supports_screen_capture_kit() {
                log::info!("[MACOS_ENGINE] SCK is supported, calling sck.start()...");
                match sck.start(region.x, region.y, region.width, region.height, show_cursor) {
                    Ok(()) => {
                        log::info!("[MACOS_ENGINE] SCK start succeeded!");
                        self.using_sck = true;
                        self.sck_last_seq = 0;
                        return Ok(());
                    }
                    Err(e) => {
                        log::warn!("[MACOS_ENGINE] ScreenCaptureKit init failed, falling back to CoreGraphics: {}", e);
                        self.using_sck = false;
                    }
                }
            } else {
                log::info!("[MACOS_ENGINE] ScreenCaptureKit not supported on this macOS version");
            }
        } else {
            log::warn!("[MACOS_ENGINE] self.sck is None!");
        }

        // Fallback: legacy CoreGraphics screenshot capture.
        log::info!("[MACOS_ENGINE] Using CoreGraphics fallback");
        self.capture_region(region)?;
        self.using_sck = false;
        Ok(())
    }

    fn stop(&mut self) {
        self.is_active = false;
        self.last_frame = None;
        if self.using_sck {
            if let Some(ref mut sck) = self.sck {
                sck.stop();
            }
        }
        self.using_sck = false;
        info!("Stopped macOS capture");
    }

    fn is_active(&self) -> bool {
        self.is_active
    }

    fn has_new_frame(&self) -> bool {
        if !self.is_active {
            return false;
        }

        if self.using_sck {
            if let Some(ref sck) = self.sck {
                return sck.latest_seq() != self.sck_last_seq;
            }
            return false;
        }

        true
    }

    fn get_frame(&mut self) -> Option<CaptureFrame> {
        if !self.is_active {
            return None;
        }

        if self.using_sck {
            if let Some(region) = self.region {
                if let Some(ref sck) = self.sck {
                    if let Some((data, w, h, seq)) = sck.latest_frame_rgba() {
                        if seq != self.sck_last_seq {
                            self.sck_last_seq = seq;
                        }
                        
// Get IOSurface data for GPU acceleration (includes retained pointer)
						let gpu_texture = sck.latest_iosurface().map(|(iosurface_ptr, iosurface_id, pixel_format, crop_x, crop_y, crop_w, crop_h)| {
							super::GpuTextureHandle::Metal {
								iosurface_ptr,
                                iosurface_id,
                                pixel_format,
                                crop_x,
                                crop_y,
                                crop_w,
                                crop_h,
                            }
                        });
                        
                        return Some(CaptureFrame {
                            data,
                            width: w,
                            height: h,
                            stride: w * 4,
                            offset_x: region.x,
                            offset_y: region.y,
                            gpu_texture,
                        });
                    }
                }
            }
            return None;
        }

        if let Some(region) = self.region {
            if self.capture_region(region).is_ok() {
                if let Some(pixel_data) = &self.last_frame {
                    return Some(CaptureFrame {
                        data: (**pixel_data).clone(),
                        width: self.frame_width,
                        height: self.frame_height,
                        stride: self.frame_width * 4,
                        offset_x: region.x,
                        offset_y: region.y,
                        gpu_texture: None, // CoreGraphics fallback doesn't support GPU
                    });
                }
            }
        }
        None
    }

    fn set_cursor_visible(&mut self, visible: bool) -> Result<()> {
        self.show_cursor = visible;

        // ScreenCaptureKit cursor visibility is a stream configuration; for now,
        // we apply it on next start (or by stopping/starting capture).
        Ok(())
    }

    fn update_region(&mut self, region: CaptureRect) -> Result<()> {
        self.region = Some(region);

        if self.using_sck {
            if let Some(ref sck) = self.sck {
                sck.update_region_points(region.x, region.y, region.width, region.height);
            }
        }
        Ok(())
    }

    fn get_region(&self) -> Option<CaptureRect> {
        self.region
    }
    
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl MacOSCaptureEngine {
    /// Get the current monitor origin (for monitor change detection)
    pub fn get_monitor_origin(&self) -> (i32, i32) {
        self.monitor_origin
    }
    
    /// Detect which display contains the given point and return its bounds
    #[cfg(target_os = "macos")]
    fn get_display_for_point(x: i32, y: i32) -> Option<(i32, i32, u32, u32)> {
        use core_graphics::display::{CGDisplay, CGRect};
        use core_graphics::geometry::{CGPoint, CGSize};
        
        // Create a 1x1 rect at the point to query
        let rect = CGRect::new(
            &CGPoint::new(x as f64, y as f64),
            &CGSize::new(1.0, 1.0)
        );
        let display_count = 1;
        let mut display_id: u32 = 0;
        
        unsafe {
            // Get display at point (using 1x1 rect)
            if core_graphics::display::CGGetDisplaysWithRect(
                rect,
                display_count,
                &mut display_id,
                std::ptr::null_mut()
            ) == 0 {
                let display = CGDisplay::new(display_id);
                let bounds = display.bounds();
                return Some((
                    bounds.origin.x as i32,
                    bounds.origin.y as i32,
                    bounds.size.width as u32,
                    bounds.size.height as u32,
                ));
            }
        }
        None
    }
}
