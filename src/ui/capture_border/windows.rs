// ui/capture_border/windows.rs - Windows Win32 Implementation
//
// Uses UpdateLayeredWindow API for per-pixel alpha transparency.
// WM_NCHITTEST for native resize/move handling.

use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;
use winit::event_loop::EventLoopProxy;
use crate::UserEvent;
use log::info;

use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM, RECT, POINT, COLORREF},
    Graphics::Gdi::*,
    UI::WindowsAndMessaging::*,
    UI::Shell::{DefSubclassProc, SetWindowSubclass},
};

use super::{BorderStyle, BorderColors, CaptureBorder};

/// Global HWND storage for the capture border window
static CAPTURE_BORDER_HWND: AtomicIsize = AtomicIsize::new(0);

/// Global event proxy storage (needed for subclass callback)
static EVENT_PROXY: Mutex<Option<EventLoopProxy<UserEvent>>> = Mutex::new(None);

/// Global style storage
static BORDER_STYLE: Mutex<BorderStyle> = Mutex::new(BorderStyle {
    border_width: 5,
    corner_size: 30,
    corner_thickness: 8,
    show_rec_indicator: true,
});

/// Global colors storage
static BORDER_COLORS: Mutex<BorderColors> = Mutex::new(BorderColors {
    border: 0xE0FF9A3C,
    corner: 0xFFFFFFFF,
    transparent: 0x00000000,
    rec_red: 0xB0FF4040,
    rec_bg: 0x80181818,
    rec_text: 0xD0FFFFFF,
});

/// Capture border window that uses Win32 layered window for true transparency
pub struct CaptureBorderWindow {
    hwnd: isize,
    event_proxy: Option<EventLoopProxy<UserEvent>>,
    style: BorderStyle,
    colors: BorderColors,
}

impl CaptureBorderWindow {
    /// Initialize from an existing winit window
    pub fn from_winit_window(window: &winit::window::Window) -> Option<Self> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        
        let handle = window.window_handle().ok()?;
        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            let hwnd = win32_handle.hwnd.get() as isize;
            
            unsafe {
                Self::setup_layered_window(HWND(hwnd as *mut _));
            }
            
            CAPTURE_BORDER_HWND.store(hwnd, Ordering::SeqCst);
            
