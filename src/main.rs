// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, State};

// Import capture engine from library
use rustframe_capture::capture::{create_capture_engine, CaptureEngine, CaptureRect};
use rustframe_capture::config;
use rustframe_capture::display_info;
use rustframe_capture::window_filter;

// Import modules
mod destination_window;
mod hollow_border;
mod logging;
mod platform;
mod platform_info;
mod rec_indicator;
mod separation_layer;
mod single_instance;
mod traits; // Cross-platform trait definitions

use destination_window::{DestinationWindow, DestinationWindowConfig};
use hollow_border::HollowBorder;
use platform::window_enumerator::{self, AvailableApp};
use rec_indicator::RecIndicator;
use separation_layer::SeparationLayer;

// Global state for Windows (not thread-safe, but only accessed from commands)
lazy_static! {
    static ref HOLLOW_BORDER: Mutex<Option<HollowBorder>> = Mutex::new(None);
    static ref DESTINATION_WINDOW: Mutex<Option<DestinationWindow>> = Mutex::new(None);
    static ref SEPARATION_LAYER: Mutex<Option<SeparationLayer>> = Mutex::new(None);
    static ref REC_INDICATOR: Mutex<Option<RecIndicator>> = Mutex::new(None);
    // Global flag to track if cleanup has been performed
    static ref CLEANUP_PERFORMED: AtomicBool = AtomicBool::new(false);
    // Single instance lock - prevents multiple instances from running
    static ref SINGLE_INSTANCE_LOCK: Mutex<Option<single_instance::SingleInstanceLock>> = Mutex::new(None);
}

// ============================================================================
// Collision Detection & Auto-Repositioning
// ============================================================================

/// Check if two rectangles intersect
fn rectangles_intersect(r1: (i32, i32, i32, i32), r2: (i32, i32, i32, i32)) -> bool {
    let (x1, y1, w1, h1) = r1;
    let (x2, y2, w2, h2) = r2;
    
    !(x1 + w1 <= x2 || x2 + w2 <= x1 || y1 + h1 <= y2 || y2 + h2 <= y1)
}

/// Find a safe position for preview window that doesn't intersect with border
fn find_safe_preview_position(
    border_rect: (i32, i32, i32, i32),
    preview_size: (i32, i32),
    monitors: &[MonitorInfo],
) -> (i32, i32) {
    let (bx, by, bw, bh) = border_rect;
    let (pw, ph) = preview_size;
    
    // Try positions in order of preference:
    // 1. Top-left of border
    // 2. Top-right of border  
    // 3. Bottom-left of border
    // 4. Bottom-right of border
    // 5. Move to another monitor if available
    
    let candidates = vec![
        (bx - pw - 10, by),                    // Left of border
        (bx + bw + 10, by),                    // Right of border
        (bx, by - ph - 10),                    // Above border
        (bx, by + bh + 10),                    // Below border
    ];
    
    // Find first candidate that doesn't intersect with border and fits on screen
    for (px, py) in candidates {
        let preview_rect = (px, py, pw, ph);
        if !rectangles_intersect(border_rect, preview_rect) {
            // Check if it fits on any monitor
            for monitor in monitors {
                let mx = monitor.x as i32;
                let my = monitor.y as i32;
                let mw = monitor.width as i32;
                let mh = monitor.height as i32;
                
                if px >= mx && py >= my && px + pw <= mx + mw && py + ph <= my + mh {
                    return (px, py);
                }
            }
        }
    }
    
    // If no safe position found on current monitor, try other monitors
    if monitors.len() > 1 {
        for monitor in monitors {
            let mx = monitor.x as i32;
            let my = monitor.y as i32;
            let mw = monitor.width as i32;
            let mh = monitor.height as i32;
            
            // Try top-left corner of other monitors
            let px = mx + 50;
            let py = my + 50;
            let preview_rect = (px, py, pw, ph);
            
            if px >= mx && py >= my && px + pw <= mx + mw && py + ph <= my + mh {
                if !rectangles_intersect(border_rect, preview_rect) {
                    return (px, py);
                }
            }
        }
    }
    
    // Fallback: slightly offset from border
    (bx + bw + 20, by + 20)
}



/// Auto-reposition preview window if it intersects with border
fn auto_reposition_preview_if_needed(
    border_x: i32,
    border_y: i32, 
    border_width: i32,
    border_height: i32,
    monitors: &[MonitorInfo],
) {
    if let Ok(mut dest_lock) = DESTINATION_WINDOW.try_lock() {
        if let Some(ref mut dest_window) = *dest_lock {
            // Get current preview position and size
            if let Some((px, py, pw, ph)) = dest_window.get_rect() {
                let border_rect = (border_x, border_y, border_width, border_height);
                let preview_rect = (px, py, pw, ph);
                
                // Check for intersection
                if rectangles_intersect(border_rect, preview_rect) {
                    tracing::info!(
                        "Border and preview intersect, auto-repositioning preview. Border: {:?}, Preview: {:?}", 
                        border_rect, preview_rect
                    );
                    
                    let (new_x, new_y) = find_safe_preview_position(
                        border_rect,
                        (pw, ph),
                        monitors,
                    );
                    
                    dest_window.set_pos(new_x, new_y);
                    tracing::info!("Preview repositioned to ({}, {})", new_x, new_y);

                    // Keep separation layer aligned with preview when present
                    #[cfg(any(target_os = "windows", target_os = "macos"))]
                    if let Ok(sep_lock) = SEPARATION_LAYER.try_lock() {
                        if let Some(ref sep) = *sep_lock {
                            sep.update_position(new_x, new_y, pw, ph);
                        }
                    }
                }
            }
        }
    }
}

/// Perform cleanup of all capture resources
/// This function is safe to call multiple times - it will only execute once
fn perform_cleanup() {
    // Check if cleanup has already been performed
    if CLEANUP_PERFORMED.swap(true, Ordering::SeqCst) {
        tracing::debug!("Cleanup already performed, skipping");
        return;
    }

    tracing::info!("Performing cleanup of all capture resources");

    // Stop mouse hook first (before destroying windows)
    platform::input::stop_click_capture();
    tracing::debug!("Mouse hook stopped");

    // Clean up hollow border window (use spawn to avoid blocking)
    if let Ok(mut border) = HOLLOW_BORDER.try_lock() {
        if let Some(b) = border.take() {
            std::thread::spawn(move || {
                drop(b); // Drop in background thread
            });
            tracing::debug!("Hollow border cleanup initiated");
        }
    }

    // Clean up destination window (use spawn to avoid blocking)
    if let Ok(mut dest) = DESTINATION_WINDOW.try_lock() {
        if let Some(d) = dest.take() {
            std::thread::spawn(move || {
                drop(d); // Drop in background thread
            });
            tracing::debug!("Destination window cleanup initiated");
        }
    }

    // Clean up REC indicator (use spawn to avoid blocking)
    if let Ok(mut rec) = REC_INDICATOR.try_lock() {
        if let Some(r) = rec.take() {
            std::thread::spawn(move || {
                drop(r); // Drop in background thread
            });
            tracing::debug!("REC indicator cleanup initiated");
        }
    }



    // Clear click capture data
    platform::input::clear_clicks();
    tracing::debug!("Click capture data cleared");

    // Release single instance lock
    if let Ok(mut lock) = SINGLE_INSTANCE_LOCK.try_lock() {
        if lock.is_some() {
            *lock = None;
            tracing::debug!("Single instance lock released");
        }
    }

    tracing::info!("Cleanup completed successfully");
}

/// Reset cleanup flag (for testing or restart scenarios)
#[allow(dead_code)]
fn reset_cleanup_flag() {
    CLEANUP_PERFORMED.store(false, Ordering::SeqCst);
}

// ============================================================================
// Click Highlight Rendering
// ============================================================================

/// Draw a click highlight circle with alpha blending
/// Frame data is BGRA format
fn draw_click_highlight(
    data: &mut Vec<u8>,
    width: i32,
    height: i32,
    center_x: i32,
    center_y: i32,
    color: [u8; 4],    // RGBA format
    alpha_factor: f32, // 0.0 to 1.0 for fade effect
    radius: i32,       // Outer radius (scaled for display)
) {
    let inner_radius = (radius as f32 * 0.4).max(4.0) as i32; // Inner circle = 40% of radius, min 4px

    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let px = center_x + dx;
            let py = center_y + dy;

            // Bounds check
            if px < 0 || px >= width || py < 0 || py >= height {
                continue;
            }

            let dist_sq = dx * dx + dy * dy;
            let dist = (dist_sq as f32).sqrt();

            // Skip if outside radius
            if dist > radius as f32 {
                continue;
            }

            // Calculate alpha based on distance (solid inner, fading outer ring)
            let ring_alpha = if dist <= inner_radius as f32 {
                1.0 // Solid inner circle
            } else {
                // Fade from inner to outer edge
                1.0 - (dist - inner_radius as f32) / (radius - inner_radius) as f32
            };

            let final_alpha = config::colors::normalize_alpha(color[3]) * alpha_factor * ring_alpha;

            if final_alpha <= 0.0 {
                continue;
            }

            // Pixel index in BGRA format
            let idx = ((py * width + px) * 4) as usize;
            if idx + 3 >= data.len() {
                continue;
            }

            // Alpha blend (frame buffer is RGBA on macOS, BGRA on Windows)
            let inv_alpha = 1.0 - final_alpha;
            #[cfg(target_os = "macos")]
            {
                data[idx] = (color[0] as f32 * final_alpha + data[idx] as f32 * inv_alpha) as u8; // R
                data[idx + 1] =
                    (color[1] as f32 * final_alpha + data[idx + 1] as f32 * inv_alpha) as u8; // G
                data[idx + 2] =
                    (color[2] as f32 * final_alpha + data[idx + 2] as f32 * inv_alpha) as u8;
                // B
            }
            #[cfg(not(target_os = "macos"))]
            {
                data[idx] = (color[2] as f32 * final_alpha + data[idx] as f32 * inv_alpha) as u8; // B
                data[idx + 1] =
                    (color[1] as f32 * final_alpha + data[idx + 1] as f32 * inv_alpha) as u8; // G
                data[idx + 2] =
                    (color[0] as f32 * final_alpha + data[idx + 2] as f32 * inv_alpha) as u8;
                // R
            }
            // Keep original alpha at data[idx + 3]
        }
    }
}

// ============================================================================
// State Management
// ============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum PreviewMode {
    TauriCanvas, // Cross-platform, WebView overhead (not implemented on macOS/Linux)
    #[cfg(windows)]
    WinApiGdi, // Windows-only, lightweight native
    #[cfg(not(windows))]
    Native, // macOS/Linux native preview window
}

impl Default for PreviewMode {
    fn default() -> Self {
        #[cfg(windows)]
        {
            PreviewMode::WinApiGdi
        }
        #[cfg(not(windows))]
        {
            PreviewMode::Native
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum CaptureMethod {
    /// Windows.Graphics.Capture (Windows only, modern, GPU-backed)
    #[cfg(windows)]
    Wgc,
    /// GDI screen copy (Windows only, broad compatibility)
    #[cfg(windows)]
    GdiCopy,
    /// macOS/Linux CoreGraphics-based capture
    #[cfg(not(windows))]
    CoreGraphics,
}

impl Default for CaptureMethod {
    fn default() -> Self {
        #[cfg(windows)]
        {
            CaptureMethod::Wgc
        }
        #[cfg(not(windows))]
        {
            CaptureMethod::CoreGraphics
        }
    }
}

impl std::fmt::Display for CaptureMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(windows)]
            CaptureMethod::Wgc => write!(f, "WGC"),
            #[cfg(windows)]
            CaptureMethod::GdiCopy => write!(f, "GdiCopy"),
            #[cfg(not(windows))]
            CaptureMethod::CoreGraphics => write!(f, "CoreGraphics"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    // Mouse & Cursor
    pub show_cursor: bool,
    #[serde(default = "default_capture_clicks")]
    pub capture_clicks: bool,
    #[serde(default = "default_click_color")]
    pub click_highlight_color: [u8; 4],
    #[serde(default = "default_click_dissolve_ms")]
    pub click_dissolve_ms: u32,
    #[serde(default = "default_click_radius")]
    pub click_highlight_radius: u32,

    // Border
    pub show_border: bool,
    pub border_color: [u8; 4],
    pub border_width: u32,

    // Performance
    pub target_fps: u32,
    #[serde(default = "default_gpu_acceleration")]
    pub gpu_acceleration: bool,

    // Capture Method
    #[serde(default)]
    pub capture_method: CaptureMethod,

    // Preview Mode
    pub preview_mode: PreviewMode,

    // Advanced (hidden) WinAPI Destination Window overrides (Windows-only behavior)
    // These are intentionally not exposed in the UI by default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winapi_destination_alpha: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winapi_destination_topmost: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winapi_destination_click_through: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winapi_destination_toolwindow: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winapi_destination_layered: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winapi_destination_appwindow: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winapi_destination_noactivate: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winapi_destination_overlapped: Option<bool>,

    // Optional post-start behavior (Windows-only UI behavior)
    // If set, after starting capture we will try to hide the preview window from the taskbar/Alt-Tab.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winapi_destination_hide_taskbar_after_ms: Option<u32>,

    // Region Memory
    pub remember_last_region: bool,
    pub last_region: Option<[i32; 4]>, // [x, y, width, height]

    // REC Indicator
    pub show_rec_indicator: bool,
    pub rec_indicator_size: String, // "small", "medium", "large"

    // Window Filtering (Exclusion/Inclusion)
    #[serde(default)]
    pub window_filter: rustframe_capture::window_filter::WindowFilterSettings,

    // Logging
    #[serde(default = "default_log_level")]
    pub log_level: String, // "Off", "Error", "Warn", "Info", "Debug", "Trace"
    #[serde(default = "default_log_to_file")]
    pub log_to_file: bool,
    #[serde(default = "default_log_retention_days")]
    pub log_retention_days: u32,
}

// Default functions for serde
fn default_capture_clicks() -> bool {
    true // Enable click capture by default
}

fn default_click_color() -> [u8; 4] {
    [255, 255, 0, 180] // Yellow with alpha
}

fn default_click_radius() -> u32 {
    20 // Default radius in points (will be scaled for Retina)
}

fn default_click_dissolve_ms() -> u32 {
    300
}

fn default_log_level() -> String {
    "Error".to_string() // Default: only errors
}

fn default_log_to_file() -> bool {
    true // Enable file logging by default
}

fn default_log_retention_days() -> u32 {
    config::capture::LOG_RETENTION_DAYS as u32
}

fn default_gpu_acceleration() -> bool {
    true // GPU acceleration enabled with retained IOSurface
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_cursor: false, // Disabled by default to avoid double cursor in screen sharing
            capture_clicks: true, // Default to enabled for testing
            click_highlight_color: config::capture::DEFAULT_CLICK_HIGHLIGHT_COLOR,
            click_dissolve_ms: 300, // Reduced from 5000ms - 300ms is plenty for click feedback
            click_highlight_radius: 20,
            show_border: true,
            border_color: [255, 0, 0, 255],
            border_width: config::window::DEFAULT_BORDER_WIDTH as u32,
            target_fps: config::capture::DEFAULT_TARGET_FPS,
            gpu_acceleration: true,
            capture_method: CaptureMethod::default(),
            preview_mode: PreviewMode::default(),
            winapi_destination_alpha: None,
            winapi_destination_topmost: None,
            winapi_destination_click_through: None,
            winapi_destination_toolwindow: None,
            winapi_destination_layered: None,
            winapi_destination_appwindow: None,
            winapi_destination_noactivate: None,
            winapi_destination_overlapped: None,
            winapi_destination_hide_taskbar_after_ms: None,
            remember_last_region: true,
            last_region: Some([100, 100, 600, 400]),
            show_rec_indicator: true,
            rec_indicator_size: config::rec_indicator::DEFAULT_SIZE.to_string(),
            window_filter: rustframe_capture::window_filter::WindowFilterSettings::default(),
            log_level: "Error".to_string(),
            log_to_file: true,
            log_retention_days: config::capture::LOG_RETENTION_DAYS as u32,
        }
    }
}

