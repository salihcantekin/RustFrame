// main.rs - RustFrame Application Entry Point
//
// Cross-platform screen capture tool using egui for UI.
// Supports Windows (primary), with macOS and Linux stubs ready for implementation.

mod app;
mod capture;
mod platform;
mod renderer;
mod ui;

use std::sync::Arc;
use anyhow::{Context, Result};
use egui::Color32;
use log::{error, info, LevelFilter};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowAttributes, WindowId, WindowLevel},
};

use app::{AppMode, AppState};
use capture::{CaptureEngine, create_capture_engine};
use renderer::Renderer;
use ui::{BorderFrameUi, CaptureBorder, CaptureBorderWindow, DestinationUi, OverlayUi, RustFrameTheme, SettingsDialog, SystemTray};

/// Custom user events for the event loop
#[derive(Debug, Clone)]
pub enum UserEvent {
    /// Toggle capture on/off
    ToggleCapture,
    /// Toggle overlay visibility
    ToggleOverlay,
    /// Show settings dialog
    ShowSettings,
    /// Show about dialog
    ShowAbout,
    /// Exit application
    Exit,
    /// Resize border window
    ResizeBorderWindow { delta_x: f32, delta_y: f32, from_corner: ResizeCorner },
    /// Move border window
    MoveBorderWindow { delta_x: f32, delta_y: f32 },
    /// Border drag operation ended (mouse released) - time to sync destination window
    BorderDragEnded,
    /// Border window was resized by native Win32 (for updating capture region)
    BorderResized { x: i32, y: i32, width: u32, height: u32 },
}

/// Resize corner direction
#[derive(Debug, Clone)]
pub enum ResizeCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    TopEdge,
    BottomEdge,
    LeftEdge,
    RightEdge,
}

/// Main application struct
struct RustFrameApp {
    /// Application state
    state: AppState,
    /// Capture engine
    capture_engine: Option<Box<dyn CaptureEngine>>,
    /// Overlay window (selection UI)
    overlay_window: Option<Arc<Window>>,
    /// Overlay renderer
    overlay_renderer: Option<Renderer>,
    /// Overlay UI
    overlay_ui: OverlayUi,
    /// Border frame window (shown during capture)
    border_window: Option<Arc<Window>>,
    /// Native border window handler (uses Win32 layered window for true transparency)
    #[cfg(windows)]
    capture_border: Option<CaptureBorderWindow>,
    /// Border frame UI (for non-Windows platforms)
    #[cfg(not(windows))]
    border_renderer: Option<Renderer>,
    /// Border frame UI
    border_ui: BorderFrameUi,
    /// Destination window (captured frame)
    destination_window: Option<Arc<Window>>,
    /// Destination renderer
    destination_renderer: Option<Renderer>,
    /// Destination UI
    destination_ui: DestinationUi,
    /// Settings dialog
    settings_dialog: SettingsDialog,
    /// System tray icon
    tray: SystemTray,
    /// Pending border window creation (x, y, width, height)
    pending_border_create: Option<(i32, i32, u32, u32)>,
    /// Event loop proxy
    event_proxy: Option<winit::event_loop::EventLoopProxy<UserEvent>>,
    /// Last frame time for FPS limiting
    last_frame_time: std::time::Instant,
}

impl RustFrameApp {
    fn new(event_proxy: winit::event_loop::EventLoopProxy<UserEvent>) -> Self {
        // Create capture engine
        let capture_engine = match create_capture_engine() {
            Ok(engine) => Some(engine),
            Err(e) => {
                error!("Failed to create capture engine: {}", e);
                None
            }
        };
        
        Self {
            state: AppState::new(cfg!(debug_assertions)),
            capture_engine,
            overlay_window: None,
            overlay_renderer: None,
            overlay_ui: OverlayUi::new(),
            border_window: None,
            #[cfg(windows)]
            capture_border: None,
            #[cfg(not(windows))]
            border_renderer: None,
            border_ui: BorderFrameUi::new(),
            destination_window: None,
            destination_renderer: None,
            destination_ui: DestinationUi::new(),
            settings_dialog: SettingsDialog::new(),
            tray: SystemTray::new(),
            pending_border_create: None,
            event_proxy: Some(event_proxy),
            last_frame_time: std::time::Instant::now(),
        }
    }
    
