//! RustFrame - Iced UI Version
//!
//! Modern screen capture application with:
//! - Main window (opak, modern UI for setup and controls)
//! - Hollow border (WinAPI, transparent interior, resizable/draggable)
//! - Destination window (capture preview, shareable)
//! - Settings dialog

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::BTreeMap;
use std::time::{Duration, Instant};
use std::path::PathBuf;
use std::fs;

use iced::{
    daemon, window, Color, Element, Length, Padding, Size, Subscription, Task, Theme,
    Alignment, Center,
};
use iced::widget::{
    button, checkbox, column, container, row, scrollable, slider, text, 
    horizontal_space, vertical_space, center, text_input, Row,
};

// Color picker from iced_aw
use iced_aw::ColorPicker;
use iced_aw::iced_fonts::REQUIRED_FONT_BYTES;

use log::{debug, error, info};
use serde::{Serialize, Deserialize};

// WinAPI for window resize and subclassing
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM, LRESULT, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect,
    WM_NCHITTEST, HTLEFT, HTRIGHT, HTTOP, HTBOTTOM,
    HTTOPLEFT, HTTOPRIGHT, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTCLIENT,
};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use std::sync::atomic::{AtomicIsize, Ordering};

// Global to track main window HWND for subclass
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);

mod hollow_border;
use hollow_border::HollowBorder;

mod mouse_hook;

mod destination_window;
use destination_window::DestinationWindow;

use rustframe::capture::{CaptureRect, CaptureEngine};
use rustframe::WindowsCaptureEngine;

// ============================================================================
// Window Subclass for Resize (WinAPI)
// ============================================================================

const RESIZE_BORDER: i32 = 8;  // Resize hit area thickness
const TITLE_BAR_HEIGHT: i32 = 48;  // Height of custom title bar for drag
const TITLE_BAR_BUTTONS_WIDTH: i32 = 140;  // Width reserved for buttons on right side

/// Subclass procedure for main window to handle resize via WM_NCHITTEST
#[cfg(windows)]
unsafe extern "system" fn main_window_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _uidsubclass: usize,
    _dwrefdata: usize,
) -> LRESULT {
    if msg == WM_NCHITTEST {
        let x = (lparam.0 & 0xFFFF) as i16 as i32;
        let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

        let mut rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);

        let border = RESIZE_BORDER;
        let corner_size = border * 2;

        // Check corners first (larger hit area for easier grabbing)
        let on_left = x >= rect.left && x < rect.left + corner_size;
        let on_right = x >= rect.right - corner_size && x < rect.right;
        let on_top = y >= rect.top && y < rect.top + corner_size;
        let on_bottom = y >= rect.bottom - corner_size && y < rect.bottom;

        // Corner hit tests
        if on_top && on_left {
            return LRESULT(HTTOPLEFT as isize);
        }
        if on_top && on_right {
            return LRESULT(HTTOPRIGHT as isize);
        }
        if on_bottom && on_left {
            return LRESULT(HTBOTTOMLEFT as isize);
        }
        if on_bottom && on_right {
            return LRESULT(HTBOTTOMRIGHT as isize);
        }

        // Edge hit tests
        if x >= rect.left && x < rect.left + border {
            return LRESULT(HTLEFT as isize);
        }
        if x >= rect.right - border && x < rect.right {
            return LRESULT(HTRIGHT as isize);
        }
        if y >= rect.top && y < rect.top + border {
            return LRESULT(HTTOP as isize);
        }
        if y >= rect.bottom - border && y < rect.bottom {
            return LRESULT(HTBOTTOM as isize);
        }

        // Title bar area = drag (caption), but exclude right side for buttons
        if y >= rect.top + border && y < rect.top + TITLE_BAR_HEIGHT {
            // Exclude right side where Settings and Close buttons are
            if x < rect.right - TITLE_BAR_BUTTONS_WIDTH {
                return LRESULT(HTCAPTION as isize);
            }
            // Right side = client area (buttons clickable)
            return LRESULT(HTCLIENT as isize);
        }

        // Client area
        return LRESULT(HTCLIENT as isize);
    }

    DefSubclassProc(hwnd, msg, wparam, lparam)
}

/// Install subclass on the main window for resize support
fn install_main_window_subclass(hwnd: HWND) {
    unsafe {
        let _ = SetWindowSubclass(hwnd, Some(main_window_subclass_proc), 1, 0);
        MAIN_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
        info!("Installed resize subclass on main window");
    }
}

// ============================================================================
// Click Highlight Drawing
// ============================================================================

const CLICK_CIRCLE_RADIUS: i32 = 20;

/// Draw a filled circle on the RGBA frame data for mouse click highlight
fn draw_click_circle(
    data: &mut [u8],
    width: u32,
    height: u32,
    center_x: i32,
    center_y: i32,
    color: &[u8; 4],  // RGBA
    opacity: f32,
) {
    let radius = CLICK_CIRCLE_RADIUS;
    let width = width as i32;
    let height = height as i32;
    
    // Bounding box
    let min_x = (center_x - radius).max(0);
    let max_x = (center_x + radius).min(width - 1);
    let min_y = (center_y - radius).max(0);
    let max_y = (center_y + radius).min(height - 1);
    
    let r2 = (radius * radius) as f32;
    
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = (x - center_x) as f32;
            let dy = (y - center_y) as f32;
            let dist2 = dx * dx + dy * dy;
            
            if dist2 <= r2 {
                // Inside circle - blend with existing pixel
                let idx = ((y * width + x) * 4) as usize;
                if idx + 3 < data.len() {
                    // Calculate edge softness (anti-aliasing)
                    let edge_factor = if dist2 > r2 * 0.7 {
                        let t = (r2 - dist2) / (r2 * 0.3);
                        t.clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    
                    let final_opacity = opacity * edge_factor * (color[3] as f32 / 255.0);
                    
                    // Alpha blend
                    let src_r = color[0] as f32;
                    let src_g = color[1] as f32;
                    let src_b = color[2] as f32;
                    
                    let dst_r = data[idx] as f32;
                    let dst_g = data[idx + 1] as f32;
                    let dst_b = data[idx + 2] as f32;
                    
                    data[idx] = ((src_r * final_opacity + dst_r * (1.0 - final_opacity)) as u8).min(255);
                    data[idx + 1] = ((src_g * final_opacity + dst_g * (1.0 - final_opacity)) as u8).min(255);
                    data[idx + 2] = ((src_b * final_opacity + dst_b * (1.0 - final_opacity)) as u8).min(255);
                }
            }
        }
    }
}

// ============================================================================
// Constants
// ============================================================================

const DEFAULT_WIDTH: u32 = 800;
const DEFAULT_HEIGHT: u32 = 600;
const MIN_WIDTH: u32 = 320;
const MIN_HEIGHT: u32 = 240;

/// Get the primary monitor's refresh rate with ~5% tolerance for fractional rates
/// This allows selecting standard rates (60, 120, 144, 240) when actual rate is slightly lower
fn get_monitor_refresh_rate() -> u32 {
    #[cfg(windows)]
    {
        use windows::Win32::Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS};
        unsafe {
            let mut devmode: DEVMODEW = std::mem::zeroed();
            devmode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
            if EnumDisplaySettingsW(None, ENUM_CURRENT_SETTINGS, &mut devmode).as_bool() {
                let hz = devmode.dmDisplayFrequency;
                log::info!("Monitor raw refresh rate: {} Hz", hz);
                
                // Round to nearest standard refresh rate with ~5% tolerance
                // This handles fractional rates like 59.94->60, 119.88->120, etc.
                let standard_rates = [30, 60, 75, 90, 120, 144, 165, 180, 240, 360];
                
                for &rate in &standard_rates {
                    // If actual Hz is within 5% below the standard rate, use the standard
                    let lower_bound = (rate as f32 * 0.95) as u32;
                    if hz >= lower_bound && hz <= rate {
                        log::info!("Matched standard rate: {} Hz (bounds: {}-{})", rate, lower_bound, rate);
                        return rate;
                    }
                }
                
                // If no standard rate matches, return actual (rounded up slightly)
                log::info!("No standard rate matched, using raw: {} Hz", hz);
                return hz.max(30);
            }
        }
    }
    60 // Default
}

/// Get screen dimensions
fn get_screen_dimensions() -> (u32, u32) {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
        unsafe {
            let w = GetSystemMetrics(SM_CXSCREEN) as u32;
            let h = GetSystemMetrics(SM_CYSCREEN) as u32;
            return (w, h);
        }
    }
    #[cfg(not(windows))]
    (1920, 1080)
}

/// Embedded icon bytes (icon.ico)
const ICON_BYTES: &[u8] = include_bytes!("../icon.ico");

/// Load the application icon for windows
fn load_app_icon() -> Option<window::Icon> {
    use image::ImageReader;
    use std::io::Cursor;
    
    let reader = ImageReader::new(Cursor::new(ICON_BYTES))
        .with_guessed_format()
        .ok()?;
    let img = reader.decode().ok()?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    window::icon::from_rgba(rgba.into_raw(), width, height).ok()
}

// Main window corner radius
const MAIN_WINDOW_RADIUS: f32 = 12.0;

// Color palette (modern dark theme)
mod colors {
    use iced::Color;
    
    pub const BG_PRIMARY: Color = Color::from_rgb(0.09, 0.09, 0.12);
    pub const BG_SECONDARY: Color = Color::from_rgb(0.12, 0.12, 0.16);
    pub const BG_TERTIARY: Color = Color::from_rgb(0.16, 0.16, 0.22);
    pub const BG_HOVER: Color = Color::from_rgb(0.18, 0.18, 0.24);
    
    pub const ACCENT: Color = Color::from_rgb(0.35, 0.55, 0.95);
    pub const ACCENT_HOVER: Color = Color::from_rgb(0.45, 0.65, 1.0);
    pub const SUCCESS: Color = Color::from_rgb(0.2, 0.75, 0.45);
    pub const SUCCESS_HOVER: Color = Color::from_rgb(0.25, 0.85, 0.55);
    pub const DANGER: Color = Color::from_rgb(0.9, 0.3, 0.35);
    pub const DANGER_HOVER: Color = Color::from_rgb(1.0, 0.4, 0.45);
    
    pub const TEXT_PRIMARY: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.95);
    pub const TEXT_SECONDARY: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.6);
    pub const TEXT_MUTED: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.4);
    
    pub const BORDER: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.1);
    pub const PILL_BG: Color = Color::from_rgb(0.28, 0.28, 0.38);  // Much brighter for clickable pills
    pub const PILL_DISABLED: Color = Color::from_rgb(0.14, 0.14, 0.18);  // Darker for disabled state
    pub const PILL_SELECTED: Color = Color::from_rgb(0.35, 0.55, 0.95);
}

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum WindowType {
    Main,
    Destination,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppMode {
    Setup,
    Capturing,
}

/// Settings tab for categorized settings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum SettingsTab {
    #[default]
    General,
    Window,
    Capture,
    Advanced,
}

