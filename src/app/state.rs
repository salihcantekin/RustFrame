// app/state.rs - Core Application State
//
// This module defines the application state that persists across frames
// and is shared between UI components.

use std::time::Instant;
use serde::{Deserialize, Serialize};

/// Preset window sizes for quick selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowPreset {
    /// Full HD 1920x1080
    FullHD,
    /// HD 1280x720
    HD,
    /// 4K UHD 3840x2160
    UHD4K,
    /// Square 1080x1080 (good for social media)
    Square1080,
    /// Square 720x720
    Square720,
    /// Vertical 1080x1920 (phone format)
    Vertical1080,
    /// Webcam 640x480
    Webcam,
    /// Small 320x240
    Small,
    /// Custom size (user defined)
    Custom,
}

impl WindowPreset {
    /// Get all available presets
    pub fn all() -> &'static [WindowPreset] {
        &[
            WindowPreset::FullHD,
            WindowPreset::HD,
            WindowPreset::UHD4K,
            WindowPreset::Square1080,
            WindowPreset::Square720,
            WindowPreset::Vertical1080,
            WindowPreset::Webcam,
            WindowPreset::Small,
            WindowPreset::Custom,
        ]
    }

    /// Get the dimensions for this preset
    pub fn dimensions(&self) -> (u32, u32) {
        match self {
            WindowPreset::FullHD => (1920, 1080),
            WindowPreset::HD => (1280, 720),
            WindowPreset::UHD4K => (3840, 2160),
            WindowPreset::Square1080 => (1080, 1080),
            WindowPreset::Square720 => (720, 720),
            WindowPreset::Vertical1080 => (1080, 1920),
            WindowPreset::Webcam => (640, 480),
            WindowPreset::Small => (320, 240),
            WindowPreset::Custom => (800, 600), // Default custom size
        }
    }

    /// Get display name
    pub fn display_name(&self) -> &'static str {
        match self {
            WindowPreset::FullHD => "Full HD (1920×1080)",
            WindowPreset::HD => "HD (1280×720)",
            WindowPreset::UHD4K => "4K UHD (3840×2160)",
            WindowPreset::Square1080 => "Square (1080×1080)",
            WindowPreset::Square720 => "Square (720×720)",
            WindowPreset::Vertical1080 => "Vertical (1080×1920)",
            WindowPreset::Webcam => "Webcam (640×480)",
            WindowPreset::Small => "Small (320×240)",
            WindowPreset::Custom => "Custom Size",
        }
    }

    /// Get short name for compact display
    pub fn short_name(&self) -> &'static str {
        match self {
            WindowPreset::FullHD => "1080p",
            WindowPreset::HD => "720p",
            WindowPreset::UHD4K => "4K",
            WindowPreset::Square1080 => "1080×1080",
            WindowPreset::Square720 => "720×720",
            WindowPreset::Vertical1080 => "1080×1920",
            WindowPreset::Webcam => "Webcam",
            WindowPreset::Small => "Small",
            WindowPreset::Custom => "Custom",
        }
    }
}

impl Default for WindowPreset {
    fn default() -> Self {
        WindowPreset::HD
    }
}

/// Window position presets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionPreset {
    /// Center of the primary monitor
    Center,
    /// Top-left corner
    TopLeft,
    /// Top-right corner
    TopRight,
    /// Bottom-left corner
    BottomLeft,
    /// Bottom-right corner
    BottomRight,
    /// Custom position (user defined)
    Custom,
}

impl PositionPreset {
    /// Get all available presets
    pub fn all() -> &'static [PositionPreset] {
        &[
            PositionPreset::Center,
            PositionPreset::TopLeft,
            PositionPreset::TopRight,
            PositionPreset::BottomLeft,
            PositionPreset::BottomRight,
            PositionPreset::Custom,
        ]
    }

    /// Get display name
    pub fn display_name(&self) -> &'static str {
        match self {
            PositionPreset::Center => "Center",
            PositionPreset::TopLeft => "Top Left",
            PositionPreset::TopRight => "Top Right",
            PositionPreset::BottomLeft => "Bottom Left",
            PositionPreset::BottomRight => "Bottom Right",
            PositionPreset::Custom => "Custom Position",
        }
    }

    /// Calculate position for given window size and screen dimensions
    pub fn calculate_position(&self, window_width: u32, window_height: u32, screen_width: u32, screen_height: u32) -> (i32, i32) {
        let margin = 50; // Margin from edges
        match self {
            PositionPreset::Center => (
                (screen_width as i32 - window_width as i32) / 2,
                (screen_height as i32 - window_height as i32) / 2,
            ),
            PositionPreset::TopLeft => (margin, margin),
            PositionPreset::TopRight => (
                screen_width as i32 - window_width as i32 - margin,
                margin,
            ),
            PositionPreset::BottomLeft => (
                margin,
                screen_height as i32 - window_height as i32 - margin,
            ),
            PositionPreset::BottomRight => (
                screen_width as i32 - window_width as i32 - margin,
                screen_height as i32 - window_height as i32 - margin,
            ),
            PositionPreset::Custom => (100, 100), // Default custom position
        }
    }
}

