//! Pure WinAPI Destination Window
//!
//! Runs in its own thread with dedicated message loop.
//! This is necessary because Tauri's WebView2 message loop doesn't pump
//! messages for other WinAPI windows created in the main thread.
//!
//! GPU Rendering Support:
//! - Uses DirectX 11 SwapChain for zero-copy GPU rendering
//! - Falls back to GDI BitBlt for CPU rendering (compatibility)
//! - Selectable via gpu_acceleration setting

use crate::traits::PreviewWindow;
use rustframe_capture::display_info;
use std::mem;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use super::d3d11_renderer::D3D11Renderer;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, GetDC, InvalidateRect, ReleaseDC, StretchDIBits, ValidateRect, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC, PAINTSTRUCT, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect,
    GetMessageW, GetSystemMetrics, GetWindowRect, PostMessageW, PostQuitMessage, RegisterClassExW,
    SetWindowPos, CS_HREDRAW, CS_VREDRAW, MSG, SM_CXSCREEN, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SWP_ASYNCWINDOWPOS, WM_USER, WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW,
    WS_POPUP, WS_VISIBLE, ShowWindow, SW_HIDE, SW_SHOW, SWP_FRAMECHANGED,
    GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE,
};

use windows::Win32::Foundation::COLORREF;

// Only used in release builds for positioning.
#[cfg(not(debug_assertions))]
use windows::Win32::UI::WindowsAndMessaging::SM_CYSCREEN;

use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    SetLayeredWindowAttributes, LWA_ALPHA, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_TOOLWINDOW,
    WS_EX_TRANSPARENT,
};

use lazy_static::lazy_static;
use log::{debug, error, info};

lazy_static! {
    /// Global frame buffer - render thread writes, window thread reads (CPU path)
    static ref FRAME_BUFFER: Arc<Mutex<Option<FrameData>>> = Arc::new(Mutex::new(None));
    /// Global GPU texture data - render thread writes, window thread reads (GPU path)
    static ref GPU_TEXTURE_DATA: Arc<Mutex<Option<GpuTextureData>>> = Arc::new(Mutex::new(None));
    /// Global HWND for the destination window (stored as isize for thread safety)
    static ref DEST_HWND: Mutex<isize> = Mutex::new(0);
    /// Global D3D11 renderer (created in window thread)
    static ref D3D11_RENDERER: Mutex<Option<D3D11Renderer>> = Mutex::new(None);
    /// Global masking state - when true, render solid color instead of captured content
    static ref MASKING_ENABLED: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
}

struct FrameData {
    data: Vec<u8>,
    width: u32,
    height: u32,
}

struct GpuTextureData {
    texture_ptr: usize,
    crop_x: i32,
    crop_y: i32,
    crop_width: u32,
    crop_height: u32,
}

impl Drop for GpuTextureData {
    fn drop(&mut self) {
        // Release the COM reference when texture data is dropped
        if self.texture_ptr != 0 {
            use windows::core::Interface;
            use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
            unsafe {
                // Reconstruct the texture to call Release
                let texture = ID3D11Texture2D::from_raw(self.texture_ptr as *mut _);
                // from_raw takes ownership, drop will call Release
                drop(texture);
            }
        }
    }
}

const CLASS_NAME: PCWSTR = w!("RustFrameDestination");
const WM_FRAME_UPDATE: u32 = WM_USER + 1;

static WINDOW_THREAD_RUNNING: AtomicBool = AtomicBool::new(false);

/// Convert points to pixels using current DPI scale
fn points_to_pixels(points: u32) -> u32 {
    let display = display_info::get();
    if display.initialized {
        display.points_to_pixels(points as f64) as u32
    } else {
        points // Fallback: assume 1.0 scale
    }
}

