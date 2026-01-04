// capture/macos.rs - macOS Screen Capture Implementation
//
// Uses CoreGraphics to capture screen content for macOS

use super::{CaptureEngine, CaptureFrame, CaptureRect};
use anyhow::{anyhow, Result};
use core_graphics::display::{CGDisplay, kCGWindowListOptionOnScreenOnly, kCGNullWindowID};
use core_graphics::window::kCGWindowImageDefault;
use core_graphics::image::CGImage;
use std::sync::Arc;
use log::info;

/// macOS capture engine using CoreGraphics
pub struct MacOSCaptureEngine {
    is_active: bool,
    region: Option<CaptureRect>,
    show_cursor: bool,
    last_frame: Option<Arc<Vec<u8>>>,
    frame_width: u32,
    frame_height: u32,
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

    /// Get the main display
    fn get_display_for_point(x: i32, y: i32) -> Result<CGDisplay> {
        let displays = CGDisplay::active_displays()
            .map_err(|_| anyhow!("Failed to get active displays"))?;
        
        if displays.is_empty() {
            return Err(anyhow!("No displays found"));
        }

        // Find the display containing the point
        for display_id in displays {
            let display = CGDisplay::new(display_id);
            let bounds = display.bounds();
            
            if x >= bounds.origin.x as i32 
                && x < (bounds.origin.x + bounds.size.width) as i32
                && y >= bounds.origin.y as i32 
                && y < (bounds.origin.y + bounds.size.height) as i32 {
                return Ok(display);
            }
        }

        // Default to main display if point is outside all displays
        Ok(CGDisplay::main())
    }

    /// Capture a region of the screen
    fn capture_region(&mut self, region: CaptureRect) -> Result<()> {
        let center_x = region.x + (region.width as i32) / 2;
        let center_y = region.y + (region.height as i32) / 2;
        
        log::info!(
            "capture_region called: ({}, {}) to ({}, {}) center: ({}, {})",
            region.x, region.y,
            region.x + region.width as i32,
            region.y + region.height as i32,
            center_x, center_y
        );
        
        let display = Self::get_display_for_point(center_x, center_y)
            .map_err(|e| {
                log::error!("Failed to get display: {}", e);
                e
            })?;
        
        log::info!("Got display, creating screen image...");

        // Create image from display - this can panic on some systems
        let screen_image = match std::panic::catch_unwind(|| {
            display.image()
        }) {
            Ok(Some(img)) => {
                log::info!("Screen image created successfully: {}x{}", img.width(), img.height());
                img
            },
            Ok(None) => {
                log::error!("Failed to capture screen image - display.image() returned None");
                return Err(anyhow!("Failed to capture screen image"));
            },
            Err(e) => {
                log::error!("Panic during screen capture: {:?}", e);
                return Err(anyhow!("Panic during screen capture"));
            }
        };

        // Convert CGImage to RGBA8 pixel data
        log::info!("Converting image to RGBA8...");
        self.convert_image_to_rgba8(&screen_image, region)
            .map_err(|e| {
                log::error!("Failed to convert image: {}", e);
                e
            })?;
        
        log::info!("Capture region completed successfully");
        Ok(())
    }

    /// Convert CGImage to RGBA8 format and store in buffer
    fn convert_image_to_rgba8(&mut self, image: &CGImage, region: CaptureRect) -> Result<()> {
        let img_width = image.width();
        let img_height = image.height();
        
        log::info!(
            "Converting CGImage {}x{} for region ({},{}) {}x{}",
            img_width, img_height, region.x, region.y, region.width, region.height
        );
        
        // Clamp region to image bounds
        let x = region.x.max(0).min(img_width as i32);
        let y = region.y.max(0).min(img_height as i32);
        let width = region.width.min((img_width as i32 - x).max(0) as u32);
        let height = region.height.min((img_height as i32 - y).max(0) as u32);
        
        if width == 0 || height == 0 {
            return Err(anyhow!("Invalid region dimensions after clamping"));
        }
        
        let bytes_per_row = width as usize * 4; // RGBA8
        
        // Create bitmap context with RGBA8 format
        let color_space = core_graphics::color_space::CGColorSpace::create_device_rgb();
        
        let mut pixel_data = vec![0u8; bytes_per_row * height as usize];
        
        let context = core_graphics::context::CGContext::create_bitmap_context(
            Some(pixel_data.as_mut_ptr() as *mut _),
            width as usize,
            height as usize,
            8, // bits per component
            bytes_per_row,
            &color_space,
            core_graphics::base::kCGImageAlphaPremultipliedLast,
        );
        
        // Draw the entire image, positioned so the region appears at (0,0) in the context
        // macOS has inverted Y axis for drawing
        context.draw_image(
            core_graphics::geometry::CGRect {
                origin: core_graphics::geometry::CGPoint {
                    x: -(x as f64),
                    y: -(y as f64),
                },
                size: core_graphics::geometry::CGSize {
                    width: img_width as f64,
                    height: img_height as f64,
                },
            },
            image,
        );
        
        self.frame_width = width;
        self.frame_height = height;
        self.last_frame = Some(Arc::new(pixel_data));
        
        log::info!("Frame converted successfully: {}x{}", width, height);
        
        Ok(())
    }
}

impl CaptureEngine for MacOSCaptureEngine {
    fn start(&mut self, region: CaptureRect, show_cursor: bool) -> Result<()> {
        info!("Starting macOS capture for region: {:?}", region);
        
        self.region = Some(region);
        self.show_cursor = show_cursor;
        self.is_active = true;

        // Perform initial capture to validate setup
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
        // For simplicity, always indicate a new frame is available
        // In a more sophisticated implementation, this would track changes
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
