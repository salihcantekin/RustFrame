//! REC Indicator Overlay Window
//! Shows a "● REC" indicator in the top-right corner of the capture region

use lazy_static::lazy_static;
use log::info;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

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
        None
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
    }

    /// Update position when capture region moves
    pub fn update_position(
        &self,
        region_x: i32,
        region_y: i32,
        region_width: i32,
        border_width: i32,
    ) {
        if !REC_VISIBLE.load(Ordering::SeqCst) {
            return;
        }

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
