//! Hollow Border Window - WinAPI Implementation
//!
//! Creates a transparent, hollow border window that can be resized and moved.
//! The interior is completely click-through (HTTRANSPARENT).
//! Uses SetWindowRgn to create the hollow effect.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use log::info;

#[cfg(windows)]
use windows::Win32::{
    Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, CombineRgn, CreatePen, CreateRectRgn, CreateSolidBrush, DeleteObject,
        EndPaint, FillRect, GetStockObject, InvalidateRect, Rectangle, SelectObject, SetBkMode,
        SetWindowRgn, HDC, HBRUSH, HGDIOBJ, HOLLOW_BRUSH,
        PAINTSTRUCT, PS_SOLID, RGN_DIFF, TRANSPARENT,
    },
    UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
    UI::WindowsAndMessaging::*,
};

const VK_ESCAPE: u32 = 0x1B;

// Thread-local storage for border properties
thread_local! {
    static BORDER_WIDTH: Cell<i32> = const { Cell::new(4) };
    static BORDER_COLOR: Cell<u32> = const { Cell::new(0x4080FF) }; // BGR format: Blue-ish
    static HOLLOW_HWND: Cell<isize> = const { Cell::new(0) };
}

// Global flag for ESC key pressed
static ESC_PRESSED: AtomicBool = AtomicBool::new(false);

/// Check if ESC was pressed and reset the flag
pub fn was_esc_pressed() -> bool {
    ESC_PRESSED.swap(false, Ordering::SeqCst)
}

/// Hollow border window handle and control
pub struct HollowBorder {
    hwnd: HWND,
    is_visible: Arc<AtomicBool>,
}