fn create_capture_engine_for_settings(
    settings: &Settings,
) -> Result<Box<dyn CaptureEngine>, String> {
    #[cfg(target_os = "windows")]
    {
        use rustframe_capture::capture::windows::{
            WindowsCaptureEngine, WindowsGdiCopyCaptureEngine,
        };
        return match settings.capture_method {
            CaptureMethod::Wgc => WindowsCaptureEngine::new()
                .map(|e| Box::new(e) as Box<dyn CaptureEngine>)
                .map_err(|e| e.to_string()),
            CaptureMethod::GdiCopy => WindowsGdiCopyCaptureEngine::new()
                .map(|e| Box::new(e) as Box<dyn CaptureEngine>)
                .map_err(|e| e.to_string()),
            _ => Err("Invalid capture method for Windows".to_string()),
        };
    }

    #[cfg(target_os = "macos")]
    {
        use rustframe_capture::capture::macos::MacOSCaptureEngine;
        let _ = settings; // CoreGraphics is the only method on macOS
        MacOSCaptureEngine::new()
            .map(|e| Box::new(e) as Box<dyn CaptureEngine>)
            .map_err(|e| e.to_string())
    }

    #[cfg(target_os = "linux")]
    {
        use rustframe_capture::capture::linux::LinuxCaptureEngine;
        let _ = settings; // Only stub method available on Linux for now
        LinuxCaptureEngine::new()
            .map(|e| Box::new(e) as Box<dyn CaptureEngine>)
            .map_err(|e| e.to_string())
    }
}

#[derive(Clone)]
struct AppState {
    capture_engine: Arc<Mutex<Option<Box<dyn CaptureEngine>>>>,
    settings: Arc<Mutex<Settings>>,
    active_profile: Arc<Mutex<Option<String>>>,
    is_capturing: Arc<Mutex<bool>>,
    settings_modal_open: Arc<Mutex<bool>>,
    render_thread_stop: Arc<Mutex<bool>>,
    render_thread_handle: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
    monitors: Arc<Mutex<Vec<MonitorInfo>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CaptureProfileInfo {
    /// Profile id (derived from filename), e.g. "discord" for profile_discord.json
    id: String,
    /// Filename, e.g. "profile_discord.json"
    file_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CaptureProfileHints {
    /// If present in the selected profile, the preview window will be hidden from taskbar/Alt-Tab after start.
    #[serde(skip_serializing_if = "Option::is_none")]
    hide_taskbar_after_ms: Option<u32>,
}

fn rustframe_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("RustFrame"))
}

fn rustframe_profiles_dir() -> Option<PathBuf> {
    rustframe_config_dir().map(|d| d.join("Profiles"))
}

fn get_os_profile_subdir() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    }
}

fn scan_capture_profiles(dir: &Path) -> Vec<CaptureProfileInfo> {
    let mut profiles = Vec::new();

    // First, check OS-specific subdirectory
    let os_dir = dir.join(get_os_profile_subdir());
    if os_dir.exists() {
        scan_profiles_from_dir(&os_dir, &mut profiles);
    }

    // Also scan root directory for backward compatibility
    scan_profiles_from_dir(dir, &mut profiles);

    profiles.sort_by(|a, b| a.id.cmp(&b.id));
    profiles.dedup_by(|a, b| a.id == b.id);
    profiles
}

fn scan_profiles_from_dir(dir: &Path, profiles: &mut Vec<CaptureProfileInfo>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip directories
        if path.is_dir() {
            continue;
        }

        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        // Support both "profile_xyz.json" (old) and "xyz.json" (new) formats
        let id = if file_name.starts_with("profile_") {
            file_name
                .trim_start_matches("profile_")
                .trim_end_matches(".json")
                .to_string()
        } else {
            file_name.trim_end_matches(".json").to_string()
        };

        if id.is_empty() {
            continue;
        }

        // Only include valid JSON object profiles
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        if !value.is_object() {
            continue;
        }

        profiles.push(CaptureProfileInfo { id, file_name });
    }
}

fn read_profile_overrides(dir: &Path, profile_id: &str) -> Option<serde_json::Value> {
    // Try new format first: Profiles/os/profilename.json
    let os_subdir = dir.join(get_os_profile_subdir());
    let new_format_path = os_subdir.join(format!("{}.json", profile_id));

    if new_format_path.exists() {
        if let Ok(raw) = std::fs::read_to_string(&new_format_path) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
                if value.is_object() {
                    return Some(value);
                }
            }
        }
    }

    // Try old format: profile_profilename.json in root
    let old_format_name = format!("profile_{}.json", profile_id);
    let old_format_path = dir.join(&old_format_name);

    if old_format_path.exists() {
        if let Ok(raw) = std::fs::read_to_string(&old_format_path) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
                if value.is_object() {
                    return Some(value);
                }
            }
        }
    }

    // Try simple format in root: profilename.json
    let simple_format_path = dir.join(format!("{}.json", profile_id));
    if simple_format_path.exists() {
        if let Ok(raw) = std::fs::read_to_string(&simple_format_path) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
                if value.is_object() {
                    return Some(value);
                }
            }
        }
    }

    None
}

fn merge_json(base: &mut serde_json::Value, overlay: serde_json::Value) {
    match (base, overlay) {
        (serde_json::Value::Object(base_obj), serde_json::Value::Object(overlay_obj)) => {
            for (k, v) in overlay_obj {
                match base_obj.get_mut(&k) {
                    Some(existing) => merge_json(existing, v),
                    None => {
                        base_obj.insert(k, v);
                    }
                }
            }
        }
        (base_slot, overlay_value) => {
            *base_slot = overlay_value;
        }
    }
}

fn decode_argb_u32_to_rgba(color: u32) -> [u8; 4] {
    let a = ((color >> 24) & 0xFF) as u8;
    let r = ((color >> 16) & 0xFF) as u8;
    let g = ((color >> 8) & 0xFF) as u8;
    let b = (color & 0xFF) as u8;
    [r, g, b, a]
}

fn sanitize_settings_json_for_platform(value: &mut serde_json::Value) {
    let serde_json::Value::Object(obj) = value else {
        return;
    };

    // Migrate legacy/bundled formats into the current Settings schema.
    // - border_color: u32 (ARGB) -> [r,g,b,a]
    // - capture_method: "auto" or an unsupported variant -> remove (let defaults apply)
    // - preview_mode: variant not available on this platform -> remove (let defaults apply)

    if let Some(border_color) = obj.get("border_color").and_then(|v| v.as_u64()) {
        if border_color <= u32::MAX as u64 {
            let rgba = decode_argb_u32_to_rgba(border_color as u32);
            obj.insert("border_color".to_string(), serde_json::json!(rgba));
        }
    }

    if let Some(cm) = obj.get("capture_method").and_then(|v| v.as_str()) {
        let invalid_for_platform = cm == "auto" || {
            #[cfg(target_os = "windows")]
            {
                cm == "CoreGraphics"
            }
            #[cfg(not(target_os = "windows"))]
            {
                cm == "Wgc" || cm == "GdiCopy"
            }
        };

        if invalid_for_platform {
            obj.remove("capture_method");
        }
    }

    if let Some(pm) = obj.get("preview_mode").and_then(|v| v.as_str()) {
        let invalid_for_platform = {
            #[cfg(target_os = "windows")]
            {
                false
            }
            #[cfg(not(target_os = "windows"))]
            {
                pm == "WinApiGdi"
            }
        };

        if invalid_for_platform {
            obj.remove("preview_mode");
        }
    }

    // Normalize window_filter section
    if let Some(window_filter) = obj.get_mut("window_filter").and_then(|v| v.as_object_mut()) {
        // Force auto_exclude_preview to true (checkbox removed in UI)
        window_filter.insert(
            "auto_exclude_preview".to_string(),
            serde_json::Value::Bool(true),
        );

        // Normalize mode to snake_case expected by serde
        if let Some(mode_val) = window_filter.get("mode") {
            if let Some(mode_str) = mode_val.as_str() {
                let normalized_owned = mode_str.to_lowercase();
                let normalized = match normalized_owned.as_str() {
                    "none" => "none",
                    "exclude" | "exclude_list" => "exclude_list",
                    "include" | "include_only" => "include_only",
                    other => other,
                };
                window_filter.insert(
                    "mode".to_string(),
                    serde_json::Value::String(normalized.to_string()),
                );
            }
        }

        // Ensure included_windows exists for include-only flow
        if !window_filter.contains_key("included_windows") {
            window_filter.insert(
                "included_windows".to_string(),
                serde_json::Value::Array(vec![]),
            );
        }
    }
}

fn bundled_platform_default_settings_json() -> &'static str {
    include_str!(concat!(env!("OUT_DIR"), "/rustframe_default_settings.json"))
}

fn load_bundled_default_overrides() -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(bundled_platform_default_settings_json())
        .unwrap_or_else(|_| serde_json::json!({}))
}

mod bundled_profiles {
    include!(concat!(env!("OUT_DIR"), "/rustframe_bundled_profiles.rs"));
}

fn bundled_profiles_for_platform() -> &'static [(&'static str, &'static str)] {
    bundled_profiles::PROFILES
}

fn bootstrap_profiles_if_missing(config_dir: &Path) {
    let profiles_dir = config_dir.join("Profiles").join(get_os_profile_subdir());
    let _ = std::fs::create_dir_all(&profiles_dir);

    for (file_name, contents) in bundled_profiles_for_platform() {
        let dst = profiles_dir.join(file_name);
        if dst.exists() {
            continue;
        }
        if let Err(e) = std::fs::write(&dst, contents) {
            log::warn!("Failed to seed profile {}: {}", dst.display(), e);
        }
    }
}

fn bootstrap_settings_if_missing(config_dir: &Path) {
    let settings_path = config_dir.join("settings.json");
    if settings_path.exists() {
        return;
    }

    // Create a fully-populated, platform-aware settings.json.
    let mut merged =
        serde_json::to_value(Settings::default()).unwrap_or_else(|_| serde_json::json!({}));
    merge_json(&mut merged, load_bundled_default_overrides());
    let mut merged_obj = merged;
    sanitize_settings_json_for_platform(&mut merged_obj);

    let settings: Settings = serde_json::from_value(merged_obj).unwrap_or_default();
    if let Err(e) = persist_settings_to_disk(&settings) {
        log::warn!("Failed to bootstrap settings.json: {}", e);
    }
}

fn persist_settings_to_disk(settings: &Settings) -> Result<(), String> {
    let Some(rustframe_dir) = rustframe_config_dir() else {
        return Err("Could not find config directory".to_string());
    };
    let _ = std::fs::create_dir_all(&rustframe_dir);
    let settings_path = rustframe_dir.join("settings.json");

    let mut existing_value: serde_json::Value = match std::fs::read_to_string(&settings_path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|_| serde_json::json!({})),
        Err(_) => serde_json::json!({}),
    };

    let new_value = serde_json::to_value(settings).map_err(|e| e.to_string())?;

    if let (serde_json::Value::Object(ref mut existing_obj), serde_json::Value::Object(new_obj)) =
        (&mut existing_value, new_value)
    {
        for (k, v) in new_obj {
            existing_obj.insert(k, v);
        }
    } else {
        existing_value = serde_json::to_value(settings).unwrap_or_else(|_| serde_json::json!({}));
    }

    let pretty = serde_json::to_string_pretty(&existing_value).map_err(|e| e.to_string())?;
    std::fs::write(settings_path, pretty).map_err(|e| e.to_string())?;
    Ok(())
}

