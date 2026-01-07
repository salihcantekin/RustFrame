//! REC Indicator Overlay Window
//! Shows a "● REC" indicator in the top-right corner of the capture region

use crate::traits::RecordingIndicator;
use lazy_static::lazy_static;
use log::info;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

fn rec_indicator_disabled_by_env() -> bool {
    match std::env::var("RUSTFRAME_DISABLE_REC_INDICATOR") {
        Ok(val) => {
            let val = val.trim().to_ascii_lowercase();
            val == "1" || val == "true" || val == "yes" || val == "on"
        }
        Err(_) => false,
    }
}

#[cfg(target_os = "macos")]
use cocoa::appkit::{NSWindow, NSWindowStyleMask, NSBackingStoreType, NSColor, NSView, NSTextField};
#[cfg(target_os = "macos")]
use cocoa::base::{id, nil, YES, NO};
#[cfg(target_os = "macos")]
use cocoa::foundation::{NSRect, NSPoint, NSSize, NSString};
#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl, class};

#[cfg(target_os = "macos")]
extern "C" {
    static _dispatch_main_q: std::ffi::c_void;
    fn dispatch_sync_f(
        queue: *const std::ffi::c_void,
        context: *mut std::ffi::c_void,
        work: extern "C" fn(*mut std::ffi::c_void),
    );
    fn dispatch_async_f(
        queue: *const std::ffi::c_void,
        context: *mut std::ffi::c_void,
        work: extern "C" fn(*mut std::ffi::c_void),
    );
    fn pthread_main_np() -> i32;
}

#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
#[cfg(windows)]
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, EndPaint, FillRect, SelectObject,
    SetBkMode, SetTextColor, TextOutW, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET,
    DEFAULT_PITCH, FF_SWISS, FW_BOLD, HBRUSH, HGDIOBJ, OUT_DEFAULT_PRECIS, PAINTSTRUCT,
    TRANSPARENT,
};
#[cfg(windows)]
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::SetLayeredWindowAttributes;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::LWA_COLORKEY;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, PostMessageW,
    PostQuitMessage, RegisterClassExW, SetWindowPos, TranslateMessage, CS_HREDRAW, CS_VREDRAW,
    HCURSOR, HICON, HWND_TOPMOST, MSG, SWP_NOACTIVATE, SWP_SHOWWINDOW, WM_CLOSE, WM_DESTROY,
    WM_PAINT, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    WS_POPUP,
};

lazy_static! {
    static ref REC_HWND: Mutex<isize> = Mutex::new(0);
    static ref REC_VISIBLE: AtomicBool = AtomicBool::new(false);
    static ref REC_SIZE: Mutex<String> = Mutex::new("medium".to_string());
    static ref REC_POSITION: Mutex<(i32, i32)> = Mutex::new((0, 0)); // Top-right position
}

static REC_THREAD_RUNNING: AtomicBool = AtomicBool::new(false);
static REC_CLASS_REGISTERED: AtomicBool = AtomicBool::new(false);

pub struct RecIndicator {
    thread_handle: Option<thread::JoinHandle<()>>,
    stop_flag: Arc<AtomicBool>,
}

unsafe impl Send for RecIndicator {}
unsafe impl Sync for RecIndicator {}

impl RecIndicator {
    #[cfg(windows)]
    pub fn new() -> Option<Self> {
        if rec_indicator_disabled_by_env() {
            log::warn!("REC indicator disabled via RUSTFRAME_DISABLE_REC_INDICATOR");
            return None;
        }

        info!("Creating REC indicator window");

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        let thread_handle = thread::spawn(move || {
            run_rec_thread(stop_flag_clone);
        });

        // Wait for window to be created
        for _ in 0..50 {
            thread::sleep(std::time::Duration::from_millis(10));
            if let Ok(hwnd_lock) = REC_HWND.lock() {
                if *hwnd_lock != 0 {
                    break;
                }
            }
        }

        Some(Self {
            thread_handle: Some(thread_handle),
            stop_flag,
        })
    }

