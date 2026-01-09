//! Platform-specific Destination Window Implementation
//!
//! This module provides a destination window that displays captured frames.
//! The implementation is platform-specific.

#[cfg(target_os = "windows")]
mod d3d11_renderer; // DirectX 11 GPU renderer

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;