    fn create_overlay_window(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        info!("Creating overlay window");
        
        // Get window size - use remembered region if enabled, otherwise from settings
        let (width, height) = if self.state.settings.remember_region && self.state.settings.last_width > 0 {
            (self.state.settings.last_width, self.state.settings.last_height)
        } else {
            self.state.settings.get_window_dimensions()
        };
        info!("Window size: {}x{}", width, height);
        
        let window_attrs = WindowAttributes::default()
            .with_title("RustFrame - Select Region")
            .with_decorations(true)  // Native title bar for smooth drag/resize
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_resizable(true)
            .with_min_inner_size(PhysicalSize::new(400u32, 350u32))  // Minimum size to keep UI visible
            .with_inner_size(PhysicalSize::new(width.max(400), height.max(350)));
        
        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .context("Failed to create overlay window")?
        );
        
        // Apply position and icon from settings
        #[cfg(target_os = "windows")]
        {
            use platform::windows::{get_hwnd_from_window, get_primary_monitor_rect, set_window_icon};
            
            if let Some(hwnd) = get_hwnd_from_window(&window) {
                // Set window icon from embedded resource
                set_window_icon(hwnd);
                
                // Get screen dimensions for position calculation
                let screen_rect = get_primary_monitor_rect();
                let screen_width = screen_rect.width;
                let screen_height = screen_rect.height;
                
                // Use remembered position if enabled, otherwise use preset
                let (x, y) = if self.state.settings.remember_region && self.state.settings.last_width > 0 {
                    (self.state.settings.last_x, self.state.settings.last_y)
                } else {
                    self.state.settings.get_window_position(screen_width, screen_height)
                };
                
                // Set window position
                window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
                info!("Window position set to: ({}, {})", x, y);
            }
        }
        
        // Use normal renderer (not transparent) for consistent dark background
        let renderer = Renderer::new(window.clone())
            .context("Failed to create overlay renderer")?;
        
        self.overlay_window = Some(window);
        self.overlay_renderer = Some(renderer);
        
