// platform/windows.rs - Windows-specific Platform Implementation
//
// This module contains all Windows-specific code using Win32 API.

use crate::app::CaptureRect;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use windows::Win32::{
    Foundation::{HWND, RECT, LPARAM, WPARAM, LRESULT},
    Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
        CreateRectRgn, CombineRgn, RGN_DIFF, SetWindowRgn, DeleteObject,
        EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS,
    },
    Graphics::Dwm::{DwmExtendFrameIntoClientArea, DwmEnableBlurBehindWindow, DWM_BLURBEHIND, DWM_BB_ENABLE},
    UI::WindowsAndMessaging::*,
};
use windows::Win32::UI::Controls::MARGINS;

// Global mouse hook state
static MOUSE_HOOK_ENABLED: AtomicBool = AtomicBool::new(false);

/// Wrapper for HHOOK to make it Send + Sync
struct HookHandle(HHOOK);
unsafe impl Send for HookHandle {}
unsafe impl Sync for HookHandle {}

lazy_static::lazy_static! {
    static ref MOUSE_CLICKS: Arc<Mutex<Vec<MouseClick>>> = Arc::new(Mutex::new(Vec::new()));
    static ref MOUSE_HOOK_HANDLE: Arc<Mutex<Option<HookHandle>>> = Arc::new(Mutex::new(None));
}

/// Represents a mouse click event
#[derive(Debug, Clone, Copy)]
pub struct MouseClick {
    pub x: i32,
    pub y: i32,
    pub timestamp: std::time::Instant,
    pub is_left: bool,
}

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

/// Start native window dragging using Win32 API - this is the smoothest way
pub fn start_window_drag(hwnd: isize) {
    unsafe {
        // Use PostMessage for non-blocking drag initiation
        // WM_NCLBUTTONDOWN with HTCAPTION simulates title bar drag
        let _ = PostMessageW(
            Some(HWND(hwnd as *mut _)),
            WM_NCLBUTTONDOWN,
            WPARAM(HTCAPTION as usize),
            LPARAM(0),
        );
    }
}

/// Get the primary monitor's refresh rate in Hz
pub fn get_monitor_refresh_rate() -> u32 {
    unsafe {
        let mut dev_mode: DEVMODEW = std::mem::zeroed();
        dev_mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
        
        if EnumDisplaySettingsW(None, ENUM_CURRENT_SETTINGS, &mut dev_mode).as_bool() {
            let refresh = dev_mode.dmDisplayFrequency;
            if refresh > 0 && refresh <= 500 {
                return refresh;
            }
        }
        
        // Fallback to 60Hz if we can't detect
        60
    }
}

/// Low-level mouse hook callback
unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && MOUSE_HOOK_ENABLED.load(Ordering::Relaxed) {
        let mouse_struct = lparam.0 as *const MSLLHOOKSTRUCT;
        if !mouse_struct.is_null() {
            let is_click = match wparam.0 as u32 {
                WM_LBUTTONDOWN | WM_RBUTTONDOWN => true,
                _ => false,
            };
            
            if is_click {
                let is_left = wparam.0 as u32 == WM_LBUTTONDOWN;
                let pt = (*mouse_struct).pt;
                
                if let Ok(mut clicks) = MOUSE_CLICKS.lock() {
                    clicks.push(MouseClick {
                        x: pt.x,
                        y: pt.y,
                        timestamp: std::time::Instant::now(),
                        is_left,
                    });
                    
                    // Keep only recent clicks (last 2 seconds worth)
                    let now = std::time::Instant::now();
                    clicks.retain(|c| now.duration_since(c.timestamp).as_secs_f32() < 2.0);
                }
            }
        }
    }
    
    CallNextHookEx(None, code, wparam, lparam)
}

