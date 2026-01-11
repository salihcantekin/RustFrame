//! Window Enumeration Module
//!
//! Platform-specific window enumeration for the window exclusion settings dialog.
//! Provides list of running applications and their open windows.

use serde::{Deserialize, Serialize};

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

            // For now, return app with empty windows list
            // In production, we would enumerate windows for this app using CGWindowListCopyWindowInfo
            log::debug!("[WinEnum] App: {} ({})", app_name_str, bundle_id_str);
            apps.push(AvailableApp {
                bundle_id: bundle_id_str,
                app_name: app_name_str,
                windows: Vec::new(), // TODO: Enumerate windows for this app
            });
        }
    }

    Ok(apps)
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
