Host key verification failed.
scp: Connection closed
// capture/macos.rs - macOS Screen Capture Implementation
//
// Uses CoreGraphics to capture screen content for macOS
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

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGWindowListCreateImage(
        screenBounds: CGRect,
        listOption: u32,
        windowID: u32,
        imageOption: u32,
    ) -> *mut core_graphics::sys::CGImage;
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
}

// CGWindowImageOption values
const kCGWindowImageDefault: u32 = 0;
const kCGWindowImageNominalResolution: u32 = 1 << 4;

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
#[cfg(target_os = "macos")]
extern "C" fn capture_on_main_thread(context: *mut std::ffi::c_void) {
    let ctx = unsafe { &mut *(context as *mut CaptureContext) };
    
    autoreleasepool(|| {
        // Check permission first
        unsafe {
            if !CGPreflightScreenCaptureAccess() {
                ctx.error = Some("Screen Recording permission not granted. Enable it in System Settings > Privacy & Security > Screen Recording".to_string());
                return;
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
        
        let image_ptr = unsafe {
            CGWindowListCreateImage(
                capture_rect,
                kCGWindowListOptionOnScreenOnly,
                kCGNullWindowID,
                kCGWindowImageDefault | kCGWindowImageNominalResolution,
            )
        };
        
        if image_ptr.is_null() {
            ctx.error = Some("CGWindowListCreateImage returned NULL - screen recording permission may be denied".to_string());
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
        log::info!(
            "capture_region: capturing at ({}, {}) size: {}x{}",
            region.x, region.y, region.width, region.height
        );
        
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
        info!("Starting macOS capture for region: {:?}", region);

        self.region = Some(region);
        self.show_cursor = show_cursor;
        self.is_active = true;

        self.capture_region(region)?;
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
