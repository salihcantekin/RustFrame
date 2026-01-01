// platform/mod.rs - Platform Abstraction Layer
//
// This module provides platform-specific implementations behind common traits.
// The goal is to isolate Windows/macOS/Linux specific code here.

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "linux")]
pub mod linux;

// Re-export the current platform's implementations
#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "linux")]
pub use linux::*;

use crate::app::CaptureRect;

/// Trait for platform-specific window operations
pub trait PlatformWindow {
    /// Get the native window handle
    fn native_handle(&self) -> RawHandle;
    
    /// Set window position
    fn set_position(&self, x: i32, y: i32);
    
    /// Get window position
    fn get_position(&self) -> (i32, i32);
    
    /// Set window size
    fn set_size(&self, width: u32, height: u32);
    
    /// Get window size
    fn get_size(&self) -> (u32, u32);
    
    /// Show the window
    fn show(&self);
    
    /// Hide the window
    fn hide(&self);
    
    /// Set window as topmost
    fn set_topmost(&self, topmost: bool);
    
    /// Exclude window from screen capture
    fn set_exclude_from_capture(&self, exclude: bool);
}

/// Trait for platform-specific screen capture
pub trait PlatformCapture {
    /// Start capturing a region
    fn start(&mut self, region: CaptureRect) -> anyhow::Result<()>;
    
    /// Stop capturing
    fn stop(&mut self);
    
    /// Get the latest captured frame data (BGRA format)
    fn get_frame(&mut self) -> Option<CaptureFrame>;
    
    /// Update cursor visibility setting
    fn set_cursor_visible(&mut self, visible: bool);
    
    /// Check if a new frame is available
    fn has_new_frame(&self) -> bool;
}

/// A captured frame with pixel data
#[derive(Debug)]
pub struct CaptureFrame {
    /// BGRA pixel data
    pub data: Vec<u8>,
    /// Frame width
    pub width: u32,
    /// Frame height  
    pub height: u32,
    /// Bytes per row (may include padding)
    pub stride: u32,
}

/// Platform-agnostic raw window handle
#[derive(Debug, Clone, Copy)]
pub enum RawHandle {
    #[cfg(target_os = "windows")]
    Windows(isize), // HWND
    #[cfg(target_os = "macos")]
    MacOS(*mut std::ffi::c_void), // NSWindow
    #[cfg(target_os = "linux")]
    Linux(u64), // X11 Window or Wayland surface
}

/// Get the primary monitor's work area (excluding taskbar)
pub fn get_primary_monitor_rect() -> CaptureRect {
    #[cfg(target_os = "windows")]
    {
        windows::get_primary_monitor_rect()
    }
    #[cfg(target_os = "macos")]
    {
        macos::get_primary_monitor_rect()
    }
    #[cfg(target_os = "linux")]
    {
        linux::get_primary_monitor_rect()
    }
}
