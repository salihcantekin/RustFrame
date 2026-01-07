//! Display Information Manager
//!
//! Central management of display properties (scale factor, resolution, bounds).
//! All coordinate conversions should use this module to ensure consistency.
//!
//! # Cross-Platform Support
//!
//! ## macOS
//! Uses NSScreen.mainScreen to get accurate display information including:
//! - backingScaleFactor (2.0 for Retina displays)
//! - Frame bounds in points
//! - Automatically handles coordinate system (bottom-left origin)
//!
//! ## Windows  
//! Uses Win32 GetDeviceCaps API to query:
//! - LOGPIXELSX for DPI (scale factor = DPI / 96.0)
//! - HORZRES, VERTRES for screen resolution
//! - Handles high-DPI displays correctly
//!
//! ## Linux
//! Detects X11 or Wayland and provides sensible defaults:
//! - Checks DISPLAY environment variable for X11
//! - Checks WAYLAND_DISPLAY for Wayland
//! - Uses 1.0 scale factor by default
//! - TODO: Integrate with X11/Wayland APIs for accurate detection
//!
//! # Usage
//!
//! ```rust
//! // Initialize once at application startup
//! display_info::initialize()?;
//!
//! // Get display info anywhere
//! let info = display_info::get();
//! println!("Scale: {}x", info.scale_factor);
//!
//! // Convert coordinates
//! let pixels = info.points_to_pixels(100.0);
//! let (x_px, y_px) = info.point_to_pixel_coords(x_pt, y_pt);
//! ```

use std::sync::{Arc, RwLock};
use lazy_static::lazy_static;
use log::{info, warn};

lazy_static! {
    /// Global display information singleton
    static ref DISPLAY_INFO: Arc<RwLock<DisplayInfo>> = Arc::new(RwLock::new(DisplayInfo::default()));
}

/// Display information for coordinate system management
#[derive(Debug, Clone)]
pub struct DisplayInfo {
    /// Backing scale factor (2.0 for Retina, 1.0 for standard)
    pub scale_factor: f64,
    /// Screen width in points (logical coordinates)
    pub width_points: f64,
    /// Screen height in points (logical coordinates)  
    pub height_points: f64,
    /// Screen width in pixels (physical coordinates)
    pub width_pixels: u32,
    /// Screen height in pixels (physical coordinates)
    pub height_pixels: u32,
    /// Whether display info has been initialized
    pub initialized: bool,
}

impl Default for DisplayInfo {
    fn default() -> Self {
        Self {
            scale_factor: 1.0,
            width_points: 0.0,
            height_points: 0.0,
            width_pixels: 0,
            height_pixels: 0,
            initialized: false,
        }
    }
}

impl DisplayInfo {
    /// Convert points to pixels
    pub fn points_to_pixels(&self, points: f64) -> i32 {
        (points * self.scale_factor).round() as i32
    }
    
    /// Convert pixels to points
    pub fn pixels_to_points(&self, pixels: i32) -> f64 {
        pixels as f64 / self.scale_factor
    }
    
    /// Convert point coordinates to pixel coordinates
    pub fn point_to_pixel_coords(&self, x_points: i32, y_points: i32) -> (i32, i32) {
        (
            self.points_to_pixels(x_points as f64),
            self.points_to_pixels(y_points as f64),
        )
    }
    
    /// Convert pixel coordinates to point coordinates  
    pub fn pixel_to_point_coords(&self, x_pixels: i32, y_pixels: i32) -> (i32, i32) {
        (
            self.pixels_to_points(x_pixels) as i32,
            self.pixels_to_points(y_pixels) as i32,
        )
    }
    
    /// Convert macOS CGEvent coordinates to screen capture pixel coordinates
    /// 
    /// IMPORTANT: CGEventGetLocation returns coordinates in:
    /// - Origin: TOP-LEFT (not bottom-left like NSEvent.mouseLocation!)
    /// - Units: POINTS (not pixels)
    /// 
    /// Screen capture uses:
    /// - Origin: TOP-LEFT (same as CGEvent)
    /// - Units: PIXELS
    /// 
    /// So we ONLY need to scale, NO Y-axis flip!
    #[cfg(target_os = "macos")]
    pub fn macos_event_to_screen_pixels(&self, x_points: f64, y_points: f64) -> (i32, i32) {
        // CGEvent already uses top-left origin, so just scale to pixels
        let x_pixels = (x_points * self.scale_factor) as i32;
        let y_pixels = (y_points * self.scale_factor) as i32;
        
        (x_pixels, y_pixels)
    }
    
    /// Convert screen pixels (top-left origin) to macOS NSEvent coordinates (bottom-left origin, points)
    #[cfg(target_os = "macos")]
    pub fn screen_pixels_to_macos_event(&self, x_pixels: i32, y_pixels: i32) -> (f64, f64) {
        // Convert to points
        let x_points = x_pixels as f64 / self.scale_factor;
        let y_points = y_pixels as f64 / self.scale_factor;
        
        // Flip Y coordinate (macOS uses bottom-left origin)
        let y_flipped_points = self.height_points - y_points;
        
        (x_points, y_flipped_points)
    }
}

