//! Platform-agnostic window filtering logic
//!
//! Manages exclusion/inclusion of windows during capture to prevent:
//! - Infinity mirror effect (preview window capturing itself)
//! - User-selected windows from appearing in capture
//!
//! This module provides the data structures; actual filtering happens
//! in platform-specific capture engines (macos_sck.rs, windows.rs, etc.)

use serde::{Deserialize, Serialize};

/// Identifies a window across application restarts
/// Uses Bundle ID + Window Name for stability on macOS
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct WindowIdentifier {
    /// Application bundle identifier (e.g., "com.google.Chrome")
    /// Platform-specific: macOS uses bundle IDs, Windows uses executable name
    pub app_id: String,

    /// Window name/title for disambiguation
    /// Allows multiple windows of same app to be filtered differently
    pub window_name: String,
}

impl WindowIdentifier {
    pub fn new(app_id: String, window_name: String) -> Self {
        Self { app_id, window_name }
    }

    /// Create identifier for preview window (special marker)
    pub fn preview_window() -> Self {
        Self {
            app_id: "com.rustframe.preview".to_string(),
            window_name: "RustFrame Preview".to_string(),
        }
    }

    /// Check if this is the preview window marker
    pub fn is_preview_window(&self) -> bool {
        self.app_id == "com.rustframe.preview"
    }

    /// Create identifier for a specific application window by bundle ID and window name
    /// Example: WindowIdentifier::app_window("com.google.Chrome", "Gmail")
    pub fn app_window(bundle_id: &str, window_name: &str) -> Self {
        Self {
            app_id: bundle_id.to_string(),
            window_name: window_name.to_string(),
        }
    }

    /// Create identifier matching any window from an app by bundle ID only
    /// Matches all windows from the specified bundle
    /// Example: WindowIdentifier::app_all_windows("com.apple.Safari")
    pub fn app_all_windows(bundle_id: &str) -> Self {
        Self {
            app_id: bundle_id.to_string(),
            window_name: String::new(),
        }
    }

    /// Create identifier matching windows by name only (cross-app matching)
    /// Example: WindowIdentifier::window_by_name("Inspector")
    pub fn window_by_name(window_name: &str) -> Self {
        Self {
            app_id: String::new(),
            window_name: window_name.to_string(),
        }
    }
}

/// Window filtering mode
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WindowFilterMode {
    /// No filtering - capture everything
    None,

    /// Exclude listed windows from capture
    ExcludeList,

    /// Capture only listed windows (include-only mode)
    /// Future feature, prepared for forward compatibility
    IncludeOnly,
}

/// Settings for window filtering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowFilterSettings {
    /// Active filtering mode
    pub mode: WindowFilterMode,

    /// List of windows to exclude (when mode = ExcludeList)
    pub excluded_windows: Vec<WindowIdentifier>,

    /// List of windows to include (when mode = IncludeOnly)
    /// Currently unused, prepared for future feature
    pub included_windows: Vec<WindowIdentifier>,

    /// Automatically exclude preview window (prevents infinity mirror)
    /// Default: true (always active, user can only disable in dev mode)
    pub auto_exclude_preview: bool,

    /// Developer mode: allow disabling preview window exclusion
    /// Set via environment variable: RUSTFRAME_DEV_MODE=1
    pub dev_mode: bool,
}

impl Default for WindowFilterSettings {
    fn default() -> Self {
        Self {
            mode: WindowFilterMode::ExcludeList,
            excluded_windows: Vec::new(),
            included_windows: Vec::new(),
            auto_exclude_preview: true,
            dev_mode: std::env::var("RUSTFRAME_DEV_MODE").is_ok(),
        }
    }
}

impl WindowFilterSettings {
    /// Get list of windows to exclude for this capture session
    /// Takes into account auto_exclude_preview setting
    pub fn get_exclusions(&self, preview_window_id: Option<&WindowIdentifier>) -> Vec<WindowIdentifier> {
        let mut exclusions = self.excluded_windows.clone();

        // Add preview window to exclusions if:
        // - auto_exclude_preview is enabled (default)
        // - AND dev_mode is disabled (dev can override)
        if self.auto_exclude_preview && !self.dev_mode {
            if let Some(preview_id) = preview_window_id {
                if !exclusions.contains(preview_id) {
                    exclusions.push(preview_id.clone());
                }
            }
        }

        exclusions
    }

    /// Check if a window should be captured
    pub fn should_capture(&self, window_id: &WindowIdentifier, preview_window_id: Option<&WindowIdentifier>) -> bool {
        match self.mode {
            WindowFilterMode::None => true,
            WindowFilterMode::ExcludeList => {
                let exclusions = self.get_exclusions(preview_window_id);
                !exclusions.contains(window_id)
            }
            WindowFilterMode::IncludeOnly => {
                // Future: check against included_windows
                !self.included_windows.is_empty() && self.included_windows.contains(window_id)
            }
        }
    }

    /// Add a window to exclusion list
    pub fn add_exclusion(&mut self, window: WindowIdentifier) {
        if !self.excluded_windows.contains(&window) {
            self.excluded_windows.push(window);
        }
    }

    /// Remove a window from exclusion list
    pub fn remove_exclusion(&self, window: &WindowIdentifier) -> Self {
        let mut new_settings = self.clone();
        new_settings.excluded_windows.retain(|w| w != window);
        new_settings
    }

    /// Add a simple window exclusion by bundle ID (matches all windows from app)
    pub fn exclude_app(&mut self, bundle_id: &str) {
        self.add_exclusion(WindowIdentifier::app_all_windows(bundle_id));
    }

    /// Add a specific window exclusion by app and window name
    pub fn exclude_app_window(&mut self, bundle_id: &str, window_name: &str) {
        self.add_exclusion(WindowIdentifier::app_window(bundle_id, window_name));
    }

    /// Clear all exclusions except auto-excluded preview window
    pub fn clear_manual_exclusions(&mut self) {
        self.excluded_windows.clear();
    }

    /// Get count of manually excluded windows (not including auto-excluded preview)
    pub fn manual_exclusion_count(&self) -> usize {
        self.excluded_windows.iter()
            .filter(|w| !w.is_preview_window())
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preview_window_marker() {
        let preview = WindowIdentifier::preview_window();
        assert!(preview.is_preview_window());

        let normal = WindowIdentifier::new("com.app".to_string(), "Window 1".to_string());
        assert!(!normal.is_preview_window());
    }

    #[test]
    fn test_auto_exclude_preview() {
        let mut settings = WindowFilterSettings::default();
        settings.mode = WindowFilterMode::ExcludeList;

        let preview = WindowIdentifier::preview_window();
        let chrome = WindowIdentifier::new("com.google.Chrome".to_string(), "Tab 1".to_string());

        // Preview should be automatically excluded
        let exclusions = settings.get_exclusions(Some(&preview));
        assert!(exclusions.contains(&preview));

        // Chrome should not be in exclusion list unless explicitly added
        assert!(!exclusions.contains(&chrome));
    }

    #[test]
    fn test_dev_mode_override() {
        let mut settings = WindowFilterSettings::default();
        settings.mode = WindowFilterMode::ExcludeList;
        settings.dev_mode = true; // Dev mode enabled

        let preview = WindowIdentifier::preview_window();

        // In dev mode, preview is NOT automatically excluded
        let exclusions = settings.get_exclusions(Some(&preview));
        assert!(!exclusions.contains(&preview));
    }
}
