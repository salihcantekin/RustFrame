//! RustFrame - Screen Capture Library
//!
//! This library provides the core functionality for screen capture and rendering.

// Only include modules that don't depend on egui
pub mod capture;

// Re-export commonly used types
pub use capture::{CaptureEngine, CaptureFrame, CaptureRect, CaptureSettings};

#[cfg(target_os = "windows")]
pub use capture::windows::WindowsCaptureEngine;

#[cfg(target_os = "macos")]
pub use capture::macos::MacOSCaptureEngine;

#[cfg(target_os = "linux")]
pub use capture::linux::LinuxCaptureEngine;
