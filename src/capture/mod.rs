// capture/mod.rs - Screen Capture Module
//
// This module provides platform-specific screen capture implementations.
// Each platform has its own submodule with the actual capture logic.

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "linux")]
pub mod linux;

/// Screen region to capture
#[derive(Debug, Clone, PartialEq)]
pub struct CaptureRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl CaptureRect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }
}

/// Capture settings
#[derive(Debug, Clone)]
pub struct CaptureSettings {
    pub show_cursor: bool,
    pub show_border: bool,
    pub border_width: u32,
    pub exclude_from_capture: bool,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            show_cursor: true,
            show_border: true,
            border_width: 3,
            exclude_from_capture: true,
        }
    }
}

impl CaptureSettings {
    /// Development mode settings
    pub fn for_development() -> Self {
        Self {
            exclude_from_capture: false,
            ..Default::default()
        }
    }
}

/// A captured frame containing pixel data
#[derive(Debug)]
pub struct CaptureFrame {
    /// BGRA pixel data
    pub data: Vec<u8>,
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// Bytes per row (may include padding)
    pub stride: u32,
}

/// Trait for platform-specific capture engines
pub trait CaptureEngine: Send {
    /// Start capturing the specified region
    fn start(&mut self, region: CaptureRect, show_cursor: bool) -> anyhow::Result<()>;
    
    /// Stop the capture session
    fn stop(&mut self);
    
    /// Check if capture is currently active
    fn is_active(&self) -> bool;
    
    /// Check if a new frame is available
    fn has_new_frame(&self) -> bool;
    
    /// Get the latest captured frame
    /// Returns None if no new frame is available
    fn get_frame(&mut self) -> Option<CaptureFrame>;
    
    /// Update cursor visibility setting
    fn set_cursor_visible(&mut self, visible: bool) -> anyhow::Result<()>;
    
    /// Get the current capture region
    fn get_region(&self) -> Option<CaptureRect>;
    
    /// Update the capture region (called when border is resized/moved)
    fn update_region(&mut self, region: CaptureRect) -> anyhow::Result<()>;
}

/// Create a platform-specific capture engine
pub fn create_capture_engine() -> anyhow::Result<Box<dyn CaptureEngine>> {
    #[cfg(target_os = "windows")]
    {
        Ok(Box::new(windows::WindowsCaptureEngine::new()?))
    }
    
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(macos::MacOSCaptureEngine::new()?))
    }
    
    #[cfg(target_os = "linux")]
    {
        Ok(Box::new(linux::LinuxCaptureEngine::new()?))
    }
}
