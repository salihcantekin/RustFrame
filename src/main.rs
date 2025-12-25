// main.rs - RustFrame Application Entry Point
//
// This is the orchestrator for the entire application. It manages:
// 1. Window creation (overlay selector + destination/mirror window)
// 2. Event loop handling (mouse/keyboard input)
// 3. Coordination between capture and rendering subsystems
// 4. System tray icon with context menu

// Hide console window in release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Result;
use log::{info, error};
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

// Tray icon and menu
use tray_icon::{TrayIconBuilder, TrayIcon, Icon};
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, CheckMenuItem};

mod bitmap_font;
mod capture;
mod constants;
mod renderer;
mod settings_dialog;
mod utils;
mod window_manager;

use window_manager::{OverlayWindow, DestinationWindow};
use capture::{CaptureEngine, CaptureSettings};
use renderer::Renderer;

/// Menu item IDs for tray icon context menu
mod menu_ids {
    pub const TOGGLE_CURSOR: &str = "toggle_cursor";
    pub const TOGGLE_BORDER: &str = "toggle_border";
    pub const TOGGLE_EXCLUDE: &str = "toggle_exclude";
    pub const SETTINGS: &str = "settings";
    pub const EXIT: &str = "exit";
}

/// Main application state
/// This holds all the windows, capture engine, and renderers
struct RustFrameApp {
    /// The transparent overlay window used for region selection
    overlay_window: Option<OverlayWindow>,

    /// The destination window that displays the captured content
    destination_window: Option<DestinationWindow>,

    /// The Windows.Graphics.Capture engine
    capture_engine: Option<CaptureEngine>,

    /// Renderer for the destination window
    renderer: Option<Renderer>,

    /// Capture settings (cursor, border, etc.)
    settings: CaptureSettings,

    /// Track if we're in "selection mode" or "capture mode"
    is_selecting: bool,

    /// Track if the overlay window is being dragged/moved
    is_dragging: bool,

    /// Last mouse position during drag (for calculating delta)
    last_mouse_pos: Option<(f64, f64)>,

    /// System tray icon
    tray_icon: Option<TrayIcon>,
    
    /// Menu items for updating check state
    menu_cursor: Option<CheckMenuItem>,
    menu_border: Option<CheckMenuItem>,
    menu_exclude: Option<CheckMenuItem>,
    
    /// Development mode flag (shows extra options)
    dev_mode: bool,
    
    /// Startup time - used to ignore Enter key for first 500ms
    startup_time: Instant,
}

impl RustFrameApp {
    fn new(dev_mode: bool) -> Self {
        let settings = if dev_mode {
            info!("Starting in DEVELOPMENT mode (destination window visible)");
            CaptureSettings::for_development()
        } else {
            info!("Starting in PRODUCTION mode (destination hidden)");
            CaptureSettings::default()
        };
        
        Self {
            overlay_window: None,
            destination_window: None,
            capture_engine: None,
            renderer: None,
            settings,
            is_selecting: true,
            is_dragging: false,
            last_mouse_pos: None,
            tray_icon: None,
            menu_cursor: None,
            menu_border: None,
            menu_exclude: None,
            dev_mode,
            startup_time: Instant::now(),
        }
    }
    