/// Initialize display information from the operating system
#[cfg(target_os = "macos")]
pub fn initialize() -> anyhow::Result<()> {
    use cocoa::base::id;
    use cocoa::foundation::NSRect;
    use objc::*;
    
    unsafe {
        let screen: id = msg_send![class!(NSScreen), mainScreen];
        if screen.is_null() {
            return Err(anyhow::anyhow!("Failed to get main screen"));
        }
        
        let frame: NSRect = msg_send![screen, frame];
        let scale_factor: f64 = msg_send![screen, backingScaleFactor];
        
        let width_points = frame.size.width;
        let height_points = frame.size.height;
        let width_pixels = (width_points * scale_factor).round() as u32;
        let height_pixels = (height_points * scale_factor).round() as u32;
        
        let mut display_info = DISPLAY_INFO.write()
            .map_err(|e| anyhow::anyhow!("Failed to lock display info: {}", e))?;
        
        *display_info = DisplayInfo {
            scale_factor,
            width_points,
            height_points,
            width_pixels,
            height_pixels,
            initialized: true,
        };
        
        info!("[DISPLAY_INFO] Initialized: {}x{} points ({}x{} pixels) @ {:.1}x scale",
            width_points as u32, height_points as u32,
            width_pixels, height_pixels,
            scale_factor);
        
        Ok(())
    }
}

#[cfg(target_os = "windows")]
pub fn initialize() -> anyhow::Result<()> {
    use windows::Win32::Graphics::Gdi::{
        GetDC, GetDeviceCaps, ReleaseDC, 
        LOGPIXELSX, HORZRES, VERTRES, HDC
    };
    use windows::Win32::Foundation::HWND;
    
    unsafe {
        let hdc = GetDC(HWND(0));
        if hdc.0 == 0 {
            return Err(anyhow::anyhow!("Failed to get device context"));
        }
        
        let dpi = GetDeviceCaps(hdc, LOGPIXELSX);
        let scale_factor = (dpi as f64 / 96.0).max(1.0); // 96 DPI is baseline, min 1.0
        
        let width_pixels = GetDeviceCaps(hdc, HORZRES) as u32;
        let height_pixels = GetDeviceCaps(hdc, VERTRES) as u32;
        
        let _ = ReleaseDC(HWND(0), hdc);
        
        let width_points = width_pixels as f64 / scale_factor;
        let height_points = height_pixels as f64 / scale_factor;
        
        let mut display_info = DISPLAY_INFO.write()
            .map_err(|e| anyhow::anyhow!("Failed to lock display info: {}", e))?;
        
        *display_info = DisplayInfo {
            scale_factor,
            width_points,
            height_points,
            width_pixels,
            height_pixels,
            initialized: true,
        };
        
        info!("[DISPLAY_INFO] Initialized: {}x{} points ({}x{} pixels) @ {:.1}x scale",
            width_points as u32, height_points as u32,
            width_pixels, height_pixels,
            scale_factor);
        
        Ok(())
    }
}

#[cfg(target_os = "linux")]
pub fn initialize() -> anyhow::Result<()> {
    // Try to get display info from environment variables (set by X11/Wayland)
    let (width_pixels, height_pixels, scale_factor) = 
        if let Ok(display) = std::env::var("DISPLAY") {
            // X11 is available
            info!("[DISPLAY_INFO] X11 display detected: {}", display);
            // TODO: Use X11 APIs to get actual resolution
            // For now, use common default
            (1920, 1080, 1.0)
        } else if std::env::var("WAYLAND_DISPLAY").is_ok() {
            // Wayland is available
            info!("[DISPLAY_INFO] Wayland display detected");
            // TODO: Use Wayland APIs to get actual resolution
            (1920, 1080, 1.0)
        } else {
            warn!("[DISPLAY_INFO] No display server detected, using defaults");
            (1920, 1080, 1.0)
        };
    
    let width_points = width_pixels as f64 / scale_factor;
    let height_points = height_pixels as f64 / scale_factor;
    
    let mut display_info = DISPLAY_INFO.write()
        .map_err(|e| anyhow::anyhow!("Failed to lock display info: {}", e))?;
    
    *display_info = DisplayInfo {
        scale_factor,
        width_points,
        height_points,
        width_pixels,
        height_pixels,
        initialized: true,
    };
    
    info!("[DISPLAY_INFO] Initialized: {}x{} points ({}x{} pixels) @ {:.1}x scale",
        width_points as u32, height_points as u32,
        width_pixels, height_pixels,
        scale_factor);
    
    Ok(())
}

/// Get the current display information
pub fn get() -> DisplayInfo {
    DISPLAY_INFO.read()
        .map(|info| info.clone())
        .unwrap_or_default()
}

/// Get the scale factor (convenience function)
pub fn scale_factor() -> f64 {
    get().scale_factor
}

/// Check if display info has been initialized
pub fn is_initialized() -> bool {
    DISPLAY_INFO.read()
        .map(|info| info.initialized)
        .unwrap_or(false)
}
