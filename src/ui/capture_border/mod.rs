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
}

impl Default for BorderStyle {
    fn default() -> Self {
        Self {
            border_width: 5,
            corner_size: 30,
            corner_thickness: 8,
            show_rec_indicator: true,
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
