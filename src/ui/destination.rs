// ui/destination.rs - Destination Window UI
//
// This module handles the destination window that displays captured content.
// It renders the captured frames and any UI overlays.

use egui::{Color32, TextureHandle, TextureOptions, Vec2};
use log::info;

/// Destination window UI
pub struct DestinationUi {
    /// Handle to the captured frame texture
    frame_texture: Option<TextureHandle>,
    /// Current frame dimensions
    frame_size: (u32, u32),
    /// Show debug info overlay
    show_debug: bool,
    /// Frame counter
    frame_count: u64,
    /// Whether capture is active
    is_capturing: bool,
    /// Test pattern texture for demo
    test_texture: Option<TextureHandle>,
    /// User requested to stop capture
    stop_requested: bool,
}

impl Default for DestinationUi {
    fn default() -> Self {
        Self::new()
    }
}

impl DestinationUi {
    pub fn new() -> Self {
        Self {
            frame_texture: None,
            frame_size: (0, 0),
            show_debug: false,
            frame_count: 0,
            is_capturing: false,
            test_texture: None,
            stop_requested: false,
        }
    }
    
    /// Set capturing state
    pub fn set_capturing(&mut self, capturing: bool) {
        self.is_capturing = capturing;
        if capturing {
            self.frame_count = 0;
            self.stop_requested = false;
        }
    }
    
    /// Check if stop was requested and clear the flag
    pub fn take_stop_request(&mut self) -> bool {
        let requested = self.stop_requested;
        self.stop_requested = false;
        requested
    }
    
    /// Request stop from keyboard
    pub fn request_stop(&mut self) {
        if self.is_capturing {
            self.stop_requested = true;
        }
    }
    
    /// Create a test pattern texture
    fn create_test_pattern(&mut self, ctx: &egui::Context) {
        if self.test_texture.is_some() {
            return;
        }
        
        let width = 320;
        let height = 240;
        let mut pixels = vec![0u8; width * height * 4];
        
        // Create a colorful test pattern
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) * 4;
                
                // Color bars pattern
                let bar = x * 8 / width;
                let (r, g, b) = match bar {
                    0 => (255, 255, 255), // White
                    1 => (255, 255, 0),   // Yellow
                    2 => (0, 255, 255),   // Cyan
                    3 => (0, 255, 0),     // Green
                    4 => (255, 0, 255),   // Magenta
                    5 => (255, 0, 0),     // Red
                    6 => (0, 0, 255),     // Blue
                    _ => (0, 0, 0),       // Black
                };
                
                // Add some animation effect based on frame count
                let wave = ((x as f32 / 20.0 + self.frame_count as f32 / 10.0).sin() * 20.0) as i32;
                let r = (r as i32 + wave).clamp(0, 255) as u8;
                
