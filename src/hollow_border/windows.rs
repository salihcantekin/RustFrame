//! Hollow Border Window - WinAPI Implementation
//!
//! Creates a resizable border window for capture region selection.
//! Interior is draggable, borders are resizable from edges and corners.
//! Runs in its own thread with dedicated message loop.

use crate::traits::BorderWindow;
use rustframe_capture::config::timing::*;
use rustframe_capture::config::window::*;
use rustframe_capture::display_info;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use lazy_static::lazy_static;
use log::info;

#[cfg(windows)]
use windows::Win32::{
    Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, CreatePen, CreateSolidBrush, DeleteObject, EndPaint, FillRect, GetStockObject,
        InvalidateRect, Rectangle, SelectObject, SetBkMode, SetWindowRgn, HBRUSH, HDC, HGDIOBJ,
        HOLLOW_BRUSH, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
    },
    UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
    UI::WindowsAndMessaging::*,
};

lazy_static! {
    /// Global HWND for the hollow border (stored as isize for thread safety)
    static ref HOLLOW_HWND: Mutex<isize> = Mutex::new(0);
    /// Global rect for the hollow border - updated by window thread, read by render thread
    static ref HOLLOW_RECT: Mutex<(i32, i32, i32, i32)> = Mutex::new(DEFAULT_REGION);
    /// Border width (scaled for DPI)
    static ref BORDER_WIDTH: Mutex<i32> = Mutex::new({
        let display = display_info::get();
        if display.initialized {
            display.points_to_pixels(DEFAULT_BORDER_WIDTH as f64)
        } else {
            DEFAULT_BORDER_WIDTH
        }
    });
    /// Border color (BGR format)
    static ref BORDER_COLOR: Mutex<u32> = Mutex::new(DEFAULT_BORDER_COLOR);
    /// Callback for border interaction completion (move/resize finished)
    static ref BORDER_INTERACTION_COMPLETE_CALLBACK: Mutex<Option<Box<dyn Fn(i32, i32, i32, i32) + Send + Sync>>> = Mutex::new(None);
    /// Callback for live border movement (fires during drag/resize for REC indicator)
    static ref BORDER_LIVE_MOVE_CALLBACK: Mutex<Option<Box<dyn Fn(i32, i32, i32, i32) + Send + Sync>>> = Mutex::new(None);
}

static WINDOW_THREAD_RUNNING: AtomicBool = AtomicBool::new(false);
/// Preview mode: interior is draggable, not click-through
/// Capture mode: interior is click-through, only top edge drags
static PREVIEW_MODE: AtomicBool = AtomicBool::new(true);
/// Flag indicating border is being dragged/resized
static BORDER_INTERACTING: AtomicBool = AtomicBool::new(false);

/// Check if border is currently being dragged or resized
pub fn is_border_interacting() -> bool {
    BORDER_INTERACTING.load(Ordering::SeqCst)
}

/// Check if HOLLOW_HWND is valid (non-zero)
/// Used to ensure previous border window is fully cleaned up
pub fn is_hollow_hwnd_valid() -> bool {
    HOLLOW_HWND.lock().map(|h| *h != 0).unwrap_or(false)
}

/// Register callback to be notified when border drag/resize completes
/// Fires only on mouse up - allows updating capture region after interaction
pub fn set_border_interaction_complete_callback<F>(callback: F)
where
    F: Fn(i32, i32, i32, i32) + Send + Sync + 'static,
{
    if let Ok(mut cb) = BORDER_INTERACTION_COMPLETE_CALLBACK.lock() {
        *cb = Some(Box::new(callback));
    }
}

/// Register callback for live border movement updates (fires during drag/resize)
/// Used for REC indicator to follow border in real-time
pub fn set_border_live_move_callback<F>(callback: F)
where
    F: Fn(i32, i32, i32, i32) + Send + Sync + 'static,
{
    if let Ok(mut cb) = BORDER_LIVE_MOVE_CALLBACK.lock() {
        *cb = Some(Box::new(callback));
    }
}

/// Hollow border window - runs in its own thread with message loop
pub struct HollowBorder {
    thread_handle: Option<JoinHandle<()>>,
    stop_flag: Arc<AtomicBool>,
}