fn load_settings_and_profile_from_disk(dir: &Path) -> (Settings, Option<String>) {
    let _ = std::fs::create_dir_all(dir);

    // First-run bootstrap: seed defaults and profiles only if missing.
    bootstrap_settings_if_missing(dir);
    bootstrap_profiles_if_missing(dir);

    let settings_path = dir.join("settings.json");

    let raw = std::fs::read_to_string(&settings_path).unwrap_or_else(|_| "{}".to_string());
    let mut value: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::json!({}));

    let active_profile = value
        .get("active_profile")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    sanitize_settings_json_for_platform(&mut value);

    // Merge onto current defaults so missing keys don't break deserialization.
    let mut merged =
        serde_json::to_value(Settings::default()).unwrap_or_else(|_| serde_json::json!({}));
    merge_json(&mut merged, value);

    let settings: Settings = serde_json::from_value(merged).unwrap_or_default();

    // Ensure there is always a normalized, fully-populated settings.json on disk.
    // This prevents cases where stop_capture only writes last_region into an otherwise incomplete file.
    if let Err(e) = persist_settings_to_disk(&settings) {
        log::warn!("Failed to persist normalized settings: {}", e);
    }

    (settings, active_profile)
}

fn apply_profile_overrides(
    base: &Settings,
    overrides: serde_json::Value,
) -> Result<Settings, String> {
    let mut merged = serde_json::to_value(base).map_err(|e| e.to_string())?;
    merge_json(&mut merged, overrides);
    serde_json::from_value::<Settings>(merged)
        .map_err(|e| format!("Invalid profile overrides: {}", e))
}

fn read_active_profile_from_settings_json(dir: &Path) -> Option<String> {
    let settings_path = dir.join("settings.json");
    let raw = std::fs::read_to_string(settings_path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    value
        .get("active_profile")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn write_active_profile_to_settings_json(
    dir: &Path,
    profile: Option<String>,
) -> Result<(), String> {
    let _ = std::fs::create_dir_all(dir);
    let settings_path = dir.join("settings.json");
    let mut value: serde_json::Value = match std::fs::read_to_string(&settings_path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|_| serde_json::json!({})),
        Err(_) => serde_json::json!({}),
    };

    if !value.is_object() {
        value = serde_json::json!({});
    }

    if let serde_json::Value::Object(ref mut obj) = value {
        match profile {
            Some(p) => {
                obj.insert("active_profile".to_string(), serde_json::Value::String(p));
            }
            None => {
                obj.remove("active_profile");
            }
        }
    }

    let pretty = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    std::fs::write(settings_path, pretty).map_err(|e| e.to_string())?;
    Ok(())
}

// ============================================================================
// Tauri Commands
// ============================================================================

#[tauri::command]
fn is_dev_mode() -> bool {
    cfg!(debug_assertions)
}

#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    let settings = state.settings.lock().unwrap();
    Ok(settings.clone())
}

#[tauri::command]
fn get_border_rect() -> Option<[i32; 4]> {
    let guard = match HOLLOW_BORDER.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard.as_ref().map(|b| {
        let r = b.get_inner_rect();
        [r.0, r.1, r.2, r.3]
    })
}

#[tauri::command]
async fn get_capture_profiles() -> Result<Vec<CaptureProfileInfo>, String> {
    let Some(profiles_dir) = rustframe_profiles_dir() else {
        return Ok(vec![]);
    };

    // Ensure Profiles directory exists
    let _ = std::fs::create_dir_all(&profiles_dir);

    // Scan profiles from the Profiles directory
    Ok(scan_capture_profiles(&profiles_dir))
}

#[tauri::command]
async fn get_active_capture_profile(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.active_profile.lock().unwrap().clone())
}

#[tauri::command]
async fn get_capture_profile_hints(profile: String) -> Result<CaptureProfileHints, String> {
    let Some(profiles_dir) = rustframe_profiles_dir() else {
        return Ok(CaptureProfileHints {
            hide_taskbar_after_ms: None,
        });
    };

    let Some(overrides) = read_profile_overrides(&profiles_dir, &profile) else {
        return Ok(CaptureProfileHints {
            hide_taskbar_after_ms: None,
        });
    };

    let hide_taskbar_after_ms = overrides
        .get("winapi_destination_hide_taskbar_after_ms")
        .and_then(|v| v.as_u64())
        .and_then(|v| u32::try_from(v).ok());

    Ok(CaptureProfileHints {
        hide_taskbar_after_ms,
    })
}

#[tauri::command]
async fn set_active_capture_profile(
    profile: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    *state.active_profile.lock().unwrap() = profile.clone();
    if let Some(dir) = rustframe_config_dir() {
        write_active_profile_to_settings_json(&dir, profile)?;
    }
    Ok(())
}

// ============================================================================
// Profile Management Commands
// ============================================================================

#[derive(Serialize, Deserialize, Clone)]
pub struct ProfileVersionInfo {
    pub version: String,
    pub last_updated: String,
    pub description: String,
}

#[derive(Serialize, Deserialize)]
pub struct ProfileVersionData {
    pub version: String,
    pub last_updated: String,
    pub profiles: std::collections::HashMap<String, std::collections::HashMap<String, ProfileVersionInfo>>,
}

#[derive(Serialize)]
pub struct ProfileDetails {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub file_name: String,
    pub settings: serde_json::Value,
}

const PROFILE_VERSION_URL: &str = "https://raw.githubusercontent.com/salihcantekin/RustFrame/main/resources/profiles/version.json";

fn get_profile_download_url(platform: &str, filename: &str) -> String {
    format!("https://raw.githubusercontent.com/salihcantekin/RustFrame/main/resources/profiles/{}/{}", platform, filename)
}

#[tauri::command]
async fn check_profile_updates() -> Result<ProfileVersionData, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(PROFILE_VERSION_URL)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch profile versions: {}", e))?;

    let version_data: ProfileVersionData = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse profile versions: {}", e))?;

    Ok(version_data)
}

#[tauri::command]
async fn download_profile(profile_id: String) -> Result<(), String> {
    let platform = get_os_profile_subdir();
    let Some(profiles_dir) = rustframe_profiles_dir() else {
        return Err("Could not find profiles directory".to_string());
    };

    let platform_dir = profiles_dir.join(platform);
    let _ = std::fs::create_dir_all(&platform_dir);

    let filename = format!("{}.json", profile_id);
    let url = get_profile_download_url(platform, &filename);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to download profile: {}", e))?;

    let content = response
        .text()
        .await
        .map_err(|e| format!("Failed to read profile content: {}", e))?;

    // Validate JSON before saving
    let _: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Downloaded profile has invalid JSON: {}", e))?;

    let dest_path = platform_dir.join(&filename);
    std::fs::write(&dest_path, content)
        .map_err(|e| format!("Failed to save profile: {}", e))?;

    tracing::info!("Downloaded profile '{}' to {:?}", profile_id, dest_path);
    Ok(())
}

#[tauri::command]
async fn delete_profile(profile_id: String) -> Result<(), String> {
    let platform = get_os_profile_subdir();
    let Some(profiles_dir) = rustframe_profiles_dir() else {
        return Err("Could not find profiles directory".to_string());
    };

    let platform_dir = profiles_dir.join(platform);
    let filename = format!("{}.json", profile_id);
    let profile_path = platform_dir.join(&filename);

    if !profile_path.exists() {
        return Err(format!("Profile '{}' not found", profile_id));
    }

    std::fs::remove_file(&profile_path)
        .map_err(|e| format!("Failed to delete profile: {}", e))?;

    tracing::info!("Deleted profile '{}' from {:?}", profile_id, profile_path);
    Ok(())
}

#[tauri::command]
async fn get_profile_details(profile_id: String) -> Result<ProfileDetails, String> {
    let platform = get_os_profile_subdir();
    let Some(profiles_dir) = rustframe_profiles_dir() else {
        return Err("Could not find profiles directory".to_string());
    };

    let platform_dir = profiles_dir.join(platform);
    let filename = format!("{}.json", profile_id);
    let profile_path = platform_dir.join(&filename);

    if !profile_path.exists() {
        return Err(format!("Profile '{}' not found", profile_id));
    }

    let content = std::fs::read_to_string(&profile_path)
        .map_err(|e| format!("Failed to read profile: {}", e))?;

    let settings: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse profile: {}", e))?;

    let name = settings
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(&profile_id)
        .to_string();

    let description = settings
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(ProfileDetails {
        id: profile_id.clone(),
        name,
        description,
        version: "1.0.0".to_string(), // TODO: Get from version.json
        file_name: filename,
        settings,
    })
}

#[tauri::command]
async fn get_available_windows() -> Result<Vec<AvailableApp>, String> {
    window_enumerator::enumerate_windows()
        .map_err(|e| format!("Failed to enumerate windows: {}", e))
}

#[tauri::command]
async fn save_settings(settings: Settings, state: State<'_, AppState>) -> Result<(), String> {
    let old_log_level = state.settings.lock().unwrap().log_level.clone();
    let old_log_to_file = state.settings.lock().unwrap().log_to_file;

    let mut app_settings = state.settings.lock().unwrap();
    *app_settings = settings.clone();
    drop(app_settings); // Release lock before potentially blocking operations

    // Save to disk (merge with existing JSON to preserve unknown/manual keys)
    let _ = persist_settings_to_disk(&settings);

    // If logging settings changed, reinitialize logger
    if settings.log_level != old_log_level || settings.log_to_file != old_log_to_file {
        tracing::info!(
            old_level = %old_log_level,
            new_level = %settings.log_level,
            old_file = old_log_to_file,
            new_file = settings.log_to_file,
            "Logging settings changed, reinitializing logger"
        );

        let log_level = settings
            .log_level
            .parse::<logging::LogLevel>()
            .unwrap_or(logging::LogLevel::Error);

        if let Err(e) = logging::init_logging(log_level, settings.log_to_file) {
            tracing::error!(error = %e, "Failed to reinitialize logging");
        } else {
            tracing::info!(
                log_level = %log_level.to_string(),
                log_to_file = settings.log_to_file,
                "Logging reinitialized successfully"
            );
        }

        // If retention days changed, trigger cleanup
        if settings.log_to_file {
            logging::auto_cleanup_old_logs(settings.log_retention_days);
        }
    }

    Ok(())
}

#[tauri::command]
fn get_settings_path() -> Result<String, String> {
    if let Some(config_dir) = dirs::config_dir() {
        let settings_path = config_dir.join("RustFrame").join("settings.json");
        Ok(settings_path.to_string_lossy().to_string())
    } else {
        Err("Could not find config directory".to_string())
    }
}

#[tauri::command]
fn open_settings_folder() -> Result<(), String> {
    if let Some(config_dir) = dirs::config_dir() {
        let rustframe_dir = config_dir.join("RustFrame");
        let _ = std::fs::create_dir_all(&rustframe_dir);

        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("explorer")
                .arg(&rustframe_dir)
                .spawn()
                .map_err(|e| e.to_string())?;
        }

        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg(&rustframe_dir)
                .spawn()
                .map_err(|e| e.to_string())?;
        }

        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("xdg-open")
                .arg(&rustframe_dir)
                .spawn()
                .map_err(|e| e.to_string())?;
        }

        Ok(())
    } else {
        Err("Could not find config directory".to_string())
    }
}