/// Install global mouse hook for click detection
pub fn install_mouse_hook() -> bool {
    unsafe {
        if let Ok(mut handle) = MOUSE_HOOK_HANDLE.lock() {
            if handle.is_some() {
                return true; // Already installed
            }
            
            let hook = SetWindowsHookExW(
                WH_MOUSE_LL,
                Some(mouse_hook_proc),
                None,
                0,
            );
            
            match hook {
                Ok(h) => {
                    *handle = Some(HookHandle(h));
                    MOUSE_HOOK_ENABLED.store(true, Ordering::Relaxed);
                    log::info!("Mouse hook installed successfully");
                    true
                }
                Err(e) => {
                    log::error!("Failed to install mouse hook: {:?}", e);
                    false
                }
            }
        } else {
            false
        }
    }
}

/// Uninstall global mouse hook
pub fn uninstall_mouse_hook() {
    unsafe {
        MOUSE_HOOK_ENABLED.store(false, Ordering::Relaxed);
        
        if let Ok(mut handle) = MOUSE_HOOK_HANDLE.lock() {
            if let Some(hook) = handle.take() {
                let _ = UnhookWindowsHookEx(hook.0);
                log::info!("Mouse hook uninstalled");
            }
        }
        
        // Clear click buffer
        if let Ok(mut clicks) = MOUSE_CLICKS.lock() {
            clicks.clear();
        }
    }
}

/// Get recent mouse clicks and clear the buffer
pub fn get_mouse_clicks() -> Vec<MouseClick> {
    if let Ok(clicks) = MOUSE_CLICKS.lock() {
        let result = clicks.clone();
        result
    } else {
        Vec::new()
    }
}

/// Check if mouse hook is active
pub fn is_mouse_hook_active() -> bool {
    MOUSE_HOOK_ENABLED.load(Ordering::Relaxed)
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
        let result = SetWindowPos(
            HWND(hwnd as *mut _),
            None,
            0, 0, width as i32, height as i32,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        if result.is_err() {
            log::error!("SetWindowPos failed: {:?}", result);
        } else {
            log::info!("SetWindowPos succeeded for {}x{}", width, height);
        }
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

/// Enable DWM composition transparency for a window
/// This allows true per-pixel alpha blending with wgpu
pub fn enable_window_transparency(hwnd: isize) {
    unsafe {
        use windows::Win32::Graphics::Gdi::HRGN;
        
        let hwnd = HWND(hwnd as *mut _);
        
        // Extend frame into entire client area to enable DWM composition
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        
        // Enable blur behind (optional, helps with transparency)
        let blur = DWM_BLURBEHIND {
            dwFlags: DWM_BB_ENABLE,
            fEnable: true.into(),
            hRgnBlur: HRGN::default(),
            fTransitionOnMaximized: false.into(),
        };
        let _ = DwmEnableBlurBehindWindow(hwnd, &blur);
        
        log::info!("Enabled DWM transparency for window {:?}", hwnd);
    }
}

/// Set window icon from embedded resource
pub fn set_window_icon(hwnd: isize) {
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{
            LoadImageW, SendMessageW, WM_SETICON, ICON_BIG, ICON_SMALL,
            IMAGE_ICON, LR_DEFAULTSIZE,
        };
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        
        let hwnd_win = HWND(hwnd as *mut _);
        
        // Get current module handle
        let hinstance = GetModuleHandleW(None).unwrap_or_default();
        
        // Try to load the icon with resource ID 1 (standard for main icon)
        if let Ok(icon) = LoadImageW(
            Some(hinstance.into()),
            windows::core::PCWSTR(1 as *const u16), // Resource ID 1
            IMAGE_ICON,
            0, 0,
            LR_DEFAULTSIZE,
        ) {
            // Set both big and small icons
            let _ = SendMessageW(hwnd_win, WM_SETICON, Some(WPARAM(ICON_BIG as usize)), Some(LPARAM(icon.0 as isize)));
            let _ = SendMessageW(hwnd_win, WM_SETICON, Some(WPARAM(ICON_SMALL as usize)), Some(LPARAM(icon.0 as isize)));
            log::info!("Window icon set successfully");
        } else {
            log::warn!("Failed to load window icon from resource");
        }
    }
}