    #[cfg(not(windows))]
    pub fn new() -> Option<Self> {
        #[cfg(target_os = "macos")]
        {
            if rec_indicator_disabled_by_env() {
                log::warn!("REC indicator disabled via RUSTFRAME_DISABLE_REC_INDICATOR");
                return None;
            }

            info!("Creating macOS REC indicator window");
            println!("[REC] Creating macOS REC indicator");

            // Clear any stale pointer from a previous session.
            // Otherwise `new()` may think the window is already created and consumers may call
            // show/hide/update_position using a freed NSWindow pointer.
            if let Ok(mut hwnd_lock) = REC_HWND.lock() {
                *hwnd_lock = 0;
            }

            let stop_flag = Arc::new(AtomicBool::new(false));
            let stop_flag_clone = stop_flag.clone();

            let thread_handle = thread::spawn(move || {
                run_rec_thread_macos(stop_flag_clone);
            });

            // Wait for window to be created
            for _ in 0..50 {
                thread::sleep(std::time::Duration::from_millis(10));
                if let Ok(hwnd_lock) = REC_HWND.lock() {
                    if *hwnd_lock != 0 {
                        break;
                    }
                }
            }

            Some(Self {
                thread_handle: Some(thread_handle),
                stop_flag,
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }

    /// Show the REC indicator at the specified position (top-right of capture region)
    pub fn show(&self, x: i32, y: i32, border_width: i32) {
        let (width, height) = get_indicator_dimensions();

        // Position in top-right corner, inside the border
        let pos_x = x - width - border_width - 5; // 5px padding from border
        let pos_y = y + border_width + 5;

        if let Ok(mut pos) = REC_POSITION.lock() {
            *pos = (pos_x, pos_y);
        }

        REC_VISIBLE.store(true, Ordering::SeqCst);

        #[cfg(windows)]
        if let Ok(hwnd_lock) = REC_HWND.lock() {
            let hwnd_val = *hwnd_lock;
            if hwnd_val != 0 {
                unsafe {
                    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_TOPMOST),
                        pos_x,
                        pos_y,
                        width,
                        height,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                }
            }
        }

        #[cfg(target_os = "macos")]
        if let Ok(hwnd_lock) = REC_HWND.lock() {
            let window_ptr = *hwnd_lock;
            if window_ptr != 0 {
                println!("[REC] Showing macOS REC indicator at ({}, {})", pos_x, pos_y);
                
                // Context for main thread execution
                struct ShowContext {
                    window_ptr: isize,
                    pos_x: i32,
                    pos_y: i32,
                    height: i32,
                }
                
                extern "C" fn show_on_main_thread_async(ctx_ptr: *mut std::ffi::c_void) {
                    // Take ownership of the boxed context
                    let ctx = unsafe { Box::from_raw(ctx_ptr as *mut ShowContext) };
                    show_on_main_thread(&*ctx as *const _ as *mut std::ffi::c_void);
                }
                
                extern "C" fn show_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
                    let ctx = unsafe { &*(ctx_ptr as *const ShowContext) };
                    unsafe {
                        let window: id = ctx.window_ptr as *mut objc::runtime::Object;

                        println!("[REC] show_on_main_thread: window_ptr=0x{:x}", ctx.window_ptr);
                        
                        // Get screen height for coordinate conversion (avoid CoreGraphics struct-return FFI)
                        println!("[REC] show_on_main_thread: getting screen...");
                        let screen: id = msg_send![window, screen];
                        let screen: id = if screen != nil {
                            screen
                        } else {
                            msg_send![class!(NSScreen), mainScreen]
                        };

                        if screen == nil {
                            println!("[REC] show_on_main_thread: ERROR screen is nil, aborting show");
                            return;
                        }

                        println!("[REC] show_on_main_thread: getting screen frame...");
                        let screen_frame: NSRect = msg_send![screen, frame];
                        let screen_height = screen_frame.size.height;
                        
                        // Convert top-left to bottom-left coordinates
                        let origin = NSPoint::new(
                            ctx.pos_x as f64,
                            screen_height - (ctx.pos_y as f64) - (ctx.height as f64)
                        );

                        println!("[REC] show_on_main_thread: calling setFrameOrigin");
                        let _: () = msg_send![window, setFrameOrigin: origin];
                        println!("[REC] show_on_main_thread: setFrameOrigin OK, calling orderWindow");
                        
                        // Use orderWindow:relativeTo: instead of orderFront to avoid potential delegate issues
                        // NSWindowAbove = 1 (bring window above all others)
                        let _: () = msg_send![window, orderWindow: 1i64 relativeTo: 0i64];
                        println!("[REC] show_on_main_thread: orderWindow OK");
                    }
                }
                
                let mut context = ShowContext {
                    window_ptr,
                    pos_x,
                    pos_y,
                    height,
                };
                
                let is_main = unsafe { pthread_main_np() } != 0;
                if !is_main {
                    println!("[REC] Not on main thread, dispatching show to main queue (ASYNC to avoid deadlock)");
                    unsafe {
                        // Use dispatch_async to avoid deadlock if caller is on main thread
                        dispatch_async_f(
                            &_dispatch_main_q,
                            Box::into_raw(Box::new(context)) as *mut std::ffi::c_void,
                            show_on_main_thread_async,
                        );
                    }
                } else {
                    println!("[REC] Already on main thread, showing directly");
                    show_on_main_thread(&mut context as *mut _ as *mut std::ffi::c_void);
                }
            }
        }
    }

    /// Hide the REC indicator
    pub fn hide(&self) {
        REC_VISIBLE.store(false, Ordering::SeqCst);

        #[cfg(windows)]
        if let Ok(hwnd_lock) = REC_HWND.lock() {
            let hwnd_val = *hwnd_lock;
            if hwnd_val != 0 {
                unsafe {
                    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
                    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
        }

        #[cfg(target_os = "macos")]
        if let Ok(hwnd_lock) = REC_HWND.lock() {
            let window_ptr = *hwnd_lock;
            if window_ptr != 0 {
                println!("[REC] Hiding macOS REC indicator");
                
                extern "C" fn hide_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
                    let window_ptr = ctx_ptr as isize;
                    unsafe {
                        let window: id = window_ptr as *mut objc::runtime::Object;
                        let _: () = msg_send![window, orderOut: nil];
                    }
                }
                
                let is_main = unsafe { pthread_main_np() } != 0;
                if !is_main {
                    unsafe {
                        dispatch_sync_f(
                            &_dispatch_main_q,
                            window_ptr as *mut std::ffi::c_void,
                            hide_on_main_thread,
                        );
                    }
                } else {
                    hide_on_main_thread(window_ptr as *mut std::ffi::c_void);
                }
            }
        }
    }

    /// Update position when capture region moves
    pub fn update_position(
        &self,
        region_x: i32,
        region_y: i32,
        region_width: i32,
        border_width: i32,
    ) {
        // Always update and show - don't check REC_VISIBLE
        // This ensures REC reappears after hide()

        let (width, height) = get_indicator_dimensions();

        // Position in top-right corner, inside the border
        let pos_x = region_x + region_width - width - border_width - 5;
        let pos_y = region_y + border_width + 5;

        if let Ok(mut pos) = REC_POSITION.lock() {
            *pos = (pos_x, pos_y);
        }

        #[cfg(windows)]
        if let Ok(hwnd_lock) = REC_HWND.lock() {
            let hwnd_val = *hwnd_lock;
            if hwnd_val != 0 {
                unsafe {
                    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_TOPMOST),
                        pos_x,
                        pos_y,
                        width,
                        height,
                        SWP_NOACTIVATE,
                    );
                }
            }
        }
        
        #[cfg(target_os = "macos")]
        if let Ok(hwnd_lock) = REC_HWND.lock() {
            let window_ptr = *hwnd_lock;
            if window_ptr != 0 {
                struct UpdatePosContext {
                    window_ptr: isize,
                    pos_x: i32,
                    pos_y: i32,
                    height: i32,
                }
                
                extern "C" fn update_pos_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
                    let ctx = unsafe { &*(ctx_ptr as *const UpdatePosContext) };
                    unsafe {
                        let window: id = ctx.window_ptr as *mut objc::runtime::Object;
                        
                        // Get screen height for coordinate conversion
                        let screen: id = msg_send![window, screen];
                        let screen_frame: NSRect = msg_send![screen, frame];
                        let screen_height = screen_frame.size.height;
                        
                        // Convert top-left to bottom-left coordinates
                        let origin = NSPoint::new(
                            ctx.pos_x as f64,
                            screen_height - (ctx.pos_y as f64) - (ctx.height as f64)
                        );
                        
                        let _: () = msg_send![window, setFrameOrigin: origin];
                        // Like Windows SetWindowPos with SWP_SHOWWINDOW - always show
                        let _: () = msg_send![window, orderFront: nil];
                    }
                }
                
                // Use stack context for synchronous dispatch (like Windows SetWindowPos)
                let mut context = UpdatePosContext {
                    window_ptr,
                    pos_x,
                    pos_y,
                    height,
                };
                
                unsafe {
                    let is_main = pthread_main_np() != 0;
                    if !is_main {
                        // SYNC dispatch: blocking but ensures immediate update like Windows
                        dispatch_sync_f(
                            &_dispatch_main_q,
                            &mut context as *mut _ as *mut std::ffi::c_void,
                            update_pos_on_main_thread,
                        );
                    } else {
                        // Already on main thread, call directly
                        update_pos_on_main_thread(&mut context as *mut _ as *mut std::ffi::c_void);
                    }
                }
            }
        }
        
