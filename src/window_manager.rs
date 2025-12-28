// window_manager.rs - Window Management for Overlay and Destination Windows
//
// This module manages two types of windows:
// 1. OverlayWindow: A transparent, frameless window for region selection
// 2. DestinationWindow: A normal window that displays the captured content
//
// OVERLAY WINDOW REQUIREMENTS:
// - Transparent background
// - Frameless (no title bar, no borders)
// - Always on top
// - Resizable
// - Shows a border outline so user can see the selection region
//
// DESTINATION WINDOW REQUIREMENTS:
// - Normal window with title bar
// - Displays captured content
// - Can be shared on Teams/Zoom/Discord

use anyhow::{Context, Result};
use log::info;
use std::cell::Cell;
use std::sync::Arc;
use winit::{
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event_loop::ActiveEventLoop,
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::{Window, WindowAttributes, WindowId, WindowLevel},
};

use crate::bitmap_font;
use crate::capture::CaptureRect;
use crate::constants::{colors, overlay, text_box};

#[cfg(windows)]
use windows::Win32::{
    Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{DeleteObject, GetDC, ReleaseDC},
    UI::WindowsAndMessaging::*,
};

// Thread-local storage for border width and overlay HWND
thread_local! {
    static BORDER_WIDTH: Cell<u32> = const { Cell::new(5) };
    static OVERLAY_HWND: Cell<isize> = const { Cell::new(0) };
}

/// Wrapper for the overlay (selector) window
pub struct OverlayWindow {
    window: Arc<Window>,
    border_width: Cell<u32>,
}

impl OverlayWindow {
    /// Create a new overlay window for region selection
    /// The window is semi-transparent with a visible border and help text
    pub fn new(event_loop: &ActiveEventLoop) -> Result<Self> {
        info!("Creating overlay window");

        // Configure window attributes for the overlay
        // Frameless transparent window - we'll draw our own border and content
        let attributes = WindowAttributes::default()
            .with_title("RustFrame Selection")
            .with_inner_size(LogicalSize::new(
                overlay::DEFAULT_WIDTH,
                overlay::DEFAULT_HEIGHT,
            ))
            .with_position(PhysicalPosition::new(200, 100))
            .with_min_inner_size(LogicalSize::new(overlay::MIN_WIDTH, overlay::MIN_HEIGHT))
            .with_resizable(true)
            .with_decorations(false)
            .with_transparent(true)
            .with_window_level(WindowLevel::AlwaysOnTop);

        let window = event_loop
            .create_window(attributes)
            .context("Failed to create overlay window")?;

        info!("Overlay window created with ID: {:?}", window.id());

        // Apply Windows-specific styling for layered window with alpha
        #[cfg(windows)]
        Self::apply_selection_mode_style(&window)?;

        // Draw the initial overlay content
        #[cfg(windows)]
        Self::draw_selection_overlay(&window)?;

        Ok(Self {
            window: Arc::new(window),
            border_width: Cell::new(5),
        })
    }

    /// Apply Windows-specific styling for selection mode
    /// Creates a layered window with semi-transparent content
    #[cfg(windows)]
    fn apply_selection_mode_style(window: &Window) -> Result<()> {
        use windows::Win32::UI::Shell::SetWindowSubclass;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_LAYERED, WS_EX_TOOLWINDOW,
            WS_EX_TOPMOST,
        };

        let handle = window
            .window_handle()
            .context("Failed to get window handle")?;

        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            unsafe {
                let hwnd = HWND(win32_handle.hwnd.get() as *mut std::ffi::c_void);

                // Store HWND for subclass to redraw on resize
                OVERLAY_HWND.with(|h| h.set(hwnd.0 as isize));

                // Add layered style for transparency
                let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                let new_ex_style = ex_style
                    | (WS_EX_LAYERED.0 as isize)
                    | (WS_EX_TOPMOST.0 as isize)
                    | (WS_EX_TOOLWINDOW.0 as isize);
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_ex_style);