#[tauri::command]
fn open_logs_folder() -> Result<(), String> {
    let logs_dir = logging::get_logs_dir().map_err(|e| e.to_string())?;
    let _ = std::fs::create_dir_all(&logs_dir);

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&logs_dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&logs_dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&logs_dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
fn clear_old_logs(keep_days: u32) -> Result<usize, String> {
    let logs_dir = logging::get_logs_dir().map_err(|e| e.to_string())?;
    logging::cleanup_old_logs(&logs_dir, keep_days).map_err(|e| e.to_string())
}

#[tauri::command]
async fn export_settings(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let settings = state.settings.lock().unwrap();
    let json = serde_json::to_string_pretty(&*settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to write settings: {}", e))?;
    Ok(())
}

#[tauri::command]
async fn import_settings(path: String, state: State<'_, AppState>) -> Result<Settings, String> {
    let json = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read settings file: {}", e))?;
    let imported: Settings =
        serde_json::from_str(&json).map_err(|e| format!("Invalid settings file: {}", e))?;

    // Update app state
    let mut app_settings = state.settings.lock().unwrap();
    *app_settings = imported.clone();

    // Also save to default location (preserve any extra keys in the imported JSON)
    if let Some(config_dir) = dirs::config_dir() {
        let rustframe_dir = config_dir.join("RustFrame");
        let _ = std::fs::create_dir_all(&rustframe_dir);

        let value: serde_json::Value = serde_json::from_str(&json).unwrap_or_else(|_| {
            serde_json::to_value(&imported).unwrap_or_else(|_| serde_json::json!({}))
        });
        if let Ok(pretty) = serde_json::to_string_pretty(&value) {
            let _ = std::fs::write(rustframe_dir.join("settings.json"), pretty);
        }
    }

    Ok(imported)
}

// Preview border for settings - shows border without starting capture
static PREVIEW_BORDER: std::sync::Mutex<Option<HollowBorder>> = std::sync::Mutex::new(None);

#[tauri::command]
fn show_preview_border(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    border_width: i32,
    border_color: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Check if capture is active - if so, switch existing border to preview mode
    // instead of creating a new one (they share global state)
    let is_capturing = *state.is_capturing.lock().unwrap();

    if is_capturing {
        // Capture is active - switch the capture border to preview mode
        // This makes it draggable from interior while maintaining capture
        // Use try_lock to prevent deadlock
        if let Ok(mut border_lock) = HOLLOW_BORDER.try_lock() {
            if let Some(ref mut border) = *border_lock {
                border.set_preview_mode();
                // Update position/size if different from current
                // Update position/size if different from current
                let (cur_x, cur_y, cur_w, cur_h) = border.get_rect();
                if cur_x != x || cur_y != y || cur_w != width || cur_h != height {
                    border.update_rect(x, y, width, height);
                }
                border.update_style(border_width, border_color);
                return Ok(());
            }
        } else {
            tracing::warn!("Could not acquire HOLLOW_BORDER lock in show_preview_border");
            return Err("Border is locked".to_string());
        }
        // Capture is active but no border found - this shouldn't happen
        return Err("Capture is active but no border found".to_string());
    }

    // No capture active - create or update preview border
    let mut preview = PREVIEW_BORDER.lock().map_err(|e| e.to_string())?;

    // If preview border already exists, just update it
    if let Some(border) = preview.as_mut() {
        border.update_rect(x, y, width, height);
        border.update_style(border_width, border_color);
        border.set_preview_mode();
        border.show();
        
        // Auto-reposition preview window if it intersects with border
        auto_reposition_preview_if_needed(x, y, width, height, &state.monitors.lock().unwrap());
        
        return Ok(());
    }

    // Create new preview border
    let mut border = HollowBorder::new(x, y, width, height, border_width, border_color)
        .ok_or("Failed to create preview border")?;

    // Preview mode: interior is draggable, not click-through
    border.set_preview_mode();

    *preview = Some(border);
    
    // Auto-reposition preview window if it intersects with border
    auto_reposition_preview_if_needed(x, y, width, height, &state.monitors.lock().unwrap());
    
    Ok(())
}

#[tauri::command]
fn hide_preview_border(state: State<'_, AppState>) -> Result<(), String> {
    // If capture is active, switch border back to capture mode
    let is_capturing = *state.is_capturing.lock().unwrap();

    if is_capturing {
        // Use try_lock with timeout to prevent deadlock
        if let Ok(mut border_lock) = HOLLOW_BORDER.try_lock() {
            if let Some(ref mut border) = *border_lock {
                border.set_capture_mode();
                return Ok(());
            }
        } else {
            tracing::warn!("Could not acquire HOLLOW_BORDER lock in hide_preview_border");
            return Err("Border is locked".to_string());
        }
    }

    // No capture active - hide the preview border
    let mut preview = PREVIEW_BORDER.lock().map_err(|e| e.to_string())?;

    // Reset preview mode flag so next border created starts fresh
    // Note: This is important because PREVIEW_BORDER and HOLLOW_BORDER share global state
    // which is polled by frontend. Settings dialog saves position when user
    // confirms changes.

    *preview = None;
    Ok(())
}

#[tauri::command]
fn update_preview_border(x: i32, y: i32, width: i32, height: i32) -> Result<(), String> {
    let preview = PREVIEW_BORDER.lock().map_err(|e| e.to_string())?;
    if let Some(border) = preview.as_ref() {
        border.update_rect(x, y, width, height);
    }
    Ok(())
}

#[tauri::command]
fn update_preview_border_style(border_width: i32, border_color: u32) -> Result<(), String> {
    let preview = PREVIEW_BORDER.lock().map_err(|e| e.to_string())?;
    if let Some(border) = preview.as_ref() {
        border.update_style(border_width, border_color);
    }
    Ok(())
}

#[tauri::command]
fn get_preview_border_rect() -> Result<Option<(i32, i32, i32, i32)>, String> {
    let preview = PREVIEW_BORDER.lock().map_err(|e| e.to_string())?;
    if let Some(border) = preview.as_ref() {
        Ok(Some(border.get_rect()))
    } else {
        Ok(None)
    }
}

/// macOS: Restore window z-order after drag/resize operations
/// Order from top to bottom: Border → User Windows → Separation → Destination
#[cfg(target_os = "macos")]
fn restore_window_z_order_macos() {
    use cocoa::base::id;
    use objc::{msg_send, sel, sel_impl};

    extern "C" {
        static _dispatch_main_q: std::ffi::c_void;
        fn dispatch_sync_f(
            queue: *const std::ffi::c_void,
            context: *mut std::ffi::c_void,
            work: extern "C" fn(*mut std::ffi::c_void),
        );
        fn pthread_main_np() -> i32;
    }

    #[repr(C)]
    struct ZOrderContext {
        dest_window: id,
        sep_window: id,
        border_window: id,
    }

    extern "C" fn restore_z_order_on_main(ctx_ptr: *mut std::ffi::c_void) {
        let ctx = unsafe { &*(ctx_ptr as *const ZOrderContext) };
        
        unsafe {
            // CRITICAL Z-Order (from back to front): Preview → Separation → Border
            // Do NOT use orderOut - it causes flashing by removing window from screen!
            // Just use orderBack and orderFront to reorder within the window list
            
            // Step 1: Send preview/destination to absolute back
            let _: () = msg_send![ctx.dest_window, orderBack: cocoa::base::nil];
            
            // Step 2: Place separation above preview (but still below border and user windows)
            // Use orderWindow:relativeTo: to position separation just above preview
            let _: () = msg_send![ctx.sep_window, orderWindow: 1 relativeTo: ctx.dest_window];
            
            // Step 3: Border stays on top for user interaction
            let _: () = msg_send![ctx.border_window, orderFront: cocoa::base::nil];
            
            log::info!("[Z-Order] ✅ Fixed: Border (front) → Separation (middle) → Preview (back)");
        }
    }

    let dest_lock = DESTINATION_WINDOW.lock().unwrap();
    let sep_lock = SEPARATION_LAYER.lock().unwrap();
    let border_lock = HOLLOW_BORDER.lock().unwrap();

    if let (Some(dest), Some(sep), Some(border)) = (dest_lock.as_ref(), sep_lock.as_ref(), border_lock.as_ref()) {
        let mut ctx = ZOrderContext {
            dest_window: dest.get_window(),
            sep_window: sep.get_window(),
            border_window: border.get_window(),
        };

        unsafe {
            let is_main = pthread_main_np() != 0;
            if is_main {
                restore_z_order_on_main(&mut ctx as *mut _ as *mut std::ffi::c_void);
            } else {
                dispatch_sync_f(
                    &_dispatch_main_q,
                    &mut ctx as *mut _ as *mut std::ffi::c_void,
                    restore_z_order_on_main,
                );
            }
        }
    }
}

#[tauri::command]
async fn start_capture(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    tracing::info!(
        x = x,
        y = y,
        width = width,
        height = height,
        "Starting capture"
    );
    log::info!(
        "Starting capture at ({}, {}) size {}x{}",
        x,
        y,
        width,
        height
    );

    // CRITICAL: Always close BOTH preview border and capture border first
    // PREVIEW_BORDER and HOLLOW_BORDER share global state (HOLLOW_HWND, HOLLOW_RECT, etc.)
    // and must be completely cleaned up before creating new border
    {
        tracing::info!("Cleaning up any existing borders before starting capture");

        // Close preview border first
        let mut preview = PREVIEW_BORDER.lock().map_err(|e| e.to_string())?;
        if preview.is_some() {
            tracing::info!("Closing preview border");
            *preview = None;
        }
        drop(preview); // Release lock explicitly

        // Close any existing capture border
        let mut hollow = HOLLOW_BORDER.lock().map_err(|e| e.to_string())?;
        if hollow.is_some() {
            tracing::info!("Closing existing capture border");
            *hollow = None;
        }
        drop(hollow); // Release lock explicitly

        // Give time for threads to fully clean up
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    // Ensure HOLLOW_HWND is actually cleared (defensive check)
    #[cfg(target_os = "windows")]
    {
        use crate::hollow_border::is_hollow_hwnd_valid;
        let mut retries = 0;
        while is_hollow_hwnd_valid() && retries < 15 {
            tracing::debug!("Waiting for HOLLOW_HWND to be cleared (retry {})", retries);
            std::thread::sleep(std::time::Duration::from_millis(30));
            retries += 1;
        }
        if is_hollow_hwnd_valid() {
            tracing::error!(
                "HOLLOW_HWND still valid after {} retries - forcing cleanup",
                retries
            );
            return Err("Failed to clean up previous border window".to_string());
        }
        if retries > 0 {
            tracing::info!("HOLLOW_HWND cleared after {} retries", retries);
        }
    }

    // Clean up any previous capture session first (always, not just if capturing)

    // Stop capture engine if running
    if let Some(ref mut engine) = *state.capture_engine.lock().unwrap() {
        engine.stop();
    }

    // Stop render thread if running
    *state.render_thread_stop.lock().unwrap() = true;

    // Wait for render thread to finish to avoid dropping windows while it's still rendering
    if let Some(handle) = state.render_thread_handle.lock().unwrap().take() {
        let _ = handle.join();
    }

    // Clean up windows - this will trigger Drop which must be on main thread
    tracing::debug!("Clearing HOLLOW_BORDER");
    *HOLLOW_BORDER.lock().unwrap() = None;
    tracing::debug!("Clearing DESTINATION_WINDOW");
    *DESTINATION_WINDOW.lock().unwrap() = None;
    tracing::debug!("Clearing REC_INDICATOR");
    *REC_INDICATOR.lock().unwrap() = None;

    // Reset capturing state
    *state.is_capturing.lock().unwrap() = false;

    tracing::debug!("Waiting for cleanup to complete");
    // Give a moment for cleanup
    std::thread::sleep(std::time::Duration::from_millis(100));

    tracing::debug!("Loading settings for capture start");
    log::info!("[MAIN] About to load settings...");

    // Base settings + optional active profile overrides
    tracing::debug!("Acquiring base_settings lock");
    let base_settings = state.settings.lock().unwrap().clone();
    tracing::debug!("Acquiring active_profile lock");
    let active_profile = state.active_profile.lock().unwrap().clone();

    tracing::debug!(active_profile = ?active_profile, "Profile settings loaded");

    let settings = if let (Some(profiles_dir), Some(profile_id)) =
        (rustframe_profiles_dir(), active_profile)
    {
        tracing::info!(profile_id = %profile_id, profiles_dir = ?profiles_dir, "Loading capture profile");
        match read_profile_overrides(&profiles_dir, &profile_id) {
            Some(overrides) => match apply_profile_overrides(&base_settings, overrides) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!(
                        "Failed to apply profile '{}': {} (using base settings)",
                        profile_id,
                        e
                    );
                    base_settings
                }
            },
            None => base_settings,
        }
    } else {
        base_settings
    };

    tracing::debug!(
        show_rec_indicator = settings.show_rec_indicator,
        capture_clicks = settings.capture_clicks,
        preview_mode = ?settings.preview_mode,
        "Capture settings loaded"
    );

    // Create hollow border (always WinAPI)
    // COLORREF format is 0x00BBGGRR, border_color is [R, G, B, A]
    log::info!("[MAIN] Creating hollow border...");

    let border_color = (settings.border_color[0] as u32)
        | ((settings.border_color[1] as u32) << 8)
        | ((settings.border_color[2] as u32) << 16);

    let mut hollow_border = HollowBorder::new(
        x,
        y,
        width as i32,
        height as i32,
        settings.border_width as i32,
        border_color,
    )
    .ok_or("Failed to create hollow border")?;

    // Capture mode: interior is click-through, only top edge drags
    hollow_border.set_capture_mode();

    // Apply show_border setting
    if !settings.show_border {
        hollow_border.hide();
    }

    *HOLLOW_BORDER.lock().unwrap() = Some(hollow_border);
    log::info!("[MAIN] Hollow border created successfully");

    // Create REC indicator (separate window with screen sharing excluded)
    if settings.show_rec_indicator {
        tracing::debug!("Creating REC indicator");
        if let Some(rec) = RecIndicator::new() {
            rec.set_size(&settings.rec_indicator_size);
            tracing::debug!("Showing REC indicator");
            rec.show(x, y, width as i32, settings.border_width as i32);
            tracing::debug!("Storing REC indicator in global state");
            *REC_INDICATOR.lock().unwrap() = Some(rec);
            tracing::info!("REC indicator created and shown");
        } else {
            log::warn!("[MAIN] RecIndicator::new() returned None");
        }
    } else {
        tracing::debug!("REC indicator disabled in settings");
    }

    tracing::info!(preview_mode = ?settings.preview_mode, "Creating destination window");

    #[cfg(target_os = "windows")]
    match settings.preview_mode {
        PreviewMode::WinApiGdi => {
            let config = DestinationWindowConfig {
                alpha: settings.winapi_destination_alpha,
                topmost: settings.winapi_destination_topmost,
                click_through: settings.winapi_destination_click_through,
                toolwindow: settings.winapi_destination_toolwindow,
                layered: settings.winapi_destination_layered,
                appwindow: settings.winapi_destination_appwindow,
                noactivate: settings.winapi_destination_noactivate,
                overlapped: settings.winapi_destination_overlapped,
            };
            let dest_window = DestinationWindow::new(x, y, width, height, config)
                .ok_or("Failed to create destination window")?;

            log::info!("Destination window created successfully");

            // Optional: after a delay, hide the preview window from taskbar/Alt-Tab.
            // This is useful for Discord: keep it "app-like" long enough to select in the picker,
            // then hide it.
            #[cfg(target_os = "windows")]
            if let Some(delay_ms) = settings.winapi_destination_hide_taskbar_after_ms {
                let hwnd_value = dest_window.hwnd_value();
                if hwnd_value != 0 {
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(delay_ms as u64));
                        unsafe {
                            use windows::Win32::Foundation::HWND;
                            use windows::Win32::UI::WindowsAndMessaging::{
                                GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE,
                                SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
                                WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
                            };

                            let hwnd = HWND(hwnd_value as *mut std::ffi::c_void);
                            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                            if ex != 0 {
                                // Hide from taskbar/Alt-Tab by marking as TOOLWINDOW.
                                // Also remove APPWINDOW if present.
                                let new_ex = (ex | (WS_EX_TOOLWINDOW.0 as isize))
                                    & !(WS_EX_APPWINDOW.0 as isize);
                                let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_ex);
                                let _ = SetWindowPos(
                                    hwnd,
                                    None,
                                    0,
                                    0,
                                    0,
                                    0,
                                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
                                );
                            }
                        }
                    });
                }
            }

            *DESTINATION_WINDOW.lock().unwrap() = Some(dest_window);
        }
        PreviewMode::TauriCanvas => {
            // TODO: Implement Tauri Canvas window
            log::warn!("Tauri Canvas mode not yet implemented");
            return Err("Tauri Canvas mode not yet implemented".to_string());
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        tracing::debug!(
            os = if cfg!(target_os = "macos") {
                "macOS"
            } else {
                "Linux"
            },
            "Creating native destination window"
        );
        // On macOS/Linux, always use native destination window (ignore preview_mode setting)
        // TauriCanvas is not implemented yet

        // macOS configuration optimized for screen sharing apps (Meet, Zoom, Discord)
        let config = DestinationWindowConfig {
            alpha: Some(255),
            // Don't use floating level - it hides from screen sharing pickers
            topmost: Some(false),
            click_through: Some(true),

            #[cfg(target_os = "macos")]
            macos_floating_level: Some(false), // Use normal level for visibility
            #[cfg(target_os = "macos")]
            macos_sharing_type: Some(1), // NSWindowSharingReadOnly
            #[cfg(target_os = "macos")]
            macos_collection_behavior: None, // Use defaults (managed, joinable, etc.)
            #[cfg(target_os = "macos")]
            macos_participates_in_cycle: Some(true), // Visible in window pickers

            // Windows fields (ignored on macOS)
            toolwindow: None,
            layered: None,
            appwindow: None,
            noactivate: None,
            overlapped: None,
        };

        // macOS: create preview window with same size as border (NOT inner size)
        // CRITICAL: All 3 windows (Border, Separation, Preview) must have SAME dimensions
        // Windows: already uses full border size
        #[cfg(target_os = "macos")]
        let (preview_x, preview_y, preview_w, preview_h) = (x, y, width, height);
        
        #[cfg(target_os = "windows")]
        let (preview_x, preview_y, preview_w, preview_h) = (x, y, width, height);

        tracing::debug!(preview_x, preview_y, preview_w, preview_h, "Creating DestinationWindow - SAME size as border");
        let dest_window = DestinationWindow::new(preview_x, preview_y, preview_w, preview_h, config)
            .ok_or("Failed to create destination window")?;

        tracing::info!("Destination window created successfully");
        *DESTINATION_WINDOW.lock().unwrap() = Some(dest_window);

        // Verify window is stored
        {
            let lock = DESTINATION_WINDOW.lock().unwrap();
            if lock.is_some() {
                tracing::debug!("Destination window stored successfully");
            } else {
                tracing::error!("Failed to store destination window - is None after assignment");
            }
        }
    }
    
    // Create separation layer (RegionToShare approach)
    // Positioned between border and preview in z-order
    #[cfg(target_os = "windows")]
    {
        let separation_color = 0x4682B4; // Steel Blue like RegionToShare
        let (sep_x, sep_y, sep_width, sep_height) = (x, y, width as i32, height as i32);
        
        if let Some(separation) = SeparationLayer::new(sep_x, sep_y, sep_width, sep_height, separation_color) {
            *SEPARATION_LAYER.lock().unwrap() = Some(separation);
            log::info!("✅ Separation layer created (RegionToShare style)");
        } else {
            log::warn!("⚠️ Failed to create separation layer");
        }
    }

    // macOS: Re-enabled after fixing y-coordinate and window level
    #[cfg(target_os = "macos")]
    {
        let separation_color = 0x0000FF; // Blue
        let (sep_x, sep_y, sep_width, sep_height) = (x, y, width as i32, height as i32);
        
        if let Some(separation) = SeparationLayer::new(sep_x, sep_y, sep_width, sep_height, separation_color) {
            *SEPARATION_LAYER.lock().unwrap() = Some(separation);
            log::info!("✅ Separation layer created (macOS)");
        } else {
            log::warn!("⚠️ Failed to create separation layer (macOS)");
        }

        // CRITICAL: Establish and enforce z-order after all windows created
        // Order from front to back: Border → Separation → Preview
        restore_window_z_order_macos();
        log::info!("✅ Z-order enforced (macOS): Border (front) → Separation (middle) → Preview (back)");
        log::info!("✅ All 3 windows created with SAME dimensions: ({}, {}) {}x{}", x, y, width, height);
        
        // DEBUG: Verify all windows have same position
        if let Some(border) = HOLLOW_BORDER.lock().unwrap().as_ref() {
            let (bx, by, bw, bh) = border.get_rect();
            log::info!("  → Border actual rect: ({}, {}) {}x{}", bx, by, bw, bh);
        }
        if let Some(dest) = DESTINATION_WINDOW.lock().unwrap().as_ref() {
            if let Some((dx, dy, dw, dh)) = dest.get_rect() {
                log::info!("  → Destination actual rect: ({}, {}) {}x{}", dx, dy, dw, dh);
            }
        }
    }

    // Start capture engine
    tracing::debug!("Starting capture engine");
    let mut engine_lock = state.capture_engine.lock().unwrap();

    // (Re)create capture engine so users can change capture_method from Settings
    if let Some(ref mut existing) = *engine_lock {
        tracing::debug!("Stopping existing capture engine");
        existing.stop();
    }

    tracing::info!(
        capture_method = ?settings.capture_method,
        "Creating new capture engine"
    );
    *engine_lock = Some(create_capture_engine_for_settings(&settings)?);

    if let Some(ref mut engine) = *engine_lock {
        // Offset capture region inward by border_width to exclude border from capture
        let border_offset = settings.border_width as i32;
        let region = CaptureRect {
            x: x + border_offset,
            y: y + border_offset,
            width: (width as i32 - border_offset * 2).max(1) as u32,
            height: (height as i32 - border_offset * 2).max(1) as u32,
        };
        
        // Get preview window ID for smart exclusion
        // macOS: use real window id; others: use logical marker (still skipped by should_capture)
        #[cfg(target_os = "macos")]
        let preview_window_id = {
            use window_filter::WindowIdentifier;
            DESTINATION_WINDOW.lock().unwrap().as_ref().map(|dw| {
                let window_id = dw.get_window_id();
                WindowIdentifier {
                    app_id: "com.rustframe.app".to_string(),
                    window_name: format!("RustFrame Preview {}", window_id),
                }
            })
        };
        
        #[cfg(not(target_os = "macos"))]
        let preview_window_id: Option<window_filter::WindowIdentifier> = Some(window_filter::WindowIdentifier::preview_window());
        
        // Note: Separation layer is already hidden from screen sharing via NSWindowSharingNone
        // No need to explicitly exclude it from capture
        let exclusion_list = preview_window_id.into_iter().collect::<Vec<_>>();

        log::info!("[MAIN] Calling engine.start() with region: {:?}, excluded: {} items", region, exclusion_list.len());
        engine.start(region, settings.show_cursor, Some(exclusion_list)).map_err(|e| {
            tracing::error!(error = %e, "Capture engine start failed");
            e.to_string()
        })?;
        tracing::info!("Capture engine started successfully");
    } else {
        tracing::error!("Capture engine is None after creation");
    }
    drop(engine_lock);

    // Set up cursor filtering: check if preview overlaps capture region
    #[cfg(target_os = "windows")]
    {
        use rustframe_capture::capture::windows::set_preview_bounds_and_check_overlap;
        
        // Get destination window bounds
        if let Ok(mut dest_lock) = DESTINATION_WINDOW.lock() {
            if let Some(ref mut dest_window) = *dest_lock {
                // DO NOT exclude preview from screen capture - this causes black screen in Meet/Discord!
                // We rely on z-order (putting it at bottom) or window filtering to avoid infinite mirror.
                // dest_window.exclude_from_capture();
                log::info!("✅ Preview window configured for capture visibility");
                
                // RegionToShare approach: Position preview at border location
                // It will be below separation layer in z-order
                dest_window.set_pos(x, y);
                dest_window.resize(width, height);
                log::info!("✅ Preview positioned at border location: ({}, {}) {}x{}", x, y, width, height);
                dest_window.disable_masking();
                
                // Setup z-order: Preview below separation layer
                // Get separation layer HWND
                if let Ok(sep_lock) = SEPARATION_LAYER.lock() {
                    if let Some(ref sep) = *sep_lock {
                        use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE};
                        use windows::Win32::Foundation::HWND;
                        
                        let sep_hwnd = HWND(sep.hwnd_value() as *mut _);
                        let preview_hwnd = HWND(dest_window.hwnd_value() as *mut _);
                        
                        // Position preview below separation layer
                        let _ = unsafe {
                            SetWindowPos(
                                preview_hwnd,
                                Some(sep_hwnd), // Insert below separation layer
                                0, 0, 0, 0,
                                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE
                            )
                        };
                        log::info!("✅ Z-order established: Border → Separation Layer → Preview");
                    }
                }
                
                if let Some((px, py, pw, ph)) = dest_window.get_rect() {
                    tracing::info!(
                        preview_x = px,
                        preview_y = py,
                        preview_width = pw,
                        preview_height = ph,
                        capture_x = x,
                        capture_y = y,
                        capture_width = width,
                        capture_height = height,
                        "Preview positioned at border for RegionToShare approach"
                    );
                    
                    // Check overlap for cursor filtering
                    set_preview_bounds_and_check_overlap(
                        px, py, pw, ph,                    // preview bounds
                        x, y, width as i32, height as i32  // capture bounds
                    );
                }
            }
        }
    }

    tracing::debug!(
        capture_clicks = settings.capture_clicks,
        "Checking click capture setting"
    );

    // Start click capture if enabled
    if settings.capture_clicks {
        tracing::info!("Starting click capture");
        if let Err(e) = platform::input::start_click_capture() {
            tracing::error!(error = %e, "Failed to start click capture");
        } else {
            tracing::debug!("Click capture started successfully");
        }
    } else {
        tracing::debug!("Click capture disabled in settings");
    }

    *state.is_capturing.lock().unwrap() = true;
    *state.render_thread_stop.lock().unwrap() = false;

    // Register callback for border interaction completion (mouseUp event)
    // Strategy: Pause capture during drag/resize, update everything once at the end
    // Benefits: ~40-45% CPU reduction during interaction (50% → 5-10%)
    {
        use crate::hollow_border::set_border_interaction_complete_callback;
        let engine_for_cb = state.capture_engine.clone();
        let border_w = settings.border_width;
        let app_for_cb = app.clone();
        let monitors_for_cb = state.monitors.clone();
        let settings_for_cb = state.settings.clone();

        set_border_interaction_complete_callback(move |x, y, width, height| {
            log::info!(
                "🔄 Border interaction COMPLETE - Border window: x={}, y={}, w={}, h={}",
                x,
                y,
                width,
                height
            );
            
            // Update separation layer (RegionToShare approach)
            // Now safe to do directly because it uses SWP_ASYNCWINDOWPOS and a dedicated message loop
            #[cfg(target_os = "windows")]
            {
                if let Ok(sep_lock) = SEPARATION_LAYER.try_lock() {
                    if let Some(ref sep) = *sep_lock {
                        sep.update_position(x, y, width, height);
                    }
                }
            }

            #[cfg(target_os = "macos")]
            {
                // CRITICAL: All 3 windows MUST have same position and size
                // Border, Separation, and Preview windows must be synchronized
                // Only update position during drag, restore z-order once at the end
                
                log::info!("📍 Callback received: ({}, {}) {}x{}", x, y, width, height);
                
                // Update preview/destination window to match border exactly
                if let Ok(dest_lock) = DESTINATION_WINDOW.try_lock() {
                    if let Some(ref dest) = *dest_lock {
                        log::info!("  → Updating destination...");
                        dest.update_position(x, y, width as u32, height as u32);
                    }
                }
                
                // Update separation layer to match border exactly  
                if let Ok(sep_lock) = SEPARATION_LAYER.try_lock() {
                    if let Some(ref sep) = *sep_lock {
                        log::info!("  → Updating separation...");
                        sep.update_position(x, y, width, height);
                    }
                }

                // CRITICAL: Restore z-order ONCE after all windows updated (at end of interaction)
                // This ensures smooth movement without flashing/flickering
                log::info!("  → Restoring z-order...");
                restore_window_z_order_macos();
                
                // DEBUG: Verify all windows have same position after update
                if let Ok(border_lock) = HOLLOW_BORDER.try_lock() {
                    if let Some(border) = border_lock.as_ref() {
                        let (bx, by, bw, bh) = border.get_rect();
                        log::info!("  ✓ Border after: ({}, {}) {}x{}", bx, by, bw, bh);
                    }
                }
                if let Ok(dest_lock) = DESTINATION_WINDOW.try_lock() {
                    if let Some(dest) = dest_lock.as_ref() {
                        if let Some((dx, dy, dw, dh)) = dest.get_rect() {
                            log::info!("  ✓ Destination after: ({}, {}) {}x{}", dx, dy, dw, dh);
                        }
                    }
                }
                if let Ok(sep_lock) = SEPARATION_LAYER.try_lock() {
                    if let Some(sep) = sep_lock.as_ref() {
                        if let Some((sx, sy, sw, sh)) = sep.get_rect() {
                            log::info!("  ✓ Separation after: ({}, {}) {}x{}", sx, sy, sw, sh);
                        }
                    }
                }
            }
            
            // Emit region update to frontend
            if let Err(e) = app_for_cb.emit("region-changed", serde_json::json!({
                "x": x,
                "y": y,
                "width": width,
                "height": height
            })) {
                log::error!("Failed to emit region-changed event: {}", e);
            }

            // Calculate inner region (excluding border)
            let border_offset = border_w as i32;
            let inner_width = (width - border_offset * 2).max(1);
            let inner_height = (height - border_offset * 2).max(1);
            log::info!(
                "🔄 Border offset: {}, Inner region: {}x{} pixels",
                border_offset,
                inner_width,
                inner_height
            );

            // Check if border moved to a different monitor
            let center_x = x + width / 2;
            let center_y = y + height / 2;

            #[cfg(target_os = "windows")]
            {
                use windows::Win32::Foundation::POINT;
                use windows::Win32::Graphics::Gdi::{
                    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
                };
                use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

                let current_monitor = unsafe {
                    MonitorFromPoint(
                        POINT {
                            x: center_x,
                            y: center_y,
                        },
                        MONITOR_DEFAULTTONEAREST,
                    )
                };

                if !current_monitor.is_invalid() {
                    let mut monitor_info = MONITORINFO {
                        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                        ..Default::default()
                    };

                    if unsafe { GetMonitorInfoW(current_monitor, &mut monitor_info) }.as_bool() {
                        let monitor_left = monitor_info.rcMonitor.left;
                        let monitor_top = monitor_info.rcMonitor.top;

                        // Get monitor DPI for scaling calculations
                        let mut dpi_x: u32 = 96; // Default DPI
                        let mut dpi_y: u32 = 96;
                        if unsafe {
                            GetDpiForMonitor(
                                current_monitor,
                                MDT_EFFECTIVE_DPI,
                                &mut dpi_x,
                                &mut dpi_y,
                            )
                        }
                        .is_ok()
                        {
                            let scale_factor = dpi_x as f32 / 96.0;
                            log::info!(
                                "🖥️  Monitor DPI: {}x{}, Scale factor: {:.2}x",
                                dpi_x,
                                dpi_y,
                                scale_factor
                            );
                        }

                        // Get current capture monitor origin
                        let mut engine = match engine_for_cb.try_lock() {
                            Ok(e) => e,
                            Err(e) => {
                                log::error!("❌ Failed to lock capture engine during border move: {:?}", e);
                                return;
                            }
                        };
                        let needs_restart = if let Some(ref eng) = *engine {
                            // Check if we have a WindowsCaptureEngine with monitor_origin
                            if let Some(wce) = eng
                                .as_any()
                                .downcast_ref::<rustframe_capture::capture::WindowsCaptureEngine>(
                            ) {
                                let current_origin = wce.get_monitor_origin();
                                let changed = current_origin.0 != monitor_left
                                    || current_origin.1 != monitor_top;
                                if changed {
                                    log::info!("🖥️  Monitor changed! Old origin: {:?}, New origin: ({}, {})", 
                                        current_origin, monitor_left, monitor_top);
                                }
                                changed
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                        if needs_restart {
                            // Stop current capture
                            if let Some(ref mut eng) = *engine {
                                log::info!("Stopping capture to switch monitors...");
                                eng.stop();
                            }

                            // Restart capture on new monitor
                            if let Some(ref mut eng) = *engine {
                                let new_region = crate::CaptureRect {
                                    x: x + border_offset,
                                    y: y + border_offset,
                                    width: inner_width as u32,
                                    height: inner_height as u32,
                                };

                                log::info!(
                                    "Restarting capture on new monitor with region: {:?}",
                                    new_region
                                );
                                if let Err(e) = eng.start(new_region, true, None) {
                                    log::error!("Failed to restart capture: {}", e);
                                } else {
                                    log::info!("✅ Capture restarted on new monitor");
                                }
                            }
                            drop(engine);
                        } else {
                            drop(engine);

                            // Same monitor - just update region
                            let mut engine = match engine_for_cb.try_lock() {
                                Ok(e) => e,
                                Err(e) => {
                                    log::error!("❌ Failed to lock capture engine for region update: {:?}", e);
                                    return;
                                }
                            };
                            if let Some(ref mut eng) = *engine {
                                let new_region = crate::CaptureRect {
                                    x: x + border_offset,
                                    y: y + border_offset,
                                    width: inner_width as u32,
                                    height: inner_height as u32,
                                };
                                if let Err(e) = eng.update_region(new_region) {
                                    log::error!("Failed to update capture region: {}", e);
                                } else {
                                    log::info!(
                                        "✅ Capture region updated: x={}, y={}, w={}, h={}",
                                        new_region.x,
                                        new_region.y,
                                        new_region.width,
                                        new_region.height
                                    );
                                }
                            }
                            drop(engine);
                        }
                    }
                }
            }

            #[cfg(not(target_os = "windows"))]
            {
                #[cfg(target_os = "macos")]
                {
                    // macOS: Check if border moved to different display
                    let mut engine = match engine_for_cb.try_lock() {
                        Ok(e) => e,
                        Err(e) => {
                            log::error!("❌ Failed to lock capture engine (macOS): {:?}", e);
                            return;
                        }
                    };
                    let needs_restart = if let Some(ref eng) = *engine {
                        if let Some(macos_eng) =
                            eng.as_any()
                                .downcast_ref::<rustframe_capture::capture::MacOSCaptureEngine>()
                        {
                            let current_origin = macos_eng.get_monitor_origin();

                            // Use CGGetDisplaysWithRect to find current display (using 1x1 rect at point)
                            use core_graphics::display::{CGDisplay, CGRect};
                            use core_graphics::geometry::{CGPoint, CGSize};

                            let rect = CGRect::new(
                                &CGPoint::new(center_x as f64, center_y as f64),
                                &CGSize::new(1.0, 1.0),
                            );
                            let display_count = 1;
                            let mut display_id: u32 = 0;

                            let changed = unsafe {
                                if core_graphics::display::CGGetDisplaysWithRect(
                                    rect,
                                    display_count,
                                    &mut display_id,
                                    std::ptr::null_mut(),
                                ) == 0
                                {
                                    let display = CGDisplay::new(display_id);
                                    let bounds = display.bounds();
                                    let new_origin =
                                        (bounds.origin.x as i32, bounds.origin.y as i32);

                                    if current_origin.0 != new_origin.0
                                        || current_origin.1 != new_origin.1
                                    {
                                        log::info!("🖥️  Monitor changed! Old origin: {:?}, New origin: {:?}", 
                                            current_origin, new_origin);
                                        true
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            };

                            changed
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if needs_restart {
                        // Stop current capture
                        if let Some(ref mut eng) = *engine {
                            log::info!("Stopping capture to switch monitors...");
                            eng.stop();
                        }

                        // Restart capture on new monitor
                        if let Some(ref mut eng) = *engine {
                            let new_region = crate::CaptureRect {
                                x: x + border_offset,
                                y: y + border_offset,
                                width: inner_width as u32,
                                height: inner_height as u32,
                            };

                            log::info!(
                                "Restarting capture on new monitor with region: {:?}",
                                new_region
                            );
                            if let Err(e) = eng.start(new_region, true, None) {
                                log::error!("Failed to restart capture: {}", e);
                            } else {
                                log::info!("✅ Capture restarted on new monitor");
                            }
                        }
                        drop(engine);
                    } else {
                        drop(engine);

                        // Same monitor - just update region
                        let mut engine = match engine_for_cb.try_lock() {
                            Ok(e) => e,
                            Err(e) => {
                                log::error!("❌ Failed to lock capture engine for region update (macOS): {:?}", e);
                                return;
                            }
                        };
                        if let Some(ref mut eng) = *engine {
                            let new_region = crate::CaptureRect {
                                x: x + border_offset,
                                y: y + border_offset,
                                width: inner_width as u32,
                                height: inner_height as u32,
                            };
                            if let Err(e) = eng.update_region(new_region) {
                                log::error!("Failed to update capture region: {}", e);
                            } else {
                                log::info!(
                                    "✅ Capture region updated: x={}, y={}, w={}, h={}",
                                    new_region.x,
                                    new_region.y,
                                    new_region.width,
                                    new_region.height
                                );
                            }
                        }
                        drop(engine);
                    }
                }

                #[cfg(not(target_os = "macos"))]
                {
                    // Linux and other platforms: just update region
                    let mut engine = match engine_for_cb.try_lock() {
                        Ok(e) => e,
                        Err(e) => {
                            log::error!("❌ Failed to lock capture engine (Linux): {:?}", e);
                            return;
                        }
                    };
                    if let Some(ref mut eng) = *engine {
                        let new_region = crate::CaptureRect {
                            x: x + border_offset,
                            y: y + border_offset,
                            width: inner_width as u32,
                            height: inner_height as u32,
                        };
                        if let Err(e) = eng.update_region(new_region) {
                            log::error!("Failed to update capture region: {}", e);
                        } else {
                            log::info!(
                                "✅ Capture region updated: x={}, y={}, w={}, h={}",
                                new_region.x,
                                new_region.y,
                                new_region.width,
                                new_region.height
                            );
                        }
                    }
                    drop(engine);
                }
            }
            
            // Resize destination window
            log::info!(
                "🔄 Attempting to resize destination window to {}x{} pixels...",
                inner_width,
                inner_height
            );
            
            // Windows/macOS: keep preview aligned with border; macOS also updates separation layer.
            #[cfg(target_os = "windows")]
            {
                if let Ok(mut dest_lock) = DESTINATION_WINDOW.try_lock() {
                    if let Some(ref mut dest) = *dest_lock {
                        dest.resize(inner_width as u32, inner_height as u32);
                        dest.set_pos(x, y);
                        log::info!("✅ Preview resized and positioned at border: {}x{} at ({}, {})", 
                            inner_width, inner_height, x, y);
                    }
                }
            }

            #[cfg(target_os = "macos")]
            {
                let inner_x = x + border_offset;
                let inner_y = y + border_offset;
                
                if let Ok(mut dest_lock) = DESTINATION_WINDOW.try_lock() {
                    if let Some(ref mut dest) = *dest_lock {
                        // macOS: window already created with inner size, just update position
                        dest.set_pos(inner_x, inner_y);
                        log::info!("✅ macOS preview positioned at inner rect: at ({}, {})", inner_x, inner_y);
                    }
                }

                if let Ok(sep_lock) = SEPARATION_LAYER.try_lock() {
                    if let Some(ref sep) = *sep_lock {
                        sep.update_position(inner_x, inner_y, inner_width as i32, inner_height as i32);
                        sep.show();
                    }
                }
            }

            #[cfg(not(any(target_os = "windows", target_os = "macos")))]
            if let Ok(mut dest_lock) = DESTINATION_WINDOW.try_lock() {
                if let Some(ref mut dest) = *dest_lock {
                    dest.resize(inner_width as u32, inner_height as u32);
                    log::info!("✅ Destination window resize() called");
                }
            }

            // Update REC indicator position
            if let Ok(rec_lock) = REC_INDICATOR.try_lock() {
                if let Some(ref rec) = *rec_lock {
                    rec.update_position(x, y, width, border_w as i32);
                }
            }

            // Update cursor filtering based on new border position
            // If border was moved/resized, preview might now overlap/non-overlap with it
            #[cfg(target_os = "windows")]
            {
                use rustframe_capture::capture::windows::set_preview_bounds_and_check_overlap;
                
                // Get destination window bounds
                if let Ok(dest_lock) = DESTINATION_WINDOW.try_lock() {
                    if let Some(ref dest_window) = *dest_lock {
                        if let Some((px, py, pw, ph)) = dest_window.get_rect() {
                            set_preview_bounds_and_check_overlap(
                                px, py, pw, ph,         // preview bounds
                                x, y, width, height     // capture (border) bounds
                            );
                        }
                    }
                }
            }
        });
    }

    // Register callback for live border movement (fires during drag/resize)
    // This keeps REC indicator in sync with border while dragging
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        use crate::hollow_border::set_border_live_move_callback;
        let border_w = settings.border_width;

        set_border_live_move_callback(move |x, y, width, height| {
            // Update every event for smooth following, but use try_lock to avoid blocking
            // if another thread is using the resource

            // Update Separation Layer position (maintains z-order)
            #[cfg(target_os = "windows")]
            if let Ok(sep_lock) = SEPARATION_LAYER.try_lock() {
                if let Some(ref sep) = *sep_lock {
                    sep.update_position(x, y, width, height);
                }
            }

            #[cfg(target_os = "macos")]
            {
                // On macOS, separation follows preview size (inner rect)
                let border_offset = border_w as i32;
                let inner_x = x + border_offset;
                let inner_y = y + border_offset;
                let inner_width = (width - border_offset * 2).max(1);
                let inner_height = (height - border_offset * 2).max(1);
                
                if let Ok(sep_lock) = SEPARATION_LAYER.try_lock() {
                    if let Some(ref sep) = *sep_lock {
                        sep.update_position(inner_x, inner_y, inner_width, inner_height);
                    }
                }
            }

            // Update Destination Window position (smooth follow)
            #[cfg(target_os = "windows")]
            if let Ok(dest_lock) = DESTINATION_WINDOW.try_lock() {
                if let Some(ref dest) = *dest_lock {
                    dest.set_position_and_size(x, y, width, height);
                }
            }

            #[cfg(target_os = "macos")]
            {
                let border_offset = border_w as i32;
                let inner_x = x + border_offset;
                let inner_y = y + border_offset;
                
                if let Ok(mut dest_lock) = DESTINATION_WINDOW.try_lock() {
                    if let Some(ref mut dest) = *dest_lock {
                        // macOS: window already created with inner size, just update position
                        dest.set_pos(inner_x, inner_y);
                    }
                }
            }

            // Update REC indicator position in real-time during drag (fixed: re-enabled)
            if let Ok(rec_lock) = REC_INDICATOR.try_lock() {
                if let Some(ref rec) = *rec_lock {
                    rec.update_position(x, y, width, border_w as i32);
                }
            }
            
            // Update cursor filtering in real-time during drag
            #[cfg(target_os = "windows")]
            {
                use rustframe_capture::capture::windows::set_preview_bounds_and_check_overlap;
                if let Ok(dest_lock) = DESTINATION_WINDOW.try_lock() {
                    if let Some(ref dest_window) = *dest_lock {
                        if let Some((px, py, pw, ph)) = dest_window.get_rect() {
                            set_preview_bounds_and_check_overlap(
                                px, py, pw, ph,         // preview bounds
                                x, y, width, height     // capture (border) bounds
                            );
                        }
                    }
                }
            }
            
            // Note: Cannot emit events from here as we don't have app handle in this scope
            // Region updates will be reflected when border interaction completes
        });
    }

    // Start frame rendering thread
    let engine_clone = state.capture_engine.clone();
    let settings_clone = state.settings.clone(); // Clone settings for GPU check
    let stop_flag = state.render_thread_stop.clone();
    let target_fps = settings.target_fps;
    let capture_clicks = settings.click_highlight_color;
    let click_color = settings.click_highlight_color;
    let click_dissolve_ms = settings.click_dissolve_ms as u64;
    let click_radius = settings.click_highlight_radius;

    let render_handle = std::thread::spawn(move || {
        log::info!("Frame rendering thread started");
        let frame_duration = std::time::Duration::from_millis(1000 / target_fps as u64);

        loop {
            // Check stop flag
            if *stop_flag.lock().unwrap() {
                break;
            }

            let frame_start = std::time::Instant::now();

            // PERFORMANCE OPTIMIZATION: Skip capture during border drag/resize
            // Reduces CPU from ~50% to ~5-10% during interaction
            // Border updates happen once on mouseUp event (see callback above)
            let is_interacting = crate::hollow_border::is_border_interacting();

            if is_interacting {
                // Skip frame capture and rendering during interaction
                std::thread::sleep(frame_duration);
                continue;
            }

            // Get frame from capture engine
            let frame = {
                let mut engine = engine_clone.lock().unwrap();
                if let Some(ref mut eng) = *engine {
                    eng.get_frame()
                } else {
                    None
                }
            };

            // Render frame to destination window (use try_lock to avoid blocking)
            if let Some(mut frame) = frame {
                log::debug!(
                    "Got frame: {}x{}, data len: {}, gpu: {}",
                    frame.width,
                    frame.height,
                    frame.data.len(),
                    frame.gpu_texture.is_some()
                );

                // Check if GPU acceleration is available and enabled
                let gpu_enabled = settings_clone.lock().unwrap().gpu_acceleration;
                let use_gpu = gpu_enabled && frame.gpu_texture.is_some();

                if let Ok(dest_lock) = DESTINATION_WINDOW.try_lock() {
                    if let Some(window) = dest_lock.as_ref() {
                        // Gather click data if enabled
                        let mut click_shader_data = None;
                        
                        if settings.capture_clicks {
                            let display_info = display_info::get();
                            
                            // Convert frame offset to pixels
                            let offset_x_pixels = display_info.points_to_pixels(frame.offset_x as f64);
                            let offset_y_pixels = display_info.points_to_pixels(frame.offset_y as f64);
                            let width_pixels = display_info.points_to_pixels(frame.width as f64) as u32;
                            let height_pixels = display_info.points_to_pixels(frame.height as f64) as u32;

                            let clicks = platform::input::get_recent_clicks(
                                offset_x_pixels,
                                offset_y_pixels,
                                width_pixels,
                                height_pixels,
                                click_dissolve_ms,
                            );

                            if !clicks.is_empty() {
                                // For GPU shader, we currently support only one active click (the most recent one)
                                // or we could blend them if we upgraded the shader.
                                // For now, picking the latest one is a good 80/20 solution.
                                if let Some(latest_click) = clicks.last() {
                                    let age_ms = latest_click.timestamp.elapsed().as_millis() as f32;
                                    let alpha_factor = 1.0 - (age_ms / click_dissolve_ms as f32).min(1.0);
                                    let scaled_radius = display_info.points_to_pixels(click_radius as f64);
                                    
                                    // Convert absolute pixels to frame-relative pixels
                                    let frame_x = latest_click.x as f32 - offset_x_pixels as f32;
                                    let frame_y = latest_click.y as f32 - offset_y_pixels as f32;
                                    
                                    // Normalize color to 0.0-1.0 (assuming RGBA from settings)
                                    let c = click_color;
                                    let r = c[0] as f32 / 255.0;
                                    let g = c[1] as f32 / 255.0;
                                    let b = c[2] as f32 / 255.0;
                                    let a = c[3] as f32 / 255.0;
                                    
                                    click_shader_data = Some((frame_x, frame_y, scaled_radius as f32, alpha_factor, [r, g, b, a]));
                                }

                                // For CPU fallback, we still need to draw all clicks
                                // ONLY if GPU is NOT used.
                                if !use_gpu {
                                    for click in clicks {
                                        let frame_x = click.x - offset_x_pixels;
                                        let frame_y = click.y - offset_y_pixels;
                                        let age_ms = click.timestamp.elapsed().as_millis() as f32;
                                        let alpha = 1.0 - (age_ms / click_dissolve_ms as f32).min(1.0);
                                        let radius = display_info.points_to_pixels(click_radius as f64);

                                        draw_click_highlight(
                                            &mut frame.data,
                                            frame.width as i32,
                                            frame.height as i32,
                                            frame_x,
                                            frame_y,
                                            click_color,
                                            alpha,
                                            radius,
                                        );
                                    }
                                }
                            }
                        }

                        // GPU path: Use IOSurface/Texture (if available) with optional click data
                        #[cfg(target_os = "macos")]
                        {
                            if use_gpu {
                                if let Some(rustframe_capture::capture::GpuTextureHandle::Metal {
                                    iosurface_ptr,
                                    crop_x,
                                    crop_y,
                                    crop_w,
                                    crop_h,
                                    ..
                                }) = frame.gpu_texture
                                {
                                    window.update_frame_from_iosurface_ptr(
                                        iosurface_ptr,
                                        crop_x,
                                        crop_y,
                                        crop_w,
                                        crop_h,
                                    );
                                } else {
                                    window.update_frame(frame.data, frame.width, frame.height);
                                }
                            } else {
                                window.update_frame(frame.data, frame.width, frame.height);
                            }
                        }

                        #[cfg(target_os = "windows")]
                        {
                            if use_gpu {
                                if let Some(rustframe_capture::capture::GpuTextureHandle::D3D11 {
                                    texture_ptr,
                                    crop_x,
                                    crop_y,
                                    crop_width,
                                    crop_height,
                                    ..
                                }) = frame.gpu_texture
                                {
                                    window.update_frame_from_texture(
                                        texture_ptr,
                                        crop_x,
                                        crop_y,
                                        crop_width,
                                        crop_height,
                                        click_shader_data,
                                    );
                                } else {
                                    window.update_frame(frame.data, frame.width, frame.height);
                                }
                            } else {
                                // Fall back to CPU rendering
                                window.update_frame(frame.data, frame.width, frame.height);
                            }
                        }

                        #[cfg(target_os = "linux")]
                        {
                            window.update_frame(frame.data, frame.width, frame.height);
                        }
                    }
                }
            } // DESTINATION_WINDOW lock released here

            // Frame rate limiting
            let elapsed = frame_start.elapsed();

            // During border interaction (drag/resize), use faster update rate for Meet sync
            let is_interacting_for_fps = hollow_border::is_border_interacting();

            let min_frame_duration = if is_interacting_for_fps {
                // 5ms during interaction = ~200 FPS max for smooth Meet updates
                std::time::Duration::from_millis(5)
            } else {
                frame_duration
            };

            if elapsed < min_frame_duration {
                std::thread::sleep(min_frame_duration - elapsed);
            }
        }
    });

    *state.render_thread_handle.lock().unwrap() = Some(render_handle);

    log::info!("Capture started successfully");
    Ok(())
}

#[tauri::command]
async fn stop_capture(state: State<'_, AppState>) -> Result<Settings, String> {
    tracing::info!("Stopping capture");
    log::info!("Stopping capture");

    // Save last region if remember_last_region is enabled
    let remember_last_region = state.settings.lock().unwrap().remember_last_region;
    if remember_last_region {
        let border_guard = match HOLLOW_BORDER.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let rect = border_guard.as_ref().map(|b| b.get_inner_rect());

        if let Some(rect) = rect {
            log::info!(
                "Read border position: x={}, y={}, w={}, h={}",
                rect.0,
                rect.1,
                rect.2,
                rect.3
            );

            let updated_settings = {
                let mut settings_guard = state.settings.lock().unwrap();
                settings_guard.last_region = Some([rect.0, rect.1, rect.2, rect.3]);
                settings_guard.clone()
            };

            if let Err(e) = persist_settings_to_disk(&updated_settings) {
                log::error!("Failed to persist last_region: {}", e);
            } else {
                log::info!("Successfully saved last_region to disk");
            }
        }
    }

    // Signal render thread to stop
    *state.render_thread_stop.lock().unwrap() = true;

    // Join render thread to ensure it isn't still touching NSWindow-backed objects.
    if let Some(handle) = state.render_thread_handle.lock().unwrap().take() {
        let _ = handle.join();
    }

    // Stop capture engine
    let mut engine_lock = state.capture_engine.lock().unwrap();
    if let Some(ref mut engine) = *engine_lock {
        engine.stop();
        log::info!("Capture engine stopped");
    }
    drop(engine_lock);

    // Clean up ALL borders - both capture border and preview border
    // First, clear the capture border
    tracing::debug!("Clearing HOLLOW_BORDER");
    *HOLLOW_BORDER.lock().unwrap() = None;
    log::info!("Capture border cleared");

    // Also clear preview border if it somehow exists
    tracing::debug!("Clearing PREVIEW_BORDER");
    *PREVIEW_BORDER.lock().unwrap() = None;
    log::info!("Preview border cleared");

    // Clean up capture-related windows
    tracing::debug!("Clearing DESTINATION_WINDOW");
    *DESTINATION_WINDOW.lock().unwrap() = None;
    
    // Clean up separation layer (RegionToShare approach)
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        tracing::debug!("Clearing SEPARATION_LAYER");
        *SEPARATION_LAYER.lock().unwrap() = None;
        log::info!("Separation layer cleared");
    }
    
    // Clear cursor filtering on capture stop
    #[cfg(target_os = "windows")]
    {
        use rustframe_capture::capture::windows::clear_cursor_filtering;
        clear_cursor_filtering();
        tracing::debug!("Cursor filtering cleared");
    }
    
    tracing::debug!("Clearing REC_INDICATOR");
    *REC_INDICATOR.lock().unwrap() = None;
    log::info!("Capture windows cleaned up");

    // Stop mouse hook completely and clear click capture data
    platform::input::stop_click_capture();
    platform::input::clear_clicks();

    *state.is_capturing.lock().unwrap() = false;

    // Return updated settings so frontend can sync
    let final_settings = state.settings.lock().unwrap().clone();
    log::info!("Capture stopped successfully");
    Ok(final_settings)
}

/// Cleanup borders and windows when capture fails to start
/// This is called from frontend when start_capture returns an error
#[tauri::command]
async fn cleanup_on_capture_failed(state: State<'_, AppState>) -> Result<(), String> {
    tracing::error!("Cleaning up after capture start failure");
    log::error!("Cleaning up after capture start failure");

    // Stop any capture engine that might have started
    let mut engine_lock = state.capture_engine.lock().unwrap();
    if let Some(ref mut engine) = *engine_lock {
        engine.stop();
    }
    drop(engine_lock);

    // Clean up all borders and windows
    *HOLLOW_BORDER.lock().unwrap() = None;
    *PREVIEW_BORDER.lock().unwrap() = None;
    *DESTINATION_WINDOW.lock().unwrap() = None;
    *REC_INDICATOR.lock().unwrap() = None;
    
    // Clean up separation layer (RegionToShare approach)
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        *SEPARATION_LAYER.lock().unwrap() = None;
    }

    // Clear click capture data
    platform::input::clear_clicks();

    // Ensure capturing state is false
    *state.is_capturing.lock().unwrap() = false;

    log::info!("Cleanup completed after capture failure");
    Ok(())
}

#[tauri::command]
async fn is_capturing(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(*state.is_capturing.lock().unwrap())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub id: usize,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    pub is_primary: bool,
    pub refresh_rate: u32,
}

// Windows implementation
#[cfg(target_os = "windows")]
#[tauri::command]
async fn get_monitors() -> Result<Vec<MonitorInfo>, String> {
    use std::mem;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, EnumDisplaySettingsW, GetMonitorInfoW, DEVMODEW,
        ENUM_CURRENT_SETTINGS, HDC, HMONITOR, MONITORINFOEXW,
    };
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

    let mut monitors = Vec::new();

    unsafe extern "system" fn enum_proc(
        hmonitor: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        lparam: LPARAM,
    ) -> BOOL {
        let monitors = &mut *(lparam.0 as *mut Vec<MonitorInfo>);

        let mut info: MONITORINFOEXW = mem::zeroed();
        info.monitorInfo.cbSize = mem::size_of::<MONITORINFOEXW>() as u32;

        if GetMonitorInfoW(hmonitor, &mut info.monitorInfo as *mut _ as *mut _).as_bool() {
            let rect = info.monitorInfo.rcMonitor;
            let name = String::from_utf16_lossy(&info.szDevice);
            let device_name = windows::core::PCWSTR::from_raw(info.szDevice.as_ptr());

            // Get DPI
            let mut dpi_x = 96;
            let mut dpi_y = 96;
            let _ = GetDpiForMonitor(hmonitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
            let scale_factor = dpi_x as f64 / 96.0;

            // Get refresh rate from display settings
            let mut devmode: DEVMODEW = mem::zeroed();
            devmode.dmSize = mem::size_of::<DEVMODEW>() as u16;
            let refresh_rate =
                if EnumDisplaySettingsW(device_name, ENUM_CURRENT_SETTINGS, &mut devmode).as_bool()
                {
                    devmode.dmDisplayFrequency
                } else {
                    60 // Default to 60Hz
                };

            monitors.push(MonitorInfo {
                id: monitors.len(),
                name: name.trim_end_matches('\0').to_string(),
                x: rect.left,
                y: rect.top,
                width: (rect.right - rect.left) as u32,
                height: (rect.bottom - rect.top) as u32,
                scale_factor,
                is_primary: info.monitorInfo.dwFlags == 1,
                refresh_rate,
            });
        }

        BOOL::from(true)
    }

    unsafe {
        let monitors_ptr = &mut monitors as *mut Vec<MonitorInfo> as isize;
        let _ = EnumDisplayMonitors(
            Some(HDC::default()),
            None,
            Some(enum_proc),
            LPARAM(monitors_ptr),
        );
    }

    Ok(monitors)
}

// Non-Windows implementation using Tauri API
#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn get_monitors(window: tauri::Window, state: State<'_, AppState>) -> Result<Vec<MonitorInfo>, String> {
    match window.available_monitors() {
        Ok(monitors) => {
            let mut result = Vec::new();
            for (idx, m) in monitors.into_iter().enumerate() {
                let scale_factor = m.scale_factor();
                let size = m.size().to_logical::<u32>(scale_factor);
                let position = m.position().to_logical::<i32>(scale_factor);
                let name = m
                    .name()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("Display {}", idx + 1));

                result.push(MonitorInfo {
                    id: idx,
                    name,
                    x: position.x,
                    y: position.y,
                    width: size.width,
                    height: size.height,
                    scale_factor,
                    is_primary: position.x == 0 && position.y == 0, // Heuristic: (0,0) is usually primary
                    refresh_rate: 60, // Tauri doesn't always provide this, default to 60
                });
            }
            
            // Update the global monitors state
            *state.monitors.lock().unwrap() = result.clone();
            
            Ok(result)
        }
        Err(e) => Err(format!("Failed to list monitors: {}", e)),
    }
}

