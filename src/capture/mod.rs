// capture/mod.rs - Screen Capture Module
//
// This module provides platform-specific screen capture implementations.
// Each platform has its own submodule with the actual capture logic.

#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "windows")]
pub use windows::WindowsCaptureEngine;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub use macos::MacOSCaptureEngine;

#[cfg(target_os = "linux")]
pub mod linux;

/// Screen region to capture
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl CaptureRect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Capture settings
#[derive(Debug, Clone)]
pub struct CaptureSettings {
    pub show_cursor: bool,
    pub show_border: bool,
    pub border_width: u32,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            show_cursor: true,
            show_border: true,
            border_width: 3,
        }
    }
}

/// A captured frame containing pixel data
#[derive(Debug)]
pub struct CaptureFrame {
    /// BGRA pixel data (CPU fallback)
    pub data: Vec<u8>,
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// Bytes per row (may include padding)
    pub stride: u32,
    /// X offset in screen coordinates where this frame starts
    /// (used when frame is clipped to monitor bounds)
    pub offset_x: i32,
    /// Y offset in screen coordinates where this frame starts
    pub offset_y: i32,
    /// GPU texture handle (platform-specific, optional)
    /// - macOS: Metal IOSurface ID
    /// - Windows: D3D11 texture handle
    /// - Linux: DMA-BUF file descriptor
    pub gpu_texture: Option<GpuTextureHandle>,
}

/// Platform-specific GPU texture handle
#[derive(Debug, Clone)]
pub enum GpuTextureHandle {
    #[cfg(target_os = "macos")]
    Metal {
        iosurface_ptr: *mut std::ffi::c_void, // Retained IOSurface pointer (no lookup needed)
        iosurface_id: u32,
        pixel_format: u32, // MTLPixelFormat
        crop_x: i64,       // Crop region X in pixels
        crop_y: i64,       // Crop region Y in pixels
        crop_w: i64,       // Crop width in pixels
        crop_h: i64,       // Crop height in pixels
    },
    #[cfg(target_os = "windows")]
    D3D11 {
        texture_ptr: usize,   // ID3D11Texture2D* (AddRef'd for safety)
        shared_handle: usize, // HANDLE (for cross-process, 0 for same-process)
        crop_x: i32,          // Crop region X in pixels (relative to texture)
        crop_y: i32,          // Crop region Y in pixels (relative to texture)
        crop_width: u32,      // Crop width in pixels
        crop_height: u32,     // Crop height in pixels
    },
    #[cfg(target_os = "linux")]
    DmaBuf {
        fd: i32,
        width: u32,
        height: u32,
        stride: u32,
        format: u32, // DRM fourcc
    },
}

/// Trait for platform-specific capture engines
pub trait CaptureEngine: Send {
    /// Start capturing the specified region
    ///
    /// # Parameters
    /// - `region`: Screen region to capture
    /// - `show_cursor`: Include cursor in capture
    /// - `excluded_windows`: Windows to exclude from capture (platform-specific)
    fn start(
        &mut self,
        region: CaptureRect,
        show_cursor: bool,
        excluded_windows: Option<Vec<crate::window_filter::WindowIdentifier>>,
    ) -> anyhow::Result<()>;

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

    /// Update the screen scale factor (DPI)
    /// Used to ensure correct pixel-to-point mapping on Retina/High-DPI displays
    fn set_scale_factor(&mut self, _scale: f64) -> anyhow::Result<()> {
        Ok(()) // Default implementation does nothing
    }

    /// Downcast to Any for platform-specific access
    fn as_any(&self) -> &dyn std::any::Any;
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