impl Default for PositionPreset {
    fn default() -> Self {
        PositionPreset::Center
    }
}

/// Recording/capture quality settings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureQuality {
    /// Low quality, better performance
    Low,
    /// Medium quality
    Medium,
    /// High quality
    High,
    /// Maximum quality
    Maximum,
}

impl CaptureQuality {
    pub fn display_name(&self) -> &'static str {
        match self {
            CaptureQuality::Low => "Low (Better Performance)",
            CaptureQuality::Medium => "Medium",
            CaptureQuality::High => "High",
            CaptureQuality::Maximum => "Maximum (Best Quality)",
        }
    }
}

impl Default for CaptureQuality {
    fn default() -> Self {
        CaptureQuality::High
    }
}

/// Default click highlight duration
fn default_click_duration() -> u32 { 500 }

/// Default REC indicator size (2 = Medium)
fn default_rec_size() -> u32 { 2 }

/// Default last width
fn default_last_width() -> u32 { 800 }

/// Default last height
fn default_last_height() -> u32 { 600 }

/// Capture settings that control the screen capture behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureSettings {
    // === Mouse & Cursor Settings ===
    /// Whether to show the mouse cursor in the capture
    pub show_cursor: bool,
    /// Whether to highlight mouse clicks
    pub highlight_clicks: bool,
    /// Click highlight color (RGBA)
    pub click_highlight_color: [u8; 4],
    /// Click highlight duration in milliseconds
    #[serde(default = "default_click_duration")]
    pub click_highlight_duration_ms: u32,

    // === Border Settings ===
    /// Whether to show window border after capture starts
    pub show_border: bool,
    /// Border width in pixels (only used if show_border is true)
    pub border_width: u32,
    /// Border color (RGBA)
    pub border_color: [u8; 4],
    /// Show REC indicator on border
    pub show_rec_indicator: bool,
    /// REC indicator size (1=Small, 2=Medium, 3=Large)
    #[serde(default = "default_rec_size")]
    pub rec_indicator_size: u32,

    // === Window Size Settings ===
    /// Selected window size preset
    pub size_preset: WindowPreset,
    /// Custom width (used when preset is Custom)
    pub custom_width: u32,
    /// Custom height (used when preset is Custom)
    pub custom_height: u32,

    // === Window Position Settings ===
    /// Selected position preset
    pub position_preset: PositionPreset,
    /// Custom X position (used when preset is Custom)
    pub custom_x: i32,
    /// Custom Y position (used when preset is Custom)
    pub custom_y: i32,

    // === Capture Settings ===
    /// Capture quality
    pub quality: CaptureQuality,
    /// Target framerate (FPS)
    pub target_fps: u32,
    /// Whether to exclude destination from screen capture (prevents infinite mirror)
    pub exclude_from_capture: bool,

    // === UI Settings ===
    /// Remember last capture region
    pub remember_region: bool,
    /// Auto-start capture on region selection
    pub auto_start: bool,
    /// Show keyboard shortcuts hints
    pub show_shortcuts: bool,
    
    // === Last Region (for remember_region) ===
    /// Last window X position
    #[serde(default)]
    pub last_x: i32,
    /// Last window Y position
    #[serde(default)]
    pub last_y: i32,
    /// Last window width
    #[serde(default = "default_last_width")]
    pub last_width: u32,
    /// Last window height
    #[serde(default = "default_last_height")]
    pub last_height: u32,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            // Mouse settings
            show_cursor: true,
            highlight_clicks: false,
            click_highlight_color: [255, 255, 0, 128], // Yellow, semi-transparent
            click_highlight_duration_ms: 500, // 500ms default

            // Border settings
            show_border: true,
            border_width: 3,
            border_color: [0, 120, 215, 255], // Blue
            show_rec_indicator: true,
            rec_indicator_size: 2, // Medium

            // Window size
            size_preset: WindowPreset::HD,
            custom_width: 800,
            custom_height: 600,

            // Window position
            position_preset: PositionPreset::Center,
            custom_x: 100,
            custom_y: 100,

            // Capture settings
            quality: CaptureQuality::High,
            target_fps: 60,
            exclude_from_capture: true,

            // UI settings
            remember_region: true,
            auto_start: false,
            show_shortcuts: true,
            
            // Last region defaults
            last_x: 100,
            last_y: 100,
            last_width: 800,
            last_height: 600,
        }
    }
}