/// Pure WinAPI destination window - runs in its own thread with message loop
pub struct DestinationWindow {
    thread_handle: Option<JoinHandle<()>>,
    stop_flag: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DestinationWindowConfig {
    /// Layered window alpha (0..=255). When None, defaults to 0 in release builds.
    #[cfg_attr(debug_assertions, allow(dead_code))]
    pub alpha: Option<u8>,
    /// Force WS_EX_TOPMOST. When None, defaults to true.
    pub topmost: Option<bool>,

    /// Controls WS_EX_TRANSPARENT (click-through). When None, defaults to true in release builds.
    #[cfg_attr(debug_assertions, allow(dead_code))]
    pub click_through: Option<bool>,
    /// Controls WS_EX_TOOLWINDOW (keeps out of Alt-Tab/taskbar and some window pickers).
    /// When None, defaults to true in release builds.
    #[cfg_attr(debug_assertions, allow(dead_code))]
    pub toolwindow: Option<bool>,
    /// Controls WS_EX_LAYERED. When None, defaults to true in release builds.
    #[cfg_attr(debug_assertions, allow(dead_code))]
    pub layered: Option<bool>,
    /// Controls WS_EX_APPWINDOW when toolwindow=false. When None, defaults to false.
    #[cfg_attr(debug_assertions, allow(dead_code))]
    pub appwindow: Option<bool>,

    /// Controls WS_EX_NOACTIVATE. When None, defaults to true.
    #[cfg_attr(debug_assertions, allow(dead_code))]
    pub noactivate: Option<bool>,
    /// When true, uses WS_OVERLAPPEDWINDOW (more "app-like"; may help Discord list it).
    /// When None, defaults to false (WS_POPUP).
    #[cfg_attr(debug_assertions, allow(dead_code))]
    pub overlapped: Option<bool>,
}

// SAFETY: We communicate via atomic flags and mutex-protected data
unsafe impl Send for DestinationWindow {}
unsafe impl Sync for DestinationWindow {}

impl DestinationWindow {
    /// Create a new destination window in its own thread
    pub fn new(x: i32, y: i32, width: u32, height: u32, config: DestinationWindowConfig) -> Option<Self> {
        // Scale dimensions for DPI (input is in logical points)
        let width_pixels = points_to_pixels(width);
        let height_pixels = points_to_pixels(height);

        info!(
            "Creating WinAPI destination window at ({}, {}) {}x{} points ({}x{} pixels) in dedicated thread",
            x, y, width, height, width_pixels, height_pixels
        );

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        // Spawn window thread - window will be created and message loop will run here
        let thread_handle = thread::spawn(move || {
            run_window_thread(x, y, width_pixels, height_pixels, config, stop_flag_clone);
        });

        // Wait for window to be created
        for _ in 0..50 {
            thread::sleep(std::time::Duration::from_millis(10));
            if let Ok(hwnd_lock) = DEST_HWND.lock() {
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

    /// Update the frame buffer (called from render thread)
    /// This just updates the buffer - window thread will paint on next timer tick
    pub fn update_frame(&self, data: Vec<u8>, width: u32, height: u32) {
        info!("update_frame: {}x{}, {} bytes", width, height, data.len());

        // Update the global buffer
        if let Ok(mut buffer) = FRAME_BUFFER.lock() {
            *buffer = Some(FrameData {
                data,
                width,
                height,
            });
        }

        // Optionally signal window thread to repaint immediately
        // (timer will also trigger repaint, so this is just for lower latency)
        if let Ok(hwnd_lock) = DEST_HWND.lock() {
            let hwnd_val = *hwnd_lock;
            if hwnd_val != 0 {
                unsafe {
                    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
                    let _ = PostMessageW(Some(hwnd), WM_FRAME_UPDATE, WPARAM(0), LPARAM(0));
                }
            }
        }
    }

    /// Update the frame from GPU texture (called from render thread)
    /// Zero-copy GPU path - stores texture handle and triggers window repaint
    pub fn update_frame_from_texture(
        &self,
        texture_ptr: usize,
        crop_x: i32,
        crop_y: i32,
        crop_width: u32,
        crop_height: u32,
    ) {
        debug!(
            "update_frame_from_texture: {}x{} at ({}, {}), ptr=0x{:X}",
            crop_width, crop_height, crop_x, crop_y, texture_ptr
        );

        // Store GPU texture data
        if let Ok(mut data) = GPU_TEXTURE_DATA.lock() {
            *data = Some(GpuTextureData {
                texture_ptr,
                crop_x,
                crop_y,
                crop_width,
                crop_height,
            });
        }

        // Trigger window repaint
        if let Ok(hwnd_lock) = DEST_HWND.lock() {
            let hwnd_val = *hwnd_lock;
            if hwnd_val != 0 {
                unsafe {
                    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
                    let _ = PostMessageW(Some(hwnd), WM_FRAME_UPDATE, WPARAM(0), LPARAM(0));
                }
            }
        }
    }

    /// Get the HWND value (for platform-specific operations)
    pub fn hwnd_value(&self) -> isize {
        DEST_HWND.lock().map(|h| *h).unwrap_or(0)
    }
    
    /// Get current window position and size
    pub fn get_rect(&self) -> Option<(i32, i32, i32, i32)> {
        use windows::Win32::Foundation::{HWND, RECT};
        use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
        
        let hwnd_val = self.hwnd_value();
        if hwnd_val == 0 {
            return None;
        }
        
        unsafe {
            let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_ok() {
                Some((rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top))
            } else {
                None
            }
        }
    }

    /// Set window position
    pub fn set_position(&self, x: i32, y: i32) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOSIZE, SWP_NOZORDER, SWP_NOACTIVATE};

        let hwnd_val = self.hwnd_value();
        if hwnd_val == 0 {
            return;
        }

        unsafe {
            let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
            let _ = SetWindowPos(
                hwnd,
                None, // ignored because SWP_NOZORDER
                x,
                y,
                0, // ignored
                0, // ignored
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE
            );
        }
    }

    /// Send window to back (HWND_BOTTOM) for screen sharing compatibility
    /// This keeps the window visible and on-screen but at the lowest z-order,
    /// making it less intrusive while remaining detectable by screen sharing apps
    pub fn send_to_back(&self) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOMOVE, SWP_NOSIZE, SWP_NOACTIVATE, SWP_SHOWWINDOW, HWND_BOTTOM};