// SAFETY: We communicate via atomic flags and mutex-protected data
unsafe impl Send for HollowBorder {}
unsafe impl Sync for HollowBorder {}

impl HollowBorder {
    /// Create a new hollow border window in its own thread
    #[cfg(windows)]
    pub fn new(
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        border_width: i32,
        border_color: u32,
    ) -> Option<Self> {
        info!(
            "Creating hollow border window at ({}, {}) size {}x{} in dedicated thread",
            x, y, width, height
        );

        // Store initial values
        if let Ok(mut bw) = BORDER_WIDTH.lock() {
            *bw = border_width;
        }
        if let Ok(mut bc) = BORDER_COLOR.lock() {
            *bc = border_color;
        }
        if let Ok(mut rect) = HOLLOW_RECT.lock() {
            *rect = (x, y, width, height);
        }

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        // Spawn window thread
        let thread_handle = thread::spawn(move || {
            run_window_thread(
                x,
                y,
                width,
                height,
                border_width,
                border_color,
                stop_flag_clone,
            );
        });

        // Wait for window to be created
        for _ in 0..50 {
            thread::sleep(std::time::Duration::from_millis(10));
            if let Ok(hwnd_lock) = HOLLOW_HWND.lock() {
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

    /// Get the current position and size of the window (includes border)
    pub fn get_rect(&self) -> (i32, i32, i32, i32) {
        if let Ok(rect) = HOLLOW_RECT.lock() {
            *rect
        } else {
            (0, 0, 800, 600)
        }
    }

    /// Get the inner rect (capture area inside the border)
    /// This is the area that should be captured - excludes the visual border
    pub fn get_inner_rect(&self) -> (i32, i32, i32, i32) {
        let (x, y, w, h) = self.get_rect();
        let bw = BORDER_WIDTH.lock().map(|w| *w as i32).unwrap_or(3);
        // Border is drawn inside the window, so inner area starts at border_width offset
        (x + bw, y + bw, (w - 2 * bw).max(1), (h - 2 * bw).max(1))
    }

    /// Show the hollow border
    pub fn show(&self) {
        if let Ok(hwnd_lock) = HOLLOW_HWND.lock() {
            let hwnd_val = *hwnd_lock;
            if hwnd_val != 0 {
                unsafe {
                    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOW};
                    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
                    let _ = ShowWindow(hwnd, SW_SHOW);
                }
            }
        }
    }

    /// Hide the hollow border
    pub fn hide(&self) {
        if let Ok(hwnd_lock) = HOLLOW_HWND.lock() {
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

    /// Get the HWND value (for platform-specific operations)
    pub fn hwnd_value(&self) -> isize {
        HOLLOW_HWND.lock().map(|h| *h).unwrap_or(0)
    }
    
    /// Get the HWND for desktop detection (excludes our window from checks)
    pub fn get_hwnd(&self) -> isize {
        self.hwnd_value()
    }

    /// Update the border position and size
    pub fn update_rect(&self, x: i32, y: i32, width: i32, height: i32) {
        // Update stored rect (window = capture area)
        if let Ok(mut rect) = HOLLOW_RECT.lock() {
            *rect = (x, y, width, height);
        }

        // Move and resize window (window position matches capture region exactly)
        if let Ok(hwnd_lock) = HOLLOW_HWND.lock() {
            let hwnd_val = *hwnd_lock;
            if hwnd_val != 0 {
                unsafe {
                    use windows::Win32::Graphics::Gdi::InvalidateRect;
                    use windows::Win32::UI::WindowsAndMessaging::{
                        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE,
                    };
                    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
                    let bw = BORDER_WIDTH.lock().map(|w| *w).unwrap_or(3);
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_TOPMOST),
                        x,
                        y,
                        width,
                        height,
                        SWP_NOACTIVATE,
                    );
                    // Update hollow region with new dimensions
                    apply_hollow_region(hwnd, width, height, bw);
                    // Force complete redraw with erase to prevent ghost trails during drag/resize
                    // Using erase=true clears the background before repaint
                    let _ = InvalidateRect(Some(hwnd), None, true);
                }
            }
        }
    }

    /// Update the border color without recreating the window
    pub fn update_color(&self, color: u32) {
        // Update stored color
        if let Ok(mut c) = BORDER_COLOR.lock() {
            *c = color;
        }

        // Invalidate window to trigger repaint
        if let Ok(hwnd_lock) = HOLLOW_HWND.lock() {
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

    /// Update both border width and color
    pub fn update_style(&self, width: i32, color: u32) {
        // Update stored values
        if let Ok(mut w) = BORDER_WIDTH.lock() {
            *w = width;
        }
        if let Ok(mut c) = BORDER_COLOR.lock() {
            *c = color;
        }

        if let Ok(hwnd_lock) = HOLLOW_HWND.lock() {
            let hwnd_val = *hwnd_lock;
            if hwnd_val != 0 {
                unsafe {
                    use windows::Win32::Foundation::RECT;
                    use windows::Win32::Graphics::Gdi::InvalidateRect;
                    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

                    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);

                    // Get actual window dimensions
                    let mut win_rect = RECT::default();
                    let _ = GetWindowRect(hwnd, &mut win_rect);
                    let window_w = win_rect.right - win_rect.left;
                    let window_h = win_rect.bottom - win_rect.top;

                    // Update hollow region with new border width
                    if window_w > 0 && window_h > 0 {
                        apply_hollow_region(hwnd, window_w, window_h, width);
                    }
                    let _ = InvalidateRect(Some(hwnd), None, true);
                }
            }
        }
    }

    /// Set preview mode - interior is draggable, not click-through
    pub fn set_preview_mode(&self) {
        PREVIEW_MODE.store(true, Ordering::SeqCst);
        // Update the region and layered attributes
        if let Ok(hwnd_lock) = HOLLOW_HWND.lock() {
            let hwnd_val = *hwnd_lock;
            if hwnd_val != 0 {
                unsafe {
                    use windows::Win32::Foundation::RECT;
                    use windows::Win32::Graphics::Gdi::InvalidateRect;
                    use windows::Win32::UI::WindowsAndMessaging::{
                        GetWindowRect, SetLayeredWindowAttributes, ShowWindow, LWA_ALPHA, SW_HIDE,
                        SW_SHOW,
                    };

                    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);

                    // Preview mode: use alpha transparency (15% opaque = 38/255)
                    // This makes interior visible but semi-transparent, and NOT click-through
                    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 38, LWA_ALPHA);

                    let mut win_rect = RECT::default();
                    let _ = GetWindowRect(hwnd, &mut win_rect);
                    let w = win_rect.right - win_rect.left;
                    let h = win_rect.bottom - win_rect.top;
                    let border = BORDER_WIDTH.lock().map(|b| *b).unwrap_or(4);
                    apply_hollow_region(hwnd, w, h, border);

                    // Force window refresh to apply transparency changes
                    let _ = ShowWindow(hwnd, SW_HIDE);
                    let _ = ShowWindow(hwnd, SW_SHOW);
                    let _ = InvalidateRect(Some(hwnd), None, true);
                }
            }
        }
    }

    /// Set capture mode - interior is click-through, only top edge drags
    pub fn set_capture_mode(&self) {
        PREVIEW_MODE.store(false, Ordering::SeqCst);
        // Update the region and layered attributes
        if let Ok(hwnd_lock) = HOLLOW_HWND.lock() {
            let hwnd_val = *hwnd_lock;
            if hwnd_val != 0 {
                unsafe {
                    use windows::Win32::Foundation::RECT;
                    use windows::Win32::Graphics::Gdi::InvalidateRect;
                    use windows::Win32::UI::WindowsAndMessaging::{
                        GetWindowRect, SetLayeredWindowAttributes, ShowWindow, LWA_COLORKEY,
                        SW_HIDE, SW_SHOW,
                    };

                    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);

                    // Capture mode: use color key for transparency (green = click-through)
                    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0x00FF00), 255, LWA_COLORKEY);

                    let mut win_rect = RECT::default();
                    let _ = GetWindowRect(hwnd, &mut win_rect);
                    let w = win_rect.right - win_rect.left;
                    let h = win_rect.bottom - win_rect.top;
                    let border = BORDER_WIDTH.lock().map(|b| *b).unwrap_or(4);
                    apply_hollow_region(hwnd, w, h, border);

                    // Force window refresh to apply transparency changes
                    let _ = ShowWindow(hwnd, SW_HIDE);
                    let _ = ShowWindow(hwnd, SW_SHOW);
                    let _ = InvalidateRect(Some(hwnd), None, true);
                }
            }
        }
    }
}