impl CaptureSettings {
    /// Development mode settings - destination window visible beside overlay
    pub fn for_development() -> Self {
        Self {
            exclude_from_capture: false,
            ..Default::default()
        }
    }

    /// Get the effective window dimensions based on preset or custom size
    pub fn get_window_dimensions(&self) -> (u32, u32) {
        if self.size_preset == WindowPreset::Custom {
            (self.custom_width, self.custom_height)
        } else {
            self.size_preset.dimensions()
        }
    }

    /// Get the effective window position based on preset or custom position
    pub fn get_window_position(&self, screen_width: u32, screen_height: u32) -> (i32, i32) {
        if self.position_preset == PositionPreset::Custom {
            (self.custom_x, self.custom_y)
        } else {
            let (w, h) = self.get_window_dimensions();
            self.position_preset.calculate_position(w, h, screen_width, screen_height)
        }
    }

    /// Load settings from file
    pub fn load() -> Self {
        Self::load_from_path(Self::config_path())
    }

    /// Load settings from a specific path
    pub fn load_from_path(path: std::path::PathBuf) -> Self {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(settings) = serde_json::from_str(&content) {
                    return settings;
                }
            }
        }
        Self::default()
    }

    /// Save settings to file
    pub fn save(&self) -> Result<(), std::io::Error> {
        self.save_to_path(Self::config_path())
    }

    /// Save settings to a specific path
    pub fn save_to_path(&self, path: std::path::PathBuf) -> Result<(), std::io::Error> {
        // Create config directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)
    }

    /// Get the config file path
    pub fn config_path() -> std::path::PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        path.push("RustFrame");
        path.push("settings.json");
        path
    }
}

/// Represents a rectangular region on the screen
#[derive(Debug, Clone, Copy, Default)]
pub struct CaptureRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl CaptureRect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }
    
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x 
            && px < self.x + self.width as i32 
            && py >= self.y 
            && py < self.y + self.height as i32
    }
}

/// Current mode of the application
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    /// User is selecting a region to capture
    Selecting,
    /// Capture is active and frames are being rendered
    Capturing,
}

/// Main application state
/// This is the single source of truth for all application data
#[derive(Debug)]
pub struct AppState {
    /// Current capture settings
    pub settings: CaptureSettings,
    
    /// Current application mode
    pub mode: AppMode,
    
    /// The selected capture region (in screen coordinates)
    pub capture_region: Option<CaptureRect>,
    
    /// Whether the application is in development mode
    pub dev_mode: bool,
    
    /// Whether a dialog is currently open
    pub dialog_open: bool,
    
    /// Frame counter for debugging/performance monitoring
    pub frame_count: u64,
    
    /// Application startup time
    pub startup_time: Instant,
}

impl AppState {
    /// Create a new application state
    pub fn new(dev_mode: bool) -> Self {
        // Try to load settings from file, fall back to defaults
        let mut settings = CaptureSettings::load();
        
        // In dev mode, override exclude_from_capture
        if dev_mode {
            settings.exclude_from_capture = false;
        }
        
        log::info!("Settings loaded: {:?}", settings);
        
        Self {
            settings,
            mode: AppMode::Selecting,
            capture_region: None,
            dev_mode,
            dialog_open: false,
            frame_count: 0,
            startup_time: Instant::now(),
        }
    }
    
    /// Check if capture is currently active
    pub fn is_capturing(&self) -> bool {
        self.mode == AppMode::Capturing
    }
    
    /// Start capture with the given region
    pub fn start_capture(&mut self, region: CaptureRect) {
        self.capture_region = Some(region);
        self.mode = AppMode::Capturing;
    }
    
    /// Stop capture and return to selection mode
    pub fn stop_capture(&mut self) {
        self.mode = AppMode::Selecting;
        // Keep capture_region for potential restart
    }
    
    /// Toggle cursor visibility
    pub fn toggle_cursor(&mut self) {
        self.settings.show_cursor = !self.settings.show_cursor;
    }
    
    /// Toggle border visibility
    pub fn toggle_border(&mut self) {
        self.settings.show_border = !self.settings.show_border;
    }
    
    /// Toggle production mode (exclude from capture)
    pub fn toggle_production_mode(&mut self) {
        self.settings.exclude_from_capture = !self.settings.exclude_from_capture;
    }
    
    /// Get elapsed time since startup
    pub fn elapsed(&self) -> std::time::Duration {
        self.startup_time.elapsed()
    }
    
    /// Increment frame counter
    pub fn tick(&mut self) {
        self.frame_count += 1;
    }
}
