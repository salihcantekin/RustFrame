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

use anyhow::{Result, Context, anyhow};
use log::info;
use winit::{
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes, WindowId, WindowLevel},
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
};
use std::sync::Arc;

use crate::capture::CaptureRect;

#[cfg(windows)]
use windows::Win32::{
    Foundation::{HWND, RECT, COLORREF},
    Graphics::Gdi::{CreateSolidBrush, FillRect, FrameRect, GetDC, ReleaseDC, DeleteObject, HBRUSH},
    UI::WindowsAndMessaging::*,
};

/// Wrapper for the overlay (selector) window
pub struct OverlayWindow {
    window: Arc<Window>,
}

impl OverlayWindow {
    /// Create a new overlay window for region selection
    pub fn new(event_loop: &ActiveEventLoop) -> Result<Self> {
        info!("Creating overlay window");

        // Configure window attributes for the overlay
        // Normal window with title bar - user drags/resizes to select region
        // Press ENTER to start capture, ESC to exit
        let attributes = WindowAttributes::default()
            .with_title("RustFrame - Drag to position, resize to select region, ENTER to start")
            .with_inner_size(LogicalSize::new(400, 300)) // Initial size
            .with_position(PhysicalPosition::new(100, 100)) // Initial position
            .with_min_inner_size(LogicalSize::new(200, 150)) // Minimum size
            .with_resizable(true) // User can resize to select region
            .with_decorations(true) // Normal window with title bar for easy dragging
            .with_transparent(false) // Keep opaque so it's visible
            .with_window_level(WindowLevel::AlwaysOnTop); // Stay on top

        let window = event_loop
            .create_window(attributes)
            .context("Failed to create overlay window")?;

        info!("Overlay window created with ID: {:?}", window.id());

        // Apply Windows-specific styling for true transparency and borderless overlay
        #[cfg(windows)]
        Self::apply_windows_overlay_style(&window)?;

        // Set the window background color to blue so it's visible
        #[cfg(windows)]
        Self::set_overlay_background_color(&window)?;

        // Request an initial redraw so the outline/fill is visible immediately
        window.request_redraw();

        Ok(Self {
            window: Arc::new(window),
        })
    }

    /// Apply Windows-specific styling for overlay
    /// Keep it simple - just a normal window that's easy to drag
    #[cfg(windows)]
    fn apply_windows_overlay_style(_window: &Window) -> Result<()> {
        // No special styling needed - use normal window decorations
        // This makes dragging smooth and reliable
        info!("Overlay window using standard decorations for easy dragging");
        Ok(())
    }

    /// Set the window background to blue color
    #[cfg(windows)]
    fn set_overlay_background_color(window: &Window) -> Result<()> {
        let handle = window.window_handle()
            .context("Failed to get window handle")?;

        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            unsafe {
                let hwnd = HWND(win32_handle.hwnd.get() as isize as *mut std::ffi::c_void);

                // Create a blue brush (RGB: 70, 130, 180 = steel blue)
                let brush = CreateSolidBrush(Self::rgb(70, 130, 180));
                SetClassLongPtrW(hwnd, GCLP_HBRBACKGROUND, brush.0 as isize);

                info!("Set overlay window background to blue");
            }
        }

        Ok(())
    }

    /// Draw a semi-transparent fill and border so the selection region is visible
    #[cfg(windows)]
    pub fn draw_overlay(&self) -> Result<()> {
        // Overlay is transparent; drawing on layered window with GDI causes sizing issues
        // Keep this as a no-op for now
        Ok(())
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
    /// Uses SetWindowRgn to create a "donut" shaped window region
    #[cfg(windows)]
    pub fn make_hollow_frame(&self, border_width: u32) {
        use windows::Win32::Graphics::Gdi::{CreateRectRgn, CombineRgn, SetWindowRgn, RGN_DIFF};
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, HWND_TOPMOST, SWP_NOACTIVATE};
        use windows::Win32::Foundation::HWND;
        
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

                // Create outer rectangle (full window)
                let outer_rgn = CreateRectRgn(0, 0, width, height);
                
                // Create inner rectangle (the hole)
                let inner_rgn = CreateRectRgn(border, border, width - border, height - border);
                
                // Subtract inner from outer to create hollow frame
                let _ = CombineRgn(outer_rgn, outer_rgn, inner_rgn, RGN_DIFF);
                
                // Apply the region to the window
                // The window will only exist where the region is defined (the border)
                SetWindowRgn(hwnd, outer_rgn, true);
                
                // Make window always on top
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

    /// Update the hollow frame region after resize
    #[cfg(windows)]
    pub fn update_hollow_frame(&self, border_width: u32) {
        use windows::Win32::Graphics::Gdi::{CreateRectRgn, CombineRgn, SetWindowRgn, RGN_DIFF};
        use windows::Win32::Foundation::HWND;
        
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

                // Create new outer rectangle
                let outer_rgn = CreateRectRgn(0, 0, width, height);
                
                // Create new inner rectangle
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
        let (initial_visible, initial_position) = (true, PhysicalPosition::new(550, 100));
        #[cfg(not(debug_assertions))]
        let (initial_visible, initial_position) = (false, PhysicalPosition::new(100, 100));

        info!("Creating destination window (initially {})", if initial_visible { "visible" } else { "hidden" });

        // Configure window attributes for the destination
        let attributes = WindowAttributes::default()
            .with_title("RustFrame - Screen Share This Window")
            .with_inner_size(LogicalSize::new(400, 300)) // Will be resized to match overlay
            .with_position(initial_position)
            .with_resizable(true) // User can resize
            .with_decorations(true) // Normal window with title bar
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

    /// Get the window's current size (for renderer resize)
    pub fn get_size(&self) -> PhysicalSize<u32> {
        self.window.inner_size()
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