        let hwnd_val = self.hwnd_value();
        if hwnd_val == 0 {
            return;
        }

        unsafe {
            let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
            // HWND_BOTTOM places window at the bottom of the z-order
            // but still keeps it visible and capturable
            let result = SetWindowPos(
                hwnd,
                Some(HWND_BOTTOM),
                0, 0, 0, 0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW
            );
            
            if result.is_ok() {
                tracing::debug!("Preview window sent to back (HWND_BOTTOM) for screen sharing");
            } else {
                tracing::warn!("Failed to send window to back");
            }
        }
    }

    /// Exclude window from screen capture (Windows 10 2000H+)
    /// Prevents infinite mirroring when capturing desktop regions that include the preview window
    pub fn exclude_from_capture(&self) {
        use windows::Win32::Foundation::HWND;

        const WDA_EXCLUDEFROMCAPTURE: u32 = 0x00000011;

        let hwnd_val = self.hwnd_value();
        if hwnd_val == 0 {
            return;
        }

        unsafe {
            let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
            
            // Windows API declaration - returns i32 (BOOL)
            extern "system" {
                fn SetWindowDisplayAffinity(hwnd: HWND, dwAffinity: u32) -> i32;
            }

            let result = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
            
            if result != 0 {
                tracing::info!("✅ Preview window excluded from screen capture (WDA_EXCLUDEFROMCAPTURE)");
            } else {
                tracing::warn!("⚠️ Failed to exclude window from capture (requires Windows 10 2000H+)");
            }
        }
    }

    /// Bring window to front (for debugging or special cases)
    pub fn bring_to_front(&self) {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOMOVE, SWP_NOSIZE, SWP_NOACTIVATE, HWND_TOP};

        let hwnd_val = self.hwnd_value();
        if hwnd_val == 0 {
            return;
        }

        unsafe {
            let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOP),
                0, 0, 0, 0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE
            );
            tracing::debug!("Preview window brought to front");
        }
    }

    /// Enable masking (render solid color instead of captured content)
    pub fn enable_masking(&self) {
        MASKING_ENABLED.store(true, Ordering::SeqCst);
        let hwnd_val = self.hwnd_value();
        if hwnd_val != 0 {
            unsafe {
                let _ = InvalidateRect(Some(HWND(hwnd_val as _)), None, false);
            }
        }
    }

    /// Disable masking (render captured content normally)
    pub fn disable_masking(&self) {
        MASKING_ENABLED.store(false, Ordering::SeqCst);
        let hwnd_val = self.hwnd_value();
        if hwnd_val != 0 {
            unsafe {
                let _ = InvalidateRect(Some(HWND(hwnd_val as _)), None, false);
            }
        }
    }

    /// Hide the preview window (make it invisible)
    pub fn hide(&self) {
        let hwnd_val = self.hwnd_value();
        if hwnd_val != 0 {
            unsafe {
                let _ = ShowWindow(HWND(hwnd_val as _), SW_HIDE);
            }
        }
    }

    /// Show the preview window (make it visible)
    pub fn show(&self) {
        let hwnd_val = self.hwnd_value();
        if hwnd_val != 0 {
            unsafe {
                let _ = ShowWindow(HWND(hwnd_val as _), SW_SHOW);
            }
        }
    }

    /// Set position and size of the preview window
    pub fn set_position_and_size(&self, x: i32, y: i32, width: i32, height: i32) {
        let hwnd_val = self.hwnd_value();
        if hwnd_val != 0 {
            unsafe {
                let _ = SetWindowPos(
                    HWND(hwnd_val as _),
                    None,
                    x,
                    y,
                    width,
                    height,
                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
                );
            }
        }
    }
}