            Some(Self {
                hwnd,
                event_proxy: None,
                style: BorderStyle::default(),
                colors: BorderColors::default(),
            })
        } else {
            None
        }
    }
    
    /// Setup the window as a layered window with subclass for resize/move
    unsafe fn setup_layered_window(hwnd: HWND) {
        // Add layered and topmost styles
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let new_ex_style = ex_style
            | (WS_EX_LAYERED.0 as isize)
            | (WS_EX_TOPMOST.0 as isize)
            | (WS_EX_TOOLWINDOW.0 as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_ex_style);
        
        // Install subclass for hit-testing and resize handling
        let _ = SetWindowSubclass(hwnd, Some(Self::border_subclass_proc), 1, 0);
        
        // Initial draw
        let mut rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        Self::draw_border(hwnd, width, height);
        
        info!("Capture border window setup complete with layered style");
    }
    
    /// Draw the border using UpdateLayeredWindow
    unsafe fn draw_border(hwnd: HWND, width: i32, height: i32) {
        if width <= 0 || height <= 0 {
            return;
        }
        
        let screen_dc = GetDC(None);
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        
        // Create 32-bit ARGB bitmap
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // Top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [RGBQUAD::default()],
        };
        
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let bitmap = CreateDIBSection(
            Some(mem_dc),
            &bmi,
            DIB_RGB_COLORS,
            &mut bits,
            None,
            0,
        ).unwrap_or(HBITMAP(std::ptr::null_mut()));
        
        if bitmap.is_invalid() || bits.is_null() {
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);
            return;
        }
        
        let old_bitmap = SelectObject(mem_dc, bitmap.into());
        
        // Render border pixels
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (width * height) as usize);
        Self::render_border_pixels(pixels, width, height);
        
        // Update the layered window
        let blend = BLENDFUNCTION {
            BlendOp: 0, // AC_SRC_OVER
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: 1, // AC_SRC_ALPHA
        };
        
        let size_struct = windows::Win32::Foundation::SIZE { cx: width, cy: height };
        let point_src = POINT { x: 0, y: 0 };
        
        let _ = UpdateLayeredWindow(
            hwnd,
            Some(screen_dc),
            None,
            Some(&size_struct),
            Some(mem_dc),
            Some(&point_src),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );
        
        // Cleanup
        SelectObject(mem_dc, old_bitmap);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, screen_dc);
    }
    
    /// Render border pixels to buffer
    fn render_border_pixels(pixels: &mut [u32], width: i32, height: i32) {
        // Get style and colors from global storage
        let style = BORDER_STYLE.lock().map(|s| s.clone()).unwrap_or_default();
        let colors = BORDER_COLORS.lock().map(|c| c.clone()).unwrap_or_default();
        
        let border_width = style.border_width;
        let corner_size = style.corner_size;
        let corner_thickness = style.corner_thickness;
        
        // REC indicator - FIXED sizes (no scaling)
        let rec_circle_radius = 8;
        let rec_bg_height = 26;
        let rec_bg_width = 72;
        let rec_bg_x = width - rec_bg_width - 15;
        let rec_bg_y = 12;
        let rec_bg_radius = rec_bg_height / 2; // Rounded corners
        let rec_circle_x = rec_bg_x + 15;
        let rec_circle_y = rec_bg_y + rec_bg_height / 2;
        
        // Text - fixed 2x scale
        let text_scale = 2;
        let text_x = rec_circle_x + rec_circle_radius + 8;
        let text_y = rec_circle_y - (7 * text_scale) / 2;
        
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                
                // Check if on border edge
                let on_border = x < border_width
                    || x >= width - border_width
                    || y < border_width
                    || y >= height - border_width;
                
                // Corner markers (L-shaped, thicker)
                let in_top_left = (x < corner_size && y < corner_thickness)
                    || (y < corner_size && x < corner_thickness);
                let in_top_right = (x >= width - corner_size && y < corner_thickness)
                    || (y < corner_size && x >= width - corner_thickness);
                let in_bottom_left = (x < corner_size && y >= height - corner_thickness)
                    || (y >= height - corner_size && x < corner_thickness);
                let in_bottom_right = (x >= width - corner_size && y >= height - corner_thickness)
                    || (y >= height - corner_size && x >= width - corner_thickness);
                
                let in_corner = in_top_left || in_top_right || in_bottom_left || in_bottom_right;
                
                // REC indicator (only if enabled and window is large enough)
                let show_rec = style.show_rec_indicator && width > 150 && height > 80;
                
                let (in_rec_bg, in_rec_circle, in_rec_text) = if show_rec {
                    let in_bg = Self::is_in_rounded_rect(
                        x, y, rec_bg_x, rec_bg_y, rec_bg_width, rec_bg_height, rec_bg_radius
                    );
                    
                    let dx = x - rec_circle_x;
                    let dy = y - rec_circle_y;
                    let in_circle = (dx * dx + dy * dy) <= (rec_circle_radius * rec_circle_radius);
                    
                    let in_text = Self::is_rec_text_pixel_scaled(
                        x - text_x, y - text_y, text_scale
                    );
                    
                    (in_bg, in_circle, in_text)
                } else {
                    (false, false, false)
                };
                
                pixels[idx] = if in_rec_circle {
                    colors.rec_red
                } else if in_rec_text && in_rec_bg {
                    colors.rec_text
                } else if in_rec_bg {
                    colors.rec_bg
                } else if in_corner {
                    colors.corner
                } else if on_border {
                    colors.border
                } else {
                    colors.transparent
                };
            }
        }
    }
    
    /// Check if point is inside a rounded rectangle
    fn is_in_rounded_rect(x: i32, y: i32, rx: i32, ry: i32, rw: i32, rh: i32, radius: i32) -> bool {
        if x < rx || x >= rx + rw || y < ry || y >= ry + rh {
            return false;
        }
        
        // Check corners
        let corners = [
            (rx + radius, ry + radius),           // top-left
            (rx + rw - radius, ry + radius),      // top-right
            (rx + radius, ry + rh - radius),      // bottom-left
            (rx + rw - radius, ry + rh - radius), // bottom-right
        ];
        
        // Top-left corner
        if x < rx + radius && y < ry + radius {
            let dx = x - corners[0].0;
            let dy = y - corners[0].1;
            return dx * dx + dy * dy <= radius * radius;
        }
        // Top-right corner
        if x >= rx + rw - radius && y < ry + radius {
            let dx = x - corners[1].0;
            let dy = y - corners[1].1;
            return dx * dx + dy * dy <= radius * radius;
        }
        // Bottom-left corner
        if x < rx + radius && y >= ry + rh - radius {
            let dx = x - corners[2].0;
            let dy = y - corners[2].1;
            return dx * dx + dy * dy <= radius * radius;
        }
        // Bottom-right corner
        if x >= rx + rw - radius && y >= ry + rh - radius {
            let dx = x - corners[3].0;
            let dy = y - corners[3].1;
            return dx * dx + dy * dy <= radius * radius;
        }
        
        true
    }
    
    /// Check if pixel is part of "REC" text with scaling
    fn is_rec_text_pixel_scaled(x: i32, y: i32, scale: i32) -> bool {
        let base_x = x / scale;
        let base_y = y / scale;
        Self::is_rec_text_pixel(base_x, base_y)
    }
    
    /// Check if pixel is part of "REC" text (bitmap font - base 5x7 per letter)
    fn is_rec_text_pixel(x: i32, y: i32) -> bool {
        // Simple 3-letter "REC" bitmap (5x7 each letter, 2px spacing)
        // Total: 5+2+5+2+5 = 19 width, 7 height
        const R: [[u8; 5]; 7] = [
            [1,1,1,1,0],
            [1,0,0,0,1],
            [1,0,0,0,1],
            [1,1,1,1,0],
            [1,0,1,0,0],
            [1,0,0,1,0],
            [1,0,0,0,1],
        ];
        const E: [[u8; 5]; 7] = [
            [1,1,1,1,1],
            [1,0,0,0,0],
            [1,0,0,0,0],
            [1,1,1,1,0],
            [1,0,0,0,0],
            [1,0,0,0,0],
            [1,1,1,1,1],
        ];
        const C: [[u8; 5]; 7] = [
            [0,1,1,1,1],
            [1,0,0,0,0],
            [1,0,0,0,0],
            [1,0,0,0,0],
            [1,0,0,0,0],
            [1,0,0,0,0],
            [0,1,1,1,1],
        ];
        
        if y < 0 || y >= 7 || x < 0 {
            return false;
        }
        
        let y_idx = y as usize;
        
        // R: x 0-4
        if x < 5 {
            return R[y_idx][x as usize] == 1;
        }
        // space: x 5-6
        if x < 7 {
            return false;
        }
        // E: x 7-11
        if x < 12 {
            return E[y_idx][(x - 7) as usize] == 1;
        }
        // space: x 12-13
        if x < 14 {
            return false;
        }
        // C: x 14-18
        if x < 19 {
            return C[y_idx][(x - 14) as usize] == 1;
        }
        
        false
    }
    
    /// Send resize event through global event proxy
    fn send_resize_event(hwnd: HWND) {
        unsafe {
            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_ok() {
                let x = rect.left;
                let y = rect.top;
                let width = (rect.right - rect.left) as u32;
                let height = (rect.bottom - rect.top) as u32;
                
                if let Ok(guard) = EVENT_PROXY.lock() {
                    if let Some(proxy) = guard.as_ref() {
                        let _ = proxy.send_event(UserEvent::BorderResized { x, y, width, height });
                    }
                }
            }
        }
    }
    
    /// Subclass proc for hit-testing and cursor changes
    unsafe extern "system" fn border_subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _uidsubclass: usize,
        _dwrefdata: usize,
    ) -> LRESULT {
        // Handle resize - redraw the border and notify main app
        if msg == WM_SIZE {
            let new_width = (lparam.0 & 0xFFFF) as i32;
            let new_height = ((lparam.0 >> 16) & 0xFFFF) as i32;
            if new_width > 0 && new_height > 0 {
                Self::draw_border(hwnd, new_width, new_height);
                Self::send_resize_event(hwnd);
            }
        }
        
        // Handle move - notify main app
        if msg == WM_MOVE {
            Self::send_resize_event(hwnd);
        }
        
        // Handle exit sizing/moving - final notification
        if msg == WM_EXITSIZEMOVE {
            Self::send_resize_event(hwnd);
        }
        
        // Handle cursor changes
        if msg == WM_SETCURSOR {
            let hit_test = (lparam.0 & 0xFFFF) as u16 as u32;
            
            let cursor_id = match hit_test {
                x if x == HTTOPLEFT as u32 || x == HTBOTTOMRIGHT as u32 => Some(IDC_SIZENWSE),
                x if x == HTTOPRIGHT as u32 || x == HTBOTTOMLEFT as u32 => Some(IDC_SIZENESW),
                x if x == HTLEFT as u32 || x == HTRIGHT as u32 => Some(IDC_SIZEWE),
                x if x == HTTOP as u32 || x == HTBOTTOM as u32 => Some(IDC_SIZENS),
                x if x == HTCAPTION as u32 => Some(IDC_SIZEALL),
                _ => Some(IDC_ARROW),
            };
            
            if let Some(id) = cursor_id {
                if let Ok(cur) = LoadCursorW(None, id) {
                    let _ = SetCursor(Some(cur));
                }
                return LRESULT(1);
            }
        }
        
        // Custom hit testing for frameless window
        if msg == WM_NCHITTEST {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            
            let mut rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rect);
            
            let border = 8; // Resize border size
            
            let on_left = x >= rect.left && x < rect.left + border;
            let on_right = x >= rect.right - border && x < rect.right;
            let on_top = y >= rect.top && y < rect.top + border;
            let on_bottom = y >= rect.bottom - border && y < rect.bottom;
            
            // Return hit test result
            if on_top && on_left {
                return LRESULT(HTTOPLEFT as isize);
            } else if on_top && on_right {
                return LRESULT(HTTOPRIGHT as isize);
            } else if on_bottom && on_left {
                return LRESULT(HTBOTTOMLEFT as isize);
            } else if on_bottom && on_right {
                return LRESULT(HTBOTTOMRIGHT as isize);
            } else if on_left {
                return LRESULT(HTLEFT as isize);
            } else if on_right {
                return LRESULT(HTRIGHT as isize);
            } else if on_top {
                // Top edge = caption for moving
                return LRESULT(HTCAPTION as isize);
            } else if on_bottom {
                return LRESULT(HTBOTTOM as isize);
            }
            
            // Center is transparent to mouse (click-through)
            return LRESULT(HTTRANSPARENT as isize);
        }
        
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}

impl CaptureBorder for CaptureBorderWindow {
    fn set_event_proxy(&mut self, proxy: EventLoopProxy<UserEvent>) {
        self.event_proxy = Some(proxy.clone());
        if let Ok(mut guard) = EVENT_PROXY.lock() {
            *guard = Some(proxy);
        }
    }
    
    fn redraw(&self) {
        unsafe {
            let hwnd = HWND(self.hwnd as *mut _);
            let mut rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rect);
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            Self::draw_border(hwnd, width, height);
        }
    }
    
    fn native_handle(&self) -> isize {
        self.hwnd
    }
    
    fn set_style(&mut self, style: BorderStyle) {
        self.style = style.clone();
        if let Ok(mut guard) = BORDER_STYLE.lock() {
            *guard = style;
        }
        self.redraw();
    }
    
    fn set_colors(&mut self, colors: BorderColors) {
        self.colors = colors.clone();
        if let Ok(mut guard) = BORDER_COLORS.lock() {
            *guard = colors;
        }
        self.redraw();
    }
}

// SAFETY: CaptureBorderWindow is only accessed from the main thread
unsafe impl Send for CaptureBorderWindow {}
