//! Windows Separation Layer Window
//! 
//! Mimics RegionToShare's approach: A window between border and preview
//! in z-order that shows solid color when border is over desktop.

use lazy_static::lazy_static;
use std::sync::Mutex;
use std::thread;
use std::sync::mpsc;
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM, RECT, HINSTANCE};
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect};
use windows::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject, FillRect, HDC, HBRUSH, HGDIOBJ};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassExW, SetWindowPos,
    CS_HREDRAW, CS_VREDRAW, HWND_BOTTOM, SWP_NOACTIVATE, SWP_SHOWWINDOW, WNDCLASSEXW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP, MSG, GetMessageW, TranslateMessage, DispatchMessageW,
    PostQuitMessage, SWP_ASYNCWINDOWPOS 
};

const CLASS_NAME: &str = "RustFrameSeparationLayer";

lazy_static! {
    static ref SEPARATION_HWND: Mutex<isize> = Mutex::new(0);
    static ref SEPARATION_BRUSH: Mutex<isize> = Mutex::new(0);
}

pub struct SeparationLayer {
    hwnd: isize,
    #[allow(dead_code)] // Keep handle to prevent detach if we wanted to join, but we let it run
    thread_handle: Option<thread::JoinHandle<()>>,
}

unsafe impl Send for SeparationLayer {}
unsafe impl Sync for SeparationLayer {}