                // Install subclass for hit-testing (drag/resize on frameless window)
                let _ = SetWindowSubclass(hwnd, Some(Self::selection_subclass_proc), 1, 0);

                info!("Applied layered window style for selection mode");
            }
        }

        Ok(())
    }

    /// Subclass proc for selection mode window - handles drag/resize on frameless window
    #[cfg(windows)]
    unsafe extern "system" fn selection_subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _uidsubclass: usize,
        _dwrefdata: usize,
    ) -> LRESULT {
        use windows::Win32::UI::Shell::DefSubclassProc;
        use windows::Win32::UI::WindowsAndMessaging::{
            LoadCursorW, SetCursor, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTLEFT,
            HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS,
            IDC_SIZENWSE, IDC_SIZEWE,
        };

        // Handle resize - redraw the overlay with new size
        if msg == WM_SIZE {
            // Get new window size from lparam
            let new_width = (lparam.0 & 0xFFFF) as i32;
            let new_height = ((lparam.0 >> 16) & 0xFFFF) as i32;

            if new_width > 0 && new_height > 0 {
                // Redraw the overlay with the new size
                Self::draw_selection_overlay_hwnd(hwnd, new_width, new_height);
            }
        }

        // Handle cursor changes based on hit test result
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

            // Return appropriate hit test result
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
            if on_left {
                return LRESULT(HTLEFT as isize);
            }
            if on_right {
                return LRESULT(HTRIGHT as isize);
            }
            if on_top {
                return LRESULT(HTTOP as isize);
            }
            if on_bottom {
                return LRESULT(HTBOTTOM as isize);
            }

            // Inside the window - treat as caption for dragging
            return LRESULT(HTCAPTION as isize);
        }

        DefSubclassProc(hwnd, msg, wparam, lparam)
    }

    /// Draw the selection overlay directly from HWND and size (used by subclass on resize)
    #[cfg(windows)]
    fn draw_selection_overlay_hwnd(hwnd: HWND, width: i32, height: i32) {
        use windows::Win32::Foundation::POINT;
        use windows::Win32::Graphics::Gdi::{
            CreateCompatibleDC, CreateDIBSection, DeleteDC, SelectObject, BITMAPINFO,
            BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        };
        use windows::Win32::UI::WindowsAndMessaging::{UpdateLayeredWindow, ULW_ALPHA};

        unsafe {
            if width <= 0 || height <= 0 {
                return;
            }

            // Get screen DC
            let screen_dc = GetDC(None);
            let mem_dc = CreateCompatibleDC(Some(screen_dc));

            // Create 32-bit ARGB bitmap for alpha blending
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height, // Top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let bitmap =
                match CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
                    Ok(b) => b,
                    Err(_) => {
                        let _ = DeleteDC(mem_dc);
                        let _ = ReleaseDC(None, screen_dc);
                        return;
                    }
                };
            let old_bitmap = SelectObject(mem_dc, bitmap.into());

            // Draw the overlay content to the bitmap
            let pixels =
                std::slice::from_raw_parts_mut(bits as *mut u32, (width * height) as usize);
            Self::render_overlay_pixels(pixels, width, height);

            // Update the layered window with our bitmap
            let blend = windows::Win32::Graphics::Gdi::BLENDFUNCTION {
                BlendOp: 0, // AC_SRC_OVER
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: 1, // AC_SRC_ALPHA
            };

            let size_struct = windows::Win32::Foundation::SIZE {
                cx: width,
                cy: height,
            };
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
    }

    /// Render the overlay content to a pixel buffer (shared by all overlay drawing methods)
    #[cfg(windows)]
    fn render_overlay_pixels(pixels: &mut [u32], width: i32, height: i32) {
        let border_width = overlay::BORDER_WIDTH;
        let corner_size = overlay::CORNER_SIZE;

        // Calculate text box dimensions (centered, clamped to window size)
        let tb_width = text_box::WIDTH.min(width - 20);
        let tb_height = text_box::HEIGHT.min(height - 20);
        let tb_left = (width - tb_width) / 2;
        let tb_top = (height - tb_height) / 2;
        let tb_right = tb_left + tb_width;
        let tb_bottom = tb_top + tb_height;
        let tb_border = text_box::BORDER_WIDTH;

        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;

                let on_border = x < border_width
                    || x >= width - border_width
                    || y < border_width
                    || y >= height - border_width;

                // Corner markers (L-shaped)
                let in_top_left = (x < corner_size && y < border_width * 2)
                    || (y < corner_size && x < border_width * 2);
                let in_top_right = (x >= width - corner_size && y < border_width * 2)
                    || (y < corner_size && x >= width - border_width * 2);
                let in_bottom_left = (x < corner_size && y >= height - border_width * 2)
                    || (y >= height - corner_size && x < border_width * 2);
                let in_bottom_right = (x >= width - corner_size && y >= height - border_width * 2)
                    || (y >= height - corner_size && x >= width - border_width * 2);

                let in_corner = in_top_left || in_top_right || in_bottom_left || in_bottom_right;

                // Check if in text box area
                let in_text_box = x >= tb_left && x < tb_right && y >= tb_top && y < tb_bottom;

                // Text box border
                let on_text_box_border = in_text_box
                    && (x < tb_left + tb_border
                        || x >= tb_right - tb_border
                        || y < tb_top + tb_border
                        || y >= tb_bottom - tb_border);

                pixels[idx] = if in_corner {
                    colors::CORNER
                } else if on_border {
                    colors::BORDER
                } else if on_text_box_border {
                    colors::TEXT_BORDER
                } else if in_text_box {
                    colors::TEXT_BG
                } else {
                    colors::FILL
                };
            }
        }

        // Draw help text using the bitmap font module
        bitmap_font::draw_help_text(pixels, width, height);
    }

    /// Draw the selection overlay with semi-transparent background, border, and help text
    #[cfg(windows)]
    fn draw_selection_overlay(window: &Window) -> Result<()> {
        let handle = window
            .window_handle()
            .context("Failed to get window handle")?;

        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            let hwnd = HWND(win32_handle.hwnd.get() as *mut std::ffi::c_void);
            let size = window.inner_size();
            Self::draw_selection_overlay_hwnd(hwnd, size.width as i32, size.height as i32);
            info!("Drew selection overlay with help text");
        }

        Ok(())
    }

    /// Redraw the selection overlay (called on resize)
    #[cfg(windows)]
    pub fn redraw_selection_overlay(&self) -> Result<()> {
        Self::draw_selection_overlay(&self.window)
    }

    /// Get the window ID for event routing
    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    /// Request a redraw of the overlay window
    #[allow(dead_code)]
    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    /// Hide the overlay window (called when capture starts)
    pub fn hide(&self) {
        self.window.set_visible(false);
    }

    /// Show the overlay window
    pub fn show(&self) {
        self.window.set_visible(true);
    }

    /// Set the window title
    pub fn set_title(&self, title: &str) {
        self.window.set_title(title);
    }

    /// Get the overlay's outer position (for moving destination window)
    pub fn get_outer_position(&self) -> PhysicalPosition<i32> {
        self.window
            .outer_position()
            .unwrap_or(PhysicalPosition::new(0, 0))
    }

    /// Get the overlay's inner size
    pub fn get_inner_size(&self) -> PhysicalSize<u32> {
        self.window.inner_size()
    }

    /// Get the capture rectangle (in screen coordinates)
    /// This represents the region we want to capture
    pub fn get_capture_rect(&self) -> CaptureRect {
        // Get the window's position and size in physical pixels
        // Physical pixels are actual screen pixels (important for HiDPI displays)
        let position = self
            .window
            .outer_position()
            .unwrap_or(PhysicalPosition::new(0, 0));
        let size = self.window.inner_size();

        CaptureRect {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        }
    }

    /// Get the capture rectangle INSIDE the border (excludes border from capture)
    /// This is used when border is visible to avoid capturing the border itself
    pub fn get_capture_rect_inner(&self, border_width: u32) -> CaptureRect {
        let position = self
            .window
            .outer_position()
            .unwrap_or(PhysicalPosition::new(0, 0));
        let size = self.window.inner_size();
        let border = border_width as i32;

        // Offset position by border width and reduce size by 2*border
        CaptureRect {
            x: position.x + border,
            y: position.y + border,
            width: size.width.saturating_sub(border_width * 2),
            height: size.height.saturating_sub(border_width * 2),
        }
    }

    /// Get a reference to the underlying winit window
    #[allow(dead_code)]
    pub fn get_window(&self) -> &Arc<Window> {
        &self.window
    }

    /// Move the window by a delta (for drag functionality)
    /// This is used for implementing click-and-drag movement of the overlay
    pub fn move_by(&self, delta_x: i32, delta_y: i32) {
        if let Ok(current_pos) = self.window.outer_position() {
            let new_x = current_pos.x + delta_x;
            let new_y = current_pos.y + delta_y;

            self.window
                .set_outer_position(PhysicalPosition::new(new_x, new_y));
        }
    }

    /// Convert the overlay to a hollow frame (only border visible, interior click-through)
    /// Uses SetWindowRgn for the visual appearance and subclass for hit testing
    #[cfg(windows)]
    pub fn make_hollow_frame(&self, border_width: u32) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::Graphics::Gdi::{CombineRgn, CreateRectRgn, SetWindowRgn, RGN_DIFF};
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE, HWND_TOPMOST,
            SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WS_EX_LAYERED,
            WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
        };

        self.border_width.set(border_width);
        BORDER_WIDTH.with(|b| b.set(border_width));

        let handle = match self.window.window_handle() {
            Ok(h) => h,
            Err(_) => return,
        };

        let size = self.window.inner_size();
        let width = size.width as i32;
        let height = size.height as i32;
        let border = border_width as i32;

        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            unsafe {
                let hwnd = HWND(win32_handle.hwnd.get() as *mut std::ffi::c_void);

                // Remove title bar AND thick frame - use only popup for clean look
                // WS_THICKFRAME causes the ugly blue border, so we remove it
                let new_style = WS_POPUP | WS_VISIBLE;
                SetWindowLongPtrW(hwnd, GWL_STYLE, new_style.0 as isize);

                // Add layered and toolwindow style
                let ex_style = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED;
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style.0 as isize);

                // Set a color key for transparency (we'll use a specific color for the hole)
                // Actually we'll use region approach but with extended resize area

                // Create outer rectangle including resize margin
                let resize_margin = 8i32; // Extra margin for resize handles
                let outer_rgn = CreateRectRgn(
                    -resize_margin,
                    -resize_margin,
                    width + resize_margin,
                    height + resize_margin,
                );

                // Create inner rectangle (the hole) - but leave border visible
                let inner_rgn = CreateRectRgn(border, border, width - border, height - border);

                // Subtract inner from outer to create hollow frame with resize margins
                let _ = CombineRgn(Some(outer_rgn), Some(outer_rgn), Some(inner_rgn), RGN_DIFF);

                // Apply the region
                SetWindowRgn(hwnd, Some(outer_rgn), true);

                // Install window subclass for custom hit testing
                Self::install_subclass(hwnd, border_width);

                // Force window to update
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );

                info!(
                    "Overlay converted to hollow frame (border: {}px)",
                    border_width
                );
            }
        }
    }

    /// Install a window subclass for custom WM_NCHITTEST handling
    #[cfg(windows)]
    unsafe fn install_subclass(hwnd: HWND, _border_width: u32) {
        use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};

        // Subclass procedure for custom hit testing
        unsafe extern "system" fn subclass_proc(
            hwnd: HWND,
            msg: u32,
            wparam: WPARAM,
            lparam: LPARAM,
            _uidsubclass: usize,
            _dwrefdata: usize,
        ) -> LRESULT {
            // Handle cursor changes
            if msg == WM_SETCURSOR {
                use windows::Win32::UI::WindowsAndMessaging::{
                    LoadCursorW, SetCursor, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION,
                    HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, IDC_SIZEALL, IDC_SIZENESW,
                    IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE,
                };

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
                    return LRESULT(1); // Cursor handled
                }
                return DefSubclassProc(hwnd, msg, wparam, lparam);
            }

            if msg == WM_NCHITTEST {
                // Get cursor position
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

                // Get window rect
                let mut rect = RECT::default();
                let _ = GetWindowRect(hwnd, &mut rect);

                let border = BORDER_WIDTH.with(|b| b.get()) as i32;
                let resize_margin = border.max(8); // At least 8px for resize

                // Check if on edges for resize
                let on_left = x >= rect.left - 4 && x < rect.left + resize_margin;
                let on_right = x >= rect.right - resize_margin && x < rect.right + 4;
                let on_top = y >= rect.top - 4 && y < rect.top + resize_margin;
                let on_bottom = y >= rect.bottom - resize_margin && y < rect.bottom + 4;

                // Determine hit test result
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
                    // Top center = caption (for dragging)
                    let center_start = rect.left + (rect.right - rect.left) / 3;
                    let center_end = rect.right - (rect.right - rect.left) / 3;
                    if x >= center_start && x <= center_end {
                        return LRESULT(HTCAPTION as isize);
                    }
                    return LRESULT(HTTOP as isize);
                } else if on_bottom {
                    return LRESULT(HTBOTTOM as isize);
                }

                // Check if in border area (for dragging)
                let in_border = (x >= rect.left && x < rect.left + border)
                    || (x >= rect.right - border && x < rect.right)
                    || (y >= rect.top && y < rect.top + border)
                    || (y >= rect.bottom - border && y < rect.bottom);

                if in_border {
                    return LRESULT(HTCAPTION as isize); // Allow dragging from border
                }

                // Interior is transparent (click-through)
                return LRESULT(HTTRANSPARENT as isize);
            }

            DefSubclassProc(hwnd, msg, wparam, lparam)
        }

        // Install the subclass
        let _ = SetWindowSubclass(hwnd, Some(subclass_proc), 1, 0);
    }

    /// Update the hollow frame region after resize
    #[cfg(windows)]
    pub fn update_hollow_frame(&self, border_width: u32) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::Graphics::Gdi::{CombineRgn, CreateRectRgn, SetWindowRgn, RGN_DIFF};

        self.border_width.set(border_width);
        BORDER_WIDTH.with(|b| b.set(border_width));

        let handle = match self.window.window_handle() {
            Ok(h) => h,
            Err(_) => return,
        };

        let size = self.window.inner_size();
        let width = size.width as i32;
        let height = size.height as i32;
        let border = border_width as i32;

        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            unsafe {
                let hwnd = HWND(win32_handle.hwnd.get() as *mut std::ffi::c_void);

                // Create outer rectangle with resize margin
                let resize_margin = 8i32;
                let outer_rgn = CreateRectRgn(
                    -resize_margin,
                    -resize_margin,
                    width + resize_margin,
                    height + resize_margin,
                );

                // Create inner rectangle (the hole)
                let inner_rgn = CreateRectRgn(border, border, width - border, height - border);

                // Subtract inner from outer
                let _ = CombineRgn(Some(outer_rgn), Some(outer_rgn), Some(inner_rgn), RGN_DIFF);

                // Apply the updated region
                SetWindowRgn(hwnd, Some(outer_rgn), true);
            }
        }
    }

    #[cfg(not(windows))]
    pub fn update_hollow_frame(&self, _border_width: u32) {}

    #[cfg(not(windows))]
    pub fn make_hollow_frame(&self, _border_width: u32) {
        info!("Hollow frame not supported on this platform");
    }
}