        // Mark as visible after updating position
        REC_VISIBLE.store(true, Ordering::SeqCst);
    }

    /// Set the size of the indicator
    pub fn set_size(&self, size: &str) {
        if let Ok(mut s) = REC_SIZE.lock() {
            *s = size.to_string();
        }

        // Redraw if visible
        #[cfg(windows)]
        if REC_VISIBLE.load(Ordering::SeqCst) {
            if let Ok(hwnd_lock) = REC_HWND.lock() {
                let hwnd_val = *hwnd_lock;
                if hwnd_val != 0 {
                    unsafe {
                        use windows::Win32::Graphics::Gdi::InvalidateRect;
                        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
                        let _ = InvalidateRect(Some(hwnd), None, true);
                    }
                }
            }
        }
    }
}

impl Drop for RecIndicator {
    fn drop(&mut self) {
        info!("Destroying REC indicator");
        self.stop_flag.store(true, Ordering::SeqCst);

        #[cfg(windows)]
        if let Ok(hwnd_lock) = REC_HWND.lock() {
            let hwnd_val = *hwnd_lock;
            if hwnd_val != 0 {
                unsafe {
                    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
                    let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
        }

        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        // After the thread exits (and window is closed), clear the cached pointer.
        if let Ok(mut hwnd_lock) = REC_HWND.lock() {
            *hwnd_lock = 0;
        }
    }
}

fn get_indicator_dimensions() -> (i32, i32) {
    let size = REC_SIZE
        .lock()
        .map(|s| s.clone())
        .unwrap_or("medium".to_string());
    match size.as_str() {
        "small" => (50, 18),
        "large" => (90, 30),
        _ => (70, 24), // medium
    }
}

fn get_font_size() -> i32 {
    let size = REC_SIZE
        .lock()
        .map(|s| s.clone())
        .unwrap_or("medium".to_string());
    match size.as_str() {
        "small" => 12,
        "large" => 20,
        _ => 16, // medium
    }
}

#[cfg(windows)]
fn run_rec_thread(stop_flag: Arc<AtomicBool>) {
    unsafe {
        let class_name = wide_string("RustFrameRecIndicator");
        let hinstance = GetModuleHandleW(None).unwrap_or_default();

        if !REC_CLASS_REGISTERED.load(Ordering::SeqCst) {
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(rec_window_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance.into(),
                hIcon: HICON::default(),
                hCursor: HCURSOR::default(),
                hbrBackground: HBRUSH::default(),
                lpszMenuName: PCWSTR::null(),
                lpszClassName: PCWSTR(class_name.as_ptr()),
                hIconSm: HICON::default(),
            };

            if RegisterClassExW(&wc) == 0 {
                println!("[REC] Failed to register window class");
                return;
            }
            REC_CLASS_REGISTERED.store(true, Ordering::SeqCst);
        }

        let (width, height) = get_indicator_dimensions();

        let hwnd = match CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_TRANSPARENT,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(wide_string("REC").as_ptr()),
            WS_POPUP,
            0,
            0,
            width,
            height,
            None,
            None,
            Some(hinstance.into()),
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                println!("[REC] Failed to create window: {}", e);
                return;
            }
        };

        if let Ok(mut hwnd_lock) = REC_HWND.lock() {
            *hwnd_lock = hwnd.0 as isize;
        }

        // Set transparency key (magenta will be transparent)
        let _ = SetLayeredWindowAttributes(
            hwnd,
            COLORREF(0xFF00FF), // Magenta as transparency key
            255,
            LWA_COLORKEY,
        );

        // Exclude from screen capture so it doesn't appear in recordings
        {
            use windows::Win32::UI::WindowsAndMessaging::{
                SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE,
            };
            if let Err(e) = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) {
                println!("[REC] Failed to exclude from capture: {:?}", e);
            } else {
                println!("[REC] REC indicator excluded from screen capture");
            }
        }

