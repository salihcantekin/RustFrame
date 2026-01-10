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
            use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop};
            use objc::*;
            use std::sync::atomic::Ordering;

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

                    log::info!(
                        "[MACOS_CLICK] Screen info cached: height={:.1}, scale={:.1}x",
                        frame.size.height,
                        scale_factor
                    );
                } else {
                    log::error!("[MACOS_CLICK] Failed to get main screen!");
                    return Err(anyhow::anyhow!("Could not get main screen"));
                }
            }

            log::info!("[MACOS_CLICK] Setting up CGEventTap for event-driven click capture...");

            // Use CGEventTap for event-driven click capture (no polling, 0Hz when idle)
            std::thread::spawn(|| {
                log::info!("[MACOS_CLICK] CGEventTap thread STARTED");

                unsafe {
                    use std::sync::atomic::Ordering;

                    // Get cached screen parameters
                    let screen_height_bits = SCREEN_HEIGHT.load(Ordering::Relaxed);
                    let scale_factor_bits = SCREEN_SCALE_FACTOR.load(Ordering::Relaxed);

                    // External C functions for CGEventTap
                    extern "C" {
                        fn CGEventTapCreate(
                            tap: u32,
                            place: u32,
                            options: u32,
                            events_of_interest: u64,
                            callback: extern "C" fn(
                                *mut std::ffi::c_void,
                                u32,
                                *mut std::ffi::c_void,
                                *mut std::ffi::c_void,
                            )
                                -> *mut std::ffi::c_void,
                            user_info: *mut std::ffi::c_void,
                        ) -> *mut std::ffi::c_void;

                        fn CFMachPortCreateRunLoopSource(
                            allocator: *mut std::ffi::c_void,
                            port: *mut std::ffi::c_void,
                            order: isize,
                        ) -> *mut std::ffi::c_void;

                        fn CFRunLoopAddSource(
                            rl: *mut std::ffi::c_void,
                            source: *mut std::ffi::c_void,
                            mode: *mut std::ffi::c_void,
                        );

                        fn CFRunLoopGetCurrent() -> *mut std::ffi::c_void;
                        fn CFRunLoopRun();
                        fn CFRunLoopStop(rl: *mut std::ffi::c_void);

                        fn CGEventGetLocation(
                            event: *mut std::ffi::c_void,
                        ) -> core_graphics::geometry::CGPoint;

                        static kCFRunLoopCommonModes: *mut std::ffi::c_void;
                    }

                    // Event tap callback - called on mouse down events
                    extern "C" fn event_callback(
                        _proxy: *mut std::ffi::c_void,
                        event_type: u32,
                        event: *mut std::ffi::c_void,
                        _user_info: *mut std::ffi::c_void,
                    ) -> *mut std::ffi::c_void {
                        unsafe {
                            // Get event location (in points, bottom-left origin)
                            extern "C" {
                                fn CGEventGetLocation(
                                    event: *mut std::ffi::c_void,
                                ) -> core_graphics::geometry::CGPoint;
                            }
                            let location = CGEventGetLocation(event);

                            // Use centralized display_info for coordinate conversion
                            // This ensures consistency across the entire application
                            let display_info = crate::display_info::get();
                            let (x_pixels, y_pixels) =
                                display_info.macos_event_to_screen_pixels(location.x, location.y);

                            // Determine button type from event type
                            let button = match event_type {
                                1 => Some(MouseButton::Left),    // kCGEventLeftMouseDown
                                2 => Some(MouseButton::Right),   // kCGEventRightMouseDown
                                25 => Some(MouseButton::Middle), // kCGEventOtherMouseDown
                                _ => None,
                            };

                            if let Some(btn) = button {
                                log::debug!("[MACOS_CLICK] CGEvent: ({:.1}pt, {:.1}pt) -> ({}, {})px @ {:.1}x, button: {:?}",
                                    location.x, location.y, x_pixels, y_pixels, display_info.scale_factor, btn);
                                log_click(x_pixels, y_pixels, btn);
                            }
                        }

                        // Pass event through (don't block it)
                        event
                    }

                    // Create event mask for mouse down events
                    let event_mask = (
                        1 << 1 |  // kCGEventLeftMouseDown
                        1 << 2 |  // kCGEventRightMouseDown
                        1 << 25
                        // kCGEventOtherMouseDown
                    );

                    // Create event tap
                    let event_tap = CGEventTapCreate(
                        1, // kCGSessionEventTap
                        0, // kCGHeadInsertEventTap
                        0, // kCGEventTapOptionListenOnly
                        event_mask,
                        event_callback,
                        std::ptr::null_mut(),
                    );

                    if event_tap.is_null() {
                        log::error!("[MACOS_CLICK] Failed to create CGEventTap! Check Accessibility permissions.");
                        log::error!("[MACOS_CLICK] Go to: System Settings > Privacy & Security > Accessibility");
                        return;
                    }

                    log::info!("[MACOS_CLICK] CGEventTap created successfully");

                    // Add to run loop
                    let run_loop_source =
                        CFMachPortCreateRunLoopSource(std::ptr::null_mut(), event_tap, 0);
                    let run_loop = CFRunLoopGetCurrent();
                    CFRunLoopAddSource(run_loop, run_loop_source, kCFRunLoopCommonModes);

                    log::info!("[MACOS_CLICK] Starting run loop (event-driven, 0Hz when idle, ~10% CPU reduction)");

                    // Run loop - blocks until stopped
                    while MOUSE_HOOK_INSTALLED.load(Ordering::SeqCst) {
                        CFRunLoopRun();

                        // Check if we should stop
                        if !MOUSE_HOOK_INSTALLED.load(Ordering::SeqCst) {
                            CFRunLoopStop(run_loop);
                            break;
                        }
                    }

                    log::info!("[MACOS_CLICK] Event tap stopped");
                }
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

    static CLICK_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[cfg(target_os = "macos")]
    fn log_click(x: i32, y: i32, button: MouseButton) {
        let click = ClickEvent {
            x,
            y,
            button,
            timestamp: std::time::Instant::now(),
        };

        log::debug!(
            "[MACOS_CLICK] Click detected: ({}, {}) button {:?}",
            x,
            y,
            button
        );

        if let Ok(mut clicks) = CLICK_POSITIONS.lock() {
            clicks.push(click);
            tracing::debug!(x, y, total = clicks.len(), "Click stored in buffer");
            log::info!(
                "[CLICK_STORED] Position ({}, {}), total stored: {}",
                x,
                y,
                clicks.len()
            );

            // Lazy cleanup: only every 100 clicks instead of per-click (performance optimization)
            let count = CLICK_COUNTER.fetch_add(1, Ordering::Relaxed);
            if count % 100 == 0 {
                let cutoff = std::time::Instant::now() - std::time::Duration::from_secs(5);
                let before = clicks.len();
                clicks.retain(|c| c.timestamp >= cutoff);
                let removed = before - clicks.len();
                if removed > 0 {
                    log::debug!(
                        "[CLICK_CLEANUP] Removed {} old clicks, {} remaining",
                        removed,
                        clicks.len()
                    );
                }
            }
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