// Windows implementation
#[cfg(target_os = "windows")]
#[tauri::command]
async fn get_screen_dimensions() -> Result<(u32, u32), String> {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    unsafe {
        let w = GetSystemMetrics(SM_CXSCREEN) as u32;
        let h = GetSystemMetrics(SM_CYSCREEN) as u32;
        Ok((w, h))
    }
}

// Non-Windows stub
#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn get_screen_dimensions() -> Result<(u32, u32), String> {
    // TODO: Implement for macOS and Linux using proper APIs
    Ok((1920, 1080))
}

// Windows implementation
#[cfg(target_os = "windows")]
#[tauri::command]
async fn get_monitor_refresh_rate() -> Result<u32, String> {
    use windows::Win32::Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS};
    unsafe {
        let mut devmode: DEVMODEW = std::mem::zeroed();
        devmode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
        if EnumDisplaySettingsW(None, ENUM_CURRENT_SETTINGS, &mut devmode).as_bool() {
            return Ok(devmode.dmDisplayFrequency.max(30));
        }
        Ok(60)
    }
}

// Non-Windows stub
#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn get_monitor_refresh_rate() -> Result<u32, String> {
    // TODO: Implement for macOS and Linux
    Ok(60) // Default to 60Hz
}

// ============================================================================
// Platform Info
// ============================================================================