impl Drop for DestinationWindow {
    fn drop(&mut self) {
        info!("Destroying WinAPI destination window");

        // Signal thread to stop
        self.stop_flag.store(true, Ordering::SeqCst);

        // Clear GPU texture data (will Release COM reference)
        if let Ok(mut data) = GPU_TEXTURE_DATA.lock() {
            *data = None;
        }

        // Post quit message to window thread
        if let Ok(hwnd_lock) = DEST_HWND.lock() {
            let hwnd_val = *hwnd_lock;
            if hwnd_val != 0 {
                unsafe {
                    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
                    let _ = PostMessageW(Some(hwnd), 0x0010, WPARAM(0), LPARAM(0));
                    // WM_CLOSE
                }
            }
        }

        // Wait for thread to finish
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        // Clear global HWND
        if let Ok(mut hwnd_lock) = DEST_HWND.lock() {
            *hwnd_lock = 0;
        }

        // Clear frame buffer
        if let Ok(mut buffer) = FRAME_BUFFER.lock() {
            *buffer = None;
        }
    }
}

/// Window thread - creates window and runs its own message loop
fn run_window_thread(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    config: DestinationWindowConfig,
    stop_flag: Arc<AtomicBool>,
) {
    tracing::debug!("Destination window thread started");

    unsafe {
        let hinstance = match GetModuleHandleW(None) {
            Ok(h) => h,
            Err(e) => {
                error!("Failed to get module handle: {}", e);
                return;
            }
        };

        // Register window class
        static CLASS_REGISTERED: AtomicBool = AtomicBool::new(false);
        if !CLASS_REGISTERED.swap(true, Ordering::SeqCst) {
            let wc = WNDCLASSEXW {
                cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
                style: windows::Win32::UI::WindowsAndMessaging::WNDCLASS_STYLES(0), // No redraw on resize (prevents flickering)
                lpfnWndProc: Some(window_proc),
                hInstance: hinstance.into(),
                lpszClassName: CLASS_NAME,
                ..Default::default()
            };

            if RegisterClassExW(&wc) == 0 {
                error!("Failed to register destination window class");
                CLASS_REGISTERED.store(false, Ordering::SeqCst);
                return;
            }
        }

        // Position window at the requested coordinates
        let x_pos = x;
        let y_pos = y;

        // Window style:
        // - Default: WS_OVERLAPPEDWINDOW for normal app window (z-order compatible)
        // - Optional: WS_POPUP for borderless (legacy mode)
        // FIX: Default to WS_POPUP to remove title bar/borders
        let window_style = if config.overlapped.unwrap_or(false) {
            WS_OVERLAPPEDWINDOW | WS_VISIBLE
        } else {
            WS_POPUP | WS_VISIBLE
        };

        let topmost = config.topmost.unwrap_or(false);

        // Extended styles:
        // - WS_EX_NOACTIVATE: Don't steal focus when created
        // - WS_EX_APPWINDOW: Show in taskbar/window pickers (for screen sharing)
        // - No layered/transparent: Window is fully visible but controlled via z-order
        let noactivate = config.noactivate.unwrap_or(false);

        #[cfg(debug_assertions)]
        let ex_style = {
            let mut style = if noactivate {
                WS_EX_NOACTIVATE
            } else {
                Default::default()
            };
            if topmost {
                style |= WS_EX_TOPMOST;
            }
            style
        };
        #[cfg(not(debug_assertions))]
        let ex_style = {
            let layered = config.layered.unwrap_or(false); // Default FALSE - no transparency
            let click_through = config.click_through.unwrap_or(false); // Default FALSE - window is interactive
            // FIX: Default to 'toolwindow = true' to hide from Taskbar
            let toolwindow = config.toolwindow.unwrap_or(true); 
            // FIX: Default to 'appwindow = false' to avoid taskbar icon
            let appwindow = config.appwindow.unwrap_or(false); 

            let mut style = if noactivate {
                WS_EX_NOACTIVATE
            } else {
                Default::default()
            };
            // Force layered if click_through is requested (required for transparency behavior)
            if layered || click_through {
                style |= WS_EX_LAYERED;
            }
            if click_through {
                style |= WS_EX_TRANSPARENT;
            }
            if toolwindow {
                style |= WS_EX_TOOLWINDOW;
            } 
            // Apply AppWindow only if specifically requested and NOT toolwindow
            if appwindow && !toolwindow {
                style |= WS_EX_APPWINDOW;
            }
            if topmost {
                style |= WS_EX_TOPMOST;
            }
            style
        };

        // For WS_POPUP, window size = client size (no borders)
        // For WS_OVERLAPPEDWINDOW, adjust so client is the requested size.
        let (adjusted_width, adjusted_height) = if config.overlapped.unwrap_or(true) {
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            };
            let _ = AdjustWindowRectEx(&mut rect, window_style, false, ex_style);
            (rect.right - rect.left, rect.bottom - rect.top)
        } else {
            (width as i32, height as i32)
        };

        let hwnd = match CreateWindowExW(
            ex_style,
            CLASS_NAME,
            w!("RustFrame - Share This Window"),
            window_style,
            x_pos,
            y_pos,
            adjusted_width,
            adjusted_height,
            None,
            None,
            Some(hinstance.into()),
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                error!("Failed to create destination window: {}", e);
                return;
            }
        };

        // Store HWND globally so other threads can send messages
        if let Ok(mut hwnd_lock) = DEST_HWND.lock() {
            *hwnd_lock = hwnd.0 as isize;
        }

        // NOTE: WDA_EXCLUDEFROMCAPTURE would prevent ALL captures (including Google Meet/Zoom)
        // Instead, we rely on low opacity and recommend disabling show_cursor to avoid recursive capture
        // when preview window overlaps capture region

        // In release mode, set opacity for screen sharing visibility
        // Alpha = 255 (fully opaque) ensures screen sharing apps can detect and capture the window
        // Window is positioned at bottom-right corner (on-screen but unobtrusive)
        #[cfg(not(debug_assertions))]
        {
            if config.layered.unwrap_or(false) || config.click_through.unwrap_or(false) {
                // Alpha default = 255 (fully opaque) for screen sharing compatibility
                // Can be overridden via settings.json if needed
                let alpha = config.alpha.unwrap_or(255);
                let result = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
                if result.is_ok() {
                    tracing::debug!(alpha, "Window alpha set for screen sharing visibility");
                } else {
                    tracing::warn!("Failed to set window transparency");
                }
            }
        }

        WINDOW_THREAD_RUNNING.store(true, Ordering::SeqCst);
        tracing::info!(hwnd = ?hwnd, "Destination window created");

        // Send window to back (HWND_BOTTOM) for screen sharing compatibility
        // This keeps the window visible and on-screen but at the lowest z-order,
        // making it less intrusive while remaining detectable by screen sharing apps
        #[cfg(not(debug_assertions))]
        {
            use windows::Win32::UI::WindowsAndMessaging::HWND_BOTTOM;
            let result = SetWindowPos(
                hwnd,
                Some(HWND_BOTTOM),
                0, 0, 0, 0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            
            if result.is_ok() {
                tracing::info!("Preview window sent to back (HWND_BOTTOM) for screen sharing");
            } else {
                tracing::warn!("Failed to send window to back");
            }
        }

        // Initialize DirectX 11 renderer for GPU-accelerated presentation
        // Attempt to create renderer, but continue if it fails (fallback to GDI)
        match D3D11Renderer::new(hwnd, width, height) {
            Ok(renderer) => {
                if let Ok(mut r) = D3D11_RENDERER.lock() {
                    *r = Some(renderer);
                    tracing::info!("DirectX 11 renderer initialized for GPU acceleration");
                } else {
                    tracing::warn!("Failed to store D3D11 renderer (mutex lock failed)");
                }
            }
            Err(e) => {
                tracing::warn!(error = ?e, "Failed to initialize D3D11 renderer, using GDI fallback");
            }
        }

        // Event-driven rendering: frames trigger WM_FRAME_UPDATE, no periodic timer needed

        // Message loop - THIS IS THE KEY!
        // This loop processes ALL messages for this window including mouse clicks
        let mut msg = MSG::default();
        loop {
            // Check stop flag
            if stop_flag.load(Ordering::SeqCst) {
                break;
            }

            // GetMessageW blocks until a message is available
            let result = GetMessageW(&mut msg, None, 0, 0);
            if result.0 <= 0 {
                break; // WM_QUIT or error
            }

            let _ = DispatchMessageW(&msg);
        }

        // Cleanup
        WINDOW_THREAD_RUNNING.store(false, Ordering::SeqCst);
        tracing::debug!("Destination window thread exiting");
    }
}

