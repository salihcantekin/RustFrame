//! Cross-platform window enumeration facade
//!
//! Provides a shared trait and platform-specific implementations to enumerate
//! shareable windows/apps for the Share Content dialog.

use serde::{Deserialize, Serialize};

/// A window exposed for sharing/exclusion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableWindow {
    /// Platform window identifier (CGWindowID / HWND / X11/Wayland id).
    pub id: u32,
    /// User-visible title; empty titles are filtered out upstream.
    pub title: String,
}

/// An application with its enumerated windows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableApp {
    /// Bundle ID on macOS; process/exe name on Windows; app identifier on Linux.
    pub bundle_id: String,
    /// Display name, e.g., "Zoom" or "Google Chrome".
    pub app_name: String,
    pub windows: Vec<AvailableWindow>,
}

/// Contract every platform must implement to participate in Share Content.
pub trait WindowEnumerator {
    /// Enumerate shareable windows grouped by application.
    fn enumerate_windows() -> anyhow::Result<Vec<AvailableApp>>;

    /// Whether platform reports per-window sharing state; used for diagnostics.
    #[allow(dead_code)]
    fn supports_sharing_state() -> bool {
        false
    }
}

/// Platform-dispatched enumeration entrypoint.
pub fn enumerate_windows() -> anyhow::Result<Vec<AvailableApp>> {
    PlatformWindowEnumerator::enumerate_windows()
}

// --- macOS implementation -------------------------------------------------

#[cfg(target_os = "macos")]
mod macos {
    use super::{AvailableApp, AvailableWindow, WindowEnumerator};
    use anyhow::Result;
    use core_foundation::array::{CFArray, CFArrayRef};
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use objc::class;
    use objc::msg_send;
    use objc::sel;
    use objc::sel_impl;
    use std::collections::HashMap;

    pub struct PlatformWindowEnumerator;

