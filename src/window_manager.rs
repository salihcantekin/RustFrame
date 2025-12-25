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

use anyhow::{Result, Context};
use log::info;
use winit::{
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes, WindowId, WindowLevel},
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
};
use std::sync::Arc;
use std::cell::Cell;

use crate::capture::CaptureRect;

#[cfg(windows)]
use windows::Win32::{
    Foundation::{HWND, RECT, COLORREF, LRESULT, WPARAM, LPARAM},
    Graphics::Gdi::{GetDC, ReleaseDC, DeleteObject},
    UI::WindowsAndMessaging::*,
};

/// Helper function to create wide strings for Windows API
#[cfg(windows)]
fn wide_string(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

// Constants for WM_SETCURSOR and WM_SIZE
#[cfg(windows)]
const WM_SETCURSOR: u32 = 0x0020;
#[cfg(windows)]
const WM_SIZE: u32 = 0x0005;

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
            .with_inner_size(LogicalSize::new(800, 600)) // Initial size - larger for better visibility
            .with_position(PhysicalPosition::new(200, 100)) // Initial position
            .with_min_inner_size(LogicalSize::new(400, 300)) // Minimum size
            .with_resizable(true) // User can resize to select region
            .with_decorations(false) // No title bar - we draw everything
            .with_transparent(true) // Enable transparency
            .with_window_level(WindowLevel::AlwaysOnTop); // Stay on top

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
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowLongPtrW, GWL_EXSTYLE, GetWindowLongPtrW,
            WS_EX_LAYERED, WS_EX_TOPMOST, WS_EX_TOOLWINDOW,
        };
        use windows::Win32::UI::Shell::SetWindowSubclass;
        
        let handle = window.window_handle()
            .context("Failed to get window handle")?;

        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            unsafe {
                let hwnd = HWND(win32_handle.hwnd.get() as isize as *mut std::ffi::c_void);
                
                // Store HWND for subclass to redraw on resize
                OVERLAY_HWND.with(|h| h.set(hwnd.0 as isize));
                
                // Add layered style for transparency
                let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                let new_ex_style = ex_style | (WS_EX_LAYERED.0 as isize) | (WS_EX_TOPMOST.0 as isize) | (WS_EX_TOOLWINDOW.0 as isize);
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
            LoadCursorW, SetCursor, 
            IDC_SIZEALL, IDC_SIZENWSE, IDC_SIZENESW, IDC_SIZEWE, IDC_SIZENS,
            HTCAPTION, HTTOPLEFT, HTTOPRIGHT, HTBOTTOMLEFT, HTBOTTOMRIGHT,
            HTLEFT, HTRIGHT, HTTOP, HTBOTTOM, HTCLIENT,
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
                    let _ = SetCursor(cur);
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
            if on_top && on_left { return LRESULT(HTTOPLEFT as isize); }
            if on_top && on_right { return LRESULT(HTTOPRIGHT as isize); }
            if on_bottom && on_left { return LRESULT(HTBOTTOMLEFT as isize); }
            if on_bottom && on_right { return LRESULT(HTBOTTOMRIGHT as isize); }
            if on_left { return LRESULT(HTLEFT as isize); }
            if on_right { return LRESULT(HTRIGHT as isize); }
            if on_top { return LRESULT(HTTOP as isize); }
            if on_bottom { return LRESULT(HTBOTTOM as isize); }
            
            // Inside the window - treat as caption for dragging
            return LRESULT(HTCAPTION as isize);
        }
        
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
    
    /// Draw the selection overlay directly from HWND and size (used by subclass on resize)
    #[cfg(windows)]
    fn draw_selection_overlay_hwnd(hwnd: HWND, width: i32, height: i32) {
        use windows::Win32::Graphics::Gdi::{
            CreateCompatibleDC, SelectObject, DeleteDC,
            BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CreateDIBSection, DIB_RGB_COLORS,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            UpdateLayeredWindow, ULW_ALPHA,
        };
        use windows::Win32::Foundation::POINT;
        
        unsafe {
            if width <= 0 || height <= 0 { return; }
            
            // Get screen DC
            let screen_dc = GetDC(None);
            let mem_dc = CreateCompatibleDC(screen_dc);
            
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
            let bitmap = match CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(b) => b,
                Err(_) => {
                    let _ = DeleteDC(mem_dc);
                    let _ = ReleaseDC(None, screen_dc);
                    return;
                }
            };
            let old_bitmap = SelectObject(mem_dc, bitmap);
            
            // Draw directly to the bitmap memory
            let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (width * height) as usize);
            
            let border_width = 4;
            let corner_size = 20;
            
            // Colors (ARGB format - stored as BGRA in memory)
            let border_color: u32 = 0xFF00A8FF; // Bright blue, fully opaque
            let fill_color: u32 = 0x10000000;    // Almost fully transparent
            let corner_color: u32 = 0xFF00D4FF;  // Lighter blue for corners
            let text_bg_color: u32 = 0xF0181818; // Very dark gray, almost opaque
            let text_border_color: u32 = 0xFF00A8FF; // Blue border for text box
            
            // Text box dimensions (fixed size, centered)
            let text_box_width = 280.min(width - 20);
            let text_box_height = 260.min(height - 20);
            let text_box_left = (width - text_box_width) / 2;
            let text_box_top = (height - text_box_height) / 2;
            let text_box_right = text_box_left + text_box_width;
            let text_box_bottom = text_box_top + text_box_height;
            let text_box_border = 2;
            
            for y in 0..height {
                for x in 0..width {
                    let idx = (y * width + x) as usize;
                    
                    let on_border = x < border_width || x >= width - border_width ||
                                    y < border_width || y >= height - border_width;
                    
                    // Corner markers (L-shaped)
                    let in_top_left = (x < corner_size && y < border_width * 2) || 
                                     (y < corner_size && x < border_width * 2);
                    let in_top_right = (x >= width - corner_size && y < border_width * 2) || 
                                      (y < corner_size && x >= width - border_width * 2);
                    let in_bottom_left = (x < corner_size && y >= height - border_width * 2) || 
                                        (y >= height - corner_size && x < border_width * 2);
                    let in_bottom_right = (x >= width - corner_size && y >= height - border_width * 2) || 
                                         (y >= height - corner_size && x >= width - border_width * 2);
                    
                    let in_corner = in_top_left || in_top_right || in_bottom_left || in_bottom_right;
                    
                    // Check if in text box area
                    let in_text_box = x >= text_box_left && x < text_box_right &&
                                     y >= text_box_top && y < text_box_bottom;
                    
                    // Text box border
                    let on_text_box_border = in_text_box && (
                        x < text_box_left + text_box_border || 
                        x >= text_box_right - text_box_border ||
                        y < text_box_top + text_box_border || 
                        y >= text_box_bottom - text_box_border
                    );
                    
                    if in_corner {
                        pixels[idx] = corner_color;
                    } else if on_border {
                        pixels[idx] = border_color;
                    } else if on_text_box_border {
                        pixels[idx] = text_border_color;
                    } else if in_text_box {
                        pixels[idx] = text_bg_color;
                    } else {
                        pixels[idx] = fill_color;
                    }
                }
            }
            
            // Draw help text directly to pixels
            Self::draw_help_text_pixels(pixels, width, height);
            
            // Update the layered window with our bitmap
            let blend = windows::Win32::Graphics::Gdi::BLENDFUNCTION {
                BlendOp: 0, // AC_SRC_OVER
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: 1, // AC_SRC_ALPHA
            };
            
            let size_struct = windows::Win32::Foundation::SIZE { cx: width, cy: height };
            let point_src = POINT { x: 0, y: 0 };
            
            let _ = UpdateLayeredWindow(
                hwnd,
                screen_dc,
                None, // Use current position
                Some(&size_struct),
                mem_dc,
                Some(&point_src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            
            // Cleanup
            SelectObject(mem_dc, old_bitmap);
            let _ = DeleteObject(bitmap);
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);
        }
    }
    
    /// Draw the selection overlay with semi-transparent background, border, and help text
    #[cfg(windows)]
    fn draw_selection_overlay(window: &Window) -> Result<()> {
        use windows::Win32::Graphics::Gdi::{
            CreateCompatibleDC, SelectObject, DeleteDC,
            BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CreateDIBSection, DIB_RGB_COLORS,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            UpdateLayeredWindow, ULW_ALPHA,
        };
        use windows::Win32::Foundation::POINT;
        
        let handle = window.window_handle()
            .context("Failed to get window handle")?;

        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            unsafe {
                let hwnd = HWND(win32_handle.hwnd.get() as isize as *mut std::ffi::c_void);
                let size = window.inner_size();
                let width = size.width as i32;
                let height = size.height as i32;
                
                // Get screen DC
                let screen_dc = GetDC(None);
                let mem_dc = CreateCompatibleDC(screen_dc);
                
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
                let bitmap = CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
                    .context("Failed to create DIB section")?;
                let old_bitmap = SelectObject(mem_dc, bitmap);
                
                // Draw directly to the bitmap memory
                let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (width * height) as usize);
                
                let border_width = 4;
                let corner_size = 20;
                
                // Colors (ARGB format - stored as BGRA in memory)
                let border_color: u32 = 0xFF00A8FF; // Bright blue, fully opaque
                let fill_color: u32 = 0x10000000;    // Almost fully transparent
                let corner_color: u32 = 0xFF00D4FF;  // Lighter blue for corners
                let text_bg_color: u32 = 0xF0181818; // Very dark gray, almost opaque
                let text_border_color: u32 = 0xFF00A8FF; // Blue border for text box
                
                // Text box dimensions (fixed size, centered)
                let text_box_width = 280;
                let text_box_height = 260;
                let text_box_left = (width - text_box_width) / 2;
                let text_box_top = (height - text_box_height) / 2;
                let text_box_right = text_box_left + text_box_width;
                let text_box_bottom = text_box_top + text_box_height;
                let text_box_border = 2;
                
                for y in 0..height {
                    for x in 0..width {
                        let idx = (y * width + x) as usize;
                        
                        let on_border = x < border_width || x >= width - border_width ||
                                        y < border_width || y >= height - border_width;
                        
                        // Corner markers (L-shaped)
                        let in_top_left = (x < corner_size && y < border_width * 2) || 
                                         (y < corner_size && x < border_width * 2);
                        let in_top_right = (x >= width - corner_size && y < border_width * 2) || 
                                          (y < corner_size && x >= width - border_width * 2);
                        let in_bottom_left = (x < corner_size && y >= height - border_width * 2) || 
                                            (y >= height - corner_size && x < border_width * 2);
                        let in_bottom_right = (x >= width - corner_size && y >= height - border_width * 2) || 
                                             (y >= height - corner_size && x >= width - border_width * 2);
                        
                        let in_corner = in_top_left || in_top_right || in_bottom_left || in_bottom_right;
                        
                        // Check if in text box area
                        let in_text_box = x >= text_box_left && x < text_box_right &&
                                         y >= text_box_top && y < text_box_bottom;
                        
                        // Text box border
                        let on_text_box_border = in_text_box && (
                            x < text_box_left + text_box_border || 
                            x >= text_box_right - text_box_border ||
                            y < text_box_top + text_box_border || 
                            y >= text_box_bottom - text_box_border
                        );
                        
                        if in_corner {
                            pixels[idx] = corner_color;
                        } else if on_border {
                            pixels[idx] = border_color;
                        } else if on_text_box_border {
                            pixels[idx] = text_border_color;
                        } else if in_text_box {
                            pixels[idx] = text_bg_color;
                        } else {
                            pixels[idx] = fill_color;
                        }
                    }
                }
                
                // Draw help text directly to pixels
                Self::draw_help_text_pixels(pixels, width, height);
                
                // Update the layered window with our bitmap
                let mut blend = windows::Win32::Graphics::Gdi::BLENDFUNCTION {
                    BlendOp: 0, // AC_SRC_OVER
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: 1, // AC_SRC_ALPHA
                };
                
                let size_struct = windows::Win32::Foundation::SIZE { cx: width, cy: height };
                let point_src = POINT { x: 0, y: 0 };
                
                let _ = UpdateLayeredWindow(
                    hwnd,
                    screen_dc,
                    None, // Use current position
                    Some(&size_struct),
                    mem_dc,
                    Some(&point_src),
                    COLORREF(0),
                    Some(&blend),
                    ULW_ALPHA,
                );
                
                // Cleanup
                SelectObject(mem_dc, old_bitmap);
                let _ = DeleteObject(bitmap);
                let _ = DeleteDC(mem_dc);
                let _ = ReleaseDC(None, screen_dc);
                
                info!("Drew selection overlay with help text");
            }
        }
        
        Ok(())
    }
    
    /// Draw help text showing keyboard shortcuts - pixel-based for proper alpha
    #[cfg(windows)]
    fn draw_help_text_pixels(pixels: &mut [u32], width: i32, height: i32) {
        // Simple bitmap font for text rendering
        // Each character is 8x12 pixels, stored as bit patterns
        
        let white: u32 = 0xFFFFFFFF;
        let blue: u32 = 0xFF00D4FF;
        let gray: u32 = 0xFFB0B0B0;
        
        // Help text content - using fixed-width spacing
        let lines: &[(&str, u32, i32)] = &[
            ("RustFrame", blue, 2),      // Title, scale 2x
            ("", white, 1),
            ("Drag borders to resize", gray, 1),
            ("Drag center to move", gray, 1),
            ("", white, 1),
            ("ENTER - Start capture", white, 1),
            ("ESC   - Exit", white, 1),
            ("", white, 1),
            ("C - Toggle cursor", gray, 1),
            ("B - Toggle border", gray, 1),
            ("S - Settings", gray, 1),
        ];
        
        // Calculate starting Y position
        let line_height = 16;
        let title_height = 28;
        let total_height: i32 = lines.iter().map(|(text, _, scale)| {
            if text.is_empty() { 8 } else if *scale > 1 { title_height } else { line_height }
        }).sum();
        
        let mut y = (height - total_height) / 2;
        
        for (text, color, scale) in lines {
            if text.is_empty() {
                y += 8;
                continue;
            }
            
            let char_width = 7 * scale;
            let text_width = text.len() as i32 * char_width;
            let x = (width - text_width) / 2;
            
            Self::draw_text_line(pixels, width, height, x, y, text, *color, *scale);
            
            y += if *scale > 1 { title_height } else { line_height };
        }
    }
    
    /// Draw a single line of text using a simple bitmap font
    #[cfg(windows)]
    fn draw_text_line(pixels: &mut [u32], img_width: i32, img_height: i32, start_x: i32, start_y: i32, text: &str, color: u32, scale: i32) {
        // Simple 5x7 bitmap font (ASCII 32-127)
        // Each character is stored as 7 bytes (rows), each byte has 5 bits (columns)
        static FONT: &[u8] = &[
            // Space (32)
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            // ! (33)
            0x04, 0x04, 0x04, 0x04, 0x00, 0x04, 0x00,
            // " (34)
            0x0A, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00,
            // # (35)
            0x0A, 0x1F, 0x0A, 0x0A, 0x1F, 0x0A, 0x00,
            // $ (36)
            0x04, 0x0F, 0x14, 0x0E, 0x05, 0x1E, 0x04,
            // % (37)
            0x18, 0x19, 0x02, 0x04, 0x08, 0x13, 0x03,
            // & (38)
            0x08, 0x14, 0x14, 0x08, 0x15, 0x12, 0x0D,
            // ' (39)
            0x04, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00,
            // ( (40)
            0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02,
            // ) (41)
            0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08,
            // * (42)
            0x00, 0x04, 0x15, 0x0E, 0x15, 0x04, 0x00,
            // + (43)
            0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00,
            // , (44)
            0x00, 0x00, 0x00, 0x00, 0x04, 0x04, 0x08,
            // - (45)
            0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00,
            // . (46)
            0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00,
            // / (47)
            0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10,
            // 0-9 (48-57)
            0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E, // 0
            0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E, // 1
            0x0E, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1F, // 2
            0x0E, 0x11, 0x01, 0x06, 0x01, 0x11, 0x0E, // 3
            0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02, // 4
            0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E, // 5
            0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E, // 6
            0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08, // 7
            0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E, // 8
            0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C, // 9
            // : (58)
            0x00, 0x00, 0x04, 0x00, 0x04, 0x00, 0x00,
            // ; (59)
            0x00, 0x00, 0x04, 0x00, 0x04, 0x04, 0x08,
            // < (60)
            0x02, 0x04, 0x08, 0x10, 0x08, 0x04, 0x02,
            // = (61)
            0x00, 0x00, 0x1F, 0x00, 0x1F, 0x00, 0x00,
            // > (62)
            0x08, 0x04, 0x02, 0x01, 0x02, 0x04, 0x08,
            // ? (63)
            0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04,
            // @ (64)
            0x0E, 0x11, 0x17, 0x15, 0x17, 0x10, 0x0E,
            // A-Z (65-90)
            0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11, // A
            0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E, // B
            0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E, // C
            0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E, // D
            0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F, // E
            0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10, // F
            0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0E, // G
            0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11, // H
            0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E, // I
            0x01, 0x01, 0x01, 0x01, 0x01, 0x11, 0x0E, // J
            0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11, // K
            0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F, // L
            0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11, // M
            0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11, // N
            0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E, // O
            0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10, // P
            0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D, // Q
            0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11, // R
            0x0E, 0x11, 0x10, 0x0E, 0x01, 0x11, 0x0E, // S
            0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, // T
            0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E, // U
            0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04, // V
            0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11, // W
            0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11, // X
            0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04, // Y
            0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F, // Z
            // [ (91)
            0x0E, 0x08, 0x08, 0x08, 0x08, 0x08, 0x0E,
            // \ (92)
            0x10, 0x10, 0x08, 0x04, 0x02, 0x01, 0x01,
            // ] (93)
            0x0E, 0x02, 0x02, 0x02, 0x02, 0x02, 0x0E,
            // ^ (94)
            0x04, 0x0A, 0x11, 0x00, 0x00, 0x00, 0x00,
            // _ (95)
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1F,
            // ` (96)
            0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00,
            // a-z (97-122)
            0x00, 0x00, 0x0E, 0x01, 0x0F, 0x11, 0x0F, // a
            0x10, 0x10, 0x1E, 0x11, 0x11, 0x11, 0x1E, // b
            0x00, 0x00, 0x0E, 0x11, 0x10, 0x11, 0x0E, // c
            0x01, 0x01, 0x0F, 0x11, 0x11, 0x11, 0x0F, // d
            0x00, 0x00, 0x0E, 0x11, 0x1F, 0x10, 0x0E, // e
            0x06, 0x08, 0x1E, 0x08, 0x08, 0x08, 0x08, // f
            0x00, 0x00, 0x0F, 0x11, 0x0F, 0x01, 0x0E, // g
            0x10, 0x10, 0x1E, 0x11, 0x11, 0x11, 0x11, // h
            0x04, 0x00, 0x0C, 0x04, 0x04, 0x04, 0x0E, // i
            0x02, 0x00, 0x06, 0x02, 0x02, 0x12, 0x0C, // j
            0x10, 0x10, 0x12, 0x14, 0x18, 0x14, 0x12, // k
            0x0C, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E, // l
            0x00, 0x00, 0x1A, 0x15, 0x15, 0x11, 0x11, // m
            0x00, 0x00, 0x1E, 0x11, 0x11, 0x11, 0x11, // n
            0x00, 0x00, 0x0E, 0x11, 0x11, 0x11, 0x0E, // o
            0x00, 0x00, 0x1E, 0x11, 0x1E, 0x10, 0x10, // p
            0x00, 0x00, 0x0F, 0x11, 0x0F, 0x01, 0x01, // q
            0x00, 0x00, 0x16, 0x19, 0x10, 0x10, 0x10, // r
            0x00, 0x00, 0x0E, 0x10, 0x0E, 0x01, 0x1E, // s
            0x08, 0x08, 0x1E, 0x08, 0x08, 0x09, 0x06, // t
            0x00, 0x00, 0x11, 0x11, 0x11, 0x13, 0x0D, // u
            0x00, 0x00, 0x11, 0x11, 0x11, 0x0A, 0x04, // v
            0x00, 0x00, 0x11, 0x11, 0x15, 0x15, 0x0A, // w
            0x00, 0x00, 0x11, 0x0A, 0x04, 0x0A, 0x11, // x
            0x00, 0x00, 0x11, 0x11, 0x0F, 0x01, 0x0E, // y
            0x00, 0x00, 0x1F, 0x02, 0x04, 0x08, 0x1F, // z
        ];
        
        let mut x = start_x;
        for ch in text.chars() {
            let idx = if ch >= ' ' && ch <= 'z' {
                (ch as usize) - 32
            } else {
                0 // Space for unknown chars
            };
            
            // Draw character
            for row in 0..7 {
                let font_idx = idx * 7 + row;
                if font_idx >= FONT.len() { break; }
                let row_data = FONT[font_idx];
                
                for col in 0..5 {
                    if (row_data >> (4 - col)) & 1 == 1 {
                        // Draw pixel with scaling
                        for sy in 0..scale {
                            for sx in 0..scale {
                                let px = x + col * scale + sx;
                                let py = start_y + row as i32 * scale + sy;
                                if px >= 0 && px < img_width && py >= 0 && py < img_height {
                                    let pidx = (py * img_width + px) as usize;
                                    if pidx < pixels.len() {
                                        pixels[pidx] = color;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            x += 6 * scale + scale; // Character width + spacing
        }
    }
    
    /// Redraw the selection overlay (called on resize)
    #[cfg(windows)]
    pub fn redraw_selection_overlay(&self) -> Result<()> {
        Self::draw_selection_overlay(&self.window)
    }

    #[cfg(windows)]
    #[inline]
    fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
        COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16))
    }

    /// Get the window ID for event routing
    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    /// Request a redraw of the overlay window
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
        self.window.outer_position().unwrap_or(PhysicalPosition::new(0, 0))
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
        let position = self.window.outer_position().unwrap_or(PhysicalPosition::new(0, 0));
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
        let position = self.window.outer_position().unwrap_or(PhysicalPosition::new(0, 0));
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
    pub fn get_window(&self) -> &Arc<Window> {
        &self.window
    }

    /// Move the window by a delta (for drag functionality)
    /// This is used for implementing click-and-drag movement of the overlay
    pub fn move_by(&self, delta_x: i32, delta_y: i32) {
        if let Ok(current_pos) = self.window.outer_position() {
            let new_x = current_pos.x + delta_x;
            let new_y = current_pos.y + delta_y;

            self.window.set_outer_position(PhysicalPosition::new(new_x, new_y));
        }
    }

    /// Convert the overlay to a hollow frame (only border visible, interior click-through)
    /// Uses SetWindowRgn for the visual appearance and subclass for hit testing
    #[cfg(windows)]
    pub fn make_hollow_frame(&self, border_width: u32) {
        use windows::Win32::Graphics::Gdi::{CreateRectRgn, CombineRgn, SetWindowRgn, RGN_DIFF};
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, SetWindowLongPtrW, GetWindowLongPtrW,
            SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, HWND_TOPMOST, SWP_NOACTIVATE,
            GWL_STYLE, GWL_EXSTYLE, WS_POPUP, WS_VISIBLE, WS_THICKFRAME,
            WS_EX_TOPMOST, WS_EX_TOOLWINDOW, WS_EX_LAYERED,
            SetLayeredWindowAttributes, LWA_COLORKEY,
        };
        use windows::Win32::Foundation::HWND;
        
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
                let hwnd = HWND(win32_handle.hwnd.get() as isize as *mut std::ffi::c_void);

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
                let outer_rgn = CreateRectRgn(-resize_margin, -resize_margin, width + resize_margin, height + resize_margin);
                
                // Create inner rectangle (the hole) - but leave border visible
                let inner_rgn = CreateRectRgn(border, border, width - border, height - border);
                
                // Subtract inner from outer to create hollow frame with resize margins
                let _ = CombineRgn(outer_rgn, outer_rgn, inner_rgn, RGN_DIFF);
                
                // Apply the region
                SetWindowRgn(hwnd, outer_rgn, true);
                
                // Install window subclass for custom hit testing
                Self::install_subclass(hwnd, border_width);
                
                // Force window to update
                let _ = SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    0, 0, 0, 0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );

                info!("Overlay converted to hollow frame (border: {}px)", border_width);
            }
        }
    }

    /// Install a window subclass for custom WM_NCHITTEST handling
    #[cfg(windows)]
    unsafe fn install_subclass(hwnd: HWND, border_width: u32) {
        use windows::Win32::UI::Shell::{SetWindowSubclass, DefSubclassProc};
        use std::ptr;
        
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
                    LoadCursorW, SetCursor, IDC_SIZEALL, IDC_SIZENWSE, IDC_SIZENESW,
                    IDC_SIZEWE, IDC_SIZENS, HTCAPTION, HTTOPLEFT, HTTOPRIGHT,
                    HTBOTTOMLEFT, HTBOTTOMRIGHT, HTLEFT, HTRIGHT, HTTOP, HTBOTTOM,
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
                        let _ = SetCursor(cur);
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
                let in_border = (x >= rect.left && x < rect.left + border) ||
                               (x >= rect.right - border && x < rect.right) ||
                               (y >= rect.top && y < rect.top + border) ||
                               (y >= rect.bottom - border && y < rect.bottom);
                
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
        use windows::Win32::Graphics::Gdi::{CreateRectRgn, CombineRgn, SetWindowRgn, RGN_DIFF};
        use windows::Win32::Foundation::HWND;
        
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
                let hwnd = HWND(win32_handle.hwnd.get() as isize as *mut std::ffi::c_void);

                // Create outer rectangle with resize margin
                let resize_margin = 8i32;
                let outer_rgn = CreateRectRgn(-resize_margin, -resize_margin, width + resize_margin, height + resize_margin);
                
                // Create inner rectangle (the hole)
                let inner_rgn = CreateRectRgn(border, border, width - border, height - border);
                
                // Subtract inner from outer
                let _ = CombineRgn(outer_rgn, outer_rgn, inner_rgn, RGN_DIFF);
                
                // Apply the updated region
                SetWindowRgn(hwnd, outer_rgn, true);
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
    /// In debug mode: visible by default (for easier debugging)
    /// In release mode: hidden until capture starts
    pub fn new(event_loop: &ActiveEventLoop) -> Result<Self> {
        // Debug mode: show window for debugging, positioned next to overlay
        // Release mode: hidden until capture starts
        #[cfg(debug_assertions)]
        let (initial_visible, initial_position, with_decorations) = (true, PhysicalPosition::new(550, 100), true);
        #[cfg(not(debug_assertions))]
        let (initial_visible, initial_position, with_decorations) = (false, PhysicalPosition::new(100, 100), false);

        info!("Creating destination window (initially {})", if initial_visible { "visible" } else { "hidden" });

        // Configure window attributes for the destination
        let attributes = WindowAttributes::default()
            .with_title("RustFrame - Screen Share This Window")
            .with_inner_size(LogicalSize::new(400, 300)) // Will be resized to match overlay
            .with_position(initial_position)
            .with_resizable(with_decorations) // Resizable only in debug mode
            .with_decorations(with_decorations) // Title bar only in debug mode
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
    pub fn set_exclude_from_capture(&self, exclude: bool) -> Result<()> {
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE};
        
        let handle = self.window.window_handle()
            .context("Failed to get window handle")?;

        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            unsafe {
                let hwnd = HWND(win32_handle.hwnd.get() as isize as *mut std::ffi::c_void);
                
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
    pub fn show_at(&self, position: PhysicalPosition<i32>, size: PhysicalSize<u32>) {
        // First resize to match the overlay's inner size
        let _ = self.window.request_inner_size(size);
        // Then move to the overlay's position
        self.window.set_outer_position(position);
        // Finally show the window
        self.window.set_visible(true);
        info!("Destination window shown at {:?} with size {:?}", position, size);
    }

    /// Make the window frameless with an optional colored border
    #[cfg(windows)]
    pub fn make_frameless(&self, show_border: bool, border_width: u32) {
        use windows::Win32::UI::WindowsAndMessaging::*;
        use windows::Win32::Foundation::HWND;
        
        let handle = match self.window.window_handle() {
            Ok(h) => h,
            Err(_) => return,
        };

        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            unsafe {
                let hwnd = HWND(win32_handle.hwnd.get() as isize as *mut std::ffi::c_void);

                // Remove title bar and frame
                let style = GetWindowLongW(hwnd, GWL_STYLE);
                let new_style = style & !(WS_CAPTION.0 as i32 | WS_THICKFRAME.0 as i32);
                SetWindowLongW(hwnd, GWL_STYLE, new_style);

                if show_border {
                    // Add a thin border using WS_EX_CLIENTEDGE or WS_EX_STATICEDGE
                    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                    SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_DLGMODALFRAME.0 as i32);
                    info!("Window made frameless with border (width hint: {})", border_width);
                } else {
                    info!("Window made completely frameless");
                }

                // Force window to redraw with new styles
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    0, 0, 0, 0,
                    SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
                );
            }
        }
    }

    #[cfg(not(windows))]
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
    pub fn make_click_through_and_topmost(&self) {
        use windows::Win32::UI::WindowsAndMessaging::*;
        use windows::Win32::Foundation::HWND;
        
        let handle = match self.window.window_handle() {
            Ok(h) => h,
            Err(_) => return,
        };

        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            unsafe {
                let hwnd = HWND(win32_handle.hwnd.get() as isize as *mut std::ffi::c_void);

                // Add WS_EX_TRANSPARENT for click-through
                // Add WS_EX_TOPMOST to stay on top
                // Keep WS_EX_LAYERED for proper transparency
                let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                SetWindowLongW(
                    hwnd, 
                    GWL_EXSTYLE, 
                    ex_style | WS_EX_TRANSPARENT.0 as i32 | WS_EX_LAYERED.0 as i32
                );

                // Use SetWindowPos to make it TOPMOST
                let _ = SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    0, 0, 0, 0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );

                info!("Window set to click-through and always on top");
            }
        }
    }

    #[cfg(not(windows))]
    pub fn make_click_through_and_topmost(&self) {
        self.window.set_window_level(WindowLevel::AlwaysOnTop);
    }

    /// Request a redraw of the destination window
    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    /// Resize the destination window
    pub fn resize(&self, size: PhysicalSize<u32>) {
        let _ = self.window.request_inner_size(size);
    }

    /// Get the window's current size (for renderer resize)
    pub fn get_size(&self) -> PhysicalSize<u32> {
        self.window.inner_size()
    }

    /// Get the window's current position
    pub fn get_outer_position(&self) -> PhysicalPosition<i32> {
        self.window.outer_position().unwrap_or(PhysicalPosition::new(0, 0))
    }

    /// Position destination window OFF-SCREEN (production mode)
    /// User won't see it, but Google Meet can still capture it
    /// This prevents infinite mirror since dest is outside capture region
    #[cfg(windows)]
    pub fn position_offscreen(&self, size: PhysicalSize<u32>) {
        use windows::Win32::UI::WindowsAndMessaging::*;
        use windows::Win32::Foundation::HWND;
        
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
                let hwnd = HWND(win32_handle.hwnd.get() as isize as *mut std::ffi::c_void);
                
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
                    0, 0, 0, 0,
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
    pub fn position_beside_overlay(&self, overlay_position: PhysicalPosition<i32>, size: PhysicalSize<u32>) {
        use windows::Win32::UI::WindowsAndMessaging::*;
        use windows::Win32::Foundation::HWND;
        
        // Position to the right of overlay
        let dest_position = PhysicalPosition::new(
            overlay_position.x + size.width as i32 + 20,
            overlay_position.y
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
                let hwnd = HWND(win32_handle.hwnd.get() as isize as *mut std::ffi::c_void);
                
                // Restore title bar and frame
                let style = GetWindowLongW(hwnd, GWL_STYLE);
                let new_style = style | WS_CAPTION.0 as i32 | WS_THICKFRAME.0 as i32;
                SetWindowLongW(hwnd, GWL_STYLE, new_style);
                
                // Remove WS_EX_TOOLWINDOW to show in taskbar again
                let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                let new_ex_style = ex_style & !(WS_EX_TOOLWINDOW.0 as i32);
                SetWindowLongW(hwnd, GWL_EXSTYLE, new_ex_style);
                
                // Force window to update with new styles and bring to front
                let _ = SetWindowPos(
                    hwnd,
                    HWND_TOP,
                    0, 0, 0, 0,
                    SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                );
            }
        }
        
        info!("Destination window positioned BESIDE overlay at {:?} (with title bar)", dest_position);
    }
    
    #[cfg(not(windows))]
    pub fn position_beside_overlay(&self, overlay_position: PhysicalPosition<i32>, size: PhysicalSize<u32>) {
        let dest_position = PhysicalPosition::new(
            overlay_position.x + size.width as i32 + 20,
            overlay_position.y
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