/// Window procedure - handles all window messages
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{
        DestroyWindow, MA_NOACTIVATE, WM_ACTIVATE, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_MOUSEACTIVATE,
        WM_PAINT, HWND_BOTTOM, SWP_NOSIZE, SWP_NOMOVE, SWP_NOACTIVATE,
    };

    match msg {
        WM_ACTIVATE => {
            // When user clicks on preview window (making it active), immediately send it to back
            // This prevents preview from blocking other windows during Discord setup period
            let activated = (wparam.0 & 0xFFFF) != 0; // WA_INACTIVE = 0, WA_ACTIVE = 1, WA_CLICKACTIVE = 2
            if activated {
                tracing::debug!("Preview window activated by user, sending to back");
                unsafe {
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_BOTTOM),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
            }
            LRESULT(0)
        }
        WM_FRAME_UPDATE => {
            // Frame update signal - just trigger repaint (event-driven)
            // Resize and actual painting will happen in WM_PAINT
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => {
            // Prevent window from being activated on click
            LRESULT(MA_NOACTIVATE as isize)
        }
        WM_PAINT => {
            // Check if masking is enabled (border overlaps desktop)
            let is_masked = MASKING_ENABLED.load(Ordering::SeqCst);
            
            if is_masked {
                // Render solid black color
                let mut ps = PAINTSTRUCT::default();
                unsafe {
                    let hdc = BeginPaint(hwnd, &mut ps);
                    let mut rect = RECT::default();
                    GetClientRect(hwnd, &mut rect);
                    
                    use windows::Win32::Graphics::Gdi::{CreateSolidBrush, FillRect, DeleteObject};
                    let brush = CreateSolidBrush(COLORREF(0x00000000)); // Black
                    FillRect(hdc, &rect, brush);
                    let _ = DeleteObject(brush.into());
                    
                    EndPaint(hwnd, &ps);
                }
                return LRESULT(0);
            }
            
            // Normal rendering path - try GPU path first, fallback to GDI if not available
            let mut gpu_rendered = false;

            // Check if we have GPU texture data
            if let Ok(gpu_data) = GPU_TEXTURE_DATA.try_lock() {
                if let Some(ref data) = *gpu_data {
                    debug!("WM_PAINT: GPU path - texture ptr=0x{:X}", data.texture_ptr);
                    // Try to render with DirectX 11
                    if let Ok(renderer) = D3D11_RENDERER.try_lock() {
                        debug!("WM_PAINT: D3D11 renderer lock acquired");
                        if let Some(ref r) = *renderer {
                            debug!(
                                "WM_PAINT: Calling render_texture with crop {}x{} at ({}, {})",
                                data.crop_width, data.crop_height, data.crop_x, data.crop_y
                            );
                            match r.render_texture(
                                data.texture_ptr,
                                data.crop_x,
                                data.crop_y,
                                data.crop_width,
                                data.crop_height,
                            ) {
                                Ok(_) => {
                                    gpu_rendered = true;
                                    debug!("WM_PAINT: GPU render successful");
                                    // Validate rect to tell Windows the paint is complete
                                    ValidateRect(Some(hwnd), None);
                                }
                                Err(e) => {
                                    tracing::warn!(error = ?e, "GPU render failed, falling back to GDI");
                                }
                            }
                        } else {
                            debug!("WM_PAINT: No D3D11 renderer available (None)");
                        }
                    } else {
                        debug!("WM_PAINT: Could not lock D3D11 renderer (lock failed)");
                    }
                } else {
                    debug!("WM_PAINT: No GPU texture data");
                }
            } else {
                debug!("WM_PAINT: Could not lock GPU_TEXTURE_DATA");
            }

            // If GPU rendering didn't happen, use GDI fallback
            if !gpu_rendered {
                debug!("WM_PAINT: Using GDI fallback");
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);

                if let Ok(buffer_lock) = FRAME_BUFFER.try_lock() {
                    if let Some(ref frame) = *buffer_lock {
                        debug!("WM_PAINT: GDI rendering {}x{}", frame.width, frame.height);
                        paint_frame_gdi(hdc, &frame.data, frame.width, frame.height);
                    } else {
                        debug!("WM_PAINT: No frame data available");
                    }
                } else {
                    debug!("WM_PAINT: Could not lock FRAME_BUFFER");
                }

                let _ = EndPaint(hwnd, &ps);
            }

            LRESULT(0)
        }
        WM_ERASEBKGND => {
            LRESULT(1) // We handle background
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Paint frame to DC using GDI with double buffering
/// This is a standard GDI painting approach that works with all capture methods
unsafe fn paint_frame_gdi(hdc: HDC, data: &[u8], width: u32, height: u32) {
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32), // Negative = top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    // Use StretchDIBits to paint directly from the buffer to the window DC
    // This is faster than CreateDIBSection + BitBlt because it avoids creating/destroying
    // GDI objects (HBITMAP, HDC) on every frame.
    let _ = StretchDIBits(
        hdc,
        0,
        0,
        width as i32,
        height as i32,
        0,
        0,
        width as i32,
        height as i32,
        Some(data.as_ptr() as *const _),
        &bmi,
        DIB_RGB_COLORS,
        SRCCOPY,
    );
}

impl PreviewWindow for DestinationWindow {
    type Config = DestinationWindowConfig;

    fn new(x: i32, y: i32, width: u32, height: u32, config: Self::Config) -> Option<Self>
    where
        Self: Sized,
    {
        DestinationWindow::new(x, y, width, height, config)
    }

    fn hwnd_value(&self) -> isize {
        DEST_HWND.lock().map(|h| *h).unwrap_or(0)
    }

    fn update_frame(&self, data: Vec<u8>, width: u32, height: u32) {
        DestinationWindow::update_frame(self, data, width, height);
    }

    fn render(&mut self, _pixels: &[u8], _width: u32, _height: u32) {
        // Windows implementation uses update_frame() + timer-based rendering
        // This method is not used in the Windows implementation
        // Kept for trait compatibility
    }

    fn resize(&mut self, width: u32, height: u32) {
        // Scale dimensions for DPI (input is in logical points)
        let width_pixels = points_to_pixels(width);
        let height_pixels = points_to_pixels(height);

        // Clear frame buffer to prevent ghost borders
        if let Ok(mut buffer) = FRAME_BUFFER.lock() {
            *buffer = None;
        }
        
        // Clear GPU texture data
        if let Ok(mut data) = GPU_TEXTURE_DATA.lock() {
            *data = None;
        }

        // Resize the destination window
        if let Ok(hwnd_lock) = DEST_HWND.lock() {
            let hwnd_val = *hwnd_lock;
            if hwnd_val != 0 {
                unsafe {
                    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        0,
                        0,
                        width_pixels as i32,
                        height_pixels as i32,
                        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
            }
        }
    }

    fn set_pos(&mut self, x: i32, y: i32) {
        // Scale coordinates for DPI (input is in logical points)
        let display = display_info::get();
        let (x_pixels, y_pixels) = if display.initialized {
            (
                display.points_to_pixels(x as f64),
                display.points_to_pixels(y as f64),
            )
        } else {
            (x, y)
        };

        // Move the destination window
        if let Ok(hwnd_lock) = DEST_HWND.lock() {
            let hwnd_val = *hwnd_lock;
            if hwnd_val != 0 {
                unsafe {
                    let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        x_pixels,
                        y_pixels,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
            }
        }
    }

    fn send_to_back(&self) {
        DestinationWindow::send_to_back(self);
    }

    fn bring_to_front(&self) {
        DestinationWindow::bring_to_front(self);
    }

    fn exclude_from_capture(&self) {
        DestinationWindow::exclude_from_capture(self);
    }
}