// Implement cross-platform BorderWindow trait
impl BorderWindow for HollowBorder {
    fn new(
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        border_width: i32,
        border_color: u32,
    ) -> Option<Self> {
        HollowBorder::new(x, y, width, height, border_width, border_color)
    }

    fn get_rect(&self) -> (i32, i32, i32, i32) {
        HollowBorder::get_rect(self)
    }

    fn get_inner_rect(&self) -> (i32, i32, i32, i32) {
        HollowBorder::get_inner_rect(self)
    }

    fn update_rect(&self, x: i32, y: i32, width: i32, height: i32) {
        HollowBorder::update_rect(self, x, y, width, height)
    }

    fn update_color(&self, color: u32) {
        HollowBorder::update_color(self, color)
    }

    fn update_style(&self, width: i32, color: u32) {
        HollowBorder::update_style(self, width, color)
    }

    fn hide(&self) {
        HollowBorder::hide(self)
    }

    fn show(&self) {
        HollowBorder::show(self)
    }

    fn hwnd_value(&self) -> isize {
        HollowBorder::hwnd_value(self)
    }

    fn set_capture_mode(&mut self) {
        // Set capture mode: interior is click-through, only edges/corners are interactive
        PREVIEW_MODE.store(false, Ordering::SeqCst);
        if let Ok(hwnd_lock) = HOLLOW_HWND.lock() {
            let hwnd_val = *hwnd_lock;
            if hwnd_val != 0 {
                unsafe {
                    use windows::Win32::Foundation::RECT;
                    use windows::Win32::Graphics::Gdi::InvalidateRect;
                    use windows::Win32::UI::WindowsAndMessaging::{
                        GetWindowRect, SetLayeredWindowAttributes, LWA_COLORKEY,
                    };

                    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
                    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0x00FF00), 255, LWA_COLORKEY);

                    let mut win_rect = RECT::default();
                    let _ = GetWindowRect(hwnd, &mut win_rect);
                    let w = win_rect.right - win_rect.left;
                    let h = win_rect.bottom - win_rect.top;
                    let border = BORDER_WIDTH.lock().map(|b| *b).unwrap_or(4);
                    apply_hollow_region(hwnd, w, h, border);
                    let _ = InvalidateRect(Some(hwnd), None, true);
                }
            }
        }
    }

    fn set_preview_mode(&mut self) {
        // Set preview mode: interior is semi-transparent and draggable
        PREVIEW_MODE.store(true, Ordering::SeqCst);
        if let Ok(hwnd_lock) = HOLLOW_HWND.lock() {
            let hwnd_val = *hwnd_lock;
            if hwnd_val != 0 {
                unsafe {
                    use windows::Win32::Foundation::RECT;
                    use windows::Win32::Graphics::Gdi::InvalidateRect;
                    use windows::Win32::UI::WindowsAndMessaging::{
                        GetWindowRect, SetLayeredWindowAttributes, LWA_ALPHA,
                    };

                    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
                    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 38, LWA_ALPHA);

                    let mut win_rect = RECT::default();
                    let _ = GetWindowRect(hwnd, &mut win_rect);
                    let w = win_rect.right - win_rect.left;
                    let h = win_rect.bottom - win_rect.top;
                    let border = BORDER_WIDTH.lock().map(|b| *b).unwrap_or(4);
                    apply_hollow_region(hwnd, w, h, border);
                    let _ = InvalidateRect(Some(hwnd), None, true);
                }
            }
        }
    }

    fn stop(&mut self) {
        // Stop flag already set in Drop, nothing extra needed
    }
}