impl SettingsTab {
    fn all() -> &'static [SettingsTab] {
        &[
            SettingsTab::General,
            SettingsTab::Window,
            SettingsTab::Capture,
            SettingsTab::Advanced,
        ]
    }
    
    fn label(&self) -> &'static str {
        match self {
            SettingsTab::General => "General",
            SettingsTab::Window => "Window",
            SettingsTab::Capture => "Capture",
            SettingsTab::Advanced => "Advanced",
        }
    }
    
    fn icon(&self) -> &'static str {
        match self {
            SettingsTab::General => "[G]",
            SettingsTab::Window => "[W]",
            SettingsTab::Capture => "[C]",
            SettingsTab::Advanced => "[A]",
        }
    }
}

/// Window position presets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
enum PositionPreset {
    #[default]
    Center,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Custom,
}

impl PositionPreset {
    fn all() -> Vec<Self> {
        vec![Self::Center, Self::TopLeft, Self::TopRight, Self::BottomLeft, Self::BottomRight, Self::Custom]
    }
    
    fn calculate(&self, window_width: u32, window_height: u32, custom_x: i32, custom_y: i32) -> (i32, i32) {
        // Get screen dimensions
        let (screen_width, screen_height) = get_screen_dimensions();
        let margin = 50i32;
        
        match self {
            Self::Center => (
                (screen_width as i32 - window_width as i32) / 2,
                (screen_height as i32 - window_height as i32) / 2,
            ),
            Self::TopLeft => (margin, margin),
            Self::TopRight => (screen_width as i32 - window_width as i32 - margin, margin),
            Self::BottomLeft => (margin, screen_height as i32 - window_height as i32 - margin),
            Self::BottomRight => (
                screen_width as i32 - window_width as i32 - margin,
                screen_height as i32 - window_height as i32 - margin,
            ),
            Self::Custom => (custom_x, custom_y),
        }
    }
}

impl std::fmt::Display for PositionPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Center => write!(f, "Center"),
            Self::TopLeft => write!(f, "Top Left"),
            Self::TopRight => write!(f, "Top Right"),
            Self::BottomLeft => write!(f, "Bottom Left"),
            Self::BottomRight => write!(f, "Bottom Right"),
            Self::Custom => write!(f, "Custom"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum SizePreset {
    W720p,
    W1080p,
    W1440p,
    W4K,
    Square,
    Custom,
}

impl SizePreset {
    fn dimensions(&self) -> (u32, u32) {
        match self {
            Self::W720p => (1280, 720),
            Self::W1080p => (1920, 1080),
            Self::W1440p => (2560, 1440),
            Self::W4K => (3840, 2160),
            Self::Square => (1080, 1080),
            Self::Custom => (DEFAULT_WIDTH, DEFAULT_HEIGHT),
        }
    }
    
    fn all() -> Vec<Self> {
        vec![Self::W720p, Self::W1080p, Self::W1440p, Self::W4K, Self::Square, Self::Custom]
    }
}

impl std::fmt::Display for SizePreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::W720p => write!(f, "720p (1280x720)"),
            Self::W1080p => write!(f, "1080p (1920x1080)"),
            Self::W1440p => write!(f, "1440p (2560x1440)"),
            Self::W4K => write!(f, "4K (3840x2160)"),
            Self::Square => write!(f, "Square (1080x1080)"),
            Self::Custom => write!(f, "Custom"),
        }
    }
}

/// Application settings
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Settings {
    // Mouse & Cursor
    show_cursor: bool,
    capture_clicks: bool,
    click_highlight_color: [u8; 4],  // RGBA
    click_dissolve_ms: u32,
    
    // Border
    border_color: [u8; 3],
    border_width: u32,
    // Performance
    target_fps: u32,
    use_gpu: bool,
    #[serde(default = "default_monitor_hz")]
    monitor_hz: u32,
    
    // Window Size
    size_preset: SizePreset,
    custom_width: u32,
    custom_height: u32,
    
    // Window Position
    #[serde(default)]
    position_preset: PositionPreset,
    #[serde(default = "default_custom_x")]
    custom_x: i32,
    #[serde(default = "default_custom_y")]
    custom_y: i32,
    
    // Behavior
    save_last_position: bool,
    remember_region: bool,
    auto_start: bool,
    show_shortcuts: bool,
    
    // Window position (for save_last_position)
    #[serde(default)]
    last_window_x: i32,
    #[serde(default)]
    last_window_y: i32,
}

fn default_monitor_hz() -> u32 { 60 }
fn default_custom_x() -> i32 { 100 }
fn default_custom_y() -> i32 { 100 }
impl Default for Settings {
    fn default() -> Self {
        // Get monitor refresh rate
        let monitor_hz = get_monitor_refresh_rate();
        
        Self {
            // Mouse & Cursor
            show_cursor: true,
            capture_clicks: false,
            click_highlight_color: [255, 255, 0, 180],  // Yellow semi-transparent
            click_dissolve_ms: 300,
            
            // Border
            border_color: [80, 130, 255],
            border_width: 4,
            // Performance
            target_fps: 60,
            use_gpu: true,
            monitor_hz,
            
            // Window Size
            size_preset: SizePreset::Custom,
            custom_width: DEFAULT_WIDTH,
            custom_height: DEFAULT_HEIGHT,
            
            // Window Position
            position_preset: PositionPreset::Center,
            custom_x: 100,
            custom_y: 100,
            
            // Behavior
            save_last_position: true,
            remember_region: false,
            auto_start: false,
            show_shortcuts: true,
            
            // Window position
            last_window_x: 100,
            last_window_y: 100,
        }
    }
}

impl Settings {
    fn config_path() -> PathBuf {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("RustFrame");
        let _ = fs::create_dir_all(&config_dir);
        config_dir.join("settings.json")
    }
    
    fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(contents) => {
                    match serde_json::from_str(&contents) {
                        Ok(settings) => {
                            info!("Settings loaded from {:?}", path);
                            return settings;
                        }
                        Err(e) => {
                            error!("Failed to parse settings: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to read settings file: {}", e);
                }
            }
        }
        Self::default()
    }
    
    fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                fs::write(&path, json)
                    .map_err(|e| format!("Failed to write settings: {}", e))?;
                info!("Settings saved to {:?}", path);
                Ok(())
            }
            Err(e) => Err(format!("Failed to serialize settings: {}", e)),
        }
    }
    
    fn current_size(&self) -> (u32, u32) {
        match self.size_preset {
            SizePreset::Custom => (self.custom_width, self.custom_height),
            preset => preset.dimensions(),
        }
    }
    
    /// Get effective window width based on size preset
    fn get_effective_width(&self) -> u32 {
        self.current_size().0
    }
    
    /// Get effective window height based on size preset
    fn get_effective_height(&self) -> u32 {
        self.current_size().1
    }
    
    fn border_color_bgr(&self) -> u32 {
        (self.border_color[2] as u32) 
            | ((self.border_color[1] as u32) << 8) 
            | ((self.border_color[0] as u32) << 16)
    }
    
    /// Get border color as hex string (e.g., "#5082FF")
    fn border_color_hex(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", 
            self.border_color[0], 
            self.border_color[1], 
            self.border_color[2])
    }
    
    /// Set border color from hex string (e.g., "#5082FF" or "5082FF")
    fn set_border_color_from_hex(&mut self, hex: &str) -> bool {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return false;
        }
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            self.border_color = [r, g, b];
            true
        } else {
            false
        }
    }
}

// ============================================================================
// Messages
// ============================================================================

#[derive(Debug, Clone)]
enum Message {
    MainWindowOpened(window::Id),
    DestinationOpened(window::Id),
    SettingsOpened(window::Id),
    WindowClosed(window::Id),
    WindowResized(window::Id, Size),
    WindowMoved(window::Id, iced::Point),
    
    // Window operations
    DragWindow,
    InstallSubclass,  // Install WinAPI subclass for resize
    UpdateWindowPosition, // Get actual window position via WinAPI
    MinimizeWindow,
    
    StartCapture,
    StopCapture,
    ToggleCapture,
    
    OpenSettings,
    CloseSettings,
    SwitchSettingsTab(SettingsTab),
    
    Tick,
    
    // Settings - Mouse
    SetShowCursor(bool),
    SetCaptureClicks(bool),
    SetClickHighlightR(u8),
    SetClickHighlightG(u8),
    SetClickHighlightB(u8),
    SetClickHighlightAlpha(u8),
    SetClickDissolveMs(u32),
    SetClickHighlightHex(String),
    SetClickHighlightColor([u8; 4]),
    
    // Settings - Border
    SetBorderWidth(u32),
    SetBorderColorR(u8),
    SetBorderColorG(u8),
    SetBorderColorB(u8),
    SetBorderColorHex(String),
    
    // Color Picker toggle and submit
    ToggleBorderColorPicker,
    ToggleClickColorPicker,
    BorderColorPickerSubmit(Color),
    BorderColorPickerCancel,
    ClickColorPickerSubmit(Color),
    ClickColorPickerCancel,
    
    // Settings - Performance
    SetTargetFps(u32),
    SetUseGpu(bool),
    
    // Settings - Window Size
    SetSizePreset(SizePreset),
    SetCustomWidth(String),
    SetCustomHeight(String),
    
    // Settings - Window Position
    SetPositionPreset(PositionPreset),
    SetCustomX(String),
    SetCustomY(String),
    
    // Settings - Behavior
    SetSaveLastPosition(bool),
    SetRememberRegion(bool),
    SetAutoStart(bool),
    SetShowShortcuts(bool),
    
    // Settings - Import/Export
    ExportSettings,
    ImportSettings,
    OpenSettingsFolder,
    
    // Links
    OpenDonationLink,
    OpenGitHub,
    
    ApplySettings,
    
    Exit,
}

// ============================================================================
// Application State
// ============================================================================

struct RustFrameApp {
    windows: BTreeMap<window::Id, WindowType>,
    main_id: Option<window::Id>,
    destination_id: Option<window::Id>,  // Iced destination (dev mode only)
    settings_id: Option<window::Id>,
    
    mode: AppMode,
    dev_mode: bool,
    
    hollow_border: Option<HollowBorder>,
    
    // WinAPI destination window (release mode only)
    winapi_destination: Option<DestinationWindow>,
    