    /// Create and show the system tray icon with context menu
    fn create_tray_icon(&mut self) {
        // Create menu items
        let menu_cursor = CheckMenuItem::with_id(
            menu_ids::TOGGLE_CURSOR,
            "Show Cursor",
            true,
            self.settings.show_cursor,
            None,
        );
        let menu_border = CheckMenuItem::with_id(
            menu_ids::TOGGLE_BORDER,
            "Show Border",
            true,
            self.settings.show_border,
            None,
        );
        
        // Production mode option only visible in dev mode
        let menu_exclude = if self.dev_mode {
            Some(CheckMenuItem::with_id(
                menu_ids::TOGGLE_EXCLUDE,
                "Production Mode (Single Window)",
                true,
                self.settings.exclude_from_capture,
                None,
            ))
        } else {
            None
        };
        
        let menu_settings = MenuItem::with_id(menu_ids::SETTINGS, "Settings...", true, None);
        let menu_exit = MenuItem::with_id(menu_ids::EXIT, "Exit", true, None);
        
        // Build the menu
        let menu = Menu::new();
        let _ = menu.append(&menu_cursor);
        let _ = menu.append(&menu_border);
        
        // Only add production mode option in dev mode
        if let Some(ref exclude_item) = menu_exclude {
            let _ = menu.append(exclude_item);
        }
        
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&menu_settings);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&menu_exit);
        
        // Store menu items for later updates
        self.menu_cursor = Some(menu_cursor);
        self.menu_border = Some(menu_border);
        self.menu_exclude = menu_exclude;
        
        // Create a simple icon (16x16 blue square)
        let icon_rgba = create_default_icon();
        let icon = Icon::from_rgba(icon_rgba, 16, 16).expect("Failed to create icon");
        
        // Build tray icon
        match TrayIconBuilder::new()
            .with_tooltip("RustFrame - Screen Capture")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .build()
        {
            Ok(tray) => {
                info!("Tray icon created successfully");
                self.tray_icon = Some(tray);
            }
            Err(e) => {
                error!("Failed to create tray icon: {}", e);
            }
        }
    }
    
    /// Handle tray menu events
    fn handle_menu_event(&mut self, event: &MenuEvent) {
        match event.id().as_ref() {
            id if id == menu_ids::TOGGLE_CURSOR => {
                self.settings.show_cursor = !self.settings.show_cursor;
                if let Some(menu) = &self.menu_cursor {
                    menu.set_checked(self.settings.show_cursor);
                }
                info!("Cursor visibility: {}", self.settings.show_cursor);
                self.update_overlay_title();
                
                // Update capture engine cursor visibility if active
                if !self.is_selecting {
                    if let Some(capture) = &self.capture_engine {
                        if let Err(e) = capture.update_cursor_visibility(self.settings.show_cursor) {
                            error!("Failed to update cursor visibility: {}", e);
                        }
                    }
                }
            }
            id if id == menu_ids::TOGGLE_BORDER => {
                self.settings.show_border = !self.settings.show_border;
                if let Some(menu) = &self.menu_border {
                    menu.set_checked(self.settings.show_border);
                }
                info!("Border visibility: {}", self.settings.show_border);
                self.update_overlay_title();
                
                // Toggle hollow frame if capture is active
                if !self.is_selecting {
                    if let Some(overlay) = &self.overlay_window {
                        if self.settings.show_border {
                            overlay.make_hollow_frame(self.settings.border_width);
                            overlay.show();
                        } else {
                            overlay.hide();
                        }
                    }
                }
            }
            id if id == menu_ids::TOGGLE_EXCLUDE => {
                self.settings.exclude_from_capture = !self.settings.exclude_from_capture;
                if let Some(menu) = &self.menu_exclude {
                    menu.set_checked(self.settings.exclude_from_capture);
                }
                info!("Production mode (dest behind overlay): {}", self.settings.exclude_from_capture);
                self.update_overlay_title();
                
                // Reposition destination window if capture is active
                if !self.is_selecting {
                    if let (Some(overlay), Some(dest)) = (&self.overlay_window, &self.destination_window) {
                        let overlay_pos = overlay.get_outer_position();
                        let size = overlay.get_inner_size();
                        
                        if self.settings.exclude_from_capture {
                            dest.position_offscreen(size);
                        } else {
                            dest.position_beside_overlay(overlay_pos, size);
                        }
                    }
                }
            }
            id if id == menu_ids::SETTINGS => {
                self.show_settings_dialog();
            }
            id if id == menu_ids::EXIT => {
                info!("Exit requested from tray menu");
                std::process::exit(0);
            }
            _ => {}
        }
    }
}

