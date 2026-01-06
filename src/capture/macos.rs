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
}

/// Context for dispatch_sync callback - returns raw pixel data, not CGImage
#[cfg(target_os = "macos")]
struct CaptureContext {
    region: CaptureRect,
    // Results - all processing done on main thread
    pixel_data: Option<Vec<u8>>,
    result_width: u32,
    result_height: u32,
    error: Option<String>,
}

/// Callback function executed on main thread
/// Does ALL CoreGraphics work here to avoid any CG calls on worker threads
/// Supports both legacy and modern capture APIs based on macOS version
#[cfg(target_os = "macos")]
extern "C" fn capture_on_main_thread(context: *mut std::ffi::c_void) {
    let ctx = unsafe { &mut *(context as *mut CaptureContext) };
    
    autoreleasepool(|| {
        // Check permission first
        unsafe {
            let has_perm = CGPreflightScreenCaptureAccess();
            if !has_perm {
                let granted = CGRequestScreenCaptureAccess();
                
                if !granted {
                    ctx.error = Some("Screen Recording permission denied. Please enable it in System Settings > Privacy & Security > Screen Recording and restart the app".to_string());
                    log::error!("Screen capture permission denied");
                    return;
                }
            }
        }
        
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
        
        // Store results
        ctx.pixel_data = Some(pixel_data);
        ctx.result_width = width;
        ctx.result_height = height;
    });
}

impl MacOSCaptureEngine {
    pub fn new() -> Result<Self> {
        // Log which capture method will be used
        #[cfg(target_os = "macos")]
        log_capture_method();
        
        Ok(Self {
            is_active: false,
            region: None,
            show_cursor: true,
            last_frame: None,
            frame_width: 0,
            frame_height: 0,
        })
    }

    /// Capture a region of the screen - dispatches ALL work to main thread
    #[cfg(target_os = "macos")]
    fn capture_region(&mut self, region: CaptureRect) -> Result<()> {
        // Create context for the callback
        let mut ctx = CaptureContext {
            region,
            pixel_data: None,
            result_width: 0,
            result_height: 0,
            error: None,
        };
        
        // Dispatch ALL capture work to main thread synchronously
        unsafe {
            let main_queue = &_dispatch_main_q as *const std::ffi::c_void;
            dispatch_sync_f(
                main_queue,
                &mut ctx as *mut CaptureContext as *mut std::ffi::c_void,
                capture_on_main_thread,
            );
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
        println!("[CAPTURE] MacOSCaptureEngine::start called");
        info!("Starting macOS capture for region: {:?}", region);

        self.region = Some(region);
        self.show_cursor = show_cursor;
        self.is_active = true;

        println!("[CAPTURE] About to call capture_region from start");
        self.capture_region(region)?;
        println!("[CAPTURE] start completed successfully");
        Ok(())
    }

    fn stop(&mut self) {
        self.is_active = false;
        self.last_frame = None;
        info!("Stopped macOS capture");
    }

    fn is_active(&self) -> bool {
        self.is_active
    }

    fn has_new_frame(&self) -> bool {
        self.is_active
    }

    fn get_frame(&mut self) -> Option<CaptureFrame> {
        if !self.is_active {
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
                    });
                }
            }
        }
        None
    }

    fn set_cursor_visible(&mut self, visible: bool) -> Result<()> {
        self.show_cursor = visible;
        Ok(())
    }

    fn update_region(&mut self, region: CaptureRect) -> Result<()> {
        self.region = Some(region);
        Ok(())
    }

    fn get_region(&self) -> Option<CaptureRect> {
        self.region
    }
}
