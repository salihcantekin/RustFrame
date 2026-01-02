//! Mouse Click Hook - Captures mouse clicks for highlight overlay
//!
//! Uses Windows low-level mouse hook to detect clicks and store them
//! for rendering as dissolving circles.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use log::info;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx,
    HHOOK, MSLLHOOKSTRUCT, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_RBUTTONDOWN, WM_MBUTTONDOWN,
};

/// A recorded mouse click
#[derive(Debug, Clone)]
pub struct ClickEvent {
    pub x: i32,
    pub y: i32,
    pub timestamp: Instant,
    pub button: MouseButton,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

// Global state for hook - using AtomicIsize for the hook handle (HHOOK contains a pointer)
static HOOK_HANDLE: AtomicIsize = AtomicIsize::new(0);
static HOOK_ACTIVE: AtomicBool = AtomicBool::new(false);
static CLICKS: Mutex<Vec<ClickEvent>> = Mutex::new(Vec::new());

/// Start capturing mouse clicks
pub fn start_capture() -> bool {
    if HOOK_ACTIVE.load(Ordering::SeqCst) {
        return true; // Already active
    }
    
    unsafe {
        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0);
        
        match hook {
            Ok(h) => {
                HOOK_HANDLE.store(h.0 as isize, Ordering::SeqCst);
                HOOK_ACTIVE.store(true, Ordering::SeqCst);
                info!("Mouse hook installed");
                true
            }
            Err(e) => {
                log::error!("Failed to install mouse hook: {:?}", e);
                false
            }
        }
    }
}

/// Stop capturing mouse clicks
pub fn stop_capture() {
    if !HOOK_ACTIVE.load(Ordering::SeqCst) {
        return;
    }
    
    let hook_ptr = HOOK_HANDLE.swap(0, Ordering::SeqCst);
    if hook_ptr != 0 {
        unsafe {
            let hhook = HHOOK(hook_ptr as *mut _);
            let _ = UnhookWindowsHookEx(hhook);
        }
    }
    
    HOOK_ACTIVE.store(false, Ordering::SeqCst);
    
    // Clear clicks
    CLICKS.lock().unwrap().clear();
    
    info!("Mouse hook removed");
}

/// Get recent clicks (within specified duration)
pub fn get_recent_clicks(max_age_ms: u32) -> Vec<ClickEvent> {
    let now = Instant::now();
    let max_age = std::time::Duration::from_millis(max_age_ms as u64);
    
    let mut clicks = CLICKS.lock().unwrap();
    
    // Remove old clicks
    clicks.retain(|c| now.duration_since(c.timestamp) < max_age);
    
    // Return remaining clicks with their age
    clicks.clone()
}

/// Calculate opacity for a click based on its age
pub fn calculate_opacity(click: &ClickEvent, dissolve_ms: u32) -> f32 {
    let age = Instant::now().duration_since(click.timestamp).as_millis() as f32;
    let max_age = dissolve_ms as f32;
    
    if age >= max_age {
        0.0
    } else {
        1.0 - (age / max_age)
    }
}

/// Low-level mouse hook procedure
unsafe extern "system" fn mouse_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code >= 0 {
        let button = match wparam.0 as u32 {
            WM_LBUTTONDOWN => Some(MouseButton::Left),
            WM_RBUTTONDOWN => Some(MouseButton::Right),
            WM_MBUTTONDOWN => Some(MouseButton::Middle),
            _ => None,
        };
        
        if let Some(btn) = button {
            let mouse_struct = &*(lparam.0 as *const MSLLHOOKSTRUCT);
            
            info!("Mouse click detected at ({}, {}) - {:?}", 
                mouse_struct.pt.x, mouse_struct.pt.y, btn);
            
            let click = ClickEvent {
                x: mouse_struct.pt.x,
                y: mouse_struct.pt.y,
                timestamp: Instant::now(),
                button: btn,
            };
            
            // Add to clicks list (limit size)
            if let Ok(mut clicks) = CLICKS.lock() {
                clicks.push(click);
                // Keep only last 20 clicks
                if clicks.len() > 20 {
                    clicks.remove(0);
                }
            }
        }
    }
    
    CallNextHookEx(None, code, wparam, lparam)
}