    capture_engine: Option<WindowsCaptureEngine>,
    current_fps: u32,
    frame_count: u32,
    last_fps_update: Instant,
    // Cached image handle to avoid per-frame allocation (dev mode only)
    preview_handle: Option<iced::widget::image::Handle>,
    
    settings: Settings,
    temp_settings: Settings,
    current_settings_tab: SettingsTab,
    
    // Color picker state
    show_border_color_picker: bool,
    show_click_color_picker: bool,
    
    status_message: String,
    main_window_size: (u32, u32),
    main_window_pos: (i32, i32),
}

impl RustFrameApp {
    fn new() -> (Self, Task<Message>) {
        let dev_mode = std::env::args().any(|arg| arg == "--dev" || arg == "-d");
        info!("RustFrame Iced starting, dev mode: {}", dev_mode);
        
        // Load settings from disk
        let settings = Settings::load();
        let (width, height) = settings.current_size();
        
        // Calculate initial position from settings preset
        let (initial_x, initial_y) = settings.position_preset.calculate(
            width, height,
            settings.custom_x,
            settings.custom_y,
        );
        
        let initial_pos = window::Position::Specific(iced::Point::new(
            initial_x as f32,
            initial_y as f32,
        ));
        
        let app = Self {
            windows: BTreeMap::new(),
            main_id: None,
            destination_id: None,
            settings_id: None,
            
            mode: AppMode::Setup,
            dev_mode,
            
            hollow_border: None,
            winapi_destination: None,
            
            capture_engine: WindowsCaptureEngine::new().ok(),
            current_fps: 0,
            frame_count: 0,
            last_fps_update: Instant::now(),
            preview_handle: None,
            
            temp_settings: settings.clone(),
            settings,
            current_settings_tab: SettingsTab::default(),
            
            show_border_color_picker: false,
            show_click_color_picker: false,
            
            status_message: "Ready to capture".to_string(),
            main_window_size: (width, height),
            main_window_pos: (initial_x, initial_y),
        };
        
        // Release mode: skip main window, go straight to setup
        // Dev mode: show main window
        if dev_mode {
            let main_settings = window::Settings {
                size: Size::new(width as f32, height as f32),
                min_size: Some(Size::new(MIN_WIDTH as f32, MIN_HEIGHT as f32)),
                position: initial_pos,
                decorations: false,
                transparent: false,
                resizable: true,
                level: window::Level::Normal,
                visible: true,
                icon: load_app_icon(),
                ..Default::default()
            };
            
            let (_, open_task) = window::open(main_settings);
            (app, open_task.map(Message::MainWindowOpened))
        } else {
            // Release mode: no main window, start in Setup mode ready to capture
            info!("Release mode: skipping main window");
            (app, Task::none())
        }
    }
    
    fn title(&self, window_id: window::Id) -> String {
        match self.windows.get(&window_id) {
            Some(WindowType::Main) => format!("RustFrame - {}x{}", 
                self.main_window_size.0, self.main_window_size.1),
            Some(WindowType::Destination) => "RustFrame - Preview".to_string(),
            Some(WindowType::Settings) => "Settings".to_string(),
            None => "RustFrame".to_string(),
        }
    }
    
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::MainWindowOpened(id) => {
                self.main_id = Some(id);
                self.windows.insert(id, WindowType::Main);
                info!("Main window opened: {:?}", id);
                
                // Install subclass for resize support after window is ready
                return Task::batch([
                    window::gain_focus(id),
                    Task::done(Message::InstallSubclass),
                    Task::done(Message::UpdateWindowPosition),
                ]);
            }
            
            Message::InstallSubclass => {
                // Install WinAPI subclass on main window for resize handling
                if let Some(id) = self.main_id {
                    info!("Installing subclass for window id: {:?}", id);
                    return window::run_with_handle(id, |handle| {
                        use iced::window::raw_window_handle::HasWindowHandle;
                        info!("run_with_handle closure executing");
                        match handle.window_handle() {
                            Ok(raw_handle) => {
                                info!("Got window handle");
                                if let iced::window::raw_window_handle::RawWindowHandle::Win32(win32) = raw_handle.as_ref() {
                                    let hwnd = HWND(win32.hwnd.get() as *mut _);
                                    info!("Got HWND: {:?}", hwnd.0);
                                    install_main_window_subclass(hwnd);
                                } else {
                                    info!("Not a Win32 window handle");
                                }
                            }
                            Err(e) => {
                                error!("Failed to get window handle: {:?}", e);
                            }
                        }
                    }).discard();
                } else {
                    error!("No main window id available for subclass installation");
                }
            }
            
            Message::UpdateWindowPosition => {
                // Get actual window position via WinAPI
                if let Some(id) = self.main_id {
                    let main_id = id; // Capture for use in map closure
                    return window::run_with_handle(id, |handle| {
                        use iced::window::raw_window_handle::HasWindowHandle;
                        if let Ok(raw_handle) = handle.window_handle() {
                            if let iced::window::raw_window_handle::RawWindowHandle::Win32(win32) = raw_handle.as_ref() {
                                let hwnd = HWND(win32.hwnd.get() as *mut _);
                                unsafe {
                                    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
                                    let mut rect = windows::Win32::Foundation::RECT::default();
                                    if GetWindowRect(hwnd, &mut rect).is_ok() {
                                        return Some((rect.left, rect.top));
                                    }
                                }
                            }
                        }
                        None
                    }).map(move |pos_opt| {
                        if let Some((x, y)) = pos_opt {
                            Message::WindowMoved(main_id, iced::Point::new(x as f32, y as f32))
                        } else {
                            Message::Tick // No-op fallback
                        }
                    });
                }
            }
            
            Message::StartCapture | Message::ToggleCapture if self.mode == AppMode::Setup => {
                info!("Starting capture...");
                
                let (width, height) = self.main_window_size;
                
                // Get actual window position via WinAPI
                let (x, y) = {
                    let stored_hwnd = MAIN_HWND.load(Ordering::SeqCst);
                    if stored_hwnd != 0 {
                        let hwnd = HWND(stored_hwnd as *mut _);
                        let mut rect = windows::Win32::Foundation::RECT::default();
                        unsafe {
                            if windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rect).is_ok() {
                                info!("Got real window position: ({}, {})", rect.left, rect.top);
                                (rect.left, rect.top)
                            } else {
                                info!("GetWindowRect failed, using stored position");
                                self.main_window_pos
                            }
                        }
                    } else {
                        info!("No MAIN_HWND, using stored position");
                        self.main_window_pos
                    }
                };
                
                let border_color = self.settings.border_color_bgr();
                let border_width = self.settings.border_width as i32;
                self.hollow_border = HollowBorder::new(
                    x, y, 
                    width as i32, height as i32,
                    border_width,
                    border_color,
                );
                
                if self.hollow_border.is_none() {
                    error!("Failed to create hollow border window");
                    self.status_message = "Error: Could not create capture border".to_string();
                    return Task::none();
                }
                
                let region = CaptureRect {
                    x: x + border_width,
                    y: y + border_width,
                    width: width.saturating_sub(border_width as u32 * 2),
                    height: height.saturating_sub(border_width as u32 * 2),
                };
                