impl HollowBorder {
    /// Create a new hollow border window at the specified position and size
    #[cfg(windows)]
    pub fn new(x: i32, y: i32, width: i32, height: i32, border_width: i32, border_color: u32) -> Option<Self> {
        use windows::core::PCWSTR;
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;

        unsafe {
            // Store border properties
            BORDER_WIDTH.set(border_width);
            BORDER_COLOR.set(border_color);

            // Register window class
            let class_name = wide_string("RustFrameHollowBorder");
            let hinstance = GetModuleHandleW(None).ok()?;

            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(Self::wnd_proc),
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

            // Register (ignore if already registered)
            let _ = RegisterClassExW(&wc);

            // Create the window
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
                PCWSTR(class_name.as_ptr()),
                PCWSTR(wide_string("RustFrame Capture Region").as_ptr()),
                WS_POPUP | WS_VISIBLE,
                x,
                y,
                width,
                height,
                None,
                None,
                Some(hinstance.into()),
                None,
            ).ok()?;

            HOLLOW_HWND.set(hwnd.0 as isize);

            // Set layered window attributes for the border color
            // Use color key for transparency
            let _ = SetLayeredWindowAttributes(
                hwnd,
                COLORREF(0x00FF00), // Bright green as transparency key
                255,
                LWA_COLORKEY,
            );

            // Apply the hollow region
            Self::apply_hollow_region(hwnd, width, height, border_width);

            // Install subclass for hit testing
            let _ = SetWindowSubclass(hwnd, Some(Self::subclass_proc), 1, 0);

            info!("Hollow border window created at ({}, {}) size {}x{}", x, y, width, height);

            Some(Self {
                hwnd,
                is_visible: Arc::new(AtomicBool::new(true)),
            })
        }
    }

    /// Apply hollow region to window (border visible, interior transparent/click-through)
    #[cfg(windows)]
    unsafe fn apply_hollow_region(hwnd: HWND, width: i32, height: i32, border: i32) {
        // Create outer rectangle (entire window)
        let outer_rgn = CreateRectRgn(0, 0, width, height);

        // Create inner rectangle (the hole)
        let inner_rgn = CreateRectRgn(border, border, width - border, height - border);

        // Subtract inner from outer to create hollow frame
        let _ = CombineRgn(Some(outer_rgn), Some(outer_rgn), Some(inner_rgn), RGN_DIFF);

        // Delete inner region (outer will be owned by window)
        let _ = DeleteObject(inner_rgn.into());

        // Apply the region (window takes ownership)
        SetWindowRgn(hwnd, Some(outer_rgn), true);
    }

    /// Window procedure for hollow border
    #[cfg(windows)]
    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_CREATE => {
                LRESULT(0)
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                
                // Get window size
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                
                let border_width = BORDER_WIDTH.get();
                let border_color = BORDER_COLOR.get();
                
                // Create pen for border
                let pen = CreatePen(PS_SOLID, border_width, COLORREF(border_color));
                let brush = GetStockObject(HOLLOW_BRUSH);
                
                let old_pen = SelectObject(hdc, pen.into());
                let old_brush = SelectObject(hdc, brush);
                
                // Draw rectangle (the region clips it to just the border)
                let _ = SetBkMode(hdc, TRANSPARENT);
                let _ = Rectangle(hdc, 0, 0, rect.right, rect.bottom);
                
                // Restore and cleanup
                SelectObject(hdc, old_pen);
                SelectObject(hdc, old_brush);
                let _ = DeleteObject(HGDIOBJ(pen.0));
                
                EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_ERASEBKGND => {
                // Fill with the transparency key color
                let hdc = HDC(wparam.0 as *mut _);
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                
                let brush = CreateSolidBrush(COLORREF(0x00FF00)); // Green = transparent
                let _ = FillRect(hdc, &rect, brush);
                let _ = DeleteObject(brush.into());
                
                LRESULT(1)
            }
            WM_DESTROY => {
                HOLLOW_HWND.set(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    /// Subclass procedure for hit testing (resize corners and top edge drag)
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

            let border = BORDER_WIDTH.get().max(8);
            let corner_size = border + 4;

            // Check corners first
            let on_left = x >= rect.left && x < rect.left + corner_size;
            let on_right = x >= rect.right - corner_size && x < rect.right;
            let on_top = y >= rect.top && y < rect.top + corner_size;
            let on_bottom = y >= rect.bottom - corner_size && y < rect.bottom;

            // Corner hit tests
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

            // Edge hit tests
            if x >= rect.left && x < rect.left + border {
                return LRESULT(HTLEFT as isize);
            }
            if x >= rect.right - border && x < rect.right {
                return LRESULT(HTRIGHT as isize);
            }
            if y >= rect.top && y < rect.top + border {
                // Top edge = drag (caption)
                return LRESULT(HTCAPTION as isize);
            }
            if y >= rect.bottom - border && y < rect.bottom {
                return LRESULT(HTBOTTOM as isize);
            }

            // Interior = transparent (click through)
            return LRESULT(HTTRANSPARENT as isize);
        }

        if msg == WM_SETCURSOR {
            let hit_test = (lparam.0 & 0xFFFF) as u16 as u32;

            let cursor_id = match hit_test {
                x if x == HTCAPTION => Some(IDC_SIZEALL),
                x if x == HTTOPLEFT || x == HTBOTTOMRIGHT => Some(IDC_SIZENWSE),
                x if x == HTTOPRIGHT || x == HTBOTTOMLEFT => Some(IDC_SIZENESW),
                x if x == HTLEFT || x == HTRIGHT => Some(IDC_SIZEWE),
                x if x == HTTOP || x == HTBOTTOM => Some(IDC_SIZENS),
                _ => None,
            };

            if let Some(id) = cursor_id {
                if let Ok(cur) = LoadCursorW(None, id) {
                    let _ = SetCursor(Some(cur));
                }
                return LRESULT(1);
            }
        }

        // Handle size changes - update the hollow region
        if msg == WM_SIZE {
            let new_width = (lparam.0 & 0xFFFF) as i32;
            let new_height = ((lparam.0 >> 16) & 0xFFFF) as i32;

            if new_width > 0 && new_height > 0 {
                let border = BORDER_WIDTH.get();
                Self::apply_hollow_region(hwnd, new_width, new_height, border);
                let _ = InvalidateRect(Some(hwnd), None, true);
            }
        }

        // Handle ESC key to stop capture
        if msg == WM_KEYDOWN {
            let vk = wparam.0 as u32;
            if vk == VK_ESCAPE {
                info!("ESC pressed in hollow border - signaling stop");
                ESC_PRESSED.store(true, Ordering::SeqCst);
                return LRESULT(0);
            }
        }

        DefSubclassProc(hwnd, msg, wparam, lparam)
    }

    /// Show the hollow border
    pub fn show(&self) {
        #[cfg(windows)]
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            self.is_visible.store(true, Ordering::SeqCst);
        }
    }

    /// Hide the hollow border
    pub fn hide(&self) {
        #[cfg(windows)]
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
            self.is_visible.store(false, Ordering::SeqCst);
        }
    }

    /// Get the current position and size
    pub fn get_rect(&self) -> (i32, i32, i32, i32) {
        #[cfg(windows)]
        unsafe {
            let mut rect = RECT::default();
            let _ = GetWindowRect(self.hwnd, &mut rect);
            (rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top)
        }
        #[cfg(not(windows))]
        (0, 0, 800, 600)
    }

    /// Set the border color (BGR format)
    pub fn set_border_color(&self, color: u32) {
        BORDER_COLOR.set(color);
        #[cfg(windows)]
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, true);
        }
    }

    /// Set the border width
    pub fn set_border_width(&self, width: i32) {
        BORDER_WIDTH.set(width);
        #[cfg(windows)]
        unsafe {
            let (_, _, w, h) = self.get_rect();
            Self::apply_hollow_region(self.hwnd, w, h, width);
            let _ = InvalidateRect(Some(self.hwnd), None, true);
        }
    }

    /// Check if visible
    pub fn is_visible(&self) -> bool {
        self.is_visible.load(Ordering::SeqCst)
    }

    /// Get the HWND
    #[cfg(windows)]
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }
}

impl Drop for HollowBorder {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            let _ = RemoveWindowSubclass(self.hwnd, Some(Self::subclass_proc), 1);
            let _ = DestroyWindow(self.hwnd);
            info!("Hollow border window destroyed");
        }
    }
}

/// Convert a Rust string to a null-terminated wide string
fn wide_string(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
