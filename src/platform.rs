//! Platform Abstraction Layer
//!
//! This module provides cross-platform abstractions for platform-specific functionality.
//! Each platform has its own implementation behind cfg attributes.

/// Platform-specific window utilities
pub mod window {
    /// Show or hide a window
    #[cfg(windows)]
    #[allow(dead_code)]
    pub fn set_window_visible(hwnd_value: isize, visible: bool) -> anyhow::Result<()> {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE, SW_SHOW};

        unsafe {
            let hwnd = HWND(hwnd_value as *mut std::ffi::c_void);
            let cmd = if visible { SW_SHOW } else { SW_HIDE };
            let _ = ShowWindow(hwnd, cmd);
        }
        Ok(())
    }

    #[cfg(not(windows))]
    pub fn set_window_visible(_hwnd_value: isize, _visible: bool) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Platform-specific input utilities
pub mod input {
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    use std::sync::Mutex;

    lazy_static::lazy_static! {
        static ref CLICK_POSITIONS: Mutex<Vec<ClickEvent>> = Mutex::new(Vec::new());
    }

    static MOUSE_HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);
    static MOUSE_HOOK_THREAD_ID: AtomicIsize = AtomicIsize::new(0);

    #[derive(Debug, Clone)]
    pub struct ClickEvent {
        pub x: i32,
        pub y: i32,
        pub button: MouseButton,
        pub timestamp: std::time::Instant,
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum MouseButton {
        Left,
        Right,
        Middle,
    }

    /// Stop the mouse hook and its message loop
    #[cfg(windows)]
    pub fn stop_click_capture() {
        if !MOUSE_HOOK_INSTALLED.load(Ordering::SeqCst) {
            return;
        }

        let thread_id = MOUSE_HOOK_THREAD_ID.load(Ordering::SeqCst);
        if thread_id != 0 {
            use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};

            unsafe {
                // Post WM_QUIT to the hook thread to exit its message loop
                let _ = PostThreadMessageW(
                    thread_id as u32,
                    WM_QUIT,
                    windows::Win32::Foundation::WPARAM(0),
                    windows::Win32::Foundation::LPARAM(0),
                );
            }

            log::info!("Posted WM_QUIT to mouse hook thread");
        }

        MOUSE_HOOK_INSTALLED.store(false, Ordering::SeqCst);
        MOUSE_HOOK_THREAD_ID.store(0, Ordering::SeqCst);
    }

    #[cfg(not(windows))]
    pub fn stop_click_capture() {
        MOUSE_HOOK_INSTALLED.store(false, Ordering::SeqCst);
    }

    /// Start capturing mouse clicks
    #[cfg(windows)]
    pub fn start_click_capture() -> anyhow::Result<()> {
        use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage, MSG, WH_MOUSE_LL,
        };

        if MOUSE_HOOK_INSTALLED.swap(true, Ordering::SeqCst) {
            return Ok(()); // Already installed
        }

        // Spawn a dedicated thread for the mouse hook with its own message loop
        std::thread::spawn(|| {
            // Store thread ID for cleanup
            let thread_id = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
            MOUSE_HOOK_THREAD_ID.store(thread_id as isize, Ordering::SeqCst);

            unsafe extern "system" fn mouse_hook_proc(
                code: i32,
                wparam: WPARAM,
                lparam: LPARAM,
            ) -> LRESULT {
                use windows::Win32::UI::WindowsAndMessaging::{
                    CallNextHookEx, MSLLHOOKSTRUCT, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN,
                };

                if code >= 0 {
                    let msg = wparam.0 as u32;
                    let button = match msg {
                        x if x == WM_LBUTTONDOWN => Some(MouseButton::Left),
                        x if x == WM_RBUTTONDOWN => Some(MouseButton::Right),
                        x if x == WM_MBUTTONDOWN => Some(MouseButton::Middle),
                        _ => None,
                    };

                    if let Some(button) = button {
                        let hook_struct = &*(lparam.0 as *const MSLLHOOKSTRUCT);
                        let event = ClickEvent {
                            x: hook_struct.pt.x,
                            y: hook_struct.pt.y,
                            button,
                            timestamp: std::time::Instant::now(),
                        };

                        if let Ok(mut clicks) = CLICK_POSITIONS.lock() {
                            clicks.push(event);
                            // Keep only last 100 clicks
                            if clicks.len() > 100 {
                                clicks.remove(0);
                            }
                        }
                    }
                }

                unsafe { CallNextHookEx(None, code, wparam, lparam) }
            }

            unsafe {
                let hook_result = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), None, 0);

                match hook_result {
                    Ok(hook) => {
                        log::info!("Mouse hook installed successfully, starting message loop");

                        // Run message loop - required for low-level hooks to work
                        let mut msg: MSG = std::mem::zeroed();
                        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                            let _ = TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }

                        // Clean up hook when message loop exits
                        use windows::Win32::UI::WindowsAndMessaging::UnhookWindowsHookEx;
                        let _ = UnhookWindowsHookEx(hook);
                        log::info!("Mouse hook uninstalled");
                    }
                    Err(e) => {
                        log::error!("Failed to install mouse hook: {}", e);
                        MOUSE_HOOK_INSTALLED.store(false, Ordering::SeqCst);
                    }
                }
            }

            // Reset flags when thread exits
            MOUSE_HOOK_INSTALLED.store(false, Ordering::SeqCst);
            MOUSE_HOOK_THREAD_ID.store(0, Ordering::SeqCst);
        });

        Ok(())
    }

    #[cfg(not(windows))]
    pub fn start_click_capture() -> anyhow::Result<()> {
        // TODO: Implement for other platforms
        Ok(())
    }

    /// Get recent click events within the specified region and time window
    pub fn get_recent_clicks(
        region_x: i32,
        region_y: i32,
        region_width: u32,
        region_height: u32,
        max_age_ms: u64,
    ) -> Vec<ClickEvent> {
        let cutoff = std::time::Instant::now() - std::time::Duration::from_millis(max_age_ms);

        if let Ok(clicks) = CLICK_POSITIONS.lock() {
            clicks
                .iter()
                .filter(|c| {
                    c.timestamp >= cutoff
                        && c.x >= region_x
                        && c.x < region_x + region_width as i32
                        && c.y >= region_y
                        && c.y < region_y + region_height as i32
                })
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Clear all stored click events
    pub fn clear_clicks() {
        if let Ok(mut clicks) = CLICK_POSITIONS.lock() {
            clicks.clear();
        }
    }
}