        REC_THREAD_RUNNING.store(true, Ordering::SeqCst);
        println!("[REC] REC indicator window created");

        // Message loop
        let mut msg = MSG::default();
        loop {
            if stop_flag.load(Ordering::SeqCst) {
                break;
            }

            let result = GetMessageW(&mut msg, None, 0, 0);
            if result.0 <= 0 {
                break;
            }

            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }

        REC_THREAD_RUNNING.store(false, Ordering::SeqCst);
        println!("[REC] REC thread exiting");
    }
}

#[cfg(windows)]
unsafe extern "system" fn rec_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut rect = RECT::default();
            let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect);

            // Fill background with transparency key color
            let bg_brush = CreateSolidBrush(COLORREF(0xFF00FF)); // Magenta
            let _ = FillRect(hdc, &rect, bg_brush);
            let _ = DeleteObject(bg_brush.into());

            // Draw rounded rectangle background (dark semi-transparent)
            let bg_color = CreateSolidBrush(COLORREF(0x303030)); // Dark gray
            let inner_rect = RECT {
                left: 2,
                top: 2,
                right: rect.right - 2,
                bottom: rect.bottom - 2,
            };
            let _ = FillRect(hdc, &inner_rect, bg_color);
            let _ = DeleteObject(bg_color.into());

            // Draw red circle (recording dot)
            let dot_size = get_font_size() - 4;
            let dot_x = 6;
            let dot_y = (rect.bottom - dot_size) / 2;

            let red_brush = CreateSolidBrush(COLORREF(0x0000FF)); // Red in BGR
            let dot_rect = RECT {
                left: dot_x,
                top: dot_y,
                right: dot_x + dot_size,
                bottom: dot_y + dot_size,
            };
            let _ = FillRect(hdc, &dot_rect, red_brush);
            let _ = DeleteObject(red_brush.into());

            // Draw "REC" text
            let font_size = get_font_size();
            let font = CreateFontW(
                font_size,
                0,
                0,
                0,
                FW_BOLD.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                (DEFAULT_PITCH.0 | FF_SWISS.0) as u32,
                PCWSTR(wide_string("Segoe UI").as_ptr()),
            );

            let old_font = SelectObject(hdc, font.into());
            let _ = SetBkMode(hdc, TRANSPARENT);
            let _ = SetTextColor(hdc, COLORREF(0x0000FF)); // Red

            let text = wide_string("REC");
            let text_x = dot_x + dot_size + 4;
            let text_y = (rect.bottom - font_size) / 2;
            let _ = TextOutW(hdc, text_x, text_y, &text[..text.len() - 1]); // Exclude null terminator

            SelectObject(hdc, old_font);
            let _ = DeleteObject(HGDIOBJ(font.0));

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            if let Ok(mut hwnd_lock) = REC_HWND.lock() {
                *hwnd_lock = 0;
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn wide_string(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// ========== macOS Implementation ==========

#[cfg(target_os = "macos")]
struct CreateRecWindowContext {
    result_window: id,
}

#[cfg(target_os = "macos")]
unsafe impl Send for CreateRecWindowContext {}

#[cfg(target_os = "macos")]
extern "C" fn create_rec_window_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
    println!("[REC] Executing create_rec_window_on_main_thread callback");
    
    let ctx = unsafe { &mut *(ctx_ptr as *mut CreateRecWindowContext) };
    
    unsafe {
        let (width, height) = get_indicator_dimensions();
        
        // Create window frame
        let frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(width as f64, height as f64)
        );
        
        // Create borderless panel (non-activating utility window)
        const NS_NONACTIVATING_PANEL_MASK: u64 = 1 << 7;
        let style_mask_raw = NSWindowStyleMask::NSBorderlessWindowMask.bits() | NS_NONACTIVATING_PANEL_MASK;
        
        let panel_class = class!(NSPanel);
        let window: id = msg_send![panel_class, alloc];
        let window: id = msg_send![window,
            initWithContentRect:frame
            styleMask:style_mask_raw
            backing:NSBackingStoreType::NSBackingStoreBuffered as u64
            defer:NO
        ];
        
        if window == nil {
            println!("[REC] ERROR: Failed to create NSWindow");
            return;
        }

        // Keep lifetime explicit; we manually retain/release this window.
        let _: () = msg_send![window, setReleasedWhenClosed: NO];
        
        // Configure panel - non-activating
        window.setOpaque_(NO);
        let clear: id = NSColor::clearColor(nil);
        window.setBackgroundColor_(clear);
        let _: () = msg_send![window, setLevel: 3i64]; // Floating level (NSInteger)
        window.setIgnoresMouseEvents_(YES);
        
        // CRITICAL: Exclude from screen capture/sharing
        // NSWindowSharingNone = 0 - window won't appear in screen recordings/shares
        let _: () = msg_send![window, setSharingType: 0i64];
        
        // NSPanel-specific: Never become key window
        let _: () = msg_send![window, setBecomesKeyOnlyIfNeeded: YES];
        let _: () = msg_send![window, setWorksWhenModal: YES];
        
        println!("[REC] Getting content view...");
        let content_view: id = window.contentView();
        
        // Set simple dark background
        println!("[REC] Setting background color...");
        let dark_bg: id = msg_send![class!(NSColor), colorWithRed:0.2 green:0.2 blue:0.2 alpha:0.9];
        let _: () = msg_send![content_view, setWantsLayer: YES];
        
        // Create "● REC" text label (simple approach without separate dot view)
        println!("[REC] Creating text label...");
        let font_size = get_font_size() as f64;
        let text_frame = NSRect::new(
            NSPoint::new(4.0, (height as f64 - font_size) / 2.0 - 2.0),
            NSSize::new(width as f64 - 8.0, font_size + 4.0)
        );
        
        let text_label: id = msg_send![class!(NSTextField), alloc];
        let text_label: id = msg_send![text_label, initWithFrame: text_frame];
        let _: () = msg_send![text_label, setBezeled: NO];
        let _: () = msg_send![text_label, setDrawsBackground: NO];
        let _: () = msg_send![text_label, setEditable: NO];
        let _: () = msg_send![text_label, setSelectable: NO];
        
        println!("[REC] Setting text content...");
        let rec_str = NSString::alloc(nil).init_str("● REC");
        let _: () = msg_send![text_label, setStringValue: rec_str];
        
        println!("[REC] Setting font and color...");
        let font: id = msg_send![class!(NSFont), boldSystemFontOfSize: font_size * 0.7];
        let _: () = msg_send![text_label, setFont: font];
        
        let red_color: id = msg_send![class!(NSColor), redColor];
        let _: () = msg_send![text_label, setTextColor: red_color];
        
        println!("[REC] Adding subview...");
        let _: () = msg_send![content_view, addSubview: text_label];
        
        // Keep hidden until explicit show() to avoid AppKit doing work before positioning.
        let _: () = msg_send![window, orderOut: nil];

        // Keep the window alive across threads.
        // We store a raw pointer globally; without an explicit retain, it is possible
        // to end up with a stale pointer depending on AppKit lifecycle/autorelease.
        let _: id = msg_send![window, retain];
        
        log::info!("macOS REC indicator created (non-activating panel)");
        
        // Store result
        ctx.result_window = window;
    }
    
    println!("[REC] create_rec_window_on_main_thread callback finished");
}

