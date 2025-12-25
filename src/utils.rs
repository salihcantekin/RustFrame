// utils.rs - Common Utility Functions
//
// Shared utilities used across multiple modules to avoid code duplication.

/// Convert a Rust string to a null-terminated wide string (UTF-16) for Windows API
#[cfg(windows)]
pub fn wide_string(s: &str) -> Vec<u16> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// Get HWND from a winit window handle
#[cfg(windows)]
#[allow(dead_code)]
pub fn get_hwnd(window: &winit::window::Window) -> Option<windows::Win32::Foundation::HWND> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::HWND;
    
    let handle = window.window_handle().ok()?;
    if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
        Some(HWND(win32_handle.hwnd.get() as isize as *mut std::ffi::c_void))
    } else {
        None
    }
}

/// Get HWND from a winit Arc<Window>
#[cfg(windows)]
#[allow(dead_code)]
pub fn get_hwnd_arc(window: &std::sync::Arc<winit::window::Window>) -> Option<windows::Win32::Foundation::HWND> {
    get_hwnd(window.as_ref())
}
