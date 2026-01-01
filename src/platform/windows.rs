// platform/windows.rs - Windows-specific Platform Implementation
//
// This module contains all Windows-specific code using Win32 API.

use crate::app::CaptureRect;
use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
        CreateRectRgn, CombineRgn, RGN_DIFF, SetWindowRgn, DeleteObject,
    },
    UI::WindowsAndMessaging::*,
};

/// Resize direction for border window
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResizeDirection {
    TopLeft,
    Top,
    TopRight,
    Left,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
    Move,
    None,
}

/// Set cursor based on resize direction
pub fn set_cursor_for_direction(direction: ResizeDirection) {
    unsafe {
        let cursor = match direction {
            ResizeDirection::TopLeft | ResizeDirection::BottomRight => LoadCursorW(None, IDC_SIZENWSE),
            ResizeDirection::TopRight | ResizeDirection::BottomLeft => LoadCursorW(None, IDC_SIZENESW),
            ResizeDirection::Top | ResizeDirection::Bottom => LoadCursorW(None, IDC_SIZENS),
            ResizeDirection::Left | ResizeDirection::Right => LoadCursorW(None, IDC_SIZEWE),
            ResizeDirection::Move => LoadCursorW(None, IDC_SIZEALL),
            ResizeDirection::None => LoadCursorW(None, IDC_ARROW),
        };
        if let Ok(cursor) = cursor {
            SetCursor(Some(cursor));
        }
    }
}

/// Move and resize window atomically using SetWindowPos (no jitter)
pub fn set_window_pos_size(hwnd: isize, x: i32, y: i32, width: u32, height: u32) {
    unsafe {
        let _ = SetWindowPos(
            HWND(hwnd as *mut _),
            Some(HWND_TOP),
            x,
            y,
            width as i32,
            height as i32,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// Get the primary monitor's work area
pub fn get_primary_monitor_rect() -> CaptureRect {
    unsafe {
        let hwnd = GetDesktopWindow();
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            let rc = info.rcWork;
            CaptureRect {
                x: rc.left,
                y: rc.top,
                width: (rc.right - rc.left) as u32,
                height: (rc.bottom - rc.top) as u32,
            }
        } else {
            // Fallback to screen dimensions
            CaptureRect {
                x: 0,
                y: 0,
                width: GetSystemMetrics(SM_CXSCREEN) as u32,
                height: GetSystemMetrics(SM_CYSCREEN) as u32,
            }
        }
    }
}

/// Set a window to be excluded from screen capture (Windows 10 2004+)
pub fn set_window_capture_exclusion(hwnd: isize, exclude: bool) {
    unsafe {
        let affinity = if exclude {
            WDA_EXCLUDEFROMCAPTURE
        } else {
            WDA_NONE
        };
        let result = SetWindowDisplayAffinity(HWND(hwnd as *mut _), affinity);
        if result.is_ok() {
            log::info!("SetWindowDisplayAffinity succeeded: hwnd={}, exclude={}", hwnd, exclude);
        } else {
            log::error!("SetWindowDisplayAffinity failed: hwnd={}, exclude={}, error={:?}", hwnd, exclude, result);
        }
    }
}

/// Set window as topmost or not
pub fn set_window_topmost(hwnd: isize, topmost: bool) {
    unsafe {
        let insert_after = if topmost { Some(HWND_TOPMOST) } else { Some(HWND_NOTOPMOST) };
        SetWindowPos(
            HWND(hwnd as *mut _),
            insert_after,
            0, 0, 0, 0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        ).ok();
    }
}

/// Set window position
pub fn set_window_position(hwnd: isize, x: i32, y: i32) {
    unsafe {
        SetWindowPos(
            HWND(hwnd as *mut _),
            None,
            x, y, 0, 0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        ).ok();
    }
}

/// Get window position
pub fn get_window_position(hwnd: isize) -> (i32, i32) {
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(HWND(hwnd as *mut _), &mut rect).is_ok() {
            (rect.left, rect.top)
        } else {
            (0, 0)
        }
    }
}

/// Set window size
pub fn set_window_size(hwnd: isize, width: u32, height: u32) {
    unsafe {
        SetWindowPos(
            HWND(hwnd as *mut _),
            None,
            0, 0, width as i32, height as i32,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        ).ok();
    }
}

/// Get window size
pub fn get_window_size(hwnd: isize) -> (u32, u32) {
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(HWND(hwnd as *mut _), &mut rect).is_ok() {
            ((rect.right - rect.left) as u32, (rect.bottom - rect.top) as u32)
        } else {
            (0, 0)
        }
    }
}

/// Show a window
pub fn show_window(hwnd: isize) {
    unsafe {
        ShowWindow(HWND(hwnd as *mut _), SW_SHOW);
    }
}

/// Hide a window
pub fn hide_window(hwnd: isize) {
    unsafe {
        ShowWindow(HWND(hwnd as *mut _), SW_HIDE);
    }
}

/// Get HWND from winit window
pub fn get_hwnd_from_window(window: &winit::window::Window) -> Option<isize> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as isize),
        _ => None,
    }
}

/// Make a window click-through (transparent to mouse events)
pub fn set_window_click_through(hwnd: isize, click_through: bool) {
    unsafe {
        let style = GetWindowLongW(HWND(hwnd as *mut _), GWL_EXSTYLE);
        // Always keep layered style for color-key transparency
        let mut new_style = style | WS_EX_LAYERED.0 as i32;
        if click_through {
            new_style |= WS_EX_TRANSPARENT.0 as i32;
        } else {
            new_style &= !(WS_EX_TRANSPARENT.0 as i32);
        }
        SetWindowLongW(HWND(hwnd as *mut _), GWL_EXSTYLE, new_style);
    }
}

/// Setup window region with a hole in the center
/// This creates a true transparent click-through center
pub fn setup_border_window_region(hwnd: isize, width: i32, height: i32, border_width: i32) {
    unsafe {
        // Create outer region (full window)
        let outer_region = CreateRectRgn(0, 0, width, height);
        
        // Create inner region (the hole - area inside the border)
        let inner_region = CreateRectRgn(
            border_width,
            border_width,
            width - border_width,
            height - border_width,
        );
        
        // Subtract inner from outer to create a hollow frame
        let _ = CombineRgn(Some(outer_region), Some(outer_region), Some(inner_region), RGN_DIFF);
        
        // Apply the region to the window
        // Note: SetWindowRgn takes ownership of the region, don't delete it
        let _ = SetWindowRgn(HWND(hwnd as *mut _), Some(outer_region), true);
        
        // Delete the inner region (outer is now owned by window)
        let _ = DeleteObject(inner_region.into());
    }
}

/// Update the window region when resized
pub fn update_border_window_region(hwnd: isize, width: i32, height: i32, border_width: i32) {
    setup_border_window_region(hwnd, width, height, border_width);
}

/// Set window transparency (0 = fully transparent, 255 = opaque)
pub fn set_window_alpha(hwnd: isize, alpha: u8) {
    unsafe {
        // Ensure layered style is set
        let style = GetWindowLongW(HWND(hwnd as *mut _), GWL_EXSTYLE);
        if style & WS_EX_LAYERED.0 as i32 == 0 {
            SetWindowLongW(HWND(hwnd as *mut _), GWL_EXSTYLE, style | WS_EX_LAYERED.0 as i32);
        }
        
        SetLayeredWindowAttributes(
            HWND(hwnd as *mut _),
            windows::Win32::Foundation::COLORREF(0),
            alpha,
            LWA_ALPHA,
        ).ok();
    }
}
