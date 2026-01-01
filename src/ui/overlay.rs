// ui/overlay.rs - Selection Overlay UI
//
// This module renders the overlay UI for region selection.
// It shows a translucent overlay with controls and visual feedback.

use egui::{Color32, CornerRadius, Pos2, Rect, Stroke, Vec2};
use crate::app::AppState;
use super::theme::{RustFrameColors, RustFrameTheme};

/// Overlay UI state and rendering
pub struct OverlayUi {
    /// Whether the play button is hovered
    play_button_hovered: bool,
}

impl Default for OverlayUi {
    fn default() -> Self {
        Self::new()
    }
}

impl OverlayUi {
    pub fn new() -> Self {
        Self {
            play_button_hovered: false,
        }
    }
    
    /// Render the overlay UI
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> OverlayResponse {
        let mut response = OverlayResponse::default();
        
        // Get screen rect
        let screen_rect = ctx.screen_rect();
        let is_capturing = state.is_capturing();
        
        // When capturing, only draw a thin border frame - rest is transparent
        if is_capturing {
            // Draw only the border frame - center is fully transparent
            self.draw_hollow_frame(ctx, screen_rect);
            return response;
        }
        
        // Draw border frame (selection mode)
        self.draw_border_frame(ctx, screen_rect, is_capturing);
        
        // Central panel with controls (only show in selection mode)
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                // Center the control panel
                ui.with_layout(
                    egui::Layout::centered_and_justified(egui::Direction::TopDown),
                    |ui| {
                        self.draw_control_panel(ui, state, &mut response);
                    }
                );
            });
        
        response
    }
    
    /// Draw a hollow frame for capture mode - only border visible, center transparent
    fn draw_hollow_frame(&self, ctx: &egui::Context, rect: Rect) {
        let painter = ctx.layer_painter(egui::LayerId::background());
        let border_width = 3.0;
        let border_color = RustFrameColors::ORANGE_LIGHT;
        
        // Draw only the border stroke - no fill
        painter.rect_stroke(
            rect.shrink(border_width / 2.0),
            CornerRadius::ZERO,
            Stroke::new(border_width, border_color),
            egui::StrokeKind::Outside,
        );
    }
    
    /// Draw the border frame around the capture region
    fn draw_border_frame(&self, ctx: &egui::Context, rect: Rect, is_capturing: bool) {
        let painter = ctx.layer_painter(egui::LayerId::background());
        let border_color = RustFrameTheme::border_color(is_capturing);
        let border_width = 4.0;
        
        // Draw outer border using rect_stroke with 4 args
        painter.rect_stroke(
            rect.shrink(border_width / 2.0),
            CornerRadius::ZERO,
            Stroke::new(border_width, border_color),
            egui::StrokeKind::Outside,
        );
        
        // Draw corner markers
        let corner_size = 20.0;
        let corner_color = if is_capturing {
            RustFrameColors::ORANGE_LIGHT
        } else {
            RustFrameColors::BLUE_LIGHT
        };
        
        // Top-left corner
        self.draw_corner_marker(&painter, rect.left_top(), corner_size, corner_color, true, true);
        // Top-right corner
        self.draw_corner_marker(&painter, rect.right_top(), corner_size, corner_color, false, true);
        // Bottom-left corner
        self.draw_corner_marker(&painter, rect.left_bottom(), corner_size, corner_color, true, false);
        // Bottom-right corner
        self.draw_corner_marker(&painter, rect.right_bottom(), corner_size, corner_color, false, false);
    }
    
    /// Draw a corner marker (L-shaped)
    fn draw_corner_marker(
        &self,
        painter: &egui::Painter,
        pos: Pos2,
        size: f32,
        color: Color32,
        left: bool,
        top: bool,
    ) {
        let stroke = Stroke::new(3.0, color);
        let dx = if left { size } else { -size };
        let dy = if top { size } else { -size };
        
        // Horizontal line
        painter.line_segment([pos, Pos2::new(pos.x + dx, pos.y)], stroke);
        // Vertical line
        painter.line_segment([pos, Pos2::new(pos.x, pos.y + dy)], stroke);
    }
    
    /// Draw the central control panel
    fn draw_control_panel(&mut self, ui: &mut egui::Ui, state: &AppState, response: &mut OverlayResponse) {
        // Control panel background
        let panel_size = Vec2::new(300.0, 280.0);
        
        egui::Frame::new()
            .fill(RustFrameColors::BG_PANEL)
            .corner_radius(CornerRadius::same(8))
            .stroke(Stroke::new(2.0, RustFrameColors::BLUE))
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.set_min_size(panel_size);
                ui.vertical_centered(|ui| {
                    // Title
                    ui.label(
                        egui::RichText::new("🎬 RustFrame")
                            .size(24.0)
                            .color(RustFrameColors::BLUE_LIGHT)
                            .strong()
                    );
                    
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);
                    
                    // Instructions
                    ui.label(
                        egui::RichText::new("Select a screen region to capture")
                            .size(14.0)
                            .color(RustFrameColors::TEXT_GRAY)
                    );
                    
                    ui.add_space(12.0);
                    
                    // Settings status
                    ui.horizontal(|ui| {
                        Self::setting_badge(ui, "Cursor", state.settings.show_cursor);
                        Self::setting_badge(ui, "Border", state.settings.show_border);
                    });
                    
                    if state.dev_mode {
                        ui.horizontal(|ui| {
                            Self::setting_badge(ui, "Prod Mode", state.settings.exclude_from_capture);
                            ui.label(
                                egui::RichText::new("DEV")
                                    .size(10.0)
                                    .color(RustFrameColors::TEXT_YELLOW)
                                    .strong()
                            );
                        });
                    }
                    
                    ui.add_space(16.0);
                    
                    // Play button
                    let button_size = Vec2::new(64.0, 64.0);
                    let play_button = ui.add_sized(
                        button_size,
                        egui::Button::new(
                            egui::RichText::new("▶")
                                .size(32.0)
                                .color(Color32::WHITE)
                        )
                        .fill(RustFrameColors::PLAY_GREEN)
                        .corner_radius(CornerRadius::same(32))
                    );
                    
                    self.play_button_hovered = play_button.hovered();
                    
                    if play_button.clicked() {
                        response.start_capture = true;
                    }
                    
                    ui.add_space(8.0);
                    
                    // Keyboard shortcuts hint
                    ui.label(
                        egui::RichText::new("Enter to start • ESC to cancel")
                            .size(11.0)
                            .color(RustFrameColors::TEXT_GRAY)
                    );
                    ui.label(
                        egui::RichText::new("Right-click for options")
                            .size(11.0)
                            .color(RustFrameColors::TEXT_GRAY)
                    );
                });
            });
    }
    
    /// Draw a setting badge (ON/OFF indicator)
    fn setting_badge(ui: &mut egui::Ui, label: &str, enabled: bool) {
        let (bg_color, text_color) = if enabled {
            (Color32::from_rgb(0, 80, 0), RustFrameColors::TEXT_GREEN)
        } else {
            (Color32::from_rgb(80, 0, 0), RustFrameColors::TEXT_RED)
        };
        
        egui::Frame::new()
            .fill(bg_color)
            .corner_radius(CornerRadius::same(4))
            .inner_margin(egui::Margin::symmetric(8, 2))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(label)
                        .size(11.0)
                        .color(text_color)
                );
            });
    }
}

/// Response from overlay UI interaction
#[derive(Default)]
pub struct OverlayResponse {
    /// User requested to start capture
    pub start_capture: bool,
    /// User requested to cancel/exit
    pub cancel: bool,
    /// User requested settings dialog
    pub open_settings: bool,
}