#[tauri::command]
fn get_platform_info() -> platform_info::PlatformInfo {
    platform_info::PlatformInfo::detect()
}

/// Get the display scale factor for DPI-aware UI sizing
/// Returns the scale factor (1.0 for standard displays, 2.0 for Retina, etc.)
#[tauri::command]
fn get_display_scale_factor() -> f64 {
    let display_info = display_info::get();
    display_info.scale_factor
}

/// Get the application version from Cargo.toml
#[tauri::command]
fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Get recommended window size based on display scale
/// Returns (width, height) in logical pixels that accounts for DPI/scaling
#[tauri::command]
fn get_recommended_window_size() -> (u32, u32) {
    let display_info = display_info::get();

    // Instead of fixed pixels, let's target 60% of screen height (max 900px width)
    // This scales better across different screen sizes
    let screen_height = if display_info.height_points > 0.0 {
        display_info.height_points
    } else {
        900.0
    };

    let target_height = (screen_height * 0.7).min(900.0).max(700.0);
    let target_width = (target_height * 1.1).min(1100.0).max(900.0);

    // Convert to logical if the OS doesn't handle it automatically (display_info returns physical usually)
    // But Tauri usually expects logical.

    // If we assume display_info.height is physical, and we want logical size for Tauri:
    // logical = physical / scale

    let logical_width = (target_width / display_info.scale_factor).round() as u32;
    let logical_height = (target_height / display_info.scale_factor).round() as u32;

    (logical_width, logical_height)
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    // Acquire single instance lock FIRST (before any other operations)
    // This prevents multiple instances from running simultaneously
    let instance_lock = match single_instance::SingleInstanceLock::acquire() {
        Ok(lock) => lock,
        Err(_e) => {
            eprintln!("RustFrame is already running!");
            eprintln!("Attempting to activate existing window...");

            // Try to bring the existing window to foreground
            single_instance::SingleInstanceLock::activate_existing_instance();

            std::process::exit(1);
        }
    };

    // Store the lock in global state so it's held for the entire application lifetime
    *SINGLE_INSTANCE_LOCK.lock().unwrap() = Some(instance_lock);

    // Initialize logging system AFTER acquiring lock
    // Load settings early to get log level configuration
    let (initial_settings, _) = if let Some(dir) = rustframe_config_dir() {
        load_settings_and_profile_from_disk(&dir)
    } else {
        (Settings::default(), None)
    };

    // Parse log level and initialize logger
    let log_level = initial_settings
        .log_level
        .parse::<logging::LogLevel>()
        .unwrap_or(logging::LogLevel::Error);

    if let Err(e) = logging::init_logging(log_level, initial_settings.log_to_file) {
        eprintln!("Failed to initialize logging: {}", e);
    } else {
        // Log startup header with visual markers for easy identification
        tracing::info!("***********************************************************************");
        tracing::info!("*                        RUSTFRAME STARTUP                            *");
        tracing::info!("***********************************************************************");
        tracing::info!(
            version = env!("CARGO_PKG_VERSION"),
            platform = std::env::consts::OS,
            log_level = %log_level.to_string(),
            "Application started"
        );
        tracing::info!("***********************************************************************");

        // Log system information for debugging
        tracing::debug!("");
        tracing::debug!("=== SYSTEM INFORMATION ===");
        tracing::debug!(
            os = std::env::consts::OS,
            arch = std::env::consts::ARCH,
            "Platform details"
        );
    }

    // Auto-cleanup old logs in background
    if initial_settings.log_to_file {
        logging::auto_cleanup_old_logs(initial_settings.log_retention_days);
    }

    // Initialize display information (needed for all coordinate calculations)
    if let Err(e) = display_info::initialize() {
        tracing::warn!(error = %e, "Failed to initialize display info");
        eprintln!("Warning: Failed to initialize display info: {}", e);
    } else {
        tracing::debug!("Display info initialized successfully");

        // Log display configuration for debugging
        tracing::debug!("");
        tracing::debug!("=== DISPLAY CONFIGURATION ===");
        let display_config = display_info::get();
        tracing::debug!(
            scale_factor = display_config.scale_factor,
            width_points = display_config.width_points,
            height_points = display_config.height_points,
            width_pixels = display_config.width_pixels,
            height_pixels = display_config.height_pixels,
            "Display details"
        );
    }

    // Set up panic hook for cleanup on crash
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        tracing::error!(
            ?panic_info,
            "Application panic detected! Performing emergency cleanup"
        );
        log::error!("Application panic detected! Performing emergency cleanup...");
        perform_cleanup();
        default_panic(panic_info);
    }));

    // Load settings + active profile (we already loaded them above for logging, reuse them)
    let settings = initial_settings;
    let active_profile = if let Some(dir) = rustframe_config_dir() {
        load_settings_and_profile_from_disk(&dir).1
    } else {
        None
    };

    // Log active settings for debugging
    tracing::debug!("");
    tracing::debug!("=== ACTIVE SETTINGS ===");
    tracing::debug!(
        capture_method = ?settings.capture_method,
        target_fps = settings.target_fps,
        show_cursor = settings.show_cursor,
        show_border = settings.show_border,
        border_width = settings.border_width,
        border_color = ?settings.border_color,
        show_rec_indicator = settings.show_rec_indicator,
        rec_indicator_size = ?settings.rec_indicator_size,
        remember_last_region = settings.remember_last_region,
        active_profile = ?active_profile,
        log_level = ?settings.log_level,
        log_to_file = settings.log_to_file,
        log_retention_days = settings.log_retention_days,
        "Settings configuration"
    );
    tracing::debug!("");
    tracing::debug!("***********************************************************************");
    tracing::debug!("*                   INITIALIZATION COMPLETE                           *");
    tracing::debug!("***********************************************************************");
    tracing::debug!("");

    // Initialize capture engine (depends on settings)
    tracing::info!(
        capture_method = %settings.capture_method.to_string(),
        "Initializing capture engine"
    );

    let capture_engine = create_capture_engine_for_settings(&settings)
        .or_else(|e| {
            tracing::warn!(error = %e, "Failed to create capture engine with settings, using default");
            create_capture_engine().map_err(|e| e.to_string())
        })
        .expect("Failed to initialize capture engine");

    tracing::debug!("Capture engine created successfully");

    let app_state = AppState {
        capture_engine: Arc::new(Mutex::new(Some(capture_engine))),
        settings: Arc::new(Mutex::new(settings)),
        active_profile: Arc::new(Mutex::new(active_profile)),
        is_capturing: Arc::new(Mutex::new(false)),
        settings_modal_open: Arc::new(Mutex::new(false)),
        render_thread_stop: Arc::new(Mutex::new(false)),
        render_thread_handle: Arc::new(Mutex::new(None)),
        monitors: Arc::new(Mutex::new(Vec::new())),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(app_state.clone())
        .invoke_handler(tauri::generate_handler![
            is_dev_mode,
            get_platform_info,
            get_display_scale_factor,
            get_recommended_window_size,
            get_app_version,
            get_settings,
            get_border_rect,
            get_capture_profiles,
            get_active_capture_profile,
            get_capture_profile_hints,
            set_active_capture_profile,
            check_profile_updates,
            download_profile,
            delete_profile,
            get_profile_details,
            get_available_windows,
            save_settings,
            get_settings_path,
            open_settings_folder,
            open_logs_folder,
            clear_old_logs,
            export_settings,
            import_settings,
            show_preview_border,
            hide_preview_border,
            update_preview_border,
            get_preview_border_rect,
            start_capture,
            stop_capture,
            cleanup_on_capture_failed,
            is_capturing,
            get_screen_dimensions,
            get_monitor_refresh_rate,
            get_monitors,
            update_preview_border_style,
        ])
        .on_window_event(move |_window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api: _, .. } => {
                    log::info!("Window close requested, performing cleanup...");

                    // Immediately hide/destroy hollow border to unblock its message loop
                    if let Ok(mut border) = HOLLOW_BORDER.try_lock() {
                        if border.is_some() {
                            log::info!("Closing hollow border window");
                            *border = None; // Drop the border, triggering cleanup
                        }
                    }

                    // Stop capture if running
                    if *app_state.is_capturing.lock().unwrap() {
                        log::info!("Capture is running, stopping before close...");

                        // Signal render thread to stop
                        *app_state.render_thread_stop.lock().unwrap() = true;

                        // Join render thread to avoid dropping windows while it's rendering
                        if let Some(handle) = app_state.render_thread_handle.lock().unwrap().take()
                        {
                            let _ = handle.join();
                        }

                        // Stop capture engine
                        if let Ok(mut engine_lock) = app_state.capture_engine.lock() {
                            if let Some(ref mut engine) = *engine_lock {
                                engine.stop();
                                log::info!("Capture engine stopped");
                            }
                        }

                        *app_state.is_capturing.lock().unwrap() = false;
                    }

                    // Perform global cleanup
                    perform_cleanup();

                    // Allow the window to close
                    // api.prevent_close(); // Uncomment if you want to prevent close
                }
                tauri::WindowEvent::Destroyed => {
                    log::info!("Window destroyed, ensuring cleanup...");
                    perform_cleanup();
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    // Final cleanup when app exits normally
    log::info!("Application exiting normally, performing final cleanup...");
    perform_cleanup();
}
