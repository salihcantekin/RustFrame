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
    BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, EndPaint,
    GetDC, InvalidateRect, ReleaseDC, SelectObject, ValidateRect, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HDC, PAINTSTRUCT, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect,
    GetMessageW, GetSystemMetrics, GetWindowRect, PostMessageW, PostQuitMessage, RegisterClassExW,
    SetWindowPos, CS_HREDRAW, CS_VREDRAW, MSG, SM_CXSCREEN, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, WM_USER, WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW,
    WS_POPUP, WS_VISIBLE,
};

// Only used in release builds for positioning.
#[cfg(not(debug_assertions))]
use windows::Win32::UI::WindowsAndMessaging::SM_CYSCREEN;

#[cfg(not(debug_assertions))]
use windows::Win32::Foundation::COLORREF;

use windows::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(not(debug_assertions))]
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
    pub fn new(width: u32, height: u32, config: DestinationWindowConfig) -> Option<Self> {
        // Scale dimensions for DPI (input is in logical points)
        let width_pixels = points_to_pixels(width);
        let height_pixels = points_to_pixels(height);

        info!(
            "Creating WinAPI destination window {}x{} points ({}x{} pixels) in dedicated thread",
            width, height, width_pixels, height_pixels
        );

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        // Spawn window thread - window will be created and message loop will run here
        let thread_handle = thread::spawn(move || {
            run_window_thread(width_pixels, height_pixels, config, stop_flag_clone);
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
                style: CS_HREDRAW | CS_VREDRAW,
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

        // Position window on-screen for proper DWM composition
        // The window will be cloaked (hidden from user) but still composed by DWM
        // DEBUG: top-right corner for easy visibility and testing (not cloaked)
        // RELEASE: bottom-right corner, will be cloaked after creation
        #[cfg(debug_assertions)]
        let (x_pos, y_pos) = {
            let screen_width = GetSystemMetrics(SM_CXSCREEN);
            (screen_width - width as i32 - 20, 20)
        };
        #[cfg(not(debug_assertions))]
        let (x_pos, y_pos) = {
            let screen_width = GetSystemMetrics(SM_CXSCREEN);
            let screen_height = GetSystemMetrics(SM_CYSCREEN);
            // Position at bottom-right, above taskbar
            (
                screen_width - width as i32 - 20,
                screen_height - height as i32 - 80,
            )
        };

        // Window style:
        // - Default: WS_POPUP for borderless window
        // - Optional: WS_OVERLAPPEDWINDOW to look like a normal app window (can improve picker visibility)
        let window_style = if config.overlapped.unwrap_or(false) {
            WS_OVERLAPPEDWINDOW | WS_VISIBLE
        } else {
            WS_POPUP | WS_VISIBLE
        };

        let topmost = config.topmost.unwrap_or(true);

        // Extended styles:
        // - WS_EX_NOACTIVATE: Don't steal focus when created
        // - WS_EX_TOPMOST: Empirically required for Google Meet window-share to keep capturing updates
        // - WS_EX_LAYERED (release only): Allow transparency to make window nearly invisible
        // - WS_EX_TRANSPARENT (release only): Click-through so it doesn't block the user's screen
        // - WS_EX_TOOLWINDOW (release only): Keep it out of Alt-Tab/taskbar UI
        let noactivate = config.noactivate.unwrap_or(true);

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
            let layered = config.layered.unwrap_or(true);
            let click_through = config.click_through.unwrap_or(true);
            let toolwindow = config.toolwindow.unwrap_or(true);
            let appwindow = config.appwindow.unwrap_or(false);

            let mut style = if noactivate {
                WS_EX_NOACTIVATE
            } else {
                Default::default()
            };
            if layered {
                style |= WS_EX_LAYERED;
            }
            if click_through {
                style |= WS_EX_TRANSPARENT;
            }
            if toolwindow {
                style |= WS_EX_TOOLWINDOW;
            } else if appwindow {
                style |= WS_EX_APPWINDOW;
            }
            if topmost {
                style |= WS_EX_TOPMOST;
            }
            style
        };

        // For WS_POPUP, window size = client size (no borders)
        // For WS_OVERLAPPEDWINDOW, adjust so client is the requested size.
        let (adjusted_width, adjusted_height) = if config.overlapped.unwrap_or(false) {
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
            w!("RustFrame Preview"),
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

        // In release mode, set very low opacity (1/255 = 0.4%) to make window nearly invisible
        // The window is still rendered and should be capturable by screen sharing apps
        #[cfg(not(debug_assertions))]
        {
            if config.layered.unwrap_or(true) {
                // Alpha value 0 = fully transparent.
                // Some apps treat fully transparent windows specially, but many window-share flows still capture it.
                // You can override this via settings.json.
                let alpha = config.alpha.unwrap_or(0);
                let result = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
                if result.is_ok() {
                    tracing::debug!(alpha, "Window alpha set");
                } else {
                    tracing::warn!("Failed to set window transparency");
                }
            }
        }

        WINDOW_THREAD_RUNNING.store(true, Ordering::SeqCst);
        tracing::info!(hwnd = ?hwnd, "Destination window created");

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
        DestroyWindow, MA_NOACTIVATE, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_MOUSEACTIVATE,
        WM_PAINT,
    };

    match msg {
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
            // Try GPU path first, fallback to GDI if not available
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
    // Create a memory DC for double buffering
    let mem_dc = CreateCompatibleDC(Some(hdc));
    if mem_dc.is_invalid() {
        return;
    }

    // Create DIB section - this creates a device-independent bitmap
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32), // Negative = top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [Default::default()],
    };

    let mut bits_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
    let dib = CreateDIBSection(Some(hdc), &bmi, DIB_RGB_COLORS, &mut bits_ptr, None, 0);

    if let Ok(bitmap) = dib {
        if !bits_ptr.is_null() {
            // Copy frame data to DIB section
            let expected_size = (width * height * 4) as usize;
            if data.len() >= expected_size {
                std::ptr::copy_nonoverlapping(data.as_ptr(), bits_ptr as *mut u8, expected_size);
            }

            // Select DIB into memory DC and blit to window
            let old_bitmap = SelectObject(mem_dc, bitmap.into());
            let _ = BitBlt(
                hdc,
                0,
                0,
                width as i32,
                height as i32,
                Some(mem_dc),
                0,
                0,
                SRCCOPY,
            );

            // Cleanup
            SelectObject(mem_dc, old_bitmap);
            let _ = DeleteObject(bitmap.into());
        }
    }

    let _ = DeleteDC(mem_dc);
}

impl PreviewWindow for DestinationWindow {
    type Config = DestinationWindowConfig;

    fn new(width: u32, height: u32, config: Self::Config) -> Option<Self>
    where
        Self: Sized,
    {
        DestinationWindow::new(width, height, config)
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
}
