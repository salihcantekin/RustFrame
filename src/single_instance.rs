/// Single Instance Lock
/// 
/// Ensures only one instance of the application runs at a time.
/// Platform-specific implementations:
/// - Windows: Uses Named Mutex
/// - macOS/Linux: Uses file lock (flock)

#[cfg(not(windows))]
use std::path::PathBuf;
use anyhow::{Context, Result};

/// Single instance lock guard
/// When dropped, the lock is released
pub struct SingleInstanceLock {
    #[cfg(windows)]
    _mutex_handle: windows::Win32::Foundation::HANDLE,
    #[cfg(not(windows))]
    _lock_file: std::fs::File,
}

// SAFETY: On Windows, HANDLE is thread-safe once created and can be safely sent between threads.
// On Unix, file locks are maintained by the OS and the File handle is safe to send.
#[cfg(windows)]
unsafe impl Send for SingleInstanceLock {}
#[cfg(windows)]
unsafe impl Sync for SingleInstanceLock {}

impl SingleInstanceLock {
    /// Try to acquire the single instance lock
    /// 
    /// Returns Ok(lock) if this is the only instance
    /// Returns Err if another instance is already running
    pub fn acquire() -> Result<Self> {
        #[cfg(windows)]
        {
            Self::acquire_windows()
        }
        
        #[cfg(not(windows))]
        {
            Self::acquire_unix()
        }
    }
    
    /// Try to bring the existing instance's window to foreground
    /// Called when another instance is detected
    pub fn activate_existing_instance() {
        #[cfg(windows)]
        {
            Self::activate_windows();
        }
        
        #[cfg(target_os = "macos")]
        {
            Self::activate_macos();
        }
        
        #[cfg(target_os = "linux")]
        {
            // TODO: Implement for Linux (X11/Wayland)
            tracing::info!("Window activation not yet implemented for Linux");
        }
    }
    
    #[cfg(windows)]
    fn activate_windows() {
        use windows::Win32::UI::WindowsAndMessaging::{
            FindWindowW, SetForegroundWindow, ShowWindow, IsIconic, SW_RESTORE
        };
        use windows::core::PCWSTR;
        
        // Try to find the main RustFrame window
        // Tauri creates windows with a specific class name pattern
        let window_titles = [
            "RustFrame\0",
            "rustframe\0",
        ];
        
        unsafe {
            for title in &window_titles {
                let title_wide: Vec<u16> = title.encode_utf16().collect();
                
                // FindWindowW returns Result<HWND, Error>
                if let Ok(hwnd) = FindWindowW(None, PCWSTR(title_wide.as_ptr())) {
                    if !hwnd.is_invalid() {
                        tracing::info!("Found existing RustFrame window, bringing to foreground");
                        
                        // If window is minimized, restore it first
                        if IsIconic(hwnd).as_bool() {
                            let _ = ShowWindow(hwnd, SW_RESTORE);
                        }
                        
                        // Bring window to foreground
                        let _ = SetForegroundWindow(hwnd);
                        return;
                    }
                }
            }
            
            tracing::warn!("Could not find existing RustFrame window to activate");
        }
    }
    