    impl WindowEnumerator for PlatformWindowEnumerator {
        fn enumerate_windows() -> Result<Vec<AvailableApp>> {
            // System services that should not appear in the shareable list
            const IGNORED_BUNDLE_IDS: &[&str] = &[
                "com.apple.dock",
                "com.apple.controlcenter",
                "com.apple.notificationcenterui",
                "com.apple.WindowServer",
                "com.apple.Spotlight",
                "com.apple.systemuiserver",
                "com.apple.coreservices.uiagent",
            ];

            // First, get all windows from CGWindowList
            let all_windows = get_all_windows_cg()?;

            // Group windows by PID
            let mut windows_by_pid: HashMap<i32, Vec<AvailableWindow>> = HashMap::new();
            for window in all_windows {
                windows_by_pid
                    .entry(window.owning_pid)
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

                    // Ignore system UI agents and background services
                    if IGNORED_BUNDLE_IDS.contains(&bundle_id_str.as_str()) {
                        continue;
                    }

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
                        log::debug!(
                            "[WinEnum] App: {} ({}), Windows: {}",
                            app_name_str,
                            bundle_id_str,
                            windows.len()
                        );
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

        fn supports_sharing_state() -> bool {
            true
        }
    }

    #[derive(Debug)]
    struct WindowInfoInternal {
        window_id: u32,
        title: String,
        owning_pid: i32,
    }

    fn get_all_windows_cg() -> Result<Vec<WindowInfoInternal>> {
        extern "C" {
            fn CGWindowListCopyWindowInfo(option: u32, relativeToWindow: u32) -> CFArrayRef;
        }

        const KCG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS: u32 = 1 << 4;
        const KCG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY: u32 = 1 << 0;
        const KCG_WINDOW_SHARING_NONE: i32 = 0;

        let mut windows = Vec::new();

        unsafe {
            let options = KCG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY | KCG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS;
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
                let window_id = dict
                    .find(window_id_key.as_CFTypeRef() as *const _)
                    .and_then(|val_ref| {
                        let cf_num = CFNumber::wrap_under_get_rule(val_ref.cast());
                        cf_num.to_i64()
                    })
                    .unwrap_or(0) as u32;

                if window_id == 0 {
                    continue;
                }

                // Get window layer (kCGWindowLayer) and keep only layer 0 (normal windows)
                let layer_key = CFString::from_static_string("kCGWindowLayer");
                let layer = dict
                    .find(layer_key.as_CFTypeRef() as *const _)
                    .and_then(|val_ref| {
                        let cf_num = CFNumber::wrap_under_get_rule(val_ref.cast());
                        cf_num.to_i64()
                    })
                    .unwrap_or(0) as i32;

                if layer != 0 {
                    continue;
                }

                // Get window name (kCGWindowName)
                let name_key = CFString::from_static_string("kCGWindowName");
                let title = dict
                    .find(name_key.as_CFTypeRef() as *const _)
                    .and_then(|val_ref| {
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
                let owning_pid = dict
                    .find(pid_key.as_CFTypeRef() as *const _)
                    .and_then(|val_ref| {
                        let cf_num = CFNumber::wrap_under_get_rule(val_ref.cast());
                        cf_num.to_i64()
                    })
                    .unwrap_or(0) as i32;

                if owning_pid == 0 {
                    continue;
                }

                // Get sharing state (kCGWindowSharingState) and keep only shareable windows
                let sharing_key = CFString::from_static_string("kCGWindowSharingState");
                let sharing_state = dict
                    .find(sharing_key.as_CFTypeRef() as *const _)
                    .and_then(|val_ref| {
                        let cf_num = CFNumber::wrap_under_get_rule(val_ref.cast());
                        cf_num.to_i64()
                    })
                    .unwrap_or(1) as i32;

                if sharing_state == KCG_WINDOW_SHARING_NONE {
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
}

#[cfg(target_os = "macos")]
use macos::PlatformWindowEnumerator;

// --- Windows implementation (placeholder) -------------------------------

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::{AvailableApp, WindowEnumerator};
    use anyhow::Result;
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, MAX_PATH};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows::Win32::UI::Shell::QueryFullProcessImageNameW;
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowLongW, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, GWL_EXSTYLE, WS_EX_TOOLWINDOW};

    pub struct PlatformWindowEnumerator;

    impl WindowEnumerator for PlatformWindowEnumerator {
        fn enumerate_windows() -> Result<Vec<AvailableApp>> {
            let mut windows_by_pid: HashMap<u32, Vec<super::AvailableWindow>> = HashMap::new();

            unsafe {
                EnumWindows(Some(enum_window), LPARAM(&mut windows_by_pid as *mut _ as isize)).ok();
            }

            // Group by process image (exe stem) for user-facing app name
            let mut apps: Vec<AvailableApp> = Vec::new();
            for (pid, windows) in windows_by_pid {
                let (bundle_id, app_name) = match process_identity(pid) {
                    Some(name) => (name.clone(), name),
                    None => (format!("pid:{}", pid), format!("Process {}", pid)),
                };

                if windows.is_empty() {
                    continue;
                }

                apps.push(AvailableApp {
                    bundle_id,
                    app_name,
                    windows,
                });
            }

            Ok(apps)
        }
    }

    /// Callback for EnumWindows
    unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let map_ptr = lparam.0 as *mut HashMap<u32, Vec<super::AvailableWindow>>;
        if map_ptr.is_null() {
            return BOOL(0);
        }

        // Skip invisible windows
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }

        // Skip tool windows (utility/notification windows that are not shareable)
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if ex_style & WS_EX_TOOLWINDOW.0 as u32 != 0 {
            return BOOL(1);
        }

        // Skip cloaked windows (not actually visible to the user)
        let mut cloaked: i32 = 0;
        let _ = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut _, std::mem::size_of::<i32>() as u32);
        if cloaked != 0 {
            return BOOL(1);
        }

        // Require a non-empty title
        let length = GetWindowTextLengthW(hwnd);
        if length == 0 {
            return BOOL(1);
        }
        let mut buffer: Vec<u16> = vec![0; (length + 1) as usize];
        let read = GetWindowTextW(hwnd, &mut buffer);
        if read == 0 {
            return BOOL(1);
        }
        buffer.truncate(read as usize);
        let title = String::from_utf16_lossy(&buffer);
        if title.trim().is_empty() {
            return BOOL(1);
        }

        // PID
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return BOOL(1);
        }

        (*map_ptr)
            .entry(pid)
            .or_default()
            .push(super::AvailableWindow {
                id: hwnd.0 as u32,
                title,
            });

        BOOL(1)
    }

    /// Resolve process name (exe stem) from PID.
    fn process_identity(pid: u32) -> Option<String> {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = vec![0u16; MAX_PATH as usize];
            let mut size = buf.len() as u32;
            if QueryFullProcessImageNameW(handle, 0, &mut buf, &mut size).is_err() {
                return None;
            }
            buf.truncate(size as usize);
            let path = OsString::from_wide(&buf);
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string());
            stem
        }
    }
}

#[cfg(target_os = "windows")]
use windows_impl::PlatformWindowEnumerator;

// --- Linux implementation (placeholder) ---------------------------------

#[cfg(target_os = "linux")]
mod linux_impl {
    use super::{AvailableApp, WindowEnumerator};
    use anyhow::Result;

    pub struct PlatformWindowEnumerator;

    impl WindowEnumerator for PlatformWindowEnumerator {
        fn enumerate_windows() -> Result<Vec<AvailableApp>> {
            // TODO: Implement Wayland/X11 enumeration with visibility filters
            log::info!("[WinEnum] Linux implementation pending - returning empty list");
            Ok(Vec::new())
        }
    }
}

#[cfg(target_os = "linux")]
use linux_impl::PlatformWindowEnumerator;

// --- Fallback to avoid compile errors on unsupported targets ------------

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod unsupported_impl {
    use super::{AvailableApp, WindowEnumerator};
    use anyhow::Result;

    pub struct PlatformWindowEnumerator;

    impl WindowEnumerator for PlatformWindowEnumerator {
        fn enumerate_windows() -> Result<Vec<AvailableApp>> {
            Ok(Vec::new())
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
use unsupported_impl::PlatformWindowEnumerator;