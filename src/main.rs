// main.rs - RustFrame Application Entry Point
//
// This is the orchestrator for the entire application. It manages:
// 1. Window creation (overlay selector + destination/mirror window)
// 2. Event loop handling (mouse/keyboard input)
// 3. Coordination between capture and rendering subsystems

use anyhow::Result;
use log::{info, error};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

mod capture;
mod window_manager;
mod renderer;

use window_manager::{OverlayWindow, DestinationWindow};
use capture::{CaptureEngine, CaptureSettings};
use renderer::Renderer;

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
}

impl RustFrameApp {
    fn new() -> Self {
        Self {
            overlay_window: None,
            destination_window: None,
            capture_engine: None,
            renderer: None,
            settings: CaptureSettings::default(),
            is_selecting: true,
            is_dragging: false,
            last_mouse_pos: None,
        }
    }
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
            match DestinationWindow::new(event_loop) {
                Ok(dest) => {
                    info!("Destination window created successfully");
                    self.destination_window = Some(dest);
                }
                Err(e) => {
                    error!("Failed to create destination window: {}", e);
                }
            }
        }
    }

    /// Called when the event loop is about to block waiting for events
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
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
                            if let Err(e) = overlay.draw_overlay() {
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

                // If overlay window is resized, update hollow frame and capture region
                if let Some(overlay) = &self.overlay_window {
                    if overlay.window_id() == window_id {
                        // If capturing with border, update the hollow frame region
                        if !self.is_selecting && self.settings.show_border {
                            overlay.update_hollow_frame(self.settings.border_width);
                            
                            // Also update capture region
                            if let Some(capture) = &mut self.capture_engine {
                                let rect = overlay.get_capture_rect();
                                if let Err(e) = capture.update_region(rect) {
                                    error!("Failed to update capture region: {}", e);
                                }
                            }
                        }
                    }
                }

                // If destination window is resized, resize renderer
                if let Some(dest) = &self.destination_window {
                    if dest.window_id() == window_id {
                        if let Some(renderer) = &mut self.renderer {
                            renderer.resize(new_size.width, new_size.height);
                        }
                    }
                }
            }

            WindowEvent::KeyboardInput { event, .. } => {
                // Only handle key press events (not release)
                if event.state == winit::event::ElementState::Pressed {
                    use winit::keyboard::{PhysicalKey, KeyCode};
                    
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Escape) => {
                            info!("ESC pressed, exiting");
                            event_loop.exit();
                        }
                        PhysicalKey::Code(KeyCode::Enter) if self.is_selecting => {
                            info!("Region selection confirmed, starting capture");
                            self.start_capture();
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
            let rect = overlay.get_capture_rect();
            let overlay_position = overlay.get_outer_position();
            let size = overlay.get_inner_size();
            info!("Starting capture for region: {:?}", rect);

            // Convert overlay to hollow frame (click-through interior)
            if self.settings.show_border {
                overlay.make_hollow_frame(self.settings.border_width);
            } else {
                overlay.hide();
            }

            // Position destination window NEXT TO the overlay (prevents infinite mirror)
            if let Some(dest) = &self.destination_window {
                dest.set_title("RustFrame Casting - Share THIS window in Google Meet");
                
                if let Err(e) = dest.set_exclude_from_capture(self.settings.exclude_from_capture) {
                    error!("Failed to set exclude_from_capture: {}", e);
                }
                
                // Calculate position for destination (to the right of overlay)
                let dest_position = PhysicalPosition::new(
                    overlay_position.x + size.width as i32 + 20,
                    overlay_position.y
                );
                dest.show_at(dest_position, size);
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
            let exclude = if self.settings.exclude_from_capture { "ON" } else { "OFF" };
            
            let title = format!(
                "RustFrame | [C]ursor:{} [B]order:{} [E]xclude:{} | ENTER=Start ESC=Exit",
                cursor, border, exclude
            );
            overlay.set_title(&title);
        }
    }
}

fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    info!("RustFrame starting...");
    info!("Using Windows.Graphics.Capture API (not GDI/BitBlt)");

    // Create the winit event loop
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    // Create application state
    let mut app = RustFrameApp::new();

    // Run the event loop
    event_loop.run_app(&mut app)?;

    info!("RustFrame shutting down");
    Ok(())
}
