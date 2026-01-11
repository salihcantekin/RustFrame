//! Window Enumeration Module
//!
//! Platform-specific window enumeration for the window exclusion settings dialog.
//! Provides list of running applications and their open windows.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Information about a running window
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableWindow {
    pub id: u32,        // CGWindowID (macOS) or HWND (Windows)
    pub title: String,  // Window title/name
}

/// Information about a running application
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableApp {
    pub bundle_id: String,    // Bundle ID (macOS) or process name (Windows)
    pub app_name: String,     // Display name: "Zoom", "Google Chrome", etc.
    pub windows: Vec<AvailableWindow>,
}

/// Get list of running applications and their open windows
#[cfg(target_os = "macos")]
pub fn get_available_windows() -> anyhow::Result<Vec<AvailableApp>> {
    use objc::class;
    use objc::msg_send;
    use objc::sel;
    use objc::sel_impl;

    // First, get all windows from CGWindowList
    let all_windows = get_all_windows_cg()?;
    
    // Group windows by PID
    let mut windows_by_pid: HashMap<i32, Vec<AvailableWindow>> = HashMap::new();
    for window in all_windows {
        windows_by_pid.entry(window.owning_pid)
            .or_insert_with(Vec::new)
            .push(AvailableWindow {
                id: window.window_id,
                title: window.title,
            });
    }

    let mut apps = Vec::new();

    unsafe {
        // Get NSWorkspace.sharedWorkspace.runningApplications
        let workspace_cls = class!(NSWorkspace);
        let workspace: *mut objc::runtime::Object =
            msg_send![workspace_cls, performSelector: sel!(sharedWorkspace)];

        let running_apps: *mut objc::runtime::Object = msg_send![workspace, runningApplications];
        if running_apps.is_null() {
            return Ok(apps);
        }

        let count: usize = msg_send![running_apps, count];
        log::debug!("[WinEnum] Found {} running applications", count);

        for i in 0..count {
            let app: *mut objc::runtime::Object = msg_send![running_apps, objectAtIndex: i];
            if app.is_null() {
                continue;
            }

            // Get bundle identifier
            let bundle_id: *mut objc::runtime::Object = msg_send![app, bundleIdentifier];
            let bundle_id_str = if !bundle_id.is_null() {
                let cstr: *const std::os::raw::c_char = msg_send![bundle_id, UTF8String];
                if !cstr.is_null() {
                    std::ffi::CStr::from_ptr(cstr).to_string_lossy().to_string()
                } else {
                    continue; // Skip apps without bundle ID
                }
            } else {
                continue;
            };

            // Get application name
            let app_name: *mut objc::runtime::Object = msg_send![app, localizedName];
            let app_name_str = if !app_name.is_null() {
                let cstr: *const std::os::raw::c_char = msg_send![app_name, UTF8String];
                if !cstr.is_null() {
                    std::ffi::CStr::from_ptr(cstr).to_string_lossy().to_string()
                } else {
                    bundle_id_str.clone()
                }
            } else {
                bundle_id_str.clone()
            };

            // Get PID and lookup windows
            let pid: i32 = msg_send![app, processIdentifier];
            let windows = windows_by_pid.remove(&pid).unwrap_or_default();

            if !windows.is_empty() {
                log::debug!("[WinEnum] App: {} ({}), Windows: {}", app_name_str, bundle_id_str, windows.len());
                apps.push(AvailableApp {
                    bundle_id: bundle_id_str,
                    app_name: app_name_str,
                    windows,
                });
            }
        }
    }

    Ok(apps)
}

#[cfg(target_os = "macos")]
struct WindowInfoInternal {
    window_id: u32,
    title: String,
    owning_pid: i32,
}

#[cfg(target_os = "macos")]
fn get_all_windows_cg() -> anyhow::Result<Vec<WindowInfoInternal>> {
    use core_foundation::array::{CFArray, CFArrayRef};
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;

    extern "C" {
        fn CGWindowListCopyWindowInfo(option: u32, relativeToWindow: u32) -> CFArrayRef;
    }

    const kCGWindowListExcludeDesktopElements: u32 = 1 << 4;
    const kCGWindowListOptionOnScreenOnly: u32 = 1 << 0;

    let mut windows = Vec::new();

    unsafe {
        let options = kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;
        let window_list_ref = CGWindowListCopyWindowInfo(options, 0);
        
        if window_list_ref.is_null() {
            return Ok(windows);
        }

        let window_list = CFArray::<CFDictionary>::wrap_under_create_rule(window_list_ref);
        let count = window_list.len();
        
        log::debug!("[WinEnum] CGWindowList returned {} windows", count);

        for i in 0..count {
            let dict = window_list.get(i as isize);
            if dict.is_none() {
                continue;
            }
            let dict = dict.unwrap();

            // Get window ID (kCGWindowNumber)
            let window_id_key = CFString::from_static_string("kCGWindowNumber");
            let window_id = dict.find(window_id_key.as_CFTypeRef() as *const _)
                .and_then(|val_ref| unsafe {
                    let cf_num = CFNumber::wrap_under_get_rule(val_ref.cast());
                    cf_num.to_i64()
                })
                .unwrap_or(0) as u32;

            if window_id == 0 {
                continue;
            }

            // Get window name (kCGWindowName)
            let name_key = CFString::from_static_string("kCGWindowName");
            let title = dict.find(name_key.as_CFTypeRef() as *const _)
                .and_then(|val_ref| unsafe {
                    let cf_str = CFString::wrap_under_get_rule(val_ref.cast());
                    Some(cf_str.to_string())
                })
                .unwrap_or_else(String::new);

            // Skip windows without titles (usually system windows)
            if title.is_empty() {
                continue;
            }

            // Get owning PID (kCGWindowOwnerPID)
            let pid_key = CFString::from_static_string("kCGWindowOwnerPID");
            let owning_pid = dict.find(pid_key.as_CFTypeRef() as *const _)
                .and_then(|val_ref| unsafe {
                    let cf_num = CFNumber::wrap_under_get_rule(val_ref.cast());
                    cf_num.to_i64()
                })
                .unwrap_or(0) as i32;

            if owning_pid == 0 {
                continue;
            }

            windows.push(WindowInfoInternal {
                window_id,
                title,
                owning_pid,
            });
        }
    }

    log::debug!("[WinEnum] Filtered to {} windows with titles", windows.len());
    Ok(windows)
}

#[cfg(target_os = "windows")]
pub fn get_available_windows() -> anyhow::Result<Vec<AvailableApp>> {
    // TODO: Implement Windows version using EnumWindows + GetWindowThreadProcessId
    // This will be implemented in Phase 2
    log::info!("[WinEnum] Windows implementation pending - returning empty list");
    Ok(Vec::new())
}

#[cfg(target_os = "linux")]
pub fn get_available_windows() -> anyhow::Result<Vec<AvailableApp>> {
    // TODO: Implement Linux version using X11/Wayland window enumeration
    // This will be implemented in Phase 2
    log::info!("[WinEnum] Linux implementation pending - returning empty list");
    Ok(Vec::new())
}