/// Wrapper for the destination (mirror/display) window
pub struct DestinationWindow {
    window: Arc<Window>,
}

impl DestinationWindow {
    /// Create a new destination window for displaying captured content
    /// dev_mode: if true, window has title bar and is visible (for debugging)
    /// dev_mode: if false, window is hidden and frameless until capture starts
    pub fn new(event_loop: &ActiveEventLoop, dev_mode: bool) -> Result<Self> {
        // Dev mode: show window with decorations for debugging
        // Production mode: hidden and frameless until capture starts
        let (initial_visible, initial_position, with_decorations) = if dev_mode {
            (true, PhysicalPosition::new(550, 100), true)
        } else {
            (false, PhysicalPosition::new(100, 100), false)
        };

        info!(
            "Creating destination window (dev_mode={}, initially {})",
            dev_mode,
            if initial_visible { "visible" } else { "hidden" }
        );

        // Configure window attributes for the destination
        let attributes = WindowAttributes::default()
            .with_title("RustFrame - Screen Share This Window")
            .with_inner_size(LogicalSize::new(400, 300)) // Will be resized to match overlay
            .with_position(initial_position)
            .with_resizable(with_decorations) // Resizable only in dev mode
            .with_decorations(with_decorations) // Title bar only in dev mode
            .with_transparent(false) // Opaque background
            .with_visible(initial_visible);

        let window = event_loop
            .create_window(attributes)
            .context("Failed to create destination window")?;

        info!("Destination window created with ID: {:?}", window.id());

        // Note: exclude_from_capture is now called separately based on settings
        // This allows Google Meet window sharing to work when disabled

        Ok(Self {
            window: Arc::new(window),
        })
    }