                pixels[idx] = r;
                pixels[idx + 1] = g;
                pixels[idx + 2] = b;
                pixels[idx + 3] = 255;
            }
        }
        
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [width, height],
            &pixels,
        );
        
        self.test_texture = Some(ctx.load_texture(
            "test_pattern",
            image,
            TextureOptions::LINEAR,
        ));
        self.frame_size = (width as u32, height as u32);
    }
    
    /// Update the captured frame texture
    pub fn update_frame(&mut self, ctx: &egui::Context, data: &[u8], width: u32, height: u32) {
        self.frame_count += 1;
        self.frame_size = (width, height);
        
        info!("DestinationUi::update_frame called! Frame #{}, size: {}x{}, data len: {}", 
              self.frame_count, width, height, data.len());
        
        // Convert BGRA to RGBA for egui
        let rgba_data: Vec<u8> = data
            .chunks_exact(4)
            .flat_map(|bgra| [bgra[2], bgra[1], bgra[0], bgra[3]])
            .collect();
        
        // Create or update texture
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [width as usize, height as usize],
            &rgba_data,
        );
        
        if let Some(texture) = &mut self.frame_texture {
            // Update existing texture
            texture.set(image, TextureOptions::LINEAR);
        } else {
            // Create new texture
            self.frame_texture = Some(ctx.load_texture(
                "captured_frame",
                image,
                TextureOptions::LINEAR,
            ));
        }
    }
    
    /// Render the destination window UI
    pub fn show(&mut self, ctx: &egui::Context) {
        // If capturing but no real frame, create test pattern
        if self.is_capturing && self.frame_texture.is_none() {
            self.create_test_pattern(ctx);
            self.frame_count += 1;
        }
        
        // Use completely transparent frame with no margins
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE
                .fill(Color32::from_rgb(20, 20, 30))
                .inner_margin(0.0)
                .outer_margin(0.0))
            .show(ctx, |ui| {
                ui.style_mut().spacing.item_spacing = egui::vec2(0.0, 0.0);
                
                // Get available size
                let available = ui.available_size();
                
                // Use captured frame_texture if available, otherwise test_texture when capturing
                let texture = self.frame_texture.as_ref()
                    .or_else(|| if self.is_capturing { self.test_texture.as_ref() } else { None });
                
                if let Some(texture) = texture {
                    // Calculate scaling to fit window while maintaining aspect ratio
                    let tex_size = texture.size_vec2();
                    let scale = (available.x / tex_size.x).min(available.y / tex_size.y);
                    let scaled_size = tex_size * scale;
                    
                    // Center the image
                    let offset = (available - scaled_size) / 2.0;
                    ui.add_space(offset.y);
                    
                    ui.horizontal(|ui| {
                        ui.add_space(offset.x);
                        ui.image((texture.id(), scaled_size));
                    });
                    
                    // Show capture info with STOP button
                    if self.is_capturing {
                        egui::Area::new(egui::Id::new("capture_info"))
                            .anchor(egui::Align2::LEFT_TOP, [10.0, 10.0])
                            .show(ctx, |ui| {
                                egui::Frame::new()
                                    .fill(Color32::from_rgba_unmultiplied(0, 0, 0, 200))
                                    .corner_radius(egui::CornerRadius::same(8))
                                    .inner_margin(12.0)
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new("🔴 CAPTURING")
                                                    .size(14.0)
                                                    .color(Color32::from_rgb(255, 100, 100))
                                                    .strong()
                                            );
                                            ui.add_space(16.0);
                                            
                                            // Stop button
                                            if ui.add(egui::Button::new(
                                                egui::RichText::new("⏹ STOP")
                                                    .size(14.0)
                                                    .color(Color32::WHITE)
                                            ).fill(Color32::from_rgb(180, 40, 40))
                                            .corner_radius(egui::CornerRadius::same(4)))
                                            .clicked() {
                                                self.stop_requested = true;
                                            }
                                        });
                                        
                                        ui.add_space(4.0);
                                        ui.label(
                                            egui::RichText::new(format!("Frame: {} | Size: {}x{}", 
                                                self.frame_count, self.frame_size.0, self.frame_size.1))
                                                .size(11.0)
                                                .color(Color32::GRAY)
                                        );
                                        ui.label(
                                            egui::RichText::new("Press ESC to stop")
                                                .size(10.0)
                                                .color(Color32::DARK_GRAY)
                                        );
                                    });
                            });
                    }
                } else {
                    // No frame yet - show placeholder
                    ui.vertical_centered(|ui| {
                        ui.add_space(available.y / 2.0 - 40.0);
                        
                        ui.label(
                            egui::RichText::new("📺")
                                .size(48.0)
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("Waiting for capture...")
                                .size(18.0)
                                .color(Color32::GRAY)
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("Click the green button in Overlay to start")
                                .size(12.0)
                                .color(Color32::DARK_GRAY)
                        );
                    });
                }
                
                // Debug overlay
                if self.show_debug {
                    self.draw_debug_overlay(ui);
                }
            });
    }
    
    /// Draw debug information overlay
    fn draw_debug_overlay(&self, ui: &mut egui::Ui) {
        egui::Window::new("Debug")
            .anchor(egui::Align2::RIGHT_TOP, [-10.0, 10.0])
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                ui.label(format!("Frame: {}", self.frame_count));
                ui.label(format!("Size: {}x{}", self.frame_size.0, self.frame_size.1));
            });
    }
    
    /// Toggle debug overlay
    pub fn toggle_debug(&mut self) {
        self.show_debug = !self.show_debug;
    }
    
    /// Clear the current frame
    pub fn clear_frame(&mut self) {
        self.frame_texture = None;
        self.frame_size = (0, 0);
    }
}