/// Create a simple 16x16 icon (blue square with frame border)
fn create_default_icon() -> Vec<u8> {
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for y in 0..16 {
        for x in 0..16 {
            // Create a simple frame icon
            let is_border = x == 0 || x == 15 || y == 0 || y == 15;
            let is_inner_border = x == 1 || x == 14 || y == 1 || y == 14;
            
            if is_border {
                // Dark blue border
                rgba.extend_from_slice(&[0, 100, 180, 255]);
            } else if is_inner_border {
                // Light blue inner border
                rgba.extend_from_slice(&[50, 150, 220, 255]);
            } else {
                // Transparent center (like hollow frame)
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    rgba
}

impl ApplicationHandler for RustFrameApp {
    /// Called when the application is resumed (Windows-specific lifecycle)
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        info!("Application resumed");

        // Create the overlay window first (for region selection)
        if self.overlay_window.is_none() {
            match OverlayWindow::new(event_loop) {
                Ok(overlay) => {
                    info!("Overlay window created successfully");
                    self.overlay_window = Some(overlay);
                    // Set initial title with settings info
                    self.update_overlay_title();
                }
                Err(e) => {
                    error!("Failed to create overlay window: {}", e);
                }
            }
        }

        // Create the destination window
        if self.destination_window.is_none() {
            match DestinationWindow::new(event_loop, self.dev_mode) {
                Ok(dest) => {
                    info!("Destination window created successfully");
                    self.destination_window = Some(dest);
                }
                Err(e) => {
                    error!("Failed to create destination window: {}", e);
                }
            }
        }
        
        // Create tray icon
        if self.tray_icon.is_none() {
            self.create_tray_icon();
        }
    }

    /// Called when the event loop is about to block waiting for events
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Check for tray menu events
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            self.handle_menu_event(&event);
        }
        
        // During selection mode, just wait for user input
        if self.is_selecting {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }

        // Capture is active - use Poll for continuous rendering
        event_loop.set_control_flow(ControlFlow::Poll);

        if let (Some(renderer), Some(capture)) = (&mut self.renderer, &mut self.capture_engine) {
            if let Err(e) = renderer.render(capture) {
                error!("Render error in about_to_wait: {}", e);
            }
        }
    }

    /// Main event dispatcher - routes events to appropriate windows
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // Route events to the correct window
        match event {
            WindowEvent::CloseRequested => {
                info!("Close requested, shutting down");
                event_loop.exit();
            }

            WindowEvent::RedrawRequested => {
                // Handle redraw for overlay during selection
                if self.is_selecting {
                    if let Some(overlay) = &self.overlay_window {
                        if overlay.window_id() == window_id {
                            if let Err(e) = overlay.redraw_selection_overlay() {
                                error!("Overlay redraw failed: {}", e);
                            }
                        }
                    }
                }
                
                // Handle redraw for destination during capture
                if !self.is_selecting {
                    if let Some(dest) = &self.destination_window {
                        if dest.window_id() == window_id {
                            if let (Some(renderer), Some(capture)) = (&mut self.renderer, &mut self.capture_engine) {
                                if let Err(e) = renderer.render(capture) {
                                    error!("Render error: {}", e);
                                }
                            }
                        }
                    }
                }
            }

            WindowEvent::Resized(new_size) => {
                info!("Window {:?} resized to {:?}", window_id, new_size);

                // If overlay window is resized during selection, redraw the overlay
                if let Some(overlay) = &self.overlay_window {
                    if overlay.window_id() == window_id && self.is_selecting {
                        if let Err(e) = overlay.redraw_selection_overlay() {
                            error!("Failed to redraw selection overlay: {}", e);
                        }
                    }
                }

                // If overlay window is resized, update hollow frame, capture region, and destination
                if let Some(overlay) = &self.overlay_window {
                    if overlay.window_id() == window_id && !self.is_selecting {
                        // Update the hollow frame region
                        if self.settings.show_border {
                            overlay.update_hollow_frame(self.settings.border_width);
                        }
                        
                        // Update capture region (inside border if border is shown)
                        if let Some(capture) = &mut self.capture_engine {
                            let rect = if self.settings.show_border {
                                overlay.get_capture_rect_inner(self.settings.border_width)
                            } else {
                                overlay.get_capture_rect()
                            };
                            if let Err(e) = capture.update_region(rect) {
                                error!("Failed to update capture region: {}", e);
                            }
                        }
                        
                        // Resize destination window to match (minus border if shown)
                        if let Some(dest) = &self.destination_window {
                            let inner_size = if self.settings.show_border {
                                PhysicalSize::new(
                                    new_size.width.saturating_sub(self.settings.border_width * 2),
                                    new_size.height.saturating_sub(self.settings.border_width * 2)
                                )
                            } else {
                                new_size
                            };
                            dest.resize(inner_size);
                            if let Some(renderer) = &mut self.renderer {
                                renderer.resize(inner_size.width, inner_size.height);
                            }
                        }
                    }
                }

                // If destination window is resized (by user), resize renderer
                if let Some(dest) = &self.destination_window {
                    if dest.window_id() == window_id {
                        if let Some(renderer) = &mut self.renderer {
                            renderer.resize(new_size.width, new_size.height);
                        }
                    }
                }
            }

            WindowEvent::Moved(new_position) => {
                // If overlay window is moved during capture, update capture region
                if let Some(overlay) = &self.overlay_window {
                    if overlay.window_id() == window_id && !self.is_selecting {
                        // Update capture region with new position (inside border if shown)
                        if let Some(capture) = &mut self.capture_engine {
                            let rect = if self.settings.show_border {
                                overlay.get_capture_rect_inner(self.settings.border_width)
                            } else {
                                overlay.get_capture_rect()
                            };
                            if let Err(e) = capture.update_region(rect) {
                                error!("Failed to update capture region after move: {}", e);
                            }
                        }
                        info!("Overlay moved to {:?}, capture region updated", new_position);
                    }
                }
            }

            WindowEvent::KeyboardInput { event, .. } => {
                // Only handle key press events (not release)
                if event.state == winit::event::ElementState::Pressed {
                    use winit::keyboard::{PhysicalKey, KeyCode};
                    use std::time::Duration;
                    
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Escape) => {
                            info!("ESC pressed, exiting");
                            event_loop.exit();
                        }
                        PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::NumpadEnter) if self.is_selecting => {
                            // Ignore Enter for first 500ms after startup
                            // This prevents accidental capture when launching with Enter key
                            if self.startup_time.elapsed() < Duration::from_millis(500) {
                                info!("Enter ignored (startup cooldown)");
                            } else {
                                info!("Region selection confirmed, starting capture");
                                self.start_capture();
                            }
                        }
                        // Settings shortcuts (only during selection mode)
                        PhysicalKey::Code(KeyCode::KeyC) if self.is_selecting => {
                            self.settings.show_cursor = !self.settings.show_cursor;
                            info!("Cursor visibility: {}", self.settings.show_cursor);
                            self.update_overlay_title();
                        }
                        PhysicalKey::Code(KeyCode::KeyB) if self.is_selecting => {
                            self.settings.show_border = !self.settings.show_border;
                            info!("Border visibility: {}", self.settings.show_border);
                            self.update_overlay_title();
                        }
                        PhysicalKey::Code(KeyCode::KeyE) if self.is_selecting => {
                            self.settings.exclude_from_capture = !self.settings.exclude_from_capture;
                            info!("Exclude from capture: {}", self.settings.exclude_from_capture);
                            self.update_overlay_title();
                        }
                        PhysicalKey::Code(KeyCode::KeyS) if self.is_selecting => {
                            self.show_settings_dialog();
                        }
                        _ => {}
                    }
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                // Handle mouse clicks for dragging the overlay window
                if self.is_selecting {
                    if let Some(overlay) = &self.overlay_window {
                        if overlay.window_id() == window_id {
                            use winit::event::{ElementState, MouseButton};

                            match (button, state) {
                                (MouseButton::Left, ElementState::Pressed) => {
                                    self.is_dragging = true;
                                }
                                (MouseButton::Left, ElementState::Released) => {
                                    self.is_dragging = false;
                                    self.last_mouse_pos = None;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                // Handle mouse movement for dragging
                if self.is_selecting && self.is_dragging {
                    if let Some(overlay) = &mut self.overlay_window {
                        if overlay.window_id() == window_id {
                            if let Some((last_x, last_y)) = self.last_mouse_pos {
                                let delta_x = position.x - last_x;
                                let delta_y = position.y - last_y;
                                overlay.move_by(delta_x as i32, delta_y as i32);
                            }
                            self.last_mouse_pos = Some((position.x, position.y));
                        }
                    }
                }
            }

            _ => {}
        }
    }
}

impl RustFrameApp {
    /// Transition from "selection mode" to "capture mode"
    fn start_capture(&mut self) {
        if let Some(overlay) = &self.overlay_window {
            let overlay_position = overlay.get_outer_position();
            let full_size = overlay.get_inner_size();
            
            // Calculate capture rect - if border is shown, capture INSIDE the border
            let (rect, inner_size) = if self.settings.show_border {
                let r = overlay.get_capture_rect_inner(self.settings.border_width);
                let s = PhysicalSize::new(
                    full_size.width.saturating_sub(self.settings.border_width * 2),
                    full_size.height.saturating_sub(self.settings.border_width * 2)
                );
                (r, s)
            } else {
                (overlay.get_capture_rect(), full_size)
            };
            
            info!("Starting capture for region: {:?}", rect);

            // Convert overlay to hollow frame (click-through interior)
            if self.settings.show_border {
                overlay.make_hollow_frame(self.settings.border_width);
            } else {
                overlay.hide();
            }

            // Position destination window based on mode
            // Use inner_size (without border) for destination
            if let Some(dest) = &self.destination_window {
                dest.set_title("RustFrame Casting - Share THIS window in Google Meet");
                
                // Position based on mode:
                // - exclude_from_capture=true (prod mode): off-screen, user doesn't see it
                // - exclude_from_capture=false (dev mode): beside overlay, both visible
                if self.settings.exclude_from_capture {
                    dest.position_offscreen(inner_size);
                } else {
                    dest.position_beside_overlay(overlay_position, inner_size);
                }
            }

            // Initialize Windows.Graphics.Capture engine with settings
            match CaptureEngine::new(rect, &self.settings) {
                Ok(engine) => {
                    info!("Capture engine initialized");
                    self.capture_engine = Some(engine);
                    self.is_selecting = false;

                    // Initialize renderer for destination window
                    if let Some(dest) = &self.destination_window {
                        match Renderer::new(dest.get_window()) {
                            Ok(renderer) => {
                                info!("Renderer initialized");
                                self.renderer = Some(renderer);
                            }
                            Err(e) => {
                                error!("Failed to initialize renderer: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to initialize capture engine: {}", e);
                }
            }
        }
    }

    /// Update overlay title to show current settings
    fn update_overlay_title(&self) {
        if let Some(overlay) = &self.overlay_window {
            let cursor = if self.settings.show_cursor { "ON" } else { "OFF" };
            let border = if self.settings.show_border { "ON" } else { "OFF" };
            // E = Production mode: destination window behind overlay (single window view)
            // OFF = Dev mode: two windows side by side
            // ON = Prod mode: destination hidden behind overlay
            let mode = if self.settings.exclude_from_capture { "PROD(single)" } else { "DEV(side-by-side)" };
            
            let title = format!(
                "RustFrame | [C]ursor:{} [B]order:{} [E]mode:{} [S]ettings | ENTER=Start ESC=Exit",
                cursor, border, mode
            );
            overlay.set_title(&title);
        }
    }
    
    /// Show the settings dialog and apply changes
    fn show_settings_dialog(&mut self) {
        info!("Opening settings dialog...");
        
        if let Some(new_settings) = settings_dialog::show_settings_dialog(&self.settings, self.dev_mode) {
            info!("Settings changed, applying...");
            
            // Update cursor menu checkbox
            if let Some(menu) = &self.menu_cursor {
                menu.set_checked(new_settings.show_cursor);
            }
            
            // Update border menu checkbox
            if let Some(menu) = &self.menu_border {
                menu.set_checked(new_settings.show_border);
            }
            
            // Update exclude/production mode menu checkbox
            if let Some(menu) = &self.menu_exclude {
                menu.set_checked(new_settings.exclude_from_capture);
            }
            
            // Store the old settings to detect changes
            let cursor_changed = self.settings.show_cursor != new_settings.show_cursor;
            let border_changed = self.settings.show_border != new_settings.show_border;
            let mode_changed = self.settings.exclude_from_capture != new_settings.exclude_from_capture;
            let border_width_changed = self.settings.border_width != new_settings.border_width;
            
            // Apply the new settings
            self.settings = new_settings;
            
            // Update overlay title
            self.update_overlay_title();
            
            // If capture is active, apply runtime changes
            if !self.is_selecting {
                // Handle cursor visibility change
                if cursor_changed {
                    if let Some(capture) = &self.capture_engine {
                        if let Err(e) = capture.update_cursor_visibility(self.settings.show_cursor) {
                            error!("Failed to update cursor visibility: {}", e);
                        }
                    }
                }
                
                // Handle border visibility change
                if border_changed {
                    if let Some(overlay) = &self.overlay_window {
                        if self.settings.show_border {
                            overlay.make_hollow_frame(self.settings.border_width);
                            overlay.show();
                        } else {
                            overlay.hide();
                        }
                    }
                }
                
                // Handle border width change
                if border_width_changed && self.settings.show_border {
                    if let Some(overlay) = &self.overlay_window {
                        overlay.update_hollow_frame(self.settings.border_width);
                    }
                    
                    // Update capture region
                    if let (Some(overlay), Some(capture)) = (&self.overlay_window, &mut self.capture_engine) {
                        let rect = overlay.get_capture_rect_inner(self.settings.border_width);
                        if let Err(e) = capture.update_region(rect) {
                            error!("Failed to update capture region: {}", e);
                        }
                    }
                }
                
                // Handle production mode change
                if mode_changed {
                    if let (Some(overlay), Some(dest)) = (&self.overlay_window, &self.destination_window) {
                        let overlay_pos = overlay.get_outer_position();
                        let size = overlay.get_inner_size();
                        
                        if self.settings.exclude_from_capture {
                            dest.position_offscreen(size);
                        } else {
                            dest.position_beside_overlay(overlay_pos, size);
                        }
                    }
                }
            }
        } else {
            info!("Settings dialog cancelled");
        }
    }
}

fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    info!("RustFrame starting...");
    info!("Using Windows.Graphics.Capture API (not GDI/BitBlt)");

    // Determine if we should run in development mode:
    // 1. Debug builds always run in DEV mode
    // 2. Release builds with --dev argument run in DEV mode
    // 3. Otherwise, run in PRODUCTION mode
    let args: Vec<String> = std::env::args().collect();
    let has_dev_flag = args.iter().any(|arg| arg == "--dev" || arg == "-d");
    
    #[cfg(debug_assertions)]
    let dev_mode = true; // Always DEV mode in debug builds
    
    #[cfg(not(debug_assertions))]
    let dev_mode = has_dev_flag; // Only DEV mode if --dev flag is passed
    
    if has_dev_flag && cfg!(not(debug_assertions)) {
        info!("--dev flag detected, forcing development mode");
    }

    // Create the winit event loop
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    // Create application state
    let mut app = RustFrameApp::new(dev_mode);

    // Run the event loop
    event_loop.run_app(&mut app)?;

    info!("RustFrame shutting down");
    Ok(())
}