    /// Exclude/include the window from screen capture using SetWindowDisplayAffinity
    /// When excluded: Prevents infinite mirror, but Google Meet "window share" shows black
    /// When included: Google Meet "window share" works, but may cause infinite mirror if overlapping
    #[cfg(windows)]
    #[allow(dead_code)]
    pub fn set_exclude_from_capture(&self, exclude: bool) -> Result<()> {
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
        };

        let handle = self
            .window
            .window_handle()
            .context("Failed to get window handle")?;

        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            unsafe {
                let hwnd = HWND(win32_handle.hwnd.get() as *mut std::ffi::c_void);

                let affinity = if exclude {
                    WDA_EXCLUDEFROMCAPTURE
                } else {
                    WDA_NONE
                };

                SetWindowDisplayAffinity(hwnd, affinity)
                    .context("Failed to set window display affinity")?;

                info!("Destination window exclude_from_capture: {}", exclude);
            }
        }

        Ok(())
    }

    #[cfg(not(windows))]
    pub fn set_exclude_from_capture(&self, _exclude: bool) -> Result<()> {
        Ok(())
    }

    /// Show the window and move it to the specified position
    #[allow(dead_code)]
    pub fn show_at(&self, position: PhysicalPosition<i32>, size: PhysicalSize<u32>) {
        // First resize to match the overlay's inner size
        let _ = self.window.request_inner_size(size);
        // Then move to the overlay's position
        self.window.set_outer_position(position);
        // Finally show the window
        self.window.set_visible(true);
        info!(
            "Destination window shown at {:?} with size {:?}",
            position, size
        );
    }

    /// Make the window frameless with an optional colored border
    #[cfg(windows)]
    #[allow(dead_code)]
    pub fn make_frameless(&self, show_border: bool, border_width: u32) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::*;

        let handle = match self.window.window_handle() {
            Ok(h) => h,
            Err(_) => return,
        };

        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            unsafe {
                let hwnd = HWND(win32_handle.hwnd.get() as *mut std::ffi::c_void);

                // Remove title bar and frame
                let style = GetWindowLongW(hwnd, GWL_STYLE);
                let new_style = style & !(WS_CAPTION.0 as i32 | WS_THICKFRAME.0 as i32);
                SetWindowLongW(hwnd, GWL_STYLE, new_style);

                if show_border {
                    // Add a thin border using WS_EX_CLIENTEDGE or WS_EX_STATICEDGE
                    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                    SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_DLGMODALFRAME.0 as i32);
                    info!(
                        "Window made frameless with border (width hint: {})",
                        border_width
                    );
                } else {
                    info!("Window made completely frameless");
                }

                // Force window to redraw with new styles
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
                );
            }
        }
    }

    #[cfg(not(windows))]
    #[allow(dead_code)]
    pub fn make_frameless(&self, _show_border: bool, _border_width: u32) {
        // Non-Windows: just remove decorations via winit
        // Note: winit doesn't support changing decorations after creation on all platforms
        info!("make_frameless not fully supported on this platform");
    }

    /// Get the window ID for event routing
    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    /// Set the window title
    pub fn set_title(&self, title: &str) {
        self.window.set_title(title);
    }

    /// Make the window click-through (transparent to mouse events)
    /// and always on top so it doesn't disappear behind other windows
    #[cfg(windows)]
    #[allow(dead_code)]
    pub fn make_click_through_and_topmost(&self) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::*;

        let handle = match self.window.window_handle() {
            Ok(h) => h,
            Err(_) => return,
        };

        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            unsafe {
                let hwnd = HWND(win32_handle.hwnd.get() as *mut std::ffi::c_void);

                // Add WS_EX_TRANSPARENT for click-through
                // Add WS_EX_TOPMOST to stay on top
                // Keep WS_EX_LAYERED for proper transparency
                let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                SetWindowLongW(
                    hwnd,
                    GWL_EXSTYLE,
                    ex_style | WS_EX_TRANSPARENT.0 as i32 | WS_EX_LAYERED.0 as i32,
                );

                // Use SetWindowPos to make it TOPMOST
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );

                info!("Window set to click-through and always on top");
            }
        }
    }

    #[cfg(not(windows))]
    #[allow(dead_code)]
    pub fn make_click_through_and_topmost(&self) {
        self.window.set_window_level(WindowLevel::AlwaysOnTop);
    }

    /// Request a redraw of the destination window
    #[allow(dead_code)]
    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    /// Resize the destination window
    pub fn resize(&self, size: PhysicalSize<u32>) {
        let _ = self.window.request_inner_size(size);
    }

    /// Get the window's current size (for renderer resize)
    #[allow(dead_code)]
    pub fn get_size(&self) -> PhysicalSize<u32> {
        self.window.inner_size()
    }

    /// Get the window's current position
    #[allow(dead_code)]
    pub fn get_outer_position(&self) -> PhysicalPosition<i32> {
        self.window
            .outer_position()
            .unwrap_or(PhysicalPosition::new(0, 0))
    }

    /// Position destination window OFF-SCREEN (production mode)
    /// User won't see it, but Google Meet can still capture it
    /// This prevents infinite mirror since dest is outside capture region
    #[cfg(windows)]
    pub fn position_offscreen(&self, size: PhysicalSize<u32>) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::*;

        // Position far off-screen (left side, way outside visible area)
        // Google Meet can still capture the window content
        let offscreen_position = PhysicalPosition::new(-9999, 100);

        let _ = self.window.request_inner_size(size);
        self.window.set_outer_position(offscreen_position);
        self.window.set_visible(true);

        // Make frameless and hide from taskbar
        let handle = match self.window.window_handle() {
            Ok(h) => h,
            Err(_) => return,
        };

        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            unsafe {
                let hwnd = HWND(win32_handle.hwnd.get() as *mut std::ffi::c_void);

                // Remove title bar (frameless)
                let style = GetWindowLongW(hwnd, GWL_STYLE);
                let new_style = style & !(WS_CAPTION.0 as i32 | WS_THICKFRAME.0 as i32);
                SetWindowLongW(hwnd, GWL_STYLE, new_style);

                // Add WS_EX_TOOLWINDOW to hide from taskbar
                let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_TOOLWINDOW.0 as i32);

                // Force window to update with new styles
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
                );
            }
        }

        info!("Destination window positioned OFF-SCREEN (frameless, hidden from taskbar) with size {:?}", size);
    }

    #[cfg(not(windows))]
    pub fn position_offscreen(&self, size: PhysicalSize<u32>) {
        // Fallback: position off-screen
        let offscreen_position = PhysicalPosition::new(-9999, 100);
        let _ = self.window.request_inner_size(size);
        self.window.set_outer_position(offscreen_position);
        self.window.set_visible(true);
    }

    /// Position destination window BESIDE the overlay (development mode)
    /// Also restores title bar if it was hidden in PROD mode
    #[cfg(windows)]
    pub fn position_beside_overlay(
        &self,
        overlay_position: PhysicalPosition<i32>,
        size: PhysicalSize<u32>,
    ) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::*;

        // Position to the right of overlay
        let dest_position = PhysicalPosition::new(
            overlay_position.x + size.width as i32 + 20,
            overlay_position.y,
        );
        let _ = self.window.request_inner_size(size);
        self.window.set_outer_position(dest_position);
        self.window.set_visible(true);

        // Restore title bar and normal window style (in case we're coming from PROD mode)
        let handle = match self.window.window_handle() {
            Ok(h) => h,
            Err(_) => return,
        };

        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            unsafe {
                let hwnd = HWND(win32_handle.hwnd.get() as *mut std::ffi::c_void);

                // Restore title bar and frame
                let style = GetWindowLongW(hwnd, GWL_STYLE);
                let new_style = style
                    | WS_CAPTION.0 as i32
                    | WS_THICKFRAME.0 as i32
                    | WS_SYSMENU.0 as i32
                    | WS_MINIMIZEBOX.0 as i32;
                SetWindowLongW(hwnd, GWL_STYLE, new_style);

                // Remove WS_EX_TOOLWINDOW to show in taskbar again
                let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                let new_ex_style = ex_style & !(WS_EX_TOOLWINDOW.0 as i32);
                SetWindowLongW(hwnd, GWL_EXSTYLE, new_ex_style);

                // Force window to recalculate frame and redraw completely
                // Use the current client size to recalc with new frame
                let _ = SetWindowPos(
                    hwnd,
                    Option::from(HWND_TOP),
                    dest_position.x,
                    dest_position.y,
                    size.width as i32,
                    size.height as i32,
                    SWP_FRAMECHANGED | SWP_SHOWWINDOW | SWP_DRAWFRAME,
                );
            }
        }

        info!(
            "Destination window positioned BESIDE overlay at {:?} (with title bar)",
            dest_position
        );
    }

    #[cfg(not(windows))]
    pub fn position_beside_overlay(
        &self,
        overlay_position: PhysicalPosition<i32>,
        size: PhysicalSize<u32>,
    ) {
        let dest_position = PhysicalPosition::new(
            overlay_position.x + size.width as i32 + 20,
            overlay_position.y,
        );
        let _ = self.window.request_inner_size(size);
        self.window.set_outer_position(dest_position);
        self.window.set_visible(true);
    }

    /// Get a reference to the underlying winit window
    pub fn get_window(&self) -> &Arc<Window> {
        &self.window
    }
}

// Note: For a production-quality overlay, you'd want to implement:
// 1. True transparency using Win32 APIs (SetLayeredWindowAttributes)
// 2. Frameless window with custom resize handles
// 3. Visual border outline (render a colored rectangle)
// 4. Mouse cursor changes when hovering over resize areas
// 5. Drag-to-move functionality
//
// Here's a sketch of how you'd do true transparency on Windows:
//
// use windows::Win32::UI::WindowsAndMessaging::*;
// use windows::Win32::Graphics::Gdi::*;
//
// unsafe {
//     let hwnd = HWND(window.hwnd() as isize);
//
//     // Set the window as layered (required for transparency)
//     let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
//     SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_LAYERED);
//
//     // Set transparency (0 = fully transparent, 255 = opaque)
//     SetLayeredWindowAttributes(hwnd, RGB(0, 0, 0), 128, LWA_ALPHA);
//
//     // Remove window decorations
//     let style = GetWindowLongW(hwnd, GWL_STYLE);
//     SetWindowLongW(hwnd, GWL_STYLE, style & !(WS_CAPTION | WS_THICKFRAME));
// }
//
// This would require adding raw-window-handle to access the HWND from winit.