impl Drop for HollowBorder {
    fn drop(&mut self) {
        info!("Destroying hollow border window");

        // Signal thread to stop
        self.stop_flag.store(true, Ordering::SeqCst);

        // Post close message to window thread
        if let Ok(hwnd_lock) = HOLLOW_HWND.lock() {
            let hwnd_val = *hwnd_lock;
            if hwnd_val != 0 {
                unsafe {
                    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
                    let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
        }

        // Wait for thread to finish
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        // Clear global HWND
        if let Ok(mut hwnd_lock) = HOLLOW_HWND.lock() {
            *hwnd_lock = 0;
        }
    }
}

/// Window thread - creates window and runs its own message loop
#[cfg(windows)]
fn run_window_thread(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    border_width: i32,
    _border_color: u32,
    stop_flag: Arc<AtomicBool>,
) {
    use windows::core::PCWSTR;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;

    tracing::debug!("Hollow border window thread started");

    unsafe {
        let hinstance = match GetModuleHandleW(None) {
            Ok(h) => h,
            Err(e) => {
                tracing::error!(error = %e, "Failed to get module handle");
                return;
            }
        };

        // Register window class
        let class_name = wide_string("RustFrameHollowBorder");
        static CLASS_REGISTERED: AtomicBool = AtomicBool::new(false);

        if !CLASS_REGISTERED.swap(true, Ordering::SeqCst) {
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                // Removed CS_HREDRAW | CS_VREDRAW to prevent flickering during resize
                style: windows::Win32::UI::WindowsAndMessaging::WNDCLASS_STYLES(0),
                lpfnWndProc: Some(window_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance.into(),
                hIcon: HICON::default(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH::default(),
                lpszMenuName: PCWSTR::null(),
                lpszClassName: PCWSTR(class_name.as_ptr()),
                hIconSm: HICON::default(),
            };

            if RegisterClassExW(&wc) == 0 {
                tracing::error!("Failed to register window class");
                CLASS_REGISTERED.store(false, Ordering::SeqCst);
                return;
            }
        }

        // Create the window
        // Window is placed exactly at the capture region coordinates
        // Border is drawn INSIDE the window (capture area is slightly smaller than visual border)
        // This ensures border never goes outside screen bounds
        let window_x = x;
        let window_y = y;
        let window_w = width;
        let window_h = height;

        let hwnd = match CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(wide_string("RustFrame Capture Region").as_ptr()),
            WS_POPUP | WS_VISIBLE,
            window_x,
            window_y,
            window_w,
            window_h,
            None,
            None,
            Some(hinstance.into()),
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                tracing::error!(error = %e, "Failed to create hollow border window");
                return;
            }
        };

        // Store HWND globally
        if let Ok(mut hwnd_lock) = HOLLOW_HWND.lock() {
            *hwnd_lock = hwnd.0 as isize;
        }

        // Set layered window attributes for the border color
        let _ = SetLayeredWindowAttributes(
            hwnd,
            COLORREF(0x00FF00), // Bright green as transparency key
            255,
            LWA_COLORKEY,
        );

        // Exclude from capture (optional, but good practice to avoid capturing our own border)
        // WDA_EXCLUDEFROMCAPTURE = 0x00000011
        let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);

        // Apply the hollow region (using window dimensions which include border)
        apply_hollow_region(hwnd, window_w, window_h, border_width);

        // Install subclass for hit testing
        let _ = SetWindowSubclass(hwnd, Some(subclass_proc), 1, 0);

        WINDOW_THREAD_RUNNING.store(true, Ordering::SeqCst);
        tracing::info!(hwnd = ?hwnd, "Hollow border window created");

        // Message loop - THIS IS THE KEY!
        let mut msg = MSG::default();
        loop {
            if stop_flag.load(Ordering::SeqCst) {
                break;
            }

            let result = GetMessageW(&mut msg, None, 0, 0);
            if result.0 <= 0 {
                break; // WM_QUIT or error
            }

            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }

        // Cleanup
        let _ = RemoveWindowSubclass(hwnd, Some(subclass_proc), 1);
        WINDOW_THREAD_RUNNING.store(false, Ordering::SeqCst);
        tracing::debug!("Hollow border window thread exiting");
    }
}

