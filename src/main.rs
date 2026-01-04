// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::State;

// Import capture engine from library
use rustframe_capture::capture::{create_capture_engine, CaptureEngine, CaptureRect};

// Import modules
mod destination_window;
mod hollow_border;
mod platform;
mod platform_info;
mod rec_indicator;

use destination_window::DestinationWindow;
use destination_window::DestinationWindowConfig;
use hollow_border::HollowBorder;
use rec_indicator::RecIndicator;

// Global state for Windows (not thread-safe, but only accessed from commands)
lazy_static! {
    static ref HOLLOW_BORDER: Mutex<Option<HollowBorder>> = Mutex::new(None);
    static ref DESTINATION_WINDOW: Mutex<Option<DestinationWindow>> = Mutex::new(None);
    static ref REC_INDICATOR: Mutex<Option<RecIndicator>> = Mutex::new(None);
    // Global flag to track if cleanup has been performed
    static ref CLEANUP_PERFORMED: AtomicBool = AtomicBool::new(false);
}

// ============================================================================
// Cleanup - Ensures all resources are properly released
// ============================================================================

/// Perform cleanup of all capture resources
/// This function is safe to call multiple times - it will only execute once
fn perform_cleanup() {
    // Check if cleanup has already been performed
    if CLEANUP_PERFORMED.swap(true, Ordering::SeqCst) {
        log::info!("Cleanup already performed, skipping");
        return;
    }

    log::info!("Performing cleanup of all capture resources...");

    // Stop mouse hook first (before destroying windows)
    platform::input::stop_click_capture();
    log::info!("Mouse hook stopped");

    // Clean up hollow border window
    if let Ok(mut border) = HOLLOW_BORDER.try_lock() {
        if border.is_some() {
            *border = None;
            log::info!("Hollow border cleaned up");
        }
    }

    // Clean up destination window
    if let Ok(mut dest) = DESTINATION_WINDOW.try_lock() {
        if dest.is_some() {
            *dest = None;
            log::info!("Destination window cleaned up");
        }
    }

    // Clean up REC indicator
    if let Ok(mut rec) = REC_INDICATOR.try_lock() {
        if rec.is_some() {
            *rec = None;
            log::info!("REC indicator cleaned up");
        }
    }

    // Clear click capture data
    platform::input::clear_clicks();
    log::info!("Click capture data cleared");

    log::info!("Cleanup completed successfully");
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
    color: [u8; 4],    // RGBA
    alpha_factor: f32, // 0.0 to 1.0 for fade effect
) {
    let radius = 20i32; // Click highlight radius
    let inner_radius = 8i32; // Solid inner circle

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

            let final_alpha = (color[3] as f32 / 255.0) * alpha_factor * ring_alpha;

            if final_alpha <= 0.0 {
                continue;
            }

            // Pixel index in BGRA format
            let idx = ((py * width + px) * 4) as usize;
            if idx + 3 >= data.len() {
                continue;
            }

            // Alpha blend (BGRA format)
            let inv_alpha = 1.0 - final_alpha;
            data[idx] = (color[2] as f32 * final_alpha + data[idx] as f32 * inv_alpha) as u8; // B
            data[idx + 1] =
                (color[1] as f32 * final_alpha + data[idx + 1] as f32 * inv_alpha) as u8; // G
            data[idx + 2] =
                (color[0] as f32 * final_alpha + data[idx + 2] as f32 * inv_alpha) as u8;
            // R
            // Keep original alpha at data[idx + 3]
        }
    }
}

// ============================================================================
// State Management
// ============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum PreviewMode {
    TauriCanvas,      // Cross-platform, WebView overhead
    #[cfg(windows)]
    WinApiGdi,        // Windows-only, lightweight
}

impl Default for PreviewMode {
    fn default() -> Self {
        #[cfg(windows)]
        {
            PreviewMode::WinApiGdi
        }
        #[cfg(not(windows))]
        {
            PreviewMode::TauriCanvas
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    // Mouse & Cursor
    pub show_cursor: bool,
    pub capture_clicks: bool,
    pub click_highlight_color: [u8; 4],
    pub click_dissolve_ms: u32,

    // Border
    pub show_border: bool,
    pub border_color: [u8; 4],
    pub border_width: u32,

    // Performance
    pub target_fps: u32,

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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_cursor: true,
            capture_clicks: false,
            click_highlight_color: [255, 255, 0, 180],
            click_dissolve_ms: 300,
            show_border: true,
            border_color: [80, 130, 255, 255],
            border_width: 4,
            target_fps: 60,
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
            last_region: None,
            show_rec_indicator: true,
            rec_indicator_size: "medium".to_string(),
        }
    }
}