                if let Some(ref mut engine) = self.capture_engine {
                    match engine.start(region, self.settings.show_cursor) {
                        Ok(_) => {
                            self.mode = AppMode::Capturing;
                            self.frame_count = 0;
                            self.last_fps_update = Instant::now();
                            self.status_message = "Capturing...".to_string();
                            
                            // Start mouse hook if click highlighting is enabled
                            if self.settings.capture_clicks {
                                mouse_hook::start_capture();
                            }
                            
                            if let Some(main_id) = self.main_id {
                                // Dev mode only: main window exists
                                // Calculate content size (without border)
                                let content_width = width.saturating_sub(self.settings.border_width * 2);
                                let content_height = height.saturating_sub(self.settings.border_width * 2);
                                
                                // Use Iced window for debugging
                                if self.destination_id.is_none() {
                                    let dest_settings = window::Settings {
                                        size: Size::new(content_width as f32, content_height as f32),
                                        position: window::Position::Specific(iced::Point::new(
                                            (x + width as i32 + 20) as f32,
                                            y as f32,
                                        )),
                                        decorations: true,
                                        resizable: true,
                                        icon: load_app_icon(),
                                        ..Default::default()
                                    };
                                    let (_, open_task) = window::open(dest_settings);
                                    
                                    return Task::batch([
                                        window::change_mode(main_id, window::Mode::Hidden),
                                        open_task.map(Message::DestinationOpened),
                                    ]);
                                } else {
                                    return window::change_mode(main_id, window::Mode::Hidden);
                                }
                            } else {
                                // Release mode: No main window exists, just use WinAPI destination
                                let content_width = width.saturating_sub(self.settings.border_width * 2);
                                let content_height = height.saturating_sub(self.settings.border_width * 2);
                                
                                self.winapi_destination = DestinationWindow::new(
                                    content_width,
                                    content_height,
                                );
                                
                                if self.winapi_destination.is_none() {
                                    error!("Failed to create WinAPI destination window");
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to start capture: {}", e);
                            self.status_message = format!("Error: {}", e);
                            self.hollow_border = None;
                        }
                    }
                } else {
                    self.status_message = "Capture engine not available".to_string();
                    self.hollow_border = None;
                }
            }
            
            Message::StopCapture | Message::ToggleCapture if self.mode == AppMode::Capturing => {
                info!("Stopping capture...");
                
                // Stop mouse hook
                mouse_hook::stop_capture();
                
                if let Some(ref mut engine) = self.capture_engine {
                    engine.stop();
                }
                
                self.hollow_border = None;
                self.winapi_destination = None;  // Clean up WinAPI window
                
                self.mode = AppMode::Setup;
                self.current_fps = 0;
                self.preview_handle = None;
                self.status_message = "Capture stopped".to_string();
                
                let mut tasks = vec![];
                
                if let Some(main_id) = self.main_id {
                    // Dev mode: show the hidden main window
                    tasks.push(window::change_mode(main_id, window::Mode::Windowed));
                }
                
                // Close Iced destination window (dev mode only)
                if let Some(dest_id) = self.destination_id.take() {
                    self.windows.remove(&dest_id);
                    tasks.push(window::close(dest_id));
                }
                
                if !tasks.is_empty() {
                    return Task::batch(tasks);
                }
            }
            
            Message::Tick => {
                if self.mode == AppMode::Capturing {
                    // Check if ESC was pressed in hollow border
                    if hollow_border::was_esc_pressed() {
                        info!("ESC detected - stopping capture");
                        return Task::done(Message::StopCapture);
                    }
                    
                    if let Some(ref mut engine) = self.capture_engine {
                        if let Some(mut frame) = engine.get_frame() {
                            self.frame_count += 1;
                            
                            let elapsed = self.last_fps_update.elapsed().as_secs_f64();
                            if elapsed >= 1.0 {
                                self.current_fps = (self.frame_count as f64 / elapsed) as u32;
                                self.frame_count = 0;
                                self.last_fps_update = Instant::now();
                            }
                            
                            // Draw click highlights on the frame if enabled (before color conversion)
                            if self.settings.capture_clicks {
                                if let Some(ref border) = self.hollow_border {
                                    let (border_x, border_y, _, _) = border.get_rect();
                                    let border_width = self.settings.border_width as i32;
                                    let capture_x = border_x + border_width;
                                    let capture_y = border_y + border_width;
                                    
                                    let clicks = mouse_hook::get_recent_clicks(self.settings.click_dissolve_ms);
                                    for click in clicks {
                                        let opacity = mouse_hook::calculate_opacity(&click, self.settings.click_dissolve_ms);
                                        if opacity > 0.0 {
                                            let local_x = click.x - capture_x;
                                            let local_y = click.y - capture_y;
                                            
                                            draw_click_circle(
                                                &mut frame.data,
                                                frame.width,
                                                frame.height,
                                                local_x,
                                                local_y,
                                                &self.settings.click_highlight_color,
                                                opacity,
                                            );
                                        }
                                    }
                                }
                            }
                            
                            if self.dev_mode {
                                // Dev mode: Iced window needs RGBA
                                for chunk in frame.data.chunks_exact_mut(4) {
                                    chunk.swap(0, 2); // BGRA -> RGBA
                                }
                                self.preview_handle = Some(
                                    iced::widget::image::Handle::from_rgba(frame.width, frame.height, frame.data)
                                );
                            } else {
                                // Release mode: WinAPI window uses BGRA directly
                                if let Some(ref dest) = self.winapi_destination {
                                    dest.update_frame(frame.data, frame.width, frame.height);
                                    dest.process_messages();
                                }
                            }
                        }
                    }
                    
                    if let Some(ref border) = self.hollow_border {
                        let (x, y, w, h) = border.get_rect();
                        let border_width = self.settings.border_width as i32;
                        let content_w = (w - border_width * 2).max(1) as u32;
                        let content_h = (h - border_width * 2).max(1) as u32;
                        let new_region = CaptureRect {
                            x: x + border_width,
                            y: y + border_width,
                            width: content_w,
                            height: content_h,
                        };
                        
                        if let Some(ref mut engine) = self.capture_engine {
                            let _ = engine.update_region(new_region);
                        }
                        
                        // Sync destination window size with capture region
                        if let Some(dest_id) = self.destination_id {
                            return window::resize(dest_id, Size::new(content_w as f32, content_h as f32));
                        }
                    }
                }
            }
            
            Message::DestinationOpened(id) => {
                self.destination_id = Some(id);
                self.windows.insert(id, WindowType::Destination);
                info!("Destination window opened: {:?}", id);
                
                // In release mode, hide from Alt+Tab using WS_EX_TOOLWINDOW
                if !self.dev_mode {
                    return window::run_with_handle(id, |handle| {
                        use iced::window::raw_window_handle::HasWindowHandle;
                        if let Ok(raw_handle) = handle.window_handle() {
                            if let iced::window::raw_window_handle::RawWindowHandle::Win32(win32) = raw_handle.as_ref() {
                                let hwnd = HWND(win32.hwnd.get() as *mut _);
                                unsafe {
                                    use windows::Win32::UI::WindowsAndMessaging::{
                                        GetWindowLongW, SetWindowLongW, GWL_EXSTYLE,
                                        WS_EX_TOOLWINDOW, WS_EX_APPWINDOW,
                                    };
                                    // Get current extended style
                                    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
                                    // Add TOOLWINDOW (hides from taskbar/Alt+Tab), remove APPWINDOW
                                    let new_style = (ex_style | WS_EX_TOOLWINDOW.0) & !WS_EX_APPWINDOW.0;
                                    SetWindowLongW(hwnd, GWL_EXSTYLE, new_style as i32);
                                    info!("Applied WS_EX_TOOLWINDOW to destination window");
                                }
                            }
                        }
                    }).discard();
                }
            }
            
            Message::OpenSettings => {
                if self.settings_id.is_none() {
                    self.temp_settings = self.settings.clone();
                    // Update monitor_hz to current value
                    self.temp_settings.monitor_hz = get_monitor_refresh_rate();
                    self.current_settings_tab = SettingsTab::default();
                    
                    let settings = window::Settings {
                        size: Size::new(500.0, 600.0),
                        position: window::Position::Centered,
                        decorations: true,
                        resizable: false,
                        level: window::Level::AlwaysOnTop,
                        icon: load_app_icon(),
                        ..Default::default()
                    };
                    let (_, open_task) = window::open(settings);
                    return open_task.map(Message::SettingsOpened);
                }
            }
            
            Message::SettingsOpened(id) => {
                self.settings_id = Some(id);
                self.windows.insert(id, WindowType::Settings);
            }
            
            Message::SwitchSettingsTab(tab) => {
                self.current_settings_tab = tab;
            }
            
            Message::CloseSettings => {
                if let Some(id) = self.settings_id.take() {
                    self.windows.remove(&id);
                    return window::close(id);
                }
            }
            
            Message::ApplySettings => {
                self.settings = self.temp_settings.clone();
                
                // Save settings to disk
                if let Err(e) = self.settings.save() {
                    error!("Failed to save settings: {}", e);
                    self.status_message = format!("Settings applied but failed to save: {}", e);
                } else {
                    self.status_message = "Settings saved".to_string();
                }
                
                let (w, h) = self.settings.current_size();
                self.main_window_size = (w, h);
                
                // Calculate new position from settings
                let (new_x, new_y) = self.settings.position_preset.calculate(
                    w, h,
                    self.settings.custom_x,
                    self.settings.custom_y,
                );
                self.main_window_pos = (new_x, new_y);
                
                if let Some(id) = self.settings_id.take() {
                    self.windows.remove(&id);
                    
                    let mut tasks = vec![window::close(id)];
                    
                    if let Some(main_id) = self.main_id {
                        // Resize and move window to new position
                        tasks.push(window::resize(main_id, Size::new(w as f32, h as f32)));
                        tasks.push(window::move_to(main_id, iced::Point::new(new_x as f32, new_y as f32)));
                    }
                    
                    return Task::batch(tasks);
                }
            }
            
            Message::WindowClosed(id) => {
                self.windows.remove(&id);
                
                if self.main_id == Some(id) {
                    self.main_id = None;
                    
                    // If we're capturing, don't exit - main window was intentionally closed
                    if self.mode == AppMode::Capturing {
                        info!("Main window closed for capture mode (release mode)");
                        // Release GPU resources but keep capture running
                        self.preview_handle = None;
                        return Task::none();
                    }
                    
                    // Not capturing - clean up and exit
                    mouse_hook::stop_capture();
                    if let Some(ref mut engine) = self.capture_engine {
                        engine.stop();
                    }
                    self.hollow_border = None;
                    self.winapi_destination = None;
                    
                    // Release GPU resources
                    self.preview_handle = None;
                    self.capture_engine = None;
                    
                    return iced::exit();
                }
                if self.destination_id == Some(id) {
                    self.destination_id = None;
                }
                if self.settings_id == Some(id) {
                    self.settings_id = None;
                }
            }
            
            Message::WindowResized(id, size) => {
                if self.main_id == Some(id) {
                    self.main_window_size = (size.width as u32, size.height as u32);
                    debug!("Main window resized to {}x{}", size.width, size.height);
                }
            }
            
            Message::WindowMoved(id, pos) => {
                if self.main_id == Some(id) {
                    self.main_window_pos = (pos.x as i32, pos.y as i32);
                    debug!("Main window moved to ({}, {})", pos.x, pos.y);
                }
            }
            
            Message::DragWindow => {
                // DragWindow is now handled by WinAPI subclass via WM_NCHITTEST -> HTCAPTION
                // This is kept as fallback for elements that explicitly call it
                if let Some(id) = self.main_id {
                    return window::drag(id);
                }
            }
            
            // Settings - Mouse
            Message::SetShowCursor(v) => self.temp_settings.show_cursor = v,
            Message::SetCaptureClicks(v) => self.temp_settings.capture_clicks = v,
            Message::SetClickHighlightR(v) => self.temp_settings.click_highlight_color[0] = v,
            Message::SetClickHighlightG(v) => self.temp_settings.click_highlight_color[1] = v,
            Message::SetClickHighlightB(v) => self.temp_settings.click_highlight_color[2] = v,
            Message::SetClickHighlightAlpha(v) => self.temp_settings.click_highlight_color[3] = v,
            Message::SetClickDissolveMs(v) => self.temp_settings.click_dissolve_ms = v,
            Message::SetClickHighlightHex(hex) => {
                // Parse hex color like #RRGGBB or #RRGGBBAA
                let hex = hex.trim_start_matches('#');
                if hex.len() >= 6 {
                    if let (Ok(r), Ok(g), Ok(b)) = (
                        u8::from_str_radix(&hex[0..2], 16),
                        u8::from_str_radix(&hex[2..4], 16),
                        u8::from_str_radix(&hex[4..6], 16),
                    ) {
                        self.temp_settings.click_highlight_color[0] = r;
                        self.temp_settings.click_highlight_color[1] = g;
                        self.temp_settings.click_highlight_color[2] = b;
                        // Parse alpha if provided
                        if hex.len() >= 8 {
                            if let Ok(a) = u8::from_str_radix(&hex[6..8], 16) {
                                self.temp_settings.click_highlight_color[3] = a;
                            }
                        }
                    }
                }
            }
            Message::SetClickHighlightColor(color) => {
                self.temp_settings.click_highlight_color = color;
            }
            
            // Settings - Border
            Message::SetBorderWidth(v) => self.temp_settings.border_width = v,
            Message::SetBorderColorR(v) => self.temp_settings.border_color[0] = v,
            Message::SetBorderColorG(v) => self.temp_settings.border_color[1] = v,
            Message::SetBorderColorB(v) => self.temp_settings.border_color[2] = v,
            Message::SetBorderColorHex(hex) => {
                self.temp_settings.set_border_color_from_hex(&hex);
            }
            
            // Color Picker handlers
            Message::ToggleBorderColorPicker => {
                self.show_border_color_picker = !self.show_border_color_picker;
                self.show_click_color_picker = false; // Close other picker
            }
            Message::ToggleClickColorPicker => {
                self.show_click_color_picker = !self.show_click_color_picker;
                self.show_border_color_picker = false; // Close other picker
            }
            Message::BorderColorPickerSubmit(color) => {
                self.temp_settings.border_color[0] = (color.r * 255.0) as u8;
                self.temp_settings.border_color[1] = (color.g * 255.0) as u8;
                self.temp_settings.border_color[2] = (color.b * 255.0) as u8;
                self.show_border_color_picker = false;
            }
            Message::BorderColorPickerCancel => {
                self.show_border_color_picker = false;
            }
            Message::ClickColorPickerSubmit(color) => {
                self.temp_settings.click_highlight_color[0] = (color.r * 255.0) as u8;
                self.temp_settings.click_highlight_color[1] = (color.g * 255.0) as u8;
                self.temp_settings.click_highlight_color[2] = (color.b * 255.0) as u8;
                self.temp_settings.click_highlight_color[3] = (color.a * 255.0) as u8;
                self.show_click_color_picker = false;
            }
            Message::ClickColorPickerCancel => {
                self.show_click_color_picker = false;
            }
            
            // Settings - Performance
            Message::SetTargetFps(v) => {
                let max_fps = get_monitor_refresh_rate().min(240);
                self.temp_settings.target_fps = v.min(max_fps);
            }
            Message::SetUseGpu(v) => self.temp_settings.use_gpu = v,
            
            // Settings - Window Size
            Message::SetSizePreset(preset) => {
                self.temp_settings.size_preset = preset;
                if preset != SizePreset::Custom {
                    let (w, h) = preset.dimensions();
                    self.temp_settings.custom_width = w;
                    self.temp_settings.custom_height = h;
                }
            }
            Message::SetCustomWidth(s) => {
                if let Ok(w) = s.parse::<u32>() {
                    self.temp_settings.custom_width = w.max(MIN_WIDTH);
                    self.temp_settings.size_preset = SizePreset::Custom;
                }
            }
            Message::SetCustomHeight(s) => {
                if let Ok(h) = s.parse::<u32>() {
                    self.temp_settings.custom_height = h.max(MIN_HEIGHT);
                    self.temp_settings.size_preset = SizePreset::Custom;
                }
            }
            
            // Settings - Window Position
            Message::SetPositionPreset(preset) => {
                self.temp_settings.position_preset = preset;
                // Calculate and set custom_x/custom_y based on preset
                if preset != PositionPreset::Custom {
                    let window_w = self.temp_settings.get_effective_width();
                    let window_h = self.temp_settings.get_effective_height();
                    let (x, y) = preset.calculate(window_w, window_h, 0, 0); // custom_x/y ignored for presets
                    self.temp_settings.custom_x = x;
                    self.temp_settings.custom_y = y;
                }
            }
            Message::SetCustomX(s) => {
                if let Ok(x) = s.parse::<i32>() {
                    self.temp_settings.custom_x = x;
                    self.temp_settings.position_preset = PositionPreset::Custom;
                }
            }
            Message::SetCustomY(s) => {
                if let Ok(y) = s.parse::<i32>() {
                    self.temp_settings.custom_y = y;
                    self.temp_settings.position_preset = PositionPreset::Custom;
                }
            }
            
            // Settings - Behavior
            Message::SetSaveLastPosition(v) => self.temp_settings.save_last_position = v,
            Message::SetRememberRegion(v) => self.temp_settings.remember_region = v,
            Message::SetAutoStart(v) => self.temp_settings.auto_start = v,
            Message::SetShowShortcuts(v) => self.temp_settings.show_shortcuts = v,
            
            // Settings - Import/Export
            Message::ExportSettings => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("JSON", &["json"])
                    .set_file_name("rustframe_settings.json")
                    .save_file()
                {
                    match serde_json::to_string_pretty(&self.settings) {
                        Ok(json) => {
                            if let Err(e) = fs::write(&path, json) {
                                error!("Failed to export settings: {}", e);
                            } else {
                                info!("Settings exported to {:?}", path);
                            }
                        }
                        Err(e) => error!("Failed to serialize settings: {}", e),
                    }
                }
            }
            Message::ImportSettings => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("JSON", &["json"])
                    .pick_file()
                {
                    match fs::read_to_string(&path) {
                        Ok(json) => {
                            match serde_json::from_str::<Settings>(&json) {
                                Ok(imported) => {
                                    self.temp_settings = imported;
                                    info!("Settings imported from {:?}", path);
                                }
                                Err(e) => error!("Failed to parse settings: {}", e),
                            }
                        }
                        Err(e) => error!("Failed to read settings file: {}", e),
                    }
                }
            }
            Message::OpenSettingsFolder => {
                if let Some(parent) = Settings::config_path().parent() {
                    let _ = std::process::Command::new("explorer").arg(parent).spawn();
                }
            }
            
            Message::OpenDonationLink => {
                let _ = std::process::Command::new("cmd")
                    .args(["/C", "start", "https://www.paypal.com/ncp/payment/6GVW3NYM36V22"])
                    .spawn();
            }
            
            Message::OpenGitHub => {
                let _ = std::process::Command::new("cmd")
                    .args(["/C", "start", "https://github.com/salihcantekin/RustFrame"])
                    .spawn();
            }
            
            Message::MinimizeWindow => {
                if let Some(main_id) = self.main_id {
                    return window::minimize(main_id, true);
                }
            }
            
            Message::Exit => {
                if self.mode == AppMode::Capturing {
                    if let Some(ref mut engine) = self.capture_engine {
                        engine.stop();
                    }
                    self.hollow_border = None;
                }
                std::process::exit(0);
            }
            
            _ => {}
        }
        
        Task::none()
    }
    
    fn view(&self, window_id: window::Id) -> Element<'_, Message> {
        match self.windows.get(&window_id) {
            Some(WindowType::Main) => self.view_main(),
            Some(WindowType::Destination) => self.view_destination(),
            Some(WindowType::Settings) => self.view_settings(),
            None => container(text("Loading..."))
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
        }
    }
    
    fn view_main(&self) -> Element<'_, Message> {
        let title_bar = self.view_title_bar();
        let indicators = self.view_indicators();
        let play_button = self.view_play_button();
        
        let shortcuts_hint: Element<'_, Message> = if self.settings.show_shortcuts {
            row![
                text("[Enter] Start").size(10).color(colors::TEXT_MUTED),
                text("  |  ").size(10).color(colors::TEXT_MUTED),
                text("[ESC] Stop").size(10).color(colors::TEXT_MUTED),
            ].into()
        } else {
            row![].into()
        };
        
        // Left footer: size info
        let size_info = text(format!("{}x{}", self.main_window_size.0, self.main_window_size.1))
            .size(11)
            .color(colors::TEXT_MUTED);
        
        // Center footer: shortcuts
        let center_content: Element<'_, Message> = if self.settings.show_shortcuts {
            container(shortcuts_hint).width(Length::Shrink).into()
        } else {
            container(text("Drag edges to resize").size(10).color(colors::TEXT_MUTED))
                .width(Length::Shrink).into()
        };
        
        // Right footer: donation link
        let donate_btn = button(
            row![
                text("♥").size(12).color(Color::from_rgb(0.95, 0.4, 0.5)),
                text("Support").size(10).color(colors::TEXT_MUTED),
            ]
            .spacing(4)
            .align_y(Alignment::Center)
        )
        .padding(Padding::from([4, 8]))
        .style(|_, s| {
            let bg = match s {
                button::Status::Hovered => Color::from_rgba(0.95, 0.4, 0.5, 0.15),
                button::Status::Pressed => Color::from_rgba(0.95, 0.4, 0.5, 0.25),
                _ => Color::TRANSPARENT,
            };
            button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: colors::TEXT_MUTED,
                border: iced::Border {
                    radius: 4.0.into(),
                    color: Color::from_rgba(0.95, 0.4, 0.5, 0.3),
                    width: 1.0,
                },
                ..Default::default()
            }
        })
        .on_press(Message::OpenDonationLink);
        
        let footer = container(
            row![
                size_info,
                horizontal_space(),
                center_content,
                horizontal_space(),
                donate_btn,
            ]
            .align_y(Alignment::Center)
        )
        .padding(Padding::from([8, 16]))
        .width(Length::Fill);
        
        // Content layout
        let content = column![
            title_bar,
            vertical_space(),
            center(
                column![
                    indicators,
                    vertical_space().height(24),
                    play_button,
                    vertical_space().height(16),
                    text(&self.status_message)
                        .size(13)
                        .color(colors::TEXT_SECONDARY),
                ]
                .align_x(Alignment::Center)
            ),
            vertical_space(),
            footer,
        ];
        
        // Wrap the container with resize areas - rounded corners with clip
        let main_content = container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true)  // Clip content to rounded corners
            .style(|_| container::Style {
                background: Some(iced::Background::Color(colors::BG_PRIMARY)),
                border: iced::Border {
                    color: colors::ACCENT,
                    width: 2.0,
                    radius: MAIN_WINDOW_RADIUS.into(),
                },
                ..Default::default()
            });
        
        // Resize is handled by WinAPI subclass (WM_NCHITTEST)
        // No need for manual resize areas in the UI
        main_content.into()
    }
    
    fn view_title_bar(&self) -> Element<'_, Message> {
        // App icon
        let logo_icon = container(
            text("[R]").size(14).color(colors::ACCENT)
        )
        .padding(Padding::from([4, 6]))
        .style(|_| container::Style {
            background: Some(iced::Background::Color(Color::from_rgba(0.3, 0.5, 1.0, 0.15))),
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });
        
        let title = text("RustFrame")
            .size(16)
            .color(colors::TEXT_PRIMARY);
        
        // Author badge with link
        let author_badge = button(
            text("by Salih Cantekin").size(10).color(colors::TEXT_MUTED)
        )
        .padding(Padding::from([2, 6]))
        .style(|_, s| {
            let (bg, text_col) = match s {
                button::Status::Hovered => (Color::from_rgba(1.0, 1.0, 1.0, 0.1), colors::TEXT_SECONDARY),
                _ => (Color::from_rgba(1.0, 1.0, 1.0, 0.05), colors::TEXT_MUTED),
            };
            button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: text_col,
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(Message::OpenGitHub);
        
        let version_badge = container(
            text("v1.1").size(10).color(colors::TEXT_MUTED)
        )
        .padding(Padding::from([2, 6]))
        .style(|_| container::Style {
            background: Some(iced::Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.05))),
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });
        
        // Settings button with text icon
        let settings_btn = button(
            text("Settings").size(11)
        )
        .padding(Padding::from([8, 12]))
        .style(|_, s| {
            let (bg, text_col) = match s {
                button::Status::Hovered => (Color::from_rgba(1.0, 1.0, 1.0, 0.1), colors::TEXT_PRIMARY),
                button::Status::Pressed => (Color::from_rgba(1.0, 1.0, 1.0, 0.15), colors::TEXT_PRIMARY),
                _ => (Color::TRANSPARENT, colors::TEXT_SECONDARY),
            };
            button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: text_col,
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(Message::OpenSettings);
        
        // Minimize button
        let minimize_btn = button(
            text("_").size(14)
        )
        .padding(Padding::from([8, 14]))
        .style(|_, s| {
            let bg = match s {
                button::Status::Hovered => Color::from_rgba(1.0, 1.0, 1.0, 0.1),
                button::Status::Pressed => Color::from_rgba(1.0, 1.0, 1.0, 0.15),
                _ => Color::TRANSPARENT,
            };
            button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: colors::TEXT_SECONDARY,
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(Message::MinimizeWindow);
        
        // Close button with red hover
        let close_btn = button(
            text("X").size(12)
        )
        .padding(Padding::from([8, 14]))
        .style(|_, s| {
            let (bg, text_col) = match s {
                button::Status::Hovered => (colors::DANGER, Color::WHITE),
                button::Status::Pressed => (colors::DANGER_HOVER, Color::WHITE),
                _ => (Color::TRANSPARENT, colors::TEXT_SECONDARY),
            };
            button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: text_col,
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(Message::Exit);
        
        // Left side: logo + title + author + version
        let left_side = row![
            logo_icon,
            title,
            author_badge,
            version_badge,
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        
        // Right side: window controls
        let right_side = row![
            settings_btn,
            minimize_btn,
            close_btn,
        ]
        .spacing(2)
        .align_y(Alignment::Center);
        
        container(
            row![
                left_side,
                horizontal_space(),
                right_side,
            ]
            .align_y(Alignment::Center)
            .padding(Padding::from([8, 12]))
        )
        .width(Length::Fill)
        .height(Length::Fixed(48.0))
        .style(|_| container::Style {
            background: Some(iced::Background::Color(colors::BG_PRIMARY)),
            border: iced::Border {
                color: Color::from_rgba(1.0, 1.0, 1.0, 0.05),
                width: 0.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
    }
    
    fn view_indicators(&self) -> Element<'_, Message> {
        let cursor_on = self.settings.show_cursor;
        let gpu_on = self.settings.use_gpu;
        let fps_label = format!("{} FPS", self.settings.target_fps);
        
        let make_indicator = |label: String, enabled: bool| -> Element<'_, Message> {
            let color = if enabled { colors::SUCCESS } else { colors::TEXT_MUTED };
            let icon = if enabled { "[ON]" } else { "[OFF]" };
            
            container(
                row![
                    text(icon).size(10).color(color),
                    text(label).size(12).color(colors::TEXT_SECONDARY),
                ]
                .spacing(6)
                .align_y(Alignment::Center)
            )
            .padding(Padding::from([6, 12]))
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(colors::BG_SECONDARY)),
                border: iced::Border {
                    color: if enabled { colors::SUCCESS } else { colors::BORDER },
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            })
            .into()
        };
        
        row![
            make_indicator("Cursor".to_string(), cursor_on),
            make_indicator("GPU".to_string(), gpu_on),
            make_indicator(fps_label, true),
        ]
        .spacing(12)
        .into()
    }
    
    fn view_play_button(&self) -> Element<'_, Message> {
        button(
            column![
                text(">").size(48),
                text("Start Capture").size(14),
            ]
            .spacing(8)
            .align_x(Alignment::Center)
        )
        .padding(Padding::from([24, 48]))
        .style(|_, s| {
            let bg = match s {
                button::Status::Hovered | button::Status::Pressed => colors::SUCCESS_HOVER,
                _ => colors::SUCCESS,
            };
            button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: Color::WHITE,
                border: iced::Border {
                    radius: 12.0.into(),
                    ..Default::default()
                },
                shadow: iced::Shadow {
                    color: Color::from_rgba(0.2, 0.75, 0.45, 0.4),
                    offset: iced::Vector::new(0.0, 4.0),
                    blur_radius: 16.0,
                },
            }
        })
        .on_press(Message::StartCapture)
        .into()
    }
    
    fn view_destination(&self) -> Element<'_, Message> {
        // Only show content - no UI controls (they would appear in screen share)
        if let Some(ref handle) = self.preview_handle {
            // Use cached handle - no clone needed
            iced::widget::image(handle.clone())
                .width(Length::Fill)
                .height(Length::Fill)
                .content_fit(iced::ContentFit::Fill)
                .into()
        } else {
            // Black background while waiting
            container(text(""))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(iced::Background::Color(Color::BLACK)),
                    ..Default::default()
                })
                .into()
        }
    }
    
    fn view_settings(&self) -> Element<'_, Message> {
        // ===== TAB BAR =====
        let current_tab = self.current_settings_tab;
        let tab_buttons: Element<'_, Message> = row(
            SettingsTab::all().iter().map(|tab| {
                let is_selected = current_tab == *tab;
                let tab_label = format!("{} {}", tab.icon(), tab.label());
                let tab_val = *tab;
                
                button(
                    text(tab_label).size(12)
                )
                .padding(Padding::from([10, 16]))
                .style(move |_, s| {
                    let (bg, text_col, border_bottom) = if is_selected {
                        (colors::ACCENT, Color::WHITE, colors::ACCENT)
                    } else {
                        match s {
                            button::Status::Hovered => (colors::BG_HOVER, colors::TEXT_PRIMARY, colors::BORDER),
                            _ => (Color::TRANSPARENT, colors::TEXT_SECONDARY, colors::BORDER),
                        }
                    };
                    button::Style {
                        background: Some(iced::Background::Color(bg)),
                        text_color: text_col,
                        border: iced::Border {
                            radius: 6.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                })
                .on_press(Message::SwitchSettingsTab(tab_val))
                .into()
            }).collect::<Vec<_>>()
        )
        .spacing(4)
        .into();
        
        // ===== TAB CONTENT =====
        let tab_content: Element<'_, Message> = match self.current_settings_tab {
            SettingsTab::General => self.view_settings_general(),
            SettingsTab::Window => self.view_settings_window(),
            SettingsTab::Capture => self.view_settings_capture(),
            SettingsTab::Advanced => self.view_settings_advanced(),
        };
        
        // Wrap tab content with padding for scroll margins (bottom and right)
        let scrollable_content = container(
            column![
                row![
                    tab_content,
                    horizontal_space().width(20), // Right margin to prevent card cutoff
                ],
                vertical_space().height(24), // Bottom margin for last card visibility
            ]
        );
        
        // ===== BOTTOM BUTTONS =====
        let buttons = row![
            button(text("Cancel").size(13))
                .padding(Padding::from([10, 24]))
                .style(|_, s| {
                    let bg = match s {
                        button::Status::Hovered | button::Status::Pressed => colors::BG_HOVER,
                        _ => colors::BG_SECONDARY,
                    };
                    button::Style {
                        background: Some(iced::Background::Color(bg)),
                        text_color: colors::TEXT_SECONDARY,
                        border: iced::Border {
                            color: colors::BORDER,
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    }
                })
                .on_press(Message::CloseSettings),
            horizontal_space(),
            button(text("Save").size(13))
                .padding(Padding::from([10, 24]))
                .style(|_, s| {
                    let bg = match s {
                        button::Status::Hovered | button::Status::Pressed => colors::ACCENT_HOVER,
                        _ => colors::ACCENT,
                    };
                    button::Style {
                        background: Some(iced::Background::Color(bg)),
                        text_color: Color::WHITE,
                        border: iced::Border { radius: 6.0.into(), ..Default::default() },
                        ..Default::default()
                    }
                })
                .on_press(Message::ApplySettings),
        ];
        
        let content = column![
            text("Settings").size(20).color(colors::TEXT_PRIMARY),
            vertical_space().height(12),
            tab_buttons,
            vertical_space().height(12),
            scrollable(scrollable_content).height(Length::Fill),
            vertical_space().height(16),
            buttons,
        ]
        .padding(20);
        
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(colors::BG_PRIMARY)),
                ..Default::default()
            })
            .into()
    }
    
    /// General settings tab: Mouse, Cursor, Click highlights
    fn view_settings_general(&self) -> Element<'_, Message> {
        let click_preview_color = Color::from_rgba(
            self.temp_settings.click_highlight_color[0] as f32 / 255.0,
            self.temp_settings.click_highlight_color[1] as f32 / 255.0,
            self.temp_settings.click_highlight_color[2] as f32 / 255.0,
            self.temp_settings.click_highlight_color[3] as f32 / 255.0,
        );
        let dissolve_ms = self.temp_settings.click_dissolve_ms;
        let alpha = self.temp_settings.click_highlight_color[3];
        let click_hex = format!("#{:02X}{:02X}{:02X}{:02X}", 
            self.temp_settings.click_highlight_color[0],
            self.temp_settings.click_highlight_color[1],
            self.temp_settings.click_highlight_color[2],
            self.temp_settings.click_highlight_color[3]
        );
        
        let mouse_section = container(
            column![
                text("Mouse & Cursor").size(14).color(colors::ACCENT),
                vertical_space().height(12),
                checkbox("Show cursor in capture", self.temp_settings.show_cursor)
                    .on_toggle(Message::SetShowCursor)
                    .spacing(8),
                checkbox("Highlight mouse clicks", self.temp_settings.capture_clicks)
                    .on_toggle(Message::SetCaptureClicks)
                    .spacing(8),
            ]
            .spacing(4)
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(colors::BG_SECONDARY)),
            border: iced::Border {
                color: colors::BORDER,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });
        
        // Color picker button for click highlight
        let click_color_button = button(
            row![
                container(text(" "))
                    .width(24)
                    .height(24)
                    .style(move |_| container::Style {
                        background: Some(iced::Background::Color(click_preview_color)),
                        border: iced::Border { 
                            radius: 12.0.into(),  // Circle
                            color: colors::BORDER,
                            width: 1.0,
                        },
                        ..Default::default()
                    }),
                text(click_hex).size(12).color(colors::TEXT_PRIMARY),
            ].spacing(8).align_y(Alignment::Center)
        )
        .padding(Padding::from([6, 12]))
        .style(|_, _| button::Style {
            background: Some(iced::Background::Color(colors::BG_PRIMARY)),
            border: iced::Border { radius: 6.0.into(), color: colors::BORDER, width: 1.0 },
            ..Default::default()
        })
        .on_press(Message::ToggleClickColorPicker);
        
        // Color picker overlay (if open)
        let click_color_picker_element: Element<'_, Message> = if self.show_click_color_picker {
            ColorPicker::new(
                self.show_click_color_picker,
                click_preview_color,
                click_color_button,
                Message::ClickColorPickerCancel,
                Message::ClickColorPickerSubmit,
            ).into()
        } else {
            click_color_button.into()
        };
        
        let highlight_section = container(
            column![
                text("Click Highlight").size(14).color(colors::ACCENT),
                vertical_space().height(12),
                text("Highlight Color:").size(12).color(colors::TEXT_SECONDARY),
                click_color_picker_element,
                vertical_space().height(8),
                row![
                    text("Duration:").size(12).color(colors::TEXT_SECONDARY),
                    horizontal_space(),
                    text(format!("{}ms", dissolve_ms)).size(12).color(colors::TEXT_PRIMARY),
                ],
                slider(100..=1000, dissolve_ms as i32, |v| Message::SetClickDissolveMs(v as u32)),
            ]
            .spacing(4)
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(colors::BG_SECONDARY)),
            border: iced::Border {
                color: colors::BORDER,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });
        
        column![
            mouse_section,
            vertical_space().height(12),
            highlight_section,
        ]
        .into()
    }
    
    /// Window settings tab: Size, Position, Presets
    fn view_settings_window(&self) -> Element<'_, Message> {
        let custom_w = self.temp_settings.custom_width.to_string();
        let custom_h = self.temp_settings.custom_height.to_string();
        let custom_x = self.temp_settings.custom_x.to_string();
        let custom_y = self.temp_settings.custom_y.to_string();
        
        // Helper for pill-style preset buttons
        let size_pill = |preset: SizePreset, current: SizePreset| -> Element<'_, Message> {
            let is_selected = preset == current;
            let label = match preset {
                SizePreset::W720p => "720p",
                SizePreset::W1080p => "1080p",
                SizePreset::W1440p => "1440p",
                SizePreset::W4K => "4K",
                SizePreset::Square => "1:1",
                SizePreset::Custom => "Custom",
            };
            button(text(label).size(11).align_x(Center).width(Length::Fill))
                .width(Length::FillPortion(1))
                .padding(Padding::from([6, 4]))
                .style(move |_, _| iced::widget::button::Style {
                    background: Some(iced::Background::Color(
                        if is_selected { colors::PILL_SELECTED } else { colors::PILL_BG }
                    )),
                    text_color: if is_selected { Color::WHITE } else { colors::TEXT_SECONDARY },
                    border: iced::Border { radius: 12.0.into(), ..Default::default() },
                    ..Default::default()
                })
                .on_press(Message::SetSizePreset(preset))
                .into()
        };
        
        let size_section = container(
            column![
                text("Window Size").size(14).color(colors::ACCENT),
                vertical_space().height(12),
                text("Presets:").size(12).color(colors::TEXT_SECONDARY),
                row![
                    size_pill(SizePreset::W720p, self.temp_settings.size_preset),
                    size_pill(SizePreset::W1080p, self.temp_settings.size_preset),
                    size_pill(SizePreset::W1440p, self.temp_settings.size_preset),
                    size_pill(SizePreset::W4K, self.temp_settings.size_preset),
                    size_pill(SizePreset::Square, self.temp_settings.size_preset),
                ].spacing(4),
                vertical_space().height(12),
                text("Custom Size:").size(12).color(colors::TEXT_SECONDARY),
                row![
                    container(text("Width:").size(11).color(colors::TEXT_MUTED)).width(50),
                    text_input("Width", &custom_w)
                        .on_input(Message::SetCustomWidth)
                        .width(100),
                    text("px").size(11).color(colors::TEXT_MUTED),
                ].spacing(8).align_y(Alignment::Center),
                row![
                    container(text("Height:").size(11).color(colors::TEXT_MUTED)).width(50),
                    text_input("Height", &custom_h)
                        .on_input(Message::SetCustomHeight)
                        .width(100),
                    text("px").size(11).color(colors::TEXT_MUTED),
                ].spacing(8).align_y(Alignment::Center),
            ]
            .spacing(6)
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(colors::BG_SECONDARY)),
            border: iced::Border {
                color: colors::BORDER,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });
        
        // Helper for position pill buttons  
        let pos_pill = |preset: PositionPreset, current: PositionPreset| -> Element<'_, Message> {
            let is_selected = preset == current;
            let label = match preset {
                PositionPreset::Center => "Center",
                PositionPreset::TopLeft => "TL",
                PositionPreset::TopRight => "TR",
                PositionPreset::BottomLeft => "BL",
                PositionPreset::BottomRight => "BR",
                PositionPreset::Custom => "Custom",
            };
            button(text(label).size(11).align_x(Center).width(Length::Fill))
                .width(Length::FillPortion(1))
                .padding(Padding::from([6, 4]))
                .style(move |_, _| iced::widget::button::Style {
                    background: Some(iced::Background::Color(
                        if is_selected { colors::PILL_SELECTED } else { colors::PILL_BG }
                    )),
                    text_color: if is_selected { Color::WHITE } else { colors::TEXT_SECONDARY },
                    border: iced::Border { radius: 12.0.into(), ..Default::default() },
                    ..Default::default()
                })
                .on_press(Message::SetPositionPreset(preset))
                .into()
        };
        
        let position_section = container(
            column![
                text("Window Position").size(14).color(colors::ACCENT),
                vertical_space().height(12),
                text("Presets:").size(12).color(colors::TEXT_SECONDARY),
                row![
                    pos_pill(PositionPreset::TopLeft, self.temp_settings.position_preset),
                    pos_pill(PositionPreset::TopRight, self.temp_settings.position_preset),
                    pos_pill(PositionPreset::Center, self.temp_settings.position_preset),
                    pos_pill(PositionPreset::BottomLeft, self.temp_settings.position_preset),
                    pos_pill(PositionPreset::BottomRight, self.temp_settings.position_preset),
                ].spacing(4),
                vertical_space().height(12),
                text("Custom Position:").size(12).color(colors::TEXT_SECONDARY),
                row![
                    container(text("X:").size(11).color(colors::TEXT_MUTED)).width(50),
                    text_input("X", &custom_x)
                        .on_input(Message::SetCustomX)
                        .width(100),
                    text("px").size(11).color(colors::TEXT_MUTED),
                ].spacing(8).align_y(Alignment::Center),
                row![
                    container(text("Y:").size(11).color(colors::TEXT_MUTED)).width(50),
                    text_input("Y", &custom_y)
                        .on_input(Message::SetCustomY)
                        .width(100),
                    text("px").size(11).color(colors::TEXT_MUTED),
                ].spacing(8).align_y(Alignment::Center),
            ]
            .spacing(6)
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(colors::BG_SECONDARY)),
            border: iced::Border {
                color: colors::BORDER,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });
        
        let behavior_section = container(
            column![
                text("Window Behavior").size(14).color(colors::ACCENT),
                vertical_space().height(12),
                checkbox("Save last window position", self.temp_settings.save_last_position)
                    .on_toggle(Message::SetSaveLastPosition)
                    .spacing(8),
                checkbox("Remember capture region", self.temp_settings.remember_region)
                    .on_toggle(Message::SetRememberRegion)
                    .spacing(8),
            ]
            .spacing(4)
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(colors::BG_SECONDARY)),
            border: iced::Border {
                color: colors::BORDER,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });
        
        column![
            size_section,
            vertical_space().height(12),
            position_section,
            vertical_space().height(12),
            behavior_section,
        ]
        .into()
    }
    
    /// Capture settings tab: Border, Performance, FPS
    fn view_settings_capture(&self) -> Element<'_, Message> {
        let border_preview_color = Color::from_rgb(
            self.temp_settings.border_color[0] as f32 / 255.0,
            self.temp_settings.border_color[1] as f32 / 255.0,
            self.temp_settings.border_color[2] as f32 / 255.0,
        );
        let border_width = self.temp_settings.border_width;
        let hex_color = self.temp_settings.border_color_hex();
        
        // Color picker button that opens the picker
        let color_button = button(
            row![
                container(text(" "))
                    .width(24)
                    .height(24)
                    .style(move |_| container::Style {
                        background: Some(iced::Background::Color(border_preview_color)),
                        border: iced::Border { 
                            radius: 4.0.into(), 
                            color: colors::BORDER,
                            width: 1.0,
                        },
                        ..Default::default()
                    }),
                text(hex_color).size(12).color(colors::TEXT_PRIMARY),
            ].spacing(8).align_y(Alignment::Center)
        )
        .padding(Padding::from([6, 12]))
        .style(|_, _| button::Style {
            background: Some(iced::Background::Color(colors::BG_PRIMARY)),
            border: iced::Border { radius: 6.0.into(), color: colors::BORDER, width: 1.0 },
            ..Default::default()
        })
        .on_press(Message::ToggleBorderColorPicker);
        
        // Color picker overlay (if open)
        let color_picker_element: Element<'_, Message> = if self.show_border_color_picker {
            ColorPicker::new(
                self.show_border_color_picker,
                border_preview_color,
                color_button,
                Message::BorderColorPickerCancel,
                Message::BorderColorPickerSubmit,
            ).into()
        } else {
            color_button.into()
        };
        
        let border_section = container(
            column![
                text("Border Frame").size(14).color(colors::ACCENT),
                vertical_space().height(12),
                row![
                    text("Width:").size(12).color(colors::TEXT_SECONDARY),
                    horizontal_space(),
                    text(format!("{}px", border_width)).size(12).color(colors::TEXT_PRIMARY),
                ],
                slider(1..=20, border_width as i32, |v| Message::SetBorderWidth(v as u32)),
                vertical_space().height(12),
                text("Border Color:").size(12).color(colors::TEXT_SECONDARY),
                color_picker_element,
            ]
            .spacing(4)
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(colors::BG_SECONDARY)),
            border: iced::Border {
                color: colors::BORDER,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });
        
        // Get actual monitor refresh rate
        let monitor_hz = get_monitor_refresh_rate();
        let max_fps = monitor_hz.min(240);
        let target_fps = self.temp_settings.target_fps;
        let current_fps = (target_fps as i32).min(max_fps as i32);
        log::info!("FPS Settings - monitor_hz: {}, max_fps: {}, target_fps: {}", monitor_hz, max_fps, target_fps);
        
        // Build FPS pill helper - inline to avoid closure capture issues
        let make_fps_pill = |fps: u32, label: String, max: u32, current: u32| -> Element<'_, Message> {
            let is_selected = current == fps;
            let is_disabled = fps > max;
            log::info!("  FPS pill {} - is_selected: {}, is_disabled: {} (fps {} > max {})", 
                label, is_selected, is_disabled, fps, max);
            let btn = button(text(label).size(11).align_x(Center).width(Length::Fill))
                .width(Length::FillPortion(1))
                .padding(Padding::from([6, 4]))
                .style(move |_, _| iced::widget::button::Style {
                    background: Some(iced::Background::Color(
                        if is_selected { colors::PILL_SELECTED } 
                        else if is_disabled { colors::PILL_DISABLED }
                        else { colors::PILL_BG }
                    )),
                    text_color: if is_selected { Color::WHITE } 
                        else if is_disabled { colors::TEXT_MUTED }
                        else { colors::TEXT_PRIMARY },
                    border: iced::Border { radius: 12.0.into(), ..Default::default() },
                    ..Default::default()
                });
            
            if is_disabled {
                btn.into()
            } else {
                btn.on_press(Message::SetTargetFps(fps)).into()
            }
        };
        
        let perf_section = container(
            column![
                text("Performance").size(14).color(colors::ACCENT),
                vertical_space().height(12),
                checkbox("Use GPU acceleration", self.temp_settings.use_gpu)
                    .on_toggle(Message::SetUseGpu)
                    .spacing(8),
                vertical_space().height(8),
                row![
                    text("Target FPS:").size(12).color(colors::TEXT_SECONDARY),
                    horizontal_space(),
                    text(format!("{} FPS", target_fps)).size(12).color(colors::TEXT_PRIMARY),
                ],
                slider(15..=max_fps as i32, current_fps, |v| Message::SetTargetFps(v as u32)),
                vertical_space().height(8),
                text(format!("FPS Presets (Monitor: {}Hz):", monitor_hz)).size(11).color(colors::TEXT_MUTED),
                {
                    // Standard FPS presets - only show those <= max_fps, plus one above for reference
                    let standard_rates = [30u32, 60, 120, 144];
                    let show_monitor_hz = !standard_rates.contains(&monitor_hz);
                    
                    let mut row_content = row![
                        make_fps_pill(30, "30".to_string(), max_fps, target_fps),
                        make_fps_pill(60, "60".to_string(), max_fps, target_fps),
                        make_fps_pill(120, "120".to_string(), max_fps, target_fps),
                        make_fps_pill(144, "144".to_string(), max_fps, target_fps),
                    ].spacing(4);
                    
                    // Add monitor Hz button only if it's not already in standard rates
                    if show_monitor_hz {
                        row_content = row_content.push(make_fps_pill(monitor_hz, format!("{}Hz", monitor_hz), max_fps, target_fps));
                    }
                    
                    row_content
                },
            ]
            .spacing(6)
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(colors::BG_SECONDARY)),
            border: iced::Border {
                color: colors::BORDER,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });
        
        column![
            border_section,
            vertical_space().height(12),
            perf_section,
        ]
        .into()
    }
    
    /// Helper to create color preset pill button
    fn color_pill(color: [u8; 3], current: [u8; 3]) -> Element<'static, Message> {
        let is_selected = color == current;
        let bg_color = Color::from_rgb(
            color[0] as f32 / 255.0,
            color[1] as f32 / 255.0,
            color[2] as f32 / 255.0,
        );
        let hex = format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2]);
        button(text(" ").size(12))
            .width(28)
            .height(28)
            .padding(0)
            .style(move |_, _| iced::widget::button::Style {
                background: Some(iced::Background::Color(bg_color)),
                text_color: Color::TRANSPARENT,
                border: iced::Border { 
                    radius: 6.0.into(),
                    color: if is_selected { Color::WHITE } else { Color::TRANSPARENT },
                    width: if is_selected { 2.0 } else { 0.0 },
                },
                ..Default::default()
            })
            .on_press(Message::SetBorderColorHex(hex))
            .into()
    }
    
    /// Helper to create click highlight color preset pill button (RGBA)
    fn click_color_pill(color: [u8; 4], current: [u8; 4]) -> Element<'static, Message> {
        let is_selected = color[0..3] == current[0..3];
        let bg_color = Color::from_rgba(
            color[0] as f32 / 255.0,
            color[1] as f32 / 255.0,
            color[2] as f32 / 255.0,
            color[3] as f32 / 255.0,
        );
        button(text(" ").size(12))
            .width(28)
            .height(28)
            .padding(0)
            .style(move |_, _| iced::widget::button::Style {
                background: Some(iced::Background::Color(bg_color)),
                text_color: Color::TRANSPARENT,
                border: iced::Border { 
                    radius: 6.0.into(),
                    color: if is_selected { Color::WHITE } else { Color::TRANSPARENT },
                    width: if is_selected { 2.0 } else { 0.0 },
                },
                ..Default::default()
            })
            .on_press(Message::SetClickHighlightColor(color))
            .into()
    }
    
    /// Advanced settings tab: Shortcuts, About, etc.
    fn view_settings_advanced(&self) -> Element<'_, Message> {
        let ui_section = container(
            column![
                text("User Interface").size(14).color(colors::ACCENT),
                vertical_space().height(12),
                checkbox("Auto-start capture on launch", self.temp_settings.auto_start)
                    .on_toggle(Message::SetAutoStart)
                    .spacing(8),
                checkbox("Show keyboard shortcuts", self.temp_settings.show_shortcuts)
                    .on_toggle(Message::SetShowShortcuts)
                    .spacing(8),
            ]
            .spacing(4)
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(colors::BG_SECONDARY)),
            border: iced::Border {
                color: colors::BORDER,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });
        
        let shortcuts_section = container(
            column![
                text("Keyboard Shortcuts").size(14).color(colors::ACCENT),
                vertical_space().height(12),
                row![
                    text("ESC").size(11).color(colors::ACCENT).width(80),
                    text("Stop capture / Close dialog").size(11).color(colors::TEXT_SECONDARY),
                ],
                row![
                    text("Enter").size(11).color(colors::ACCENT).width(80),
                    text("Toggle capture").size(11).color(colors::TEXT_SECONDARY),
                ],
                row![
                    text("Drag edges").size(11).color(colors::ACCENT).width(80),
                    text("Resize capture area").size(11).color(colors::TEXT_SECONDARY),
                ],
            ]
            .spacing(6)
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(colors::BG_SECONDARY)),
            border: iced::Border {
                color: colors::BORDER,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });
        
        let about_section = container(
            column![
                text("About").size(14).color(colors::ACCENT),
                vertical_space().height(12),
                text("RustFrame").size(16).color(colors::TEXT_PRIMARY),
                text("Version 1.1.0").size(11).color(colors::TEXT_MUTED),
                vertical_space().height(8),
                text("Modern screen capture application").size(11).color(colors::TEXT_SECONDARY),
                text("built with Rust and Iced.").size(11).color(colors::TEXT_SECONDARY),
                vertical_space().height(8),
                text("by Salih Cantekin").size(11).color(colors::TEXT_MUTED),
            ]
            .spacing(2)
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(colors::BG_SECONDARY)),
            border: iced::Border {
                color: colors::BORDER,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });
        
        let data_section = container(
            column![
                text("Settings Data").size(14).color(colors::ACCENT),
                vertical_space().height(12),
                row![
                    text("Config:").size(11).color(colors::TEXT_MUTED),
                    text(Settings::config_path().to_string_lossy().to_string()).size(10).color(colors::TEXT_SECONDARY),
                ].spacing(8),
                vertical_space().height(12),
                row![
                    button(
                        text("Export Settings").size(11).align_x(Center).width(Length::Fill)
                    )
                    .width(Length::FillPortion(1))
                    .style(|_, _| iced::widget::button::Style {
                        background: Some(iced::Background::Color(colors::BG_TERTIARY)),
                        text_color: colors::TEXT_PRIMARY,
                        border: iced::Border { radius: 4.0.into(), ..Default::default() },
                        ..Default::default()
                    })
                    .on_press(Message::ExportSettings),
                    button(
                        text("Import Settings").size(11).align_x(Center).width(Length::Fill)
                    )
                    .width(Length::FillPortion(1))
                    .style(|_, _| iced::widget::button::Style {
                        background: Some(iced::Background::Color(colors::BG_TERTIARY)),
                        text_color: colors::TEXT_PRIMARY,
                        border: iced::Border { radius: 4.0.into(), ..Default::default() },
                        ..Default::default()
                    })
                    .on_press(Message::ImportSettings),
                ].spacing(8),
                vertical_space().height(8),
                button(
                    text("Open Settings Folder").size(11).align_x(Center).width(Length::Fill)
                )
                .width(Length::Fill)
                .style(|_, _| iced::widget::button::Style {
                    background: Some(iced::Background::Color(colors::ACCENT)),
                    text_color: colors::BG_PRIMARY,
                    border: iced::Border { radius: 4.0.into(), ..Default::default() },
                    ..Default::default()
                })
                .on_press(Message::OpenSettingsFolder),
            ]
            .spacing(4)
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(colors::BG_SECONDARY)),
            border: iced::Border {
                color: colors::BORDER,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        });
        
        column![
            ui_section,
            vertical_space().height(12),
            shortcuts_section,
            vertical_space().height(12),
            data_section,
            vertical_space().height(12),
            about_section,
        ]
        .into()
    }
    
    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![];
        
        if self.mode == AppMode::Capturing {
            let interval = Duration::from_millis(1000 / self.settings.target_fps.max(1) as u64);
            subs.push(iced::time::every(interval).map(|_| Message::Tick));
        }
        
        subs.push(window::resize_events().map(|(id, size)| Message::WindowResized(id, size)));
        subs.push(window::close_events().map(Message::WindowClosed));
        
        // Note: move_events doesn't exist in Iced 0.13.1
        // Window position is tracked via hollow_border during capture
        
        subs.push(iced::keyboard::on_key_press(|key, _modifiers| {
            match key.as_ref() {
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) => {
                    Some(Message::StopCapture)
                }
                iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter) => {
                    Some(Message::ToggleCapture)
                }
                _ => None,
            }
        }));
        
        Subscription::batch(subs)
    }
    
    fn theme(&self, _window_id: window::Id) -> Theme {
        Theme::Dark
    }
}

// ============================================================================
// Main
// ============================================================================

fn main() -> iced::Result {
    // Initialize logger with wgpu warnings filtered out
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .filter_module("wgpu_hal", log::LevelFilter::Error)
        .filter_module("wgpu_core", log::LevelFilter::Error)
        .filter_module("naga", log::LevelFilter::Error)
        .init();
    info!("RustFrame Iced starting...");
    
    std::env::set_var("WGPU_BACKEND", "dx12");
    
    daemon(
        RustFrameApp::title,
        RustFrameApp::update,
        RustFrameApp::view,
    )
    .font(REQUIRED_FONT_BYTES)
    .subscription(RustFrameApp::subscription)
    .theme(RustFrameApp::theme)
    .run_with(RustFrameApp::new)
}
