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
    use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
    use std::sync::Mutex;

    lazy_static::lazy_static! {
        static ref CLICK_POSITIONS: Mutex<Vec<ClickEvent>> = Mutex::new(Vec::new());
        static ref SCREEN_SCALE_FACTOR: AtomicU64 = AtomicU64::new(0); // Store as u64 (f64 bits)
        static ref SCREEN_HEIGHT: AtomicU64 = AtomicU64::new(0); // Store as u64 (f64 bits)
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
        #[cfg(target_os = "macos")]
        {
            use cocoa::base::{id, nil};
            use cocoa::foundation::{NSPoint, NSRect};
            use core_foundation::base::TCFType;
            use core_foundation::runloop::{CFRunLoop, kCFRunLoopDefaultMode};
            use std::sync::atomic::Ordering;
            use objc::*;
            
            log::info!("[MACOS_CLICK] start_click_capture() called");
            
            if MOUSE_HOOK_INSTALLED.swap(true, Ordering::SeqCst) {
                log::warn!("[MACOS_CLICK] Already monitoring, skipping");
                return Ok(()); // Already monitoring
            }

            // Get screen info once from main thread before spawning background thread
            unsafe {
                let screen: id = msg_send![class!(NSScreen), mainScreen];
                if screen != nil {
                    let frame: NSRect = msg_send![screen, frame];
                    let scale_factor: f64 = msg_send![screen, backingScaleFactor];
                    
                    // Store as bits in atomic
                    SCREEN_HEIGHT.store(frame.size.height.to_bits(), Ordering::Relaxed);
                    SCREEN_SCALE_FACTOR.store(scale_factor.to_bits(), Ordering::Relaxed);
                    
                    log::info!("[MACOS_CLICK] Screen info cached: height={:.1}, scale={:.1}x", 
                        frame.size.height, scale_factor);
                } else {
                    log::error!("[MACOS_CLICK] Failed to get main screen!");
                    return Err(anyhow::anyhow!("Could not get main screen"));
                }
            }

            log::info!("[MACOS_CLICK] Spawning click capture thread...");
            
            // Since macOS doesn't allow global event monitoring easily from Rust,
            // we'll poll using CGEventSourceButtonState which doesn't require accessibility permissions
            std::thread::spawn(|| {
                log::info!("[MACOS_CLICK] Click capture thread STARTED, entering poll loop");
                
                let mut last_left_state = false;
                let mut last_right_state = false;
                let mut last_other_state = false;
                
                while MOUSE_HOOK_INSTALLED.load(Ordering::SeqCst) {
                    unsafe {
                        // Use CGEventSourceButtonState to check button states
                        // 0 = left, 1 = right, 2 = middle
                        extern "C" {
                            fn CGEventSourceButtonState(
                                stateID: u32,
                                button: u32,
                            ) -> bool;
                            
                            fn CGEventGetLocation(event: *const std::ffi::c_void) -> core_graphics::geometry::CGPoint;
                        }
                        
                        // kCGEventSourceStateHIDSystemState = 1
                        let left_down = CGEventSourceButtonState(1, 0);  // Left button
                        let right_down = CGEventSourceButtonState(1, 1); // Right button  
                        let other_down = CGEventSourceButtonState(1, 2); // Middle button
                        
                        // Get current mouse location using Cocoa
                        let mouse_loc: NSPoint = msg_send![class!(NSEvent), mouseLocation];
                        
                        // Use cached screen info (avoid calling NSScreen from background thread)
                        let screen_height = f64::from_bits(SCREEN_HEIGHT.load(Ordering::Relaxed));
                        let scale_factor = f64::from_bits(SCREEN_SCALE_FACTOR.load(Ordering::Relaxed));
                        
                        if screen_height > 0.0 && scale_factor > 0.0 {
                            // NSEvent mouseLocation is in screen coordinates with bottom-left origin (in POINTS)
                            // We need top-left origin in PIXELS for screen capture coordinates
                            // 1. Flip Y coordinate: bottom-left -> top-left (still in points)
                            let flipped_y_points = screen_height - mouse_loc.y;
                            
                            // 2. Scale to pixels (multiply by backingScaleFactor)
                            let x_pixels = (mouse_loc.x * scale_factor) as i32;
                            let y_pixels = (flipped_y_points * scale_factor) as i32;
                            
                            // Detect button down transitions
                            if left_down && !last_left_state {
                                log::debug!("[MACOS_CLICK] Raw: ({:.1}pt, {:.1}pt), Scale: {:.1}x, Pixels: ({}, {})", 
                                    mouse_loc.x, mouse_loc.y, scale_factor, x_pixels, y_pixels);
                                log_click(x_pixels, y_pixels, MouseButton::Left);
                            }
                            if right_down && !last_right_state {
                                log::debug!("[MACOS_CLICK] Raw: ({:.1}pt, {:.1}pt), Scale: {:.1}x, Pixels: ({}, {})", 
                                    mouse_loc.x, mouse_loc.y, scale_factor, x_pixels, y_pixels);
                                log_click(x_pixels, y_pixels, MouseButton::Right);
                            }
                            if other_down && !last_other_state {
                                log::debug!("[MACOS_CLICK] Raw: ({:.1}pt, {:.1}pt), Scale: {:.1}x, Pixels: ({}, {})", 
                                    mouse_loc.x, mouse_loc.y, scale_factor, x_pixels, y_pixels);
                                log_click(x_pixels, y_pixels, MouseButton::Middle);
                            }
                        }
                        
                        last_left_state = left_down;
                        last_right_state = right_down;
                        last_other_state = other_down;
                    }
                    
                    std::thread::sleep(std::time::Duration::from_millis(10)); // Poll at 100Hz
                }
                
                log::info!("[MACOS_CLICK] Mouse event polling stopped");
            });
            
            Ok(())
        }
        
        #[cfg(not(target_os = "macos"))]
        {
            // Stub for Linux
            log::warn!("Click capture not implemented for this platform");
            Ok(())
        }
    }
    
    #[cfg(target_os = "macos")]
    fn log_click(x: i32, y: i32, button: MouseButton) {
        let click = ClickEvent {
            x,
            y,
            button,
            timestamp: std::time::Instant::now(),
        };
        
        log::debug!("[MACOS_CLICK] Click detected: ({}, {}) button {:?}", x, y, button);
        
        if let Ok(mut clicks) = CLICK_POSITIONS.lock() {
            clicks.push(click);
            println!("[CLICK_STORED] Position ({}, {}), total stored: {}", x, y, clicks.len());
            log::info!("[CLICK_STORED] Position ({}, {}), total stored: {}", x, y, clicks.len());
            // Keep only recent clicks (last 5 seconds)
            let cutoff = std::time::Instant::now() - std::time::Duration::from_secs(5);
            clicks.retain(|c| c.timestamp >= cutoff);
        }
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

    /// Get the screen scale factor (e.g., 2.0 for Retina displays)
    /// Returns 1.0 if not yet initialized
    pub fn get_screen_scale_factor() -> f64 {
        // Use centralized display info if available
        if crate::display_info::is_initialized() {
            return crate::display_info::scale_factor();
        }
        
        // Fallback to atomic cache for backward compatibility
        use std::sync::atomic::Ordering;
        let bits = SCREEN_SCALE_FACTOR.load(Ordering::Relaxed);
        if bits == 0 {
            1.0 // Default if not initialized
        } else {
            f64::from_bits(bits)
        }
    }
}
