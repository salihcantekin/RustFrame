// app/state.rs - Core Application State
//
// This module defines the application state that persists across frames
// and is shared between UI components.

use std::time::Instant;

/// Capture settings that control the screen capture behavior
#[derive(Debug, Clone)]
pub struct CaptureSettings {
    /// Whether to show the mouse cursor in the capture
    pub show_cursor: bool,
    /// Whether to show window border after capture starts
    pub show_border: bool,
    /// Border width in pixels (only used if show_border is true)
    pub border_width: u32,
    /// Whether to exclude destination from screen capture (prevents infinite mirror)
    pub exclude_from_capture: bool,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            show_cursor: true,
            show_border: true,
            border_width: 3,
            exclude_from_capture: true,
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
        let settings = if dev_mode {
            CaptureSettings::for_development()
        } else {
            CaptureSettings::default()
        };
        
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
