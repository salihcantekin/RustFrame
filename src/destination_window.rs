//! Pure WinAPI Destination Window
//! 
//! Lightweight window for screen sharing - no Iced/wgpu overhead.
//! Uses GDI for frame rendering.

use std::sync::atomic::{AtomicBool, Ordering};
use std::mem;

use windows::core::{PCWSTR, w};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, PAINTSTRUCT,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC,
    StretchDIBits, SRCCOPY, GetDC, ReleaseDC,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW,
    PeekMessageW, PostQuitMessage, RegisterClassExW, ShowWindow, TranslateMessage,
    WNDCLASSEXW, CS_HREDRAW, CS_VREDRAW, WS_POPUP, WS_VISIBLE,
    WS_EX_TOOLWINDOW, WS_EX_NOACTIVATE, MSG, PM_REMOVE,
    SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
    SW_SHOW, SW_HIDE,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;

use log::{info, error};

const CLASS_NAME: PCWSTR = w!("RustFrameDestination");
const OFFSCREEN_X: i32 = -10000;
const OFFSCREEN_Y: i32 = -10000;

static WINDOW_RUNNING: AtomicBool = AtomicBool::new(false);

/// Pure WinAPI destination window
pub struct DestinationWindow {
    hwnd: HWND,
    width: u32,
    height: u32,
}

impl DestinationWindow {
    /// Create a new destination window
    pub fn new(width: u32, height: u32) -> Option<Self> {
        info!("Creating WinAPI destination window {}x{}", width, height);
        
        unsafe {
            let hinstance = GetModuleHandleW(None).ok()?;
            
            // Register window class (only once)
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
                    return None;
                }
            }
            
            // Create window off-screen with TOOLWINDOW style
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                CLASS_NAME,
                w!("RustFrame Capture"),
                WS_POPUP | WS_VISIBLE,
                OFFSCREEN_X,
                OFFSCREEN_Y,
                width as i32,
                height as i32,
                None,
                None,
                Some(hinstance.into()),
                None,
            ).ok()?;
            
            WINDOW_RUNNING.store(true, Ordering::SeqCst);
            
            info!("WinAPI destination window created: {:?}", hwnd);
            
            Some(Self {
                hwnd,
                width,
                height,
            })
        }
    }
    
    /// Update the frame to display - paints directly to DC (no WM_PAINT needed)
    pub fn update_frame(&self, data: Vec<u8>, width: u32, height: u32) {
        // Resize window if needed
        if width != self.width || height != self.height {
            unsafe {
                let _ = SetWindowPos(
                    self.hwnd,
                    None,
                    OFFSCREEN_X,
                    OFFSCREEN_Y,
                    width as i32,
                    height as i32,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
        }
        
        // Paint directly to window DC (off-screen windows don't get WM_PAINT)
        unsafe {
            let hdc = GetDC(Some(self.hwnd));
            if !hdc.is_invalid() {
                paint_frame_direct(hdc, &data, width, height);
                let _ = ReleaseDC(Some(self.hwnd), hdc);
            }
        }
    }
    
    /// Process pending window messages (call from main loop)
    pub fn process_messages(&self) {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, Some(self.hwnd), 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
    
    /// Get the window handle (for screen sharing picker)
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }
    
    /// Show the window
    pub fn show(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
        }
    }
    
    /// Hide the window  
    pub fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }
}

impl Drop for DestinationWindow {
    fn drop(&mut self) {
        info!("Destroying WinAPI destination window");
        WINDOW_RUNNING.store(false, Ordering::SeqCst);
        
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

/// Window procedure
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{WM_PAINT, WM_DESTROY, WM_ERASEBKGND};
    
    match msg {
        WM_PAINT => {
            // Off-screen windows rarely get WM_PAINT, but handle it anyway
            let mut ps = PAINTSTRUCT::default();
            let _hdc = BeginPaint(hwnd, &mut ps);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => {
            LRESULT(1)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Paint frame directly to DC (for off-screen windows that don't get WM_PAINT)
unsafe fn paint_frame_direct(hdc: HDC, data: &[u8], width: u32, height: u32) {
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
    
    // Draw at 1:1 scale - window is already sized to content
    StretchDIBits(
        hdc,
        0, 0,
        width as i32, height as i32,
        0, 0,
        width as i32, height as i32,
        Some(data.as_ptr() as *const _),
        &bmi,
        DIB_RGB_COLORS,
        SRCCOPY,
    );
}