/// Apply window region based on mode
/// Preview mode: full window is interactive (draggable from interior)
/// Capture mode: hollow region (interior is click-through)
#[cfg(windows)]
unsafe fn apply_hollow_region(hwnd: HWND, width: i32, height: i32, border: i32) {
    use windows::Win32::Graphics::Gdi::{CombineRgn, CreateRectRgn, RGN_DIFF};

    if PREVIEW_MODE.load(Ordering::SeqCst) {
        // Preview mode: remove region, entire window is interactive
        SetWindowRgn(hwnd, None, true);
    } else {
        // Capture mode: hollow region, interior is click-through
        let outer_rgn = CreateRectRgn(0, 0, width, height);
        let inner_rgn = CreateRectRgn(border, border, width - border, height - border);
        let _ = CombineRgn(Some(outer_rgn), Some(outer_rgn), Some(inner_rgn), RGN_DIFF);
        let _ = DeleteObject(inner_rgn.into());
        SetWindowRgn(hwnd, Some(outer_rgn), true);
    }
}

/// Window procedure
#[cfg(windows)]
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => LRESULT(0),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);

            let border_width = BORDER_WIDTH.lock().map(|b| *b).unwrap_or(4);
            let border_color = BORDER_COLOR.lock().map(|c| *c).unwrap_or(0x4080FF);
            let corner_length = 16.min(rect.right / 5).min(rect.bottom / 5); // Shorter corner lines
            let corner_thickness = (border_width + 1).max(4); // Thinner corner lines
            let is_preview = PREVIEW_MODE.load(Ordering::SeqCst);

            // Fill background based on mode
            // Preview mode: dark gray overlay to show it's not click-through
            // Capture mode: green (color key) for transparency/click-through
            let bg_color = if is_preview {
                PREVIEW_BG_COLOR
            } else {
                CAPTURE_BG_COLOR
            }; // Dark gray vs green
            let bg_brush = CreateSolidBrush(COLORREF(bg_color));
            let _ = FillRect(hdc, &rect, bg_brush);
            let _ = DeleteObject(bg_brush.into());

            // Draw main border (thinner)
            let pen = CreatePen(PS_SOLID, border_width, COLORREF(border_color));
            let brush = GetStockObject(HOLLOW_BRUSH);

            let old_pen = SelectObject(hdc, pen.into());
            let old_brush = SelectObject(hdc, brush);

            let _ = SetBkMode(hdc, TRANSPARENT);
            let _ = Rectangle(hdc, 0, 0, rect.right, rect.bottom);

            SelectObject(hdc, old_pen);
            SelectObject(hdc, old_brush);
            let _ = DeleteObject(HGDIOBJ(pen.0));

            // Draw thicker corner lines (L-shaped at each corner)
            let corner_brush = CreateSolidBrush(COLORREF(border_color));

            // Top-left corner - horizontal line
            let tl_h = RECT {
                left: 0,
                top: 0,
                right: corner_length,
                bottom: corner_thickness,
            };
            let _ = FillRect(hdc, &tl_h, corner_brush);
            // Top-left corner - vertical line
            let tl_v = RECT {
                left: 0,
                top: 0,
                right: corner_thickness,
                bottom: corner_length,
            };
            let _ = FillRect(hdc, &tl_v, corner_brush);

            // Top-right corner - horizontal line
            let tr_h = RECT {
                left: rect.right - corner_length,
                top: 0,
                right: rect.right,
                bottom: corner_thickness,
            };
            let _ = FillRect(hdc, &tr_h, corner_brush);
            // Top-right corner - vertical line
            let tr_v = RECT {
                left: rect.right - corner_thickness,
                top: 0,
                right: rect.right,
                bottom: corner_length,
            };
            let _ = FillRect(hdc, &tr_v, corner_brush);

            // Bottom-left corner - horizontal line
            let bl_h = RECT {
                left: 0,
                top: rect.bottom - corner_thickness,
                right: corner_length,
                bottom: rect.bottom,
            };
            let _ = FillRect(hdc, &bl_h, corner_brush);
            // Bottom-left corner - vertical line
            let bl_v = RECT {
                left: 0,
                top: rect.bottom - corner_length,
                right: corner_thickness,
                bottom: rect.bottom,
            };
            let _ = FillRect(hdc, &bl_v, corner_brush);

            // Bottom-right corner - horizontal line
            let br_h = RECT {
                left: rect.right - corner_length,
                top: rect.bottom - corner_thickness,
                right: rect.right,
                bottom: rect.bottom,
            };
            let _ = FillRect(hdc, &br_h, corner_brush);
            // Bottom-right corner - vertical line
            let br_v = RECT {
                left: rect.right - corner_thickness,
                top: rect.bottom - corner_length,
                right: rect.right,
                bottom: rect.bottom,
            };
            let _ = FillRect(hdc, &br_v, corner_brush);

            let _ = DeleteObject(corner_brush.into());

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => {
            let hdc = HDC(wparam.0 as *mut _);
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);

            // Use same background color logic as WM_PAINT
            let is_preview = PREVIEW_MODE.load(Ordering::SeqCst);
            let bg_color = if is_preview { 0x202020 } else { 0x00FF00 };
            let brush = CreateSolidBrush(COLORREF(bg_color));
            let _ = FillRect(hdc, &rect, brush);
            let _ = DeleteObject(brush.into());

            LRESULT(1)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            if let Ok(mut hwnd_lock) = HOLLOW_HWND.lock() {
                *hwnd_lock = 0;
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Subclass procedure for hit testing and size tracking
#[cfg(windows)]
unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _uidsubclass: usize,
    _dwrefdata: usize,
) -> LRESULT {
    if msg == WM_NCHITTEST {
        let x = (lparam.0 & 0xFFFF) as i16 as i32;
        let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

        let mut rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);

        let border = BORDER_WIDTH.lock().map(|b| *b).unwrap_or(4).max(8);
        let corner_size = 20.max(border * 2); // Corner hit area
        let is_preview = PREVIEW_MODE.load(Ordering::SeqCst);

        let on_left = x >= rect.left && x < rect.left + corner_size;
        let on_right = x >= rect.right - corner_size && x < rect.right;
        let on_top = y >= rect.top && y < rect.top + corner_size;
        let on_bottom = y >= rect.bottom - corner_size && y < rect.bottom;

        // Corner hit tests (priority - check first)
        if on_top && on_left {
            return LRESULT(HTTOPLEFT as isize);
        }
        if on_top && on_right {
            return LRESULT(HTTOPRIGHT as isize);
        }
        if on_bottom && on_left {
            return LRESULT(HTBOTTOMLEFT as isize);
        }
        if on_bottom && on_right {
            return LRESULT(HTBOTTOMRIGHT as isize);
        }

        // Edge hit tests (narrower than corners)
        if x >= rect.left && x < rect.left + border {
            return LRESULT(HTLEFT as isize);
        }
        if x >= rect.right - border && x < rect.right {
            return LRESULT(HTRIGHT as isize);
        }
        if y >= rect.top && y < rect.top + border {
            // Top edge: HTCAPTION in capture mode (drag), HTTOP in preview (resize from top too)
            if is_preview {
                return LRESULT(HTTOP as isize);
            } else {
                return LRESULT(HTCAPTION as isize);
            }
        }
        if y >= rect.bottom - border && y < rect.bottom {
            return LRESULT(HTBOTTOM as isize);
        }

        // Interior behavior depends on mode
        if is_preview {
            // Preview mode: interior is draggable
            return LRESULT(HTCAPTION as isize);
        } else {
            // Capture mode: interior is click-through
            return LRESULT(HTTRANSPARENT as isize);
        }
    }

    if msg == WM_SETCURSOR {
        let hit_test = (lparam.0 & 0xFFFF) as u16 as u32;
        let is_preview = PREVIEW_MODE.load(Ordering::SeqCst);

        let cursor_id = match hit_test {
            x if x == HTCAPTION => Some(IDC_SIZEALL), // Move cursor
            x if x == HTTOPLEFT || x == HTBOTTOMRIGHT => Some(IDC_SIZENWSE),
            x if x == HTTOPRIGHT || x == HTBOTTOMLEFT => Some(IDC_SIZENESW),
            x if x == HTLEFT || x == HTRIGHT => Some(IDC_SIZEWE),
            x if x == HTTOP || x == HTBOTTOM => Some(IDC_SIZENS),
            _ => {
                if is_preview {
                    Some(IDC_SIZEALL)
                } else {
                    None
                }
            }
        };

        if let Some(id) = cursor_id {
            if let Ok(cur) = LoadCursorW(None, id) {
                let _ = SetCursor(Some(cur));
            }
            return LRESULT(1);
        }
    }

    // Handle size changes - update the hollow region and global rect
    if msg == WM_SIZE {
        let new_width = (lparam.0 & 0xFFFF) as i32;
        let new_height = ((lparam.0 >> 16) & 0xFFFF) as i32;

        if new_width > 0 && new_height > 0 {
            let border = BORDER_WIDTH.lock().map(|b| *b).unwrap_or(4);
            apply_hollow_region(hwnd, new_width, new_height, border);

            // Update global rect with new size
            let mut win_rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut win_rect);
            if let Ok(mut rect) = HOLLOW_RECT.lock() {
                *rect = (win_rect.left, win_rect.top, new_width, new_height);
            }

            // Call live move callback for REC indicator
            if BORDER_INTERACTING.load(Ordering::SeqCst) {
                if let Ok(cb_lock) = BORDER_LIVE_MOVE_CALLBACK.try_lock() {
                    if let Some(ref callback) = *cb_lock {
                        callback(win_rect.left, win_rect.top, new_width, new_height);
                    }
                }
            }
        }
        return DefSubclassProc(hwnd, msg, wparam, lparam);
    }

    // Handle move - update global rect and call live callback
    if msg == WM_MOVE {
        // Use GetWindowRect for consistency with other handlers
        let mut win_rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut win_rect);

        let width = win_rect.right - win_rect.left;
        let height = win_rect.bottom - win_rect.top;

        if let Ok(mut rect) = HOLLOW_RECT.lock() {
            // Store window position directly (window = capture area)
            rect.0 = win_rect.left;
            rect.1 = win_rect.top;
            rect.2 = width;
            rect.3 = height;
        }

        // Call live move callback for REC indicator during drag
        if BORDER_INTERACTING.load(Ordering::SeqCst) {
            if let Ok(cb_lock) = BORDER_LIVE_MOVE_CALLBACK.try_lock() {
                if let Some(ref callback) = *cb_lock {
                    callback(win_rect.left, win_rect.top, width, height);
                }
            }
        }

        return DefSubclassProc(hwnd, msg, wparam, lparam);
    }

    // Handle start of move/resize - set interacting flag
    if msg == WM_ENTERSIZEMOVE {
        BORDER_INTERACTING.store(true, Ordering::SeqCst);
        tracing::debug!("Border interaction started");
        return DefSubclassProc(hwnd, msg, wparam, lparam);
    }

    // Handle resize completion - update global rect and call callback
    if msg == WM_EXITSIZEMOVE {
        BORDER_INTERACTING.store(false, Ordering::SeqCst);

        let mut win_rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut win_rect);

        let (x, y, width, height) = (
            win_rect.left,
            win_rect.top,
            win_rect.right - win_rect.left,
            win_rect.bottom - win_rect.top,
        );

        if let Ok(mut rect) = HOLLOW_RECT.lock() {
            // Store window rect directly (window = capture area)
            *rect = (x, y, width, height);
        }

        let _ = InvalidateRect(Some(hwnd), None, true);
        tracing::debug!(rect = ?win_rect, "Hollow border resize complete");

        // Call the interaction complete callback to update capture region, destination window, etc.
        if let Ok(cb_lock) = BORDER_INTERACTION_COMPLETE_CALLBACK.lock() {
            if let Some(ref callback) = *cb_lock {
                callback(x, y, width, height);
            }
        }

        return DefSubclassProc(hwnd, msg, wparam, lparam);
    }

    DefSubclassProc(hwnd, msg, wparam, lparam)
}

/// Convert a Rust string to a null-terminated wide string
fn wide_string(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