impl SeparationLayer {
    /// Create separation layer window
    /// Color format: 0xRRGGBB (e.g., 0x4682B4 for Steel Blue)
    pub fn new(x: i32, y: i32, width: i32, height: i32, color: u32) -> Option<Self> {
        let (tx, rx) = mpsc::channel();

        // Spawn a dedicated thread for the window to ensure it has a message loop
        // This prevents "Not Responding" and allows SetWindowPos to work correctly
        let thread_handle = thread::spawn(move || {
            unsafe {
                // Register window class
                let hinstance = match GetModuleHandleW(None) {
                    Ok(h) => h,
                    Err(_) => {
                        let _ = tx.send(None);
                        return;
                    }
                };
                
                let class_name_wide: Vec<u16> = CLASS_NAME
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                
                let wc = WNDCLASSEXW {
                    cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                    style: CS_HREDRAW | CS_VREDRAW,
                    lpfnWndProc: Some(wndproc),
                    hInstance: hinstance.into(),
                    lpszClassName: windows::core::PCWSTR(class_name_wide.as_ptr()),
                    ..Default::default()
                };
                
                RegisterClassExW(&wc);
                
                // Create brush for background color
                // Convert 0xRRGGBB to Windows COLORREF (0x00BBGGRR)
                let r = (color >> 16) & 0xFF;
                let g = (color >> 8) & 0xFF;
                let b = color & 0xFF;
                let colorref = b << 16 | g << 8 | r;
                
                let brush = CreateSolidBrush(windows::Win32::Foundation::COLORREF(colorref));
                *SEPARATION_BRUSH.lock().unwrap() = brush.0 as isize;
                
                // Create window
                let hwnd = CreateWindowExW(
                    WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                    windows::core::PCWSTR(class_name_wide.as_ptr()),
                    w!(""),
                    WS_POPUP,
                    x,
                    y,
                    width,
                    height,
                    None,
                    None,
                    Some(HINSTANCE(hinstance.0)),
                    None,
                );

                if let Ok(hwnd) = hwnd {
                    let hwnd_val = hwnd.0 as isize;
                    *SEPARATION_HWND.lock().unwrap() = hwnd_val;
                    
                    // DO NOT exclude from capture - we WANT the blue screen to be visible in shared view
                    // The separation layer should appear in the capture stream so users see it in Meet/Discord
                    // Only the DestinationWindow (preview) and HollowBorder should be excluded
                    log::info!("✅ Separation layer created (will be visible in capture)");
                    
                    // Position at HWND_BOTTOM (above desktop, below everything else)
                    let _ = SetWindowPos(
                        hwnd,
                        Some(HWND_BOTTOM),
                        x,
                        y,
                        width,
                        height,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                    
                    log::info!("✅ Separation layer created at ({}, {}) {}x{} with color 0x{:06X}", x, y, width, height, color);
                    let _ = tx.send(Some(hwnd_val));
                } else {
                    let _ = tx.send(None);
                    return;
                }

                // Message loop
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).into() {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        });

        // Wait for window creation
        match rx.recv() {
            Ok(Some(hwnd_val)) => Some(Self { 
                hwnd: hwnd_val,
                thread_handle: Some(thread_handle)
            }),
            _ => None,
        }
    }
    
    /// Update position and size (called when border moves/resizes)
    pub fn update_position(&self, x: i32, y: i32, width: i32, height: i32) {
        // Use SWP_ASYNCWINDOWPOS to prevent blocking if the window thread is busy
        // This decouples the caller (callback thread) from the window thread
        use windows::Win32::UI::WindowsAndMessaging::SWP_NOZORDER;
        
        // Remove HWND_BOTTOM to avoid pushing separation layer below the preview window
        // Use SWP_NOZORDER to maintain current z-order (Separation > Preview)
        let result = unsafe {
            SetWindowPos(
                HWND(self.hwnd as *mut _),
                None, // Previously Some(HWND_BOTTOM)
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_ASYNCWINDOWPOS | SWP_NOZORDER,
            )
        };
        
        if result.is_ok() {
            // Log less frequently to avoid spam
            // log::info!("✅ [SEP-LAYER] SetWindowPos succeeded"); 
        } else {
            log::error!("❌ [SEP-LAYER] SetWindowPos FAILED: {:?}", result.err());
        }
    }

    pub fn hwnd_value(&self) -> isize {
        self.hwnd
    }
}

impl Drop for SeparationLayer {
    fn drop(&mut self) {
        unsafe {
            let hwnd_val = self.hwnd;
            // Post WM_CLOSE or DestroyWindow? 
            // Since it's on another thread, we should post WM_CLOSE or quit message
            use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
            
            if hwnd_val != 0 {
                // Request window to close its thread loop
                let _ = PostMessageW(Some(HWND(hwnd_val as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        
        // We can't join the thread easily here because we want to return quickly,
        // but the thread should exit when it processes WM_CLOSE/WM_QUIT
    }
}


impl SeparationLayer {
    /// Show the separation layer
    pub fn show(&self) {
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
            let _ = ShowWindow(HWND(self.hwnd as _), SW_SHOWNOACTIVATE);
        }
    }
    
    /// Hide the separation layer
    pub fn hide(&self) {
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
            let _ = ShowWindow(HWND(self.hwnd as _), SW_HIDE);
        }
    }
}

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        0x0014 => {
            // WM_ERASEBKGND - paint with our brush
            if let Ok(brush_lock) = SEPARATION_BRUSH.lock() {
                let brush = *brush_lock;
                if brush != 0 {
                    let hdc = HDC(wparam.0 as _);
                    let mut rect = RECT::default();
                    let _ = unsafe { GetClientRect(hwnd, &mut rect) };
                    unsafe { FillRect(hdc, &rect, HBRUSH(brush as _)) };
                    return LRESULT(1);
                }
            }
            LRESULT(0)
        }
        windows::Win32::UI::WindowsAndMessaging::WM_DESTROY => {
            PostQuitMessage(0);
            
            // Clean up brush when window block is destroyed (if we are the last one)
            // Ideally we'd do this after thread loop, but this works
            if let Ok(mut brush_lock) = SEPARATION_BRUSH.lock() {
                 let brush = *brush_lock;
                 if brush != 0 {
                     let _ = DeleteObject(HGDIOBJ(brush as _));
                     *brush_lock = 0;
                 }
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
