/// Platform-specific information and capabilities
/// This module provides runtime platform detection and available features
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformInfo {
    /// Operating system name
    pub os_name: String,
    /// Operating system type: "windows", "macos", "linux"
    pub os_type: String,
    /// OS version string
    pub os_version: String,
    /// Available capture methods for this platform
    pub available_capture_methods: Vec<CaptureMethodInfo>,
    /// All capture methods from all platforms (for dev mode preview)
    pub all_platforms_capture_methods: Vec<PlatformCaptureMethodInfo>,
    /// Platform-specific capabilities
    pub capabilities: PlatformCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureMethodInfo {
    /// Internal identifier
    pub id: String,
    /// Display name
    pub name: String,
    /// Description
    pub description: String,
    /// Is this the recommended method?
    pub recommended: bool,
    /// Is this method hardware accelerated?
    pub hardware_accelerated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformCaptureMethodInfo {
    /// Platform name (Windows, macOS, Linux)
    pub platform_name: String,
    /// Platform type (windows, macos, linux)
    pub platform_type: String,
    /// Internal identifier
    pub id: String,
    /// Display name
    pub name: String,
    /// Description
    pub description: String,
    /// Is this the recommended method?
    pub recommended: bool,
    /// Is this method hardware accelerated?
    pub hardware_accelerated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformCapabilities {
    /// Supports custom window rendering
    pub supports_custom_rendering: bool,
    /// Supports transparent windows
    pub supports_transparency: bool,
    /// Supports hardware acceleration
    pub supports_hardware_acceleration: bool,
    /// Has advanced WinAPI options (Windows only)
    pub has_winapi_options: bool,
}

impl PlatformInfo {
    pub fn detect() -> Self {
        let os_type = std::env::consts::OS.to_string();
        let os_name = Self::get_os_name();
        let os_version = Self::get_os_version();
        let available_capture_methods = Self::get_available_capture_methods();
        let all_platforms_capture_methods = Self::get_all_platforms_capture_methods();
        let capabilities = Self::get_platform_capabilities();

        Self {
            os_name,
            os_type,
            os_version,
            available_capture_methods,
            all_platforms_capture_methods,
            capabilities,
        }
    }

    fn get_os_name() -> String {
        #[cfg(target_os = "windows")]
        {
            "Windows".to_string()
        }
        #[cfg(target_os = "macos")]
        {
            "macOS".to_string()
        }
        #[cfg(target_os = "linux")]
        {
            "Linux".to_string()
        }
    }

    fn get_os_version() -> String {
        // Try to get detailed OS version
        #[cfg(target_os = "windows")]
        {
            Self::get_windows_version()
        }
        #[cfg(target_os = "macos")]
        {
            Self::get_macos_version()
        }
        #[cfg(target_os = "linux")]
        {
            Self::get_linux_version()
        }
    }

    #[cfg(target_os = "windows")]
    fn get_windows_version() -> String {
        // Simple version detection using std::env
        // For more detailed version, we'd need Win32_System_SystemInformation feature
        "Windows".to_string()
    }

    #[cfg(target_os = "macos")]
    fn get_macos_version() -> String {
        // Use system_info crate or fallback
        "Unknown".to_string()
    }

    #[cfg(target_os = "linux")]
    fn get_linux_version() -> String {
        // Try to read from /etc/os-release
        if let Ok(contents) = std::fs::read_to_string("/etc/os-release") {
            for line in contents.lines() {
                if line.starts_with("PRETTY_NAME=") {
                    return line
                        .trim_start_matches("PRETTY_NAME=")
                        .trim_matches('"')
                        .to_string();
                }
            }
        }
        "Unknown".to_string()
    }

    fn get_available_capture_methods() -> Vec<CaptureMethodInfo> {
        #[cfg(target_os = "windows")]
        {
            vec![
                CaptureMethodInfo {
                    id: "Wgc".to_string(),
                    name: "Windows Graphics Capture".to_string(),
                    description: "Modern, GPU-accelerated capture using Windows.Graphics.Capture API. Best performance and compatibility with modern Windows applications.".to_string(),
                    recommended: true,
                    hardware_accelerated: true,
                },
                CaptureMethodInfo {
                    id: "GdiCopy".to_string(),
                    name: "GDI BitBlt".to_string(),
                    description: "Legacy GDI-based screen capture. More compatible with older applications but slower performance.".to_string(),
                    recommended: false,
                    hardware_accelerated: false,
                },
            ]
        }

        #[cfg(target_os = "macos")]
        {
            vec![CaptureMethodInfo {
                id: "CoreGraphics".to_string(),
                name: "ScreenCaptureKit".to_string(),
                description:
                    "Modern macOS screen capture using ScreenCaptureKit. Requires macOS 12.3+."
                        .to_string(),
                recommended: true,
                hardware_accelerated: true,
            }]
        }

        #[cfg(target_os = "linux")]
        {
            vec![CaptureMethodInfo {
                id: "CoreGraphics".to_string(),
                name: "PipeWire / X11".to_string(),
                description: "Wayland/X11 screen capture. Automatically detects compositor."
                    .to_string(),
                recommended: true,
                hardware_accelerated: false,
            }]
        }
    }

    /// Get capture methods from ALL platforms (for dev mode preview)
    fn get_all_platforms_capture_methods() -> Vec<PlatformCaptureMethodInfo> {
        vec![
            // Windows methods
            PlatformCaptureMethodInfo {
                platform_name: "Windows".to_string(),
                platform_type: "windows".to_string(),
                id: "Wgc".to_string(),
                name: "Windows Graphics Capture".to_string(),
                description: "Modern, GPU-accelerated capture using Windows.Graphics.Capture API. Best performance and compatibility with modern Windows applications.".to_string(),
                recommended: true,
                hardware_accelerated: true,
            },
            PlatformCaptureMethodInfo {
                platform_name: "Windows".to_string(),
                platform_type: "windows".to_string(),
                id: "GdiCopy".to_string(),
                name: "GDI BitBlt".to_string(),
                description: "Legacy GDI-based screen capture. More compatible with older applications but slower performance.".to_string(),
                recommended: false,
                hardware_accelerated: false,
            },
            // macOS methods
            PlatformCaptureMethodInfo {
                platform_name: "macOS".to_string(),
                platform_type: "macos".to_string(),
                id: "CoreGraphics".to_string(),
                name: "ScreenCaptureKit".to_string(),
                description: "Modern macOS screen capture using ScreenCaptureKit. Requires macOS 12.3+.".to_string(),
                recommended: true,
                hardware_accelerated: true,
            },
            // Linux methods
            PlatformCaptureMethodInfo {
                platform_name: "Linux".to_string(),
                platform_type: "linux".to_string(),
                id: "CoreGraphics".to_string(),
                name: "PipeWire / X11".to_string(),
                description: "Wayland/X11 screen capture. Automatically detects compositor.".to_string(),
                recommended: true,
                hardware_accelerated: false,
            },
        ]
    }

    fn get_platform_capabilities() -> PlatformCapabilities {
        #[cfg(target_os = "windows")]
        {
            PlatformCapabilities {
                supports_custom_rendering: true,
                supports_transparency: true,
                supports_hardware_acceleration: true,
                has_winapi_options: true,
            }
        }

        #[cfg(target_os = "macos")]
        {
            PlatformCapabilities {
                supports_custom_rendering: true,
                supports_transparency: true,
                supports_hardware_acceleration: true,
                has_winapi_options: false,
            }
        }

        #[cfg(target_os = "linux")]
        {
            PlatformCapabilities {
                supports_custom_rendering: true,
                supports_transparency: true,
                supports_hardware_acceleration: false,
                has_winapi_options: false,
            }
        }
    }
}