#[cfg(target_os = "macos")]
fn run_rec_thread_macos(stop_flag: Arc<AtomicBool>) {
    println!("[REC] Starting macOS REC thread");
    
    let is_main = unsafe { pthread_main_np() } != 0;
    println!("[REC] Current thread is main: {}", is_main);
    
    let mut context = CreateRecWindowContext {
        result_window: nil,
    };
    
    if !is_main {
        println!("[REC] Not on main thread, dispatching to main queue");
        unsafe {
            dispatch_sync_f(
                &_dispatch_main_q,
                &mut context as *mut _ as *mut std::ffi::c_void,
                create_rec_window_on_main_thread,
            );
        }
        println!("[REC] Dispatch completed");
    } else {
        println!("[REC] Already on main thread, creating directly");
        create_rec_window_on_main_thread(&mut context as *mut _ as *mut std::ffi::c_void);
    }
    
    let result_window = context.result_window;
    
    if result_window != nil {
        if let Ok(mut hwnd_lock) = REC_HWND.lock() {
            *hwnd_lock = result_window as isize;
        }
        REC_THREAD_RUNNING.store(true, Ordering::SeqCst);
        println!("[REC] REC indicator window created");
    } else {
        println!("[REC] ERROR: Failed to create REC window");
    }
    
    // Keep thread alive while stop flag is not set
    while !stop_flag.load(Ordering::SeqCst) {
        thread::sleep(std::time::Duration::from_millis(100));
    }
    
    // Clean up
    if result_window != nil {
        extern "C" fn close_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
            let window = ctx_ptr as id;
            unsafe {
                let _: () = msg_send![window, orderOut: nil];
                let _: () = msg_send![window, close];

                // Balance the explicit retain done at creation time.
                let _: () = msg_send![window, release];
            }
        }

        let is_main = unsafe { pthread_main_np() } != 0;
        if !is_main {
            unsafe {
                dispatch_sync_f(
                    &_dispatch_main_q,
                    result_window as *mut std::ffi::c_void,
                    close_on_main_thread,
                );
            }
        } else {
            close_on_main_thread(result_window as *mut std::ffi::c_void);
        }
    }

    // Clear global pointer on exit.
    if let Ok(mut hwnd_lock) = REC_HWND.lock() {
        *hwnd_lock = 0;
    }
    
    REC_THREAD_RUNNING.store(false, Ordering::SeqCst);
    println!("[REC] macOS REC thread exiting");
}

// Implement cross-platform RecordingIndicator trait
impl RecordingIndicator for RecIndicator {
    fn new() -> Option<Self> {
        RecIndicator::new()
    }

    fn show(&self, x: i32, y: i32, border_width: i32) {
        self.show(x, y, border_width)
    }

    fn hide(&self) {
        self.hide()
    }

    fn update_position(&self, region_x: i32, region_y: i32, region_width: i32, border_width: i32) {
        self.update_position(region_x, region_y, region_width, border_width)
    }

    fn set_size(&self, size: &str) {
        self.set_size(size)
    }
}