    #[cfg(target_os = "macos")]
    fn activate_macos() {
        use cocoa::appkit::{NSRunningApplication, NSApplicationActivationOptions};
        use cocoa::foundation::NSString;
        use objc::{class, msg_send, sel, sel_impl};
        use objc::runtime::Object;
        
        // Strategy 1: Use AppleScript (most reliable for bringing app to front)
        if let Some(pid) = Self::read_existing_pid() {
            // AppleScript to activate the process by PID
            let script = format!(
                r#"tell application "System Events"
                    set frontmost of first process whose unix id is {} to true
                end tell"#,
                pid
            );
            
            let output = std::process::Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .output();
            
            match output {
                Ok(result) => {
                    if result.status.success() {
                        tracing::info!("Successfully activated existing instance via AppleScript (PID: {})", pid);
                        return;
                    } else {
                        let stderr = String::from_utf8_lossy(&result.stderr);
                        tracing::warn!("AppleScript activation failed: {}", stderr);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to execute osascript: {}", e);
                }
            }
        }
        
        // Strategy 2: Fall back to NSWorkspace approach
        tracing::debug!("Falling back to NSWorkspace activation...");
        
        unsafe {
            // Get shared workspace
            let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
            
            // Get all running applications
            let running_apps: *mut Object = msg_send![workspace, runningApplications];
            let count: usize = msg_send![running_apps, count];
            
            // Search for RustFrame in running applications
            for i in 0..count {
                let app: *mut Object = msg_send![running_apps, objectAtIndex: i];
                let bundle_id: *mut Object = msg_send![app, bundleIdentifier];
                let localized_name: *mut Object = msg_send![app, localizedName];
                
                // Convert NSString to Rust String for comparison
                if !bundle_id.is_null() {
                    let bundle_str = NSString::UTF8String(bundle_id as *mut Object);
                    if !bundle_str.is_null() {
                        let bundle_rust = std::ffi::CStr::from_ptr(bundle_str).to_string_lossy();
                        
                        // Check if this is our app (by bundle identifier from tauri.conf.json)
                        if bundle_rust == "com.salihcantekin.rustframe" {
                            tracing::info!("Found existing RustFrame by bundle ID, activating...");
                            
                            // Activate the application with all windows
                            // NSApplicationActivateAllWindows (1 << 0) | NSApplicationActivateIgnoringOtherApps (1 << 1)
                            let options: u64 = (1 << 0) | (1 << 1);
                            let _: () = msg_send![app, activateWithOptions: options];
                            
                            // Also try NSWorkspace launch
                            let null_desc: *mut Object = std::ptr::null_mut();
                            let null_ident: *mut *mut Object = std::ptr::null_mut();
                            let _: bool = msg_send![workspace, 
                                launchAppWithBundleIdentifier: bundle_id
                                options: 0u32
                                additionalEventParamDescriptor: null_desc
                                launchIdentifier: null_ident];
                            
                            return;
                        }
                    }
                }
                
                // Also check by localized name as fallback
                if !localized_name.is_null() {
                    let name_str = NSString::UTF8String(localized_name as *mut Object);
                    if !name_str.is_null() {
                        let name_rust = std::ffi::CStr::from_ptr(name_str).to_string_lossy();
                        
                        if name_rust.contains("RustFrame") || name_rust.contains("rustframe") {
                            tracing::info!("Found existing RustFrame by name, activating...");
                            
                            // Activate with options
                            let options: u64 = (1 << 0) | (1 << 1);
                            let _: () = msg_send![app, activateWithOptions: options];
                            
                            return;
                        }
                    }
                }
            }
            
            tracing::warn!("Could not find existing RustFrame application to activate");
        }
    }
    
    #[cfg(windows)]
    fn acquire_windows() -> Result<Self> {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::CreateMutexW;
        use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
        use windows::core::PCWSTR;
        
        // Create a named mutex with a unique name for this application
        let mutex_name = "Global\\RustFrame_SingleInstance_Mutex_2026\0";
        let mutex_name_wide: Vec<u16> = mutex_name.encode_utf16().collect();
        
        unsafe {
            let mutex_handle = CreateMutexW(
                None,
                true, // Initial owner
                PCWSTR(mutex_name_wide.as_ptr()),
            ).context("Failed to create mutex")?;
            
            // Check if another instance already exists
            let last_error = GetLastError();
            if last_error == ERROR_ALREADY_EXISTS {
                // Another instance is running, close our handle and return error
                let _ = CloseHandle(mutex_handle);
                anyhow::bail!("Another instance of RustFrame is already running");
            }
            
            tracing::info!("Single instance lock acquired (Windows Named Mutex)");
            
            Ok(Self {
                _mutex_handle: mutex_handle,
            })
        }
    }
    
    #[cfg(not(windows))]
    fn acquire_unix() -> Result<Self> {
        use std::fs::OpenOptions;
        use std::os::unix::fs::OpenOptionsExt;
        use std::io::{Write, Seek, SeekFrom};
        
        // Get lock file path in user's config directory
        let lock_path = Self::get_lock_file_path()?;
        
        // Ensure parent directory exists
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create lock directory")?;
        }
        
        // Open or create lock file
        let mut lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o644)
            .open(&lock_path)
            .context("Failed to open lock file")?;
        
        // Try to acquire exclusive lock (non-blocking)
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::io::AsRawFd;
            let fd = lock_file.as_raw_fd();
            
            // Use flock for file locking
            let result = unsafe {
                libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB)
            };
            
            if result != 0 {
                anyhow::bail!("Another instance of RustFrame is already running");
            }
        }
        
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::io::AsRawFd;
            let fd = lock_file.as_raw_fd();
            
            // Use flock for file locking
            let result = unsafe {
                libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB)
            };
            
            if result != 0 {
                anyhow::bail!("Another instance of RustFrame is already running");
            }
        }
        
        // Truncate and write PID to lock file
        lock_file.set_len(0).context("Failed to truncate lock file")?;
        lock_file.seek(SeekFrom::Start(0)).context("Failed to seek lock file")?;
        let pid = std::process::id();
        write!(lock_file, "{}", pid)
            .context("Failed to write PID to lock file")?;
        lock_file.flush().context("Failed to flush lock file")?;
        
        tracing::info!("Single instance lock acquired (Unix file lock) at {:?}, PID: {}", lock_path, pid);
        
        Ok(Self {
            _lock_file: lock_file,
        })
    }
    
    #[cfg(not(windows))]
    fn read_existing_pid() -> Option<u32> {
        use std::io::Read;
        
        let lock_path = Self::get_lock_file_path().ok()?;
        let mut file = std::fs::File::open(&lock_path).ok()?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).ok()?;
        contents.trim().parse::<u32>().ok()
    }
    
    #[cfg(not(windows))]
    fn get_lock_file_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("Failed to get config directory")?;
        
        Ok(config_dir.join("RustFrame").join(".rustframe.lock"))
    }
}

impl Drop for SingleInstanceLock {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            use windows::Win32::Foundation::CloseHandle;
            unsafe {
                let _ = CloseHandle(self._mutex_handle);
            }
            tracing::info!("Single instance lock released (Windows)");
        }
        
        #[cfg(not(windows))]
        {
            // File lock is automatically released when file is closed
            tracing::info!("Single instance lock released (Unix)");
        }
    }
}