        Ok(())
    }
    
    fn create_destination_window(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        info!("Creating destination window");
        
        let window_attrs = WindowAttributes::default()
            .with_title("RustFrame - Captured Frame")
            .with_decorations(true)  // Enable window decorations for resize/move
            .with_transparent(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_resizable(true)
            .with_inner_size(PhysicalSize::new(800, 600));
        
        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .context("Failed to create destination window")?
        );
        
        #[cfg(target_os = "windows")]
        {
            use platform::windows::set_window_capture_exclusion;
            if let Some(hwnd) = platform::windows::get_hwnd_from_window(&window) {
                set_window_capture_exclusion(hwnd, true);
            }
        }
        
        let renderer = Renderer::new(window.clone())
            .context("Failed to create destination renderer")?;
        
        self.destination_window = Some(window);
        self.destination_renderer = Some(renderer);
        
        Ok(())
    }
    
    /// Create border frame window at specified position and size
    fn create_border_window(&mut self, event_loop: &ActiveEventLoop, x: i32, y: i32, width: u32, height: u32) -> Result<()> {
        info!("Creating border frame window at ({}, {}) size {}x{}", x, y, width, height);
        
        let window_attrs = WindowAttributes::default()
            .with_title("RustFrame Border")
            .with_decorations(false)  // No decorations for clean border
            .with_transparent(true)   // Transparent background
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_resizable(true)     // Allow native resize
            .with_position(winit::dpi::PhysicalPosition::new(x, y))
            .with_inner_size(PhysicalSize::new(width, height));
        
        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .context("Failed to create border window")?
        );
        
        #[cfg(target_os = "windows")]
        {
            use platform::windows::set_window_capture_exclusion;
            if let Some(hwnd) = platform::windows::get_hwnd_from_window(&window) {
                // Exclude from capture so it doesn't appear in captured content
                set_window_capture_exclusion(hwnd, true);
            }
            
            // Setup native Win32 layered window for true transparency
            if let Some(mut capture_border) = CaptureBorderWindow::from_winit_window(&window) {
                // Apply border colors from settings
                let border_colors = ui::BorderColors::from_settings(self.state.settings.border_color);
                capture_border.set_colors(border_colors);
                
                // Apply border style from settings
                let border_style = ui::BorderStyle {
                    border_width: self.state.settings.border_width as i32,
                    corner_size: 30,
                    corner_thickness: 8,
                    show_rec_indicator: self.state.settings.show_rec_indicator,
                    rec_indicator_size: self.state.settings.rec_indicator_size,
                };
                capture_border.set_style(border_style);
                
                self.capture_border = Some(capture_border);
                info!("Created capture border with Win32 layered window");
                
                // Set event proxy for border resize notifications
                if let Some(ref proxy) = self.event_proxy {
                    if let Some(ref mut border) = self.capture_border {
                        border.set_event_proxy(proxy.clone());
                    }
                }
            }
        }
        
        #[cfg(not(windows))]
        {
            // Use transparent renderer for border window on non-Windows
            let renderer = Renderer::new_transparent(window.clone())
                .context("Failed to create border renderer")?;
            self.border_renderer = Some(renderer);
            
            // Set event proxy for border UI
            if let Some(ref proxy) = self.event_proxy {
                self.border_ui.set_event_proxy(proxy.clone());
            }
        }
        
        self.border_window = Some(window);
        
        Ok(())
    }
    
    /// Update border frame rendering
    fn update_border(&mut self) {
        // On Windows, the border is drawn by Win32 UpdateLayeredWindow
        // No need for egui rendering
        #[cfg(windows)]
        {
            // Border is automatically redrawn on WM_SIZE via subclass
            // Nothing to do here
        }
        
        #[cfg(not(windows))]
        if let Some(renderer) = &mut self.border_renderer {
            renderer.begin_frame();
            
            // Render border frame
            self.border_ui.show(renderer.egui_ctx());
            
            if let Err(e) = renderer.end_frame() {
                error!("Failed to render border: {}", e);
            }
        }
    }
    
    /// Hide and destroy border window
    fn destroy_border_window(&mut self) {
        self.border_window = None;
        #[cfg(windows)]
        {
            self.capture_border = None;
        }
        #[cfg(not(windows))]
        {
            self.border_renderer = None;
        }
    }
    
    fn update_overlay(&mut self) {
        if let Some(renderer) = &mut self.overlay_renderer {
            renderer.begin_frame();
            
            // Apply theme
            RustFrameTheme::apply(renderer.egui_ctx());
            
            // Show overlay UI
            let overlay_response = self.overlay_ui.show(
                renderer.egui_ctx(),
                &mut self.state,
            );
            
            // Handle start capture request
            if overlay_response.start_capture {
                // Get overlay window position for region selection
                let region = if let Some(window) = &self.overlay_window {
                    let pos = window.outer_position().unwrap_or(winit::dpi::PhysicalPosition::new(0, 0));
                    let size = window.outer_size();
                    crate::capture::CaptureRect::new(
                        pos.x,
                        pos.y,
                        size.width,
                        size.height,
                    )
                } else {
                    crate::capture::CaptureRect::new(100, 100, 800, 600)
                };
                
                // Start the capture engine
                if let Some(engine) = &mut self.capture_engine {
                    match engine.start(region.clone(), self.state.settings.show_cursor) {
                        Ok(()) => {
                            self.state.start_capture(crate::app::CaptureRect::new(
                                region.x, region.y, region.width, region.height
                            ));
                            self.destination_ui.set_capturing(true);
                            
                            // Set capture region for click highlight coordinate mapping
                            self.destination_ui.set_capture_region(
                                region.x, region.y, region.width, region.height
                            );
                            
                            // Set click highlight duration from settings
                            self.destination_ui.set_click_highlight_duration(
                                self.state.settings.click_highlight_duration_ms
                            );
                            
                            // Install mouse hook if highlight clicks is enabled
                            #[cfg(target_os = "windows")]
                            if self.state.settings.highlight_clicks {
                                platform::windows::install_mouse_hook();
                            }
                            
                            // Hide overlay window so we capture what's underneath
                            if let Some(window) = &self.overlay_window {
                                window.set_visible(false);
                            }
                            
                            // Schedule border window creation (will be created in about_to_wait)
                            self.pending_border_create = Some((region.x, region.y, region.width, region.height));
                            
                            info!("Capture started! Overlay hidden, border frame scheduled");
                        }
                        Err(e) => {
                            error!("Failed to start capture: {}", e);
                        }
                    }
                } else {
                    // No capture engine - use test pattern
                    self.state.start_capture(crate::app::CaptureRect::new(
                        region.x, region.y, region.width, region.height
                    ));
                    self.destination_ui.set_capturing(true);
                    info!("Capture started (test pattern mode)");
                }
            }
            
            // Handle settings dialog open request
            if overlay_response.open_settings {
                // Resize window to minimum size for settings dialog
                if let Some(window) = &self.overlay_window {
                    const SETTINGS_MIN_WIDTH: u32 = 560;
                    const SETTINGS_MIN_HEIGHT: u32 = 550;
                    
                    let current_size = window.inner_size();
                    info!("Settings open requested. Current window size: {}x{}", current_size.width, current_size.height);
                    
                    // Always resize if below minimum
                    let new_width = current_size.width.max(SETTINGS_MIN_WIDTH);
                    let new_height = current_size.height.max(SETTINGS_MIN_HEIGHT);
                    
                    if new_width != current_size.width || new_height != current_size.height {
                        // Use Win32 API directly for immediate resize
                        #[cfg(target_os = "windows")]
                        {
                            if let Some(hwnd) = platform::windows::get_hwnd_from_window(window) {
                                info!("Calling set_window_size hwnd={} to {}x{}", hwnd, new_width, new_height);
                                platform::windows::set_window_size(hwnd, new_width, new_height);
                                // Force a redraw
                                window.request_redraw();
                            } else {
                                error!("Failed to get hwnd from window");
                            }
                        }
                        
                        #[cfg(not(target_os = "windows"))]
                        {
                            let _ = window.request_inner_size(PhysicalSize::new(new_width, new_height));
                            info!("Resized overlay for settings dialog: {}x{}", new_width, new_height);
                        }
                    }
                }
                self.settings_dialog.open(&self.state.settings);
            }
            
            // Handle cancel/close request
            if overlay_response.cancel {
                self.save_window_position();
                // Request exit
                info!("User cancelled - exiting");
                std::process::exit(0);
            }
            
            // Handle settings dialog
            let settings_response = self.settings_dialog.show(renderer.egui_ctx(), &mut self.state);
            
            // Track if settings were applied
            let settings_applied = settings_response.applied;
            
            if let Err(e) = renderer.end_frame() {
                error!("Failed to render overlay: {}", e);
            }
            
            // Settings changed - apply to window (outside renderer borrow)
            if settings_applied {
                info!("Settings updated - applying to window");
                self.apply_window_settings();
            }
        }
    }
    
    /// Apply current settings to overlay window (size and position)
    fn apply_window_settings(&mut self) {
        if let Some(window) = &self.overlay_window {
            // Apply size
            let (width, height) = self.state.settings.get_window_dimensions();
            let _ = window.request_inner_size(PhysicalSize::new(width, height));
            info!("Applied window size: {}x{}", width, height);
            
            // Apply position
            #[cfg(target_os = "windows")]
            {
                use platform::windows::get_primary_monitor_rect;
                use crate::app::PositionPreset;
                
                let screen_rect = get_primary_monitor_rect();
                let (x, y) = self.state.settings.get_window_position(screen_rect.width, screen_rect.height);
                
                if self.state.settings.position_preset != PositionPreset::Center {
                    window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
                    info!("Applied window position: ({}, {})", x, y);
                } else {
                    // Center the window
                    let center_x = (screen_rect.width as i32 - width as i32) / 2;
                    let center_y = (screen_rect.height as i32 - height as i32) / 2;
                    window.set_outer_position(winit::dpi::PhysicalPosition::new(center_x, center_y));
                    info!("Applied window position (centered): ({}, {})", center_x, center_y);
                }
            }
        }
        
        // Apply cursor visibility to active capture engine
        if let Some(engine) = &mut self.capture_engine {
            if let Err(e) = engine.set_cursor_visible(self.state.settings.show_cursor) {
                error!("Failed to update cursor visibility: {}", e);
            } else {
                info!("Applied cursor visibility: {}", self.state.settings.show_cursor);
            }
        }
    }
    
    /// Save current window position and size for remember_region feature
    fn save_window_position(&mut self) {
        if !self.state.settings.remember_region {
            return;
        }
        
        if let Some(window) = &self.overlay_window {
            if let Ok(pos) = window.outer_position() {
                let size = window.inner_size();
                
                self.state.settings.last_x = pos.x;
                self.state.settings.last_y = pos.y;
                self.state.settings.last_width = size.width;
                self.state.settings.last_height = size.height;
                
                if let Err(e) = self.state.settings.save() {
                    error!("Failed to save window position: {}", e);
                } else {
                    info!("Saved window position: ({}, {}) size: {}x{}", 
                        pos.x, pos.y, size.width, size.height);
                }
            }
        }
    }
    
    fn update_destination(&mut self) {        
        // Check if stop was requested (from button or ESC key)
        if self.destination_ui.take_stop_request() {
            self.stop_capture();
            return;
        }
        
        // FPS limiting - skip frame if too soon
        let target_fps = self.state.settings.target_fps.max(1) as f64;
        let frame_duration = std::time::Duration::from_secs_f64(1.0 / target_fps);
        let elapsed = self.last_frame_time.elapsed();
        
        if elapsed < frame_duration {
            // Request redraw later to maintain frame rate
            if let Some(window) = &self.destination_window {
                window.request_redraw();
            }
            return;
        }
        self.last_frame_time = std::time::Instant::now();
        
        // Process mouse clicks for highlighting
        #[cfg(target_os = "windows")]
        if self.state.settings.highlight_clicks && self.state.is_capturing() {
            // Update capture region from current engine state (in case window moved/resized)
            if let Some(engine) = &self.capture_engine {
                if let Some(region) = engine.get_region() {
                    self.destination_ui.set_capture_region(
                        region.x, region.y, region.width, region.height
                    );
                }
            }
            
            let new_clicks = platform::windows::get_mouse_clicks();
            
            if !new_clicks.is_empty() {
                // Only send NEW clicks to destination UI
                let color = Color32::from_rgba_unmultiplied(
                    self.state.settings.click_highlight_color[0],
                    self.state.settings.click_highlight_color[1],
                    self.state.settings.click_highlight_color[2],
                    self.state.settings.click_highlight_color[3],
                );
                
                let clicks_data: Vec<_> = new_clicks.iter()
                    .map(|c| (c.x, c.y, c.timestamp, c.is_left))
                    .collect();
                self.destination_ui.add_click_highlights(clicks_data, color);
            }
        }
        
        // Clean up old highlights in destination UI
        self.destination_ui.update_click_highlights();
        
        if let Some(renderer) = &mut self.destination_renderer {
            // Check for new captured frames before begin_frame
            let has_new_frame = if let Some(engine) = &mut self.capture_engine {
                engine.has_new_frame()
            } else {
                false
            };
            
            let frame_data = if has_new_frame {
                if let Some(engine) = &mut self.capture_engine {
                    engine.get_frame()
                } else {
                    None
                }
            } else {
                None
            };
            
            renderer.begin_frame();
            
            // Update frame texture if we have new data
            if let Some(frame) = frame_data {
                self.destination_ui.update_frame(
                    renderer.egui_ctx(),
                    &frame.data,
                    frame.width,
                    frame.height,
                );
            }
            
            // Apply theme
            RustFrameTheme::apply(renderer.egui_ctx());
            
            // Show destination UI
            self.destination_ui.show(renderer.egui_ctx());
            
            if let Err(e) = renderer.end_frame() {
                error!("Failed to render destination: {}", e);
            }
        }
    }
    
    fn handle_overlay_event(&mut self, event: &WindowEvent, event_loop: &ActiveEventLoop) {
        // Handle egui events
        if let Some(renderer) = &mut self.overlay_renderer {
            if renderer.handle_event(event) {
                return; // egui consumed the event
            }
        }
        
        match event {
            WindowEvent::CloseRequested => {
                info!("Overlay close requested");
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.overlay_renderer {
                    renderer.resize(size.width, size.height);
                }
                
                // If capturing, update capture engine region to match new overlay size
                if self.state.is_capturing() {
                    if let Some(window) = &self.overlay_window {
                        if let Some(pos) = window.outer_position().ok() {
                            let new_region = crate::capture::CaptureRect::new(
                                pos.x,
                                pos.y,
                                size.width,
                                size.height,
                            );
                            if let Some(engine) = &mut self.capture_engine {
                                let _ = engine.update_region(new_region);
                            }
                        }
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                use winit::keyboard::{Key, NamedKey};
                if event.state.is_pressed() {
                    if let Key::Named(NamedKey::Escape) = event.logical_key {
                        match self.state.mode {
                            AppMode::Selecting => {
                                // Exit application
                                event_loop.exit();
                            }
                            AppMode::Capturing => {
                                // Use shared stop method
                                self.stop_capture();
                                info!("Capture stopped by user (ESC on overlay)");
                            }
                        }
                    }
                }
            }
            WindowEvent::Moved(position) => {
                // If capturing, update capture engine region to match new overlay position
                if self.state.is_capturing() {
                    if let Some(window) = &self.overlay_window {
                        let size = window.outer_size();
                        let new_region = crate::capture::CaptureRect::new(
                            position.x,
                            position.y,
                            size.width,
                            size.height,
                        );
                        if let Some(engine) = &mut self.capture_engine {
                            let _ = engine.update_region(new_region);
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.update_overlay();
            }
            _ => {}
        }
    }
    
    fn handle_destination_event(&mut self, event: &WindowEvent, event_loop: &ActiveEventLoop) {
        // Handle egui events
        if let Some(renderer) = &mut self.destination_renderer {
            if renderer.handle_event(event) {
                return;
            }
        }
        
        match event {
            WindowEvent::CloseRequested => {
                info!("Destination window close requested - exiting app");
                // Stop capture first
                if let Some(engine) = &mut self.capture_engine {
                    engine.stop();
                }
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.destination_renderer {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                use winit::keyboard::{Key, NamedKey};
                if event.state.is_pressed() {
                    if let Key::Named(NamedKey::Escape) = event.logical_key {
                        // Stop capture via destination window
                        self.destination_ui.request_stop();
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.update_destination();
            }
            _ => {}
        }
    }
    
    fn handle_border_event(&mut self, event: &WindowEvent) {
        // On Windows, border events are handled by Win32 subclass (WM_NCHITTEST)
        // Only handle keyboard events here
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                use winit::keyboard::{Key, NamedKey};
                if event.state.is_pressed() {
                    if let Key::Named(NamedKey::Escape) = event.logical_key {
                        // Stop capture via border window
                        self.destination_ui.request_stop();
                        info!("Capture stop requested via border window ESC");
                    }
                }
            }
            WindowEvent::Resized(size) => {
                // Sync destination window size when border is resized
                if let Some(dest_window) = &self.destination_window {
                    let border_offset = 10;
                    let inner_width = size.width.saturating_sub(border_offset * 2);
                    let inner_height = size.height.saturating_sub(border_offset * 2);
                    if inner_width > 0 && inner_height > 0 {
                        let _ = dest_window.request_inner_size(
                            winit::dpi::PhysicalSize::new(inner_width, inner_height)
                        );
                    }
                }
                
                // On non-Windows, resize the renderer
                #[cfg(not(windows))]
                if let Some(renderer) = &mut self.border_renderer {
                    renderer.resize(size.width, size.height);
                }
            }
            _ => {
                // On non-Windows, pass events to egui
                #[cfg(not(windows))]
                if let Some(renderer) = &mut self.border_renderer {
                    renderer.handle_event(event);
                }
            }
        }
    }
    
    /// Resize border window based on drag delta and corner/edge
    fn resize_border_window(&mut self, delta_x: f32, delta_y: f32, from_corner: ResizeCorner) {
        if let Some(window) = &self.border_window {
            let scale = window.scale_factor() as f32;
            // Convert logical delta (egui points) to physical pixels
            let dx = (delta_x * scale).round() as i32;
            let dy = (delta_y * scale).round() as i32;

            let current_pos = window.outer_position().unwrap_or(winit::dpi::PhysicalPosition::new(0, 0));
            let current_size = window.outer_size();
            
            let mut new_x = current_pos.x;
            let mut new_y = current_pos.y;
            let mut new_width = current_size.width as i32;
            let mut new_height = current_size.height as i32;
            
            match from_corner {
                ResizeCorner::TopLeft => {
                    // Resize from top-left: move position, adjust size
                    new_x += dx;
                    new_y += dy;
                    new_width -= dx;
                    new_height -= dy;
                },
                ResizeCorner::TopRight => {
                    // Resize from top-right: move only y, adjust size
                    new_y += dy;
                    new_width += dx;
                    new_height -= dy;
                },
                ResizeCorner::BottomLeft => {
                    // Resize from bottom-left: move only x, adjust size
                    new_x += dx;
                    new_width -= dx;
                    new_height += dy;
                },
                ResizeCorner::BottomRight => {
                    // Resize from bottom-right: just grow/shrink
                    new_width += dx;
                    new_height += dy;
                },
                ResizeCorner::TopEdge => {
                    new_y += dy;
                    new_height -= dy;
                },
                ResizeCorner::BottomEdge => {
                    new_height += dy;
                },
                ResizeCorner::LeftEdge => {
                    new_x += dx;
                    new_width -= dx;
                },
                ResizeCorner::RightEdge => {
                    new_width += dx;
                },
            }
            
            // Ensure minimum size and adjust position if needed
            if new_width < 100 {
                match from_corner {
                    ResizeCorner::TopLeft | ResizeCorner::BottomLeft | ResizeCorner::LeftEdge => {
                        new_x -= 100 - new_width;
                    },
                    _ => {}
                }
                new_width = 100;
            }
            
            if new_height < 100 {
                match from_corner {
                    ResizeCorner::TopLeft | ResizeCorner::TopRight | ResizeCorner::TopEdge => {
                        new_y -= 100 - new_height;
                    },
                    _ => {}
                }
                new_height = 100;
            }
            
            // Use winit native resize for proper surface update
            if new_x != current_pos.x || new_y != current_pos.y {
                let _ = window.set_outer_position(winit::dpi::PhysicalPosition::new(new_x, new_y));
            }
            if new_width != current_size.width as i32 || new_height != current_size.height as i32 {
                let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(new_width as u32, new_height as u32));
            }
            
            // Don't log every resize - too noisy
        }
    }
    
    /// Move border window by delta
    fn move_border_window(&mut self, delta_x: f32, delta_y: f32) {
        if let Some(window) = &self.border_window {
            let scale = window.scale_factor() as f32;
            let dx = (delta_x * scale).round() as i32;
            let dy = (delta_y * scale).round() as i32;

            let current_pos = window.outer_position().unwrap_or(winit::dpi::PhysicalPosition::new(0, 0));
            let current_size = window.outer_size();
            
            let new_x = current_pos.x + dx;
            let new_y = current_pos.y + dy;
            
            // Use winit native move
            let _ = window.set_outer_position(winit::dpi::PhysicalPosition::new(new_x, new_y));
        }
    }
    
    /// Called when border drag operation ends (mouse released)
    /// Updates destination window size
    fn on_border_drag_ended(&mut self) {
        if let Some(window) = &self.border_window {
            let size = window.outer_size();
            let pos = window.outer_position().unwrap_or(winit::dpi::PhysicalPosition::new(0, 0));
            
            // Resize destination window to match border (inner content size)
            if let Some(dest_window) = &self.destination_window {
                // Destination should match capture area (inside border)
                let border_offset = 8; // Match border width
                let inner_width = size.width.saturating_sub(border_offset * 2);
                let inner_height = size.height.saturating_sub(border_offset * 2);
                if inner_width > 0 && inner_height > 0 {
                    let _ = dest_window.request_inner_size(
                        winit::dpi::PhysicalSize::new(inner_width, inner_height)
                    );
                }
            }
            
            info!("Border drag ended - synced region and destination: pos=({}, {}), size={}x{}", 
                  pos.x, pos.y, size.width, size.height);
        }
    }
    
    /// Stop capture and restore overlay
    fn stop_capture(&mut self) {
        if let Some(engine) = &mut self.capture_engine {
            engine.stop();
        }
        self.state.stop_capture();
        self.destination_ui.set_capturing(false);
        
        // Uninstall mouse hook
        #[cfg(target_os = "windows")]
        platform::windows::uninstall_mouse_hook();
        
        // Clear click highlights
        self.destination_ui.clear_click_highlights();
        
        // Destroy border window
        self.destroy_border_window();
        
        // Show overlay window again
        if let Some(window) = &self.overlay_window {
            window.set_visible(true);
        }
        
        info!("Capture stopped");
    }
    
    fn request_redraw(&self) {
        if let Some(window) = &self.overlay_window {
            window.request_redraw();
        }
        if let Some(window) = &self.destination_window {
            window.request_redraw();
        }
    }
}

impl ApplicationHandler<UserEvent> for RustFrameApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        info!("Application resumed");
        
        // Initialize system tray (placeholder for now)
        info!("System tray will be implemented later");
        
        // Create windows on first resume
        if self.overlay_window.is_none() {
            if let Err(e) = self.create_overlay_window(event_loop) {
                error!("Failed to create overlay window: {}", e);
                event_loop.exit();
                return;
            }
        }
        
        if self.destination_window.is_none() {
            if let Err(e) = self.create_destination_window(event_loop) {
                error!("Failed to create destination window: {}", e);
                event_loop.exit();
                return;
            }
        }
    }
    
    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        let is_overlay = self.overlay_window
            .as_ref()
            .map(|w| w.id() == window_id)
            .unwrap_or(false);
        
        let is_destination = self.destination_window
            .as_ref()
            .map(|w| w.id() == window_id)
            .unwrap_or(false);
        
        let is_border = self.border_window
            .as_ref()
            .map(|w| w.id() == window_id)
            .unwrap_or(false);
        
        if is_overlay {
            self.handle_overlay_event(&event, event_loop);
        } else if is_destination {
            self.handle_destination_event(&event, event_loop);
        } else if is_border {
            self.handle_border_event(&event);
        }
    }
    
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Create pending border window if needed
        if let Some((x, y, width, height)) = self.pending_border_create.take() {
            if let Err(e) = self.create_border_window(event_loop, x, y, width, height) {
                error!("Failed to create border window: {}", e);
            }
        }
        
        // Directly update windows for continuous rendering
        // This ensures rendering continues even when window loses focus
        self.update_overlay();
        self.update_border();
        self.update_destination();
    }
    
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        info!("Received user event: {:?}", event);
        
        match event {
            UserEvent::ToggleCapture => {
                if self.state.mode == AppMode::Capturing {
                    self.stop_capture();
                } else {
                    info!("Toggle capture requested - would need to show overlay first");
                    // TODO: Show overlay to let user select area
                }
            },
            UserEvent::ToggleOverlay => {
                if let Some(window) = &self.overlay_window {
                    window.set_visible(!window.is_visible().unwrap_or(false));
                }
            },
            UserEvent::ShowSettings => {
                info!("Settings dialog requested");
                // TODO: Implement settings dialog
            },
            UserEvent::ShowAbout => {
                info!("About dialog requested"); 
                // TODO: Implement about dialog
            },
            UserEvent::Exit => {
                info!("Exit requested from tray");
                event_loop.exit();
            },
            UserEvent::ResizeBorderWindow { delta_x, delta_y, from_corner } => {
                // On Windows, resize is handled by native Win32 WM_NCHITTEST
                // This is only used on non-Windows platforms
                #[cfg(not(windows))]
                self.resize_border_window(delta_x, delta_y, from_corner);
                #[cfg(windows)]
                let _ = (delta_x, delta_y, from_corner); // Suppress warnings
            },
            UserEvent::MoveBorderWindow { delta_x, delta_y } => {
                // On Windows, move is handled by native Win32 WM_NCHITTEST (HTCAPTION)
                #[cfg(not(windows))]
                self.move_border_window(delta_x, delta_y);
                #[cfg(windows)]
                let _ = (delta_x, delta_y); // Suppress warnings
            },
            UserEvent::BorderDragEnded => {
                // Drag operation ended - sync destination window
                // On Windows, this is triggered by WM_EXITSIZEMOVE if needed
                // For now, we sync on every resize event
            },
            UserEvent::BorderResized { x, y, width, height } => {
                // Border window was resized/moved by native Win32
                // Update capture region and destination window
                let border_width = 4i32;
                let capture_region = capture::CaptureRect {
                    x: x + border_width,
                    y: y + border_width,
                    width: width.saturating_sub((border_width * 2) as u32),
                    height: height.saturating_sub((border_width * 2) as u32),
                };
                
                // Update capture engine region
                if let Some(engine) = &mut self.capture_engine {
                    if let Err(e) = engine.update_region(capture_region.clone()) {
                        error!("Failed to update capture region: {}", e);
                    }
                }
                
                // Update destination window size to match
                if let Some(dest_window) = &self.destination_window {
                    let _ = dest_window.request_inner_size(PhysicalSize::new(
                        capture_region.width,
                        capture_region.height,
                    ));
                }
                
                info!("Border resized to {}x{} at ({}, {})", width, height, x, y);
            },
        }
    }
}

fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .filter_module("wgpu", LevelFilter::Warn)
        .filter_module("naga", LevelFilter::Warn)
        .init();
    
    info!("Starting RustFrame");
    
    // Create event loop with UserEvent support
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .context("Failed to create event loop")?;
    // Use Poll for continuous updates during capture
    event_loop.set_control_flow(ControlFlow::Poll);
    
    // Create event proxy before creating app
    let event_proxy = event_loop.create_proxy();
    
    // Create and run application
    let mut app = RustFrameApp::new(event_proxy);
    event_loop.run_app(&mut app)
        .context("Event loop error")?;
    
    info!("RustFrame exited");
    Ok(())
}
