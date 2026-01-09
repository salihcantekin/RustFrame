//! RustFrame - Screen Capture Library
//!
//! This library provides the core functionality for screen capture and rendering.

// Configuration constants
pub mod config;

// Platform-agnostic utilities
pub mod platform_utils;

// Only include modules that don't depend on egui
pub mod capture;
pub mod display_info;

// Re-export commonly used types
pub use capture::{CaptureEngine, CaptureFrame, CaptureRect, CaptureSettings};

#[cfg(target_os = "windows")]
pub use capture::windows::WindowsCaptureEngine;

#[cfg(target_os = "macos")]
pub use capture::macos::MacOSCaptureEngine;

#[cfg(target_os = "linux")]
pub use capture::linux::LinuxCaptureEngine;