fn create_capture_engine_for_settings(settings: &Settings) -> Result<Box<dyn CaptureEngine>, String> {
    #[cfg(target_os = "windows")]
    {
        use rustframe_capture::capture::windows::{WindowsCaptureEngine, WindowsGdiCopyCaptureEngine};
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
    render_thread_stop: Arc<Mutex<bool>>,
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

fn scan_capture_profiles(dir: &Path) -> Vec<CaptureProfileInfo> {
    let mut profiles = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return profiles;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if !file_name.starts_with("profile_") {
            continue;
        }
        let id = file_name
            .trim_start_matches("profile_")
            .trim_end_matches(".json")
            .to_string();
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

    profiles.sort_by(|a, b| a.id.cmp(&b.id));
    profiles
}

fn read_profile_overrides(dir: &Path, profile_id: &str) -> Option<serde_json::Value> {
    let file_name = format!("profile_{}.json", profile_id);
    let path = dir.join(file_name);
    let raw = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    if value.is_object() { Some(value) } else { None }
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

fn apply_profile_overrides(base: &Settings, overrides: serde_json::Value) -> Result<Settings, String> {
    let mut merged = serde_json::to_value(base).map_err(|e| e.to_string())?;
    merge_json(&mut merged, overrides);
    serde_json::from_value::<Settings>(merged).map_err(|e| format!("Invalid profile overrides: {}", e))
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

fn write_active_profile_to_settings_json(dir: &Path, profile: Option<String>) -> Result<(), String> {
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
async fn get_capture_profiles() -> Result<Vec<CaptureProfileInfo>, String> {
    let Some(dir) = rustframe_config_dir() else {
        return Ok(vec![]);
    };
    let _ = std::fs::create_dir_all(&dir);
    Ok(scan_capture_profiles(&dir))
}

#[tauri::command]
async fn get_active_capture_profile(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.active_profile.lock().unwrap().clone())
}

#[tauri::command]
async fn get_capture_profile_hints(profile: String) -> Result<CaptureProfileHints, String> {
    let Some(dir) = rustframe_config_dir() else {
        return Ok(CaptureProfileHints {
            hide_taskbar_after_ms: None,
        });
    };

    let Some(overrides) = read_profile_overrides(&dir, &profile) else {
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

#[tauri::command]
async fn save_settings(settings: Settings, state: State<'_, AppState>) -> Result<(), String> {
    let mut app_settings = state.settings.lock().unwrap();
    *app_settings = settings.clone();

    // Save to disk (merge with existing JSON to preserve unknown/manual keys)
    if let Some(config_dir) = dirs::config_dir() {
        let rustframe_dir = config_dir.join("RustFrame");
        let _ = std::fs::create_dir_all(&rustframe_dir);
        let settings_path = rustframe_dir.join("settings.json");

        let mut existing_value: serde_json::Value = match std::fs::read_to_string(&settings_path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|_| serde_json::json!({})),
            Err(_) => serde_json::json!({}),
        };

        let new_value = serde_json::to_value(&settings).unwrap_or_else(|_| serde_json::json!({}));

        if let (serde_json::Value::Object(ref mut existing_obj), serde_json::Value::Object(new_obj)) =
            (&mut existing_value, new_value)
        {
            for (k, v) in new_obj {
                existing_obj.insert(k, v);
            }
        } else {
            // If the file isn't an object for some reason, fall back to writing our settings.
            existing_value = serde_json::to_value(&settings).unwrap_or_else(|_| serde_json::json!({}));
        }

        if let Ok(json) = serde_json::to_string_pretty(&existing_value) {
            let _ = std::fs::write(settings_path, json);
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
async fn export_settings(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let settings = state.settings.lock().unwrap();
    let json = serde_json::to_string_pretty(&*settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to write settings: {}", e))?;
    Ok(())
}

#[tauri::command]
async fn import_settings(path: String, state: State<'_, AppState>) -> Result<Settings, String> {
    let json =
        std::fs::read_to_string(&path).map_err(|e| format!("Failed to read settings file: {}", e))?;
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
) -> Result<(), String> {
    let mut preview = PREVIEW_BORDER.lock().map_err(|e| e.to_string())?;

    // Close existing preview if any
    if preview.is_some() {
        *preview = None;
    }

    // Create new preview border
    let border = HollowBorder::new(x, y, width, height, border_width, border_color)
        .ok_or("Failed to create preview border")?;

    // Preview mode: interior is draggable, not click-through
    border.set_preview_mode();

    *preview = Some(border);
    Ok(())
}

#[tauri::command]
fn hide_preview_border() -> Result<(), String> {
    let mut preview = PREVIEW_BORDER.lock().map_err(|e| e.to_string())?;
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

#[tauri::command]
async fn start_capture(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Base settings + optional active profile overrides
    let base_settings = state.settings.lock().unwrap().clone();
    let active_profile = state.active_profile.lock().unwrap().clone();
    let settings = if let (Some(dir), Some(profile_id)) = (rustframe_config_dir(), active_profile) {
        match read_profile_overrides(&dir, &profile_id) {
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

    log::info!(
        "Starting capture at ({}, {}) with size {}x{}",
        x,
        y,
        width,
        height
    );

    // Create hollow border (always WinAPI)
    // COLORREF format is 0x00BBGGRR, border_color is [R, G, B, A]
    let border_color = (settings.border_color[0] as u32)
        | ((settings.border_color[1] as u32) << 8)
        | ((settings.border_color[2] as u32) << 16);

    log::info!(
        "Creating hollow border with color: 0x{:06X}, width: {}",
        border_color,
        settings.border_width
    );
    let hollow_border = HollowBorder::new(
        x,
        y,
        width as i32,
        height as i32,
        settings.border_width as i32,
        border_color,
    )
    .ok_or("Failed to create hollow border")?;

    log::info!("Hollow border created successfully");

    // Capture mode: interior is click-through, only top edge drags
    hollow_border.set_capture_mode();

    // Apply show_border setting
    if !settings.show_border {
        hollow_border.hide();
    }

    *HOLLOW_BORDER.lock().unwrap() = Some(hollow_border);

    // Create and show REC indicator if enabled
    if settings.show_rec_indicator {
        let rec = RecIndicator::new().ok_or("Failed to create REC indicator")?;
        rec.set_size(&settings.rec_indicator_size);
        rec.show(x + width as i32, y, settings.border_width as i32);
        *REC_INDICATOR.lock().unwrap() = Some(rec);
        log::info!("REC indicator created and shown");
    }

    // Create destination window based on preview mode
    log::info!(
        "Creating destination window in {:?} mode",
        settings.preview_mode
    );
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
            let dest_window = DestinationWindow::new(width, height, config)
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
    match settings.preview_mode {
        PreviewMode::TauriCanvas => {
            // TODO: Implement Tauri Canvas window
            log::warn!("Tauri Canvas mode not yet implemented");
            return Err("Tauri Canvas mode not yet implemented".to_string());
        }
    }

    // Start capture engine
    log::info!("Starting capture engine");
    let mut engine_lock = state.capture_engine.lock().unwrap();
    // (Re)create capture engine so users can change capture_method from Settings
    if let Some(ref mut existing) = *engine_lock {
        existing.stop();
    }
    *engine_lock = Some(create_capture_engine_for_settings(&settings)?);

    if let Some(ref mut engine) = *engine_lock {
        let region = CaptureRect {
            x,
            y,
            width,
            height,
        };
        engine.start(region, settings.show_cursor).map_err(|e| {
            log::error!("Capture engine start failed: {}", e);
            e.to_string()
        })?;
        log::info!("Capture engine started successfully");
    }
    drop(engine_lock);

    // Start click capture if enabled
    if settings.capture_clicks {
        if let Err(e) = platform::input::start_click_capture() {
            log::warn!("Failed to start click capture: {}", e);
        } else {
            log::info!("Click capture started");
        }
    }

    *state.is_capturing.lock().unwrap() = true;
    *state.render_thread_stop.lock().unwrap() = false;

    // Start frame rendering thread
    let engine_clone = state.capture_engine.clone();
    let stop_flag = state.render_thread_stop.clone();
    let target_fps = settings.target_fps;
    let capture_clicks = settings.capture_clicks;
    let click_color = settings.click_highlight_color;
    let click_dissolve_ms = settings.click_dissolve_ms as u64;
    let border_width = settings.border_width;

    std::thread::spawn(move || {
        println!("[RENDER] Frame rendering thread started");
        log::info!("Frame rendering thread started");
        let frame_duration = std::time::Duration::from_millis(1000 / target_fps as u64);
        let mut last_region: Option<(i32, i32, i32, i32)> = None;
        let mut frame_count = 0u64;
        let mut lock_skip_count = 0u64;
        let mut last_stats_time = std::time::Instant::now();
        let mut stats_frame_count = 0u64;

        loop {
            // Check stop flag
            if *stop_flag.lock().unwrap() {
                log::info!(
                    "Render thread stopping. Stats: {} frames rendered, {} lock skips",
                    frame_count,
                    lock_skip_count
                );
                break;
            }

            let frame_start = std::time::Instant::now();

            // Check if hollow border region changed (use try_lock to avoid deadlock during resize)
            let current_rect = {
                if let Ok(border_lock) = HOLLOW_BORDER.try_lock() {
                    if let Some(hollow_border) = border_lock.as_ref() {
                        // Get inner rect (excludes the visual border)
                        Some(hollow_border.get_inner_rect())
                    } else {
                        None
                    }
                } else {
                    // Lock contention - skip this frame's region check
                    lock_skip_count += 1;
                    if lock_skip_count % 10 == 0 {
                        log::warn!("HOLLOW_BORDER lock contention: {} skips", lock_skip_count);
                    }
                    None
                }
            }; // HOLLOW_BORDER lock released here

            if let Some(current_rect) = current_rect {
                if last_region.is_none() || last_region.unwrap() != current_rect {
                    log::info!("Border region changed to: {:?}", current_rect);
                    last_region = Some(current_rect);

                    // Update capture engine region (HOLLOW_BORDER lock already released)
                    let mut engine = engine_clone.lock().unwrap();
                    if let Some(ref mut eng) = *engine {
                        let new_region = CaptureRect {
                            x: current_rect.0,
                            y: current_rect.1,
                            width: current_rect.2 as u32,
                            height: current_rect.3 as u32,
                        };

                        if let Err(e) = eng.update_region(new_region) {
                            log::error!("Failed to update capture region: {}", e);
                        } else {
                            log::info!("Capture region updated successfully");
                        }
                    }
                    drop(engine);

                    // Update REC indicator position
                    if let Ok(rec_lock) = REC_INDICATOR.try_lock() {
                        if let Some(ref rec) = *rec_lock {
                            rec.update_position(
                                current_rect.0,
                                current_rect.1,
                                current_rect.2,
                                border_width as i32,
                            );
                        }
                    }
                }
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
                // Overlay click highlights if enabled
                if capture_clicks {
                    // Use frame's offset for accurate click detection
                    // frame.offset_x/y tell us where the frame starts in screen coordinates
                    // This handles clipped regions correctly
                    let clicks = platform::input::get_recent_clicks(
                        frame.offset_x,
                        frame.offset_y,
                        frame.width,
                        frame.height,
                        click_dissolve_ms,
                    );

                    for click in clicks {
                        // Convert screen coordinates to frame coordinates
                        // Frame starts at offset_x, offset_y in screen space
                        let frame_x = click.x - frame.offset_x;
                        let frame_y = click.y - frame.offset_y;

                        // Draw click highlight circle (fading based on age)
                        let age_ms = click.timestamp.elapsed().as_millis() as f32;
                        let alpha_factor = 1.0 - (age_ms / click_dissolve_ms as f32).min(1.0);

                        draw_click_highlight(
                            &mut frame.data,
                            frame.width as i32,
                            frame.height as i32,
                            frame_x,
                            frame_y,
                            click_color,
                            alpha_factor,
                        );
                    }
                }

                if let Ok(dest_lock) = DESTINATION_WINDOW.try_lock() {
                    if let Some(window) = dest_lock.as_ref() {
                        window.update_frame(frame.data, frame.width, frame.height);
                        frame_count += 1;
                        stats_frame_count += 1;

                        // Print FPS stats every 2 seconds
                        let elapsed_since_stats = last_stats_time.elapsed();
                        if elapsed_since_stats.as_secs() >= 2 {
                            let actual_fps =
                                stats_frame_count as f64 / elapsed_since_stats.as_secs_f64();
                            println!(
                                "[RENDER] FPS: {:.1}, Total frames: {}, Lock skips: {}",
                                actual_fps, frame_count, lock_skip_count
                            );
                            last_stats_time = std::time::Instant::now();
                            stats_frame_count = 0;
                        }
                    }
                } else {
                    // Lock contention on destination window
                    log::warn!("DESTINATION_WINDOW lock contention, frame dropped");
                    lock_skip_count += 1;
                }
            } // DESTINATION_WINDOW lock released here

            // Frame rate limiting
            let elapsed = frame_start.elapsed();
            if elapsed < frame_duration {
                std::thread::sleep(frame_duration - elapsed);
            }
        }

        log::info!("Frame rendering thread stopped");
    });

    log::info!("Capture started successfully");
    Ok(())
}

#[tauri::command]
async fn stop_capture(state: State<'_, AppState>) -> Result<(), String> {
    log::info!("Stopping capture");

    // Signal render thread to stop
    *state.render_thread_stop.lock().unwrap() = true;

    // Give render thread time to stop
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Stop capture engine
    let mut engine_lock = state.capture_engine.lock().unwrap();
    if let Some(ref mut engine) = *engine_lock {
        engine.stop();
        log::info!("Capture engine stopped");
    }
    drop(engine_lock);

    // Clean up windows
    *HOLLOW_BORDER.lock().unwrap() = None;
    *DESTINATION_WINDOW.lock().unwrap() = None;
    *REC_INDICATOR.lock().unwrap() = None;
    log::info!("Windows cleaned up");

    // Clear click capture data
    platform::input::clear_clicks();

    *state.is_capturing.lock().unwrap() = false;

    log::info!("Capture stopped successfully");
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

// Non-Windows stub
#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn get_monitors() -> Result<Vec<MonitorInfo>, String> {
    // TODO: Implement for macOS and Linux
    Ok(vec![MonitorInfo {
        id: 0,
        name: "Primary Display".to_string(),
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
        is_primary: true,
        refresh_rate: 60,
    }])
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
    use windows::Win32::Graphics::Gdi::{
        EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS,
    };
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

// ============================================================================
// Main
// ============================================================================

fn main() {
    // Set up panic hook for cleanup on crash
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        log::error!("Application panic detected! Performing emergency cleanup...");
        perform_cleanup();
        default_panic(panic_info);
    }));

    // Load settings + active profile (active_profile is stored as an extra key in settings.json)
    let (settings, active_profile) = if let Some(dir) = rustframe_config_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let settings_path = dir.join("settings.json");
        if settings_path.exists() {
            let raw = std::fs::read_to_string(&settings_path).ok().unwrap_or_default();
            let settings: Settings = serde_json::from_str(&raw).unwrap_or_default();
            let active_profile = read_active_profile_from_settings_json(&dir);
            (settings, active_profile)
        } else {
            (Settings::default(), None)
        }
    } else {
        (Settings::default(), None)
    };

    // Initialize capture engine (depends on settings)
    let capture_engine = create_capture_engine_for_settings(&settings)
        .or_else(|_| create_capture_engine().map_err(|e| e.to_string()))
        .expect("Failed to initialize capture engine");

    let app_state = AppState {
        capture_engine: Arc::new(Mutex::new(Some(capture_engine))),
        settings: Arc::new(Mutex::new(settings)),
        active_profile: Arc::new(Mutex::new(active_profile)),
        is_capturing: Arc::new(Mutex::new(false)),
        render_thread_stop: Arc::new(Mutex::new(false)),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(app_state.clone())
        .invoke_handler(tauri::generate_handler![
            is_dev_mode,
            get_platform_info,
            get_settings,
            get_capture_profiles,
            get_active_capture_profile,
            get_capture_profile_hints,
            set_active_capture_profile,
            save_settings,
            get_settings_path,
            open_settings_folder,
            export_settings,
            import_settings,
            show_preview_border,
            hide_preview_border,
            update_preview_border,
            get_preview_border_rect,
            start_capture,
            stop_capture,
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

                    // Stop capture if running
                    if *app_state.is_capturing.lock().unwrap() {
                        log::info!("Capture is running, stopping before close...");

                        // Signal render thread to stop
                        *app_state.render_thread_stop.lock().unwrap() = true;

                        // Give render thread time to stop
                        std::thread::sleep(std::time::Duration::from_millis(50));

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
