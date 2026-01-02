// ui/capture_border/mod.rs - Platform-agnostic Capture Border Window
//
// This module provides a transparent border window for screen capture.
// The center is fully transparent and click-through, only the border is visible.
// Platform-specific implementations handle the actual rendering.

use winit::event_loop::EventLoopProxy;
use crate::UserEvent;

// Platform-specific implementations
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "macos")]
mod macos;

// Re-export platform-specific implementation
#[cfg(windows)]
pub use self::windows::CaptureBorderWindow;

#[cfg(target_os = "linux")]
pub use self::linux::CaptureBorderWindow;

#[cfg(target_os = "macos")]
pub use self::macos::CaptureBorderWindow;

/// Border visual style configuration
#[derive(Debug, Clone)]
pub struct BorderStyle {
    /// Border width in pixels
    pub border_width: i32,
    /// Corner marker size
    pub corner_size: i32,
    /// Corner marker thickness
    pub corner_thickness: i32,
    /// Show recording indicator
    pub show_rec_indicator: bool,
    /// REC indicator size (1=Small, 2=Medium, 3=Large)
    pub rec_indicator_size: u32,
}

impl Default for BorderStyle {
    fn default() -> Self {
        Self {
            border_width: 5,
            corner_size: 30,
            corner_thickness: 8,
            show_rec_indicator: true,
            rec_indicator_size: 2, // Medium
        }
    }
}

/// Border colors (ARGB format)
#[derive(Debug, Clone)]
pub struct BorderColors {
    /// Main border color
    pub border: u32,
    /// Corner marker color
    pub corner: u32,
    /// Transparent (for center)
    pub transparent: u32,
    /// Recording indicator red
    pub rec_red: u32,
    /// Recording indicator background
    pub rec_bg: u32,
    /// Recording indicator text
    pub rec_text: u32,
}

impl Default for BorderColors {
    fn default() -> Self {
        Self {
            border: 0xE0FF9A3C,      // Orange border with high alpha
            corner: 0xFFFFFFFF,       // White corners (fully opaque)
            transparent: 0x00000000,  // Fully transparent center
            rec_red: 0xB0FF4040,      // Recording indicator red (semi-transparent)
            rec_bg: 0x80181818,       // Recording indicator background
            rec_text: 0xD0FFFFFF,     // Recording indicator text
        }
    }
}

impl BorderColors {
    /// Create BorderColors from CaptureSettings border_color [R, G, B, A]
    pub fn from_settings(border_color: [u8; 4]) -> Self {
        // Convert RGBA to ARGB format (used by GDI)
        let r = border_color[0];
        let g = border_color[1];
        let b = border_color[2];
        let a = border_color[3];
        
        // ARGB format: 0xAARRGGBB -> but GDI uses BGRA in memory
        // For UpdateLayeredWindow with premultiplied alpha: 0xAABBGGRR
        let border_argb = ((a as u32) << 24) | ((b as u32) << 16) | ((g as u32) << 8) | (r as u32);
        
        Self {
            border: border_argb,
            corner: 0xFFFFFFFF,       // White corners
            transparent: 0x00000000,
            rec_red: 0xB0FF4040,
            rec_bg: 0x80181818,
            rec_text: 0xD0FFFFFF,
        }
    }
}

/// Trait for platform-specific capture border implementations
pub trait CaptureBorder: Send {
    /// Set event proxy for sending resize/move events to the main event loop
    fn set_event_proxy(&mut self, proxy: EventLoopProxy<UserEvent>);
    
    /// Force a redraw of the border
    fn redraw(&self);
    
    /// Get the native window handle (platform-specific)
    fn native_handle(&self) -> isize;
    
    /// Update border style
    fn set_style(&mut self, style: BorderStyle);
    
    /// Update border colors
    fn set_colors(&mut self, colors: BorderColors);
}
