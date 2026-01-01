// ui/settings.rs - Settings Dialog
//
// Modal dialog for changing application settings.

use egui::{Color32, CornerRadius, Stroke};
use crate::app::{AppState, CaptureSettings};
use super::theme::RustFrameColors;

/// Settings dialog state
pub struct SettingsDialog {
    /// Whether the dialog is open
    pub is_open: bool,
    /// Temporary settings (applied on OK)
    temp_settings: CaptureSettings,
    /// Border width slider value
    border_width_value: f32,
}

impl Default for SettingsDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsDialog {
    pub fn new() -> Self {
        Self {
            is_open: false,
            temp_settings: CaptureSettings::default(),
            border_width_value: 3.0,
        }
    }
    
    /// Open the dialog with current settings
    pub fn open(&mut self, current_settings: &CaptureSettings) {
        self.temp_settings = current_settings.clone();
        self.border_width_value = current_settings.border_width as f32;
        self.is_open = true;
    }
    
    /// Close the dialog without saving
    pub fn close(&mut self) {
        self.is_open = false;
    }
    
    /// Render the settings dialog
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> SettingsResponse {
        let mut response = SettingsResponse::default();
        
        if !self.is_open {
            return response;
        }
        
        // Modal backdrop
        egui::Area::new(egui::Id::new("settings_backdrop"))
            .fixed_pos([0.0, 0.0])
            .order(egui::Order::Background)
            .show(ctx, |ui| {
                let screen = ctx.screen_rect();
                ui.painter().rect_filled(
                    screen,
                    CornerRadius::ZERO,
                    Color32::from_rgba_unmultiplied(0, 0, 0, 180),
                );
            });
        
        // Settings window
        egui::Window::new("⚙ Settings")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_size([380.0, 320.0])
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(RustFrameColors::BG_DARK)
                    .stroke(Stroke::new(2.0, RustFrameColors::BLUE))
            )
            .show(ctx, |ui| {
                ui.add_space(8.0);
                
                // Cursor visibility
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.temp_settings.show_cursor, "");
                    ui.label("Show mouse cursor in capture");
                });
                
                ui.add_space(4.0);
                
                // Border visibility
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.temp_settings.show_border, "");
                    ui.label("Show border frame during capture");
                });
                
                // Border width slider (only if border is enabled)
                if self.temp_settings.show_border {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add_space(24.0);
                        ui.label("Border width:");
                        ui.add(
                            egui::Slider::new(&mut self.border_width_value, 1.0..=20.0)
                                .integer()
                                .suffix(" px")
                        );
                    });
                    self.temp_settings.border_width = self.border_width_value as u32;
                }
                
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);
                
                // Production mode (dev mode only)
                if state.dev_mode {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.temp_settings.exclude_from_capture, "");
                        ui.vertical(|ui| {
                            ui.label("Production mode (exclude from capture)");
                            ui.label(
                                egui::RichText::new("When enabled, destination window is hidden behind overlay")
                                    .size(11.0)
                                    .color(RustFrameColors::TEXT_GRAY)
                            );
                        });
                    });
                    
                    ui.add_space(8.0);
                }
                
                // Info text
                egui::Frame::new()
                    .fill(Color32::from_rgb(40, 40, 40))
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("ℹ Tip: Right-click in the overlay for quick settings")
                                .size(12.0)
                                .color(RustFrameColors::TEXT_GRAY)
                        );
                    });
                
                ui.add_space(16.0);
                
                // Buttons
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Cancel button
                        if ui.button("Cancel").clicked() {
                            self.is_open = false;
                            response.cancelled = true;
                        }
                        
                        ui.add_space(8.0);
                        
                        // OK button
                        let ok_button = egui::Button::new(
                            egui::RichText::new("  OK  ").strong()
                        ).fill(RustFrameColors::BLUE);
                        
                        if ui.add(ok_button).clicked() {
                            // Apply settings
                            state.settings = self.temp_settings.clone();
                            self.is_open = false;
                            response.applied = true;
                        }
                    });
                });
            });
        
        // Handle ESC to close
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.is_open = false;
            response.cancelled = true;
        }
        
        response
    }
}

/// Response from settings dialog
#[derive(Default)]
pub struct SettingsResponse {
    /// Settings were applied
    pub applied: bool,
    /// Dialog was cancelled
    pub cancelled: bool,
}
