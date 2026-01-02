// ui/overlay.rs - Selection Overlay UI
//
// This module renders the overlay UI for region selection.
// It shows a translucent overlay with controls and visual feedback.

use egui::{Color32, CornerRadius, Pos2, Rect, Stroke, Vec2};
use crate::app::AppState;
use super::theme::{RustFrameColors, RustFrameTheme};

/// Overlay UI state and rendering
pub struct OverlayUi;

impl Default for OverlayUi {
    fn default() -> Self {
        Self::new()
    }
}

impl OverlayUi {
    pub fn new() -> Self {
        Self
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
        
        // Draw status indicators at top-left
        self.draw_status_indicators(ctx, state);
        
        // Draw window size indicator at bottom
        self.draw_size_indicator(ctx, screen_rect);
        
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
    
    /// Draw status indicators at top-left
    fn draw_status_indicators(&self, ctx: &egui::Context, state: &AppState) {
        egui::Area::new(egui::Id::new("status_indicators"))
            .anchor(egui::Align2::LEFT_TOP, [12.0, 12.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
                    
                    // Cursor indicator
                    Self::status_pill(ui, "👆", "Cursor", state.settings.show_cursor);
                    
                    // Border indicator
                    Self::status_pill(ui, "🔲", "Border", state.settings.show_border);
                    
                    // Click highlight indicator
                    if state.settings.highlight_clicks {
                        Self::status_pill(ui, "🖱", "Clicks", true);
                    }
                    
                    // DEV mode indicator
                    if state.dev_mode {
                        Self::dev_badge(ui);
                    }
                });
            });
    }
    
    /// Draw a modern status pill indicator
    fn status_pill(ui: &mut egui::Ui, icon: &str, label: &str, enabled: bool) {
        let (bg_color, text_color, icon_color) = if enabled {
            (
                Color32::from_rgba_unmultiplied(40, 167, 69, 200),  // Green bg
                Color32::WHITE,
                Color32::WHITE,
            )
        } else {
            (
                Color32::from_rgba_unmultiplied(60, 60, 60, 180),  // Gray bg
                Color32::from_rgb(150, 150, 150),
                Color32::from_rgb(120, 120, 120),
            )
        };
        
        egui::Frame::new()
            .fill(bg_color)
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin::symmetric(10, 4))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                    ui.label(egui::RichText::new(icon).size(11.0).color(icon_color));
                    ui.label(egui::RichText::new(label).size(11.0).color(text_color).strong());
                });
            });
    }
    
    /// Draw DEV mode badge
    fn dev_badge(ui: &mut egui::Ui) {
        egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(255, 193, 7, 220))  // Yellow/amber
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin::symmetric(10, 4))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("⚡ DEV")
                        .size(11.0)
                        .color(Color32::BLACK)
                        .strong()
                );
            });
    }
    
    /// Draw window size indicator at bottom
    fn draw_size_indicator(&self, ctx: &egui::Context, rect: Rect) {
        egui::Area::new(egui::Id::new("size_indicator"))
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -12.0])
            .show(ctx, |ui| {
                ui.set_min_width(100.0);  // Prevent wrapping
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(0, 0, 0, 160))
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(egui::Margin::symmetric(12, 4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("{}×{}", rect.width() as i32, rect.height() as i32))
                                    .size(12.0)
                                    .color(Color32::from_rgb(180, 180, 180))
                                    .monospace()
                            );
                        });
                    });
            });
    }
    
    /// Draw the central control panel
    fn draw_control_panel(&mut self, ui: &mut egui::Ui, _state: &AppState, response: &mut OverlayResponse) {
        // Control panel background - more modern glass effect
        let panel_size = Vec2::new(280.0, 220.0);
        
        egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(20, 20, 25, 230))
            .corner_radius(CornerRadius::same(16))
            .stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 30)))
            .inner_margin(20.0)
            .shadow(egui::epaint::Shadow {
                offset: [0, 4],
                blur: 16,
                spread: 0,
                color: Color32::from_rgba_unmultiplied(0, 0, 0, 100),
            })
            .show(ui, |ui| {
                ui.set_min_size(panel_size);
                ui.vertical_centered(|ui| {
                    // Logo/Title
                    ui.label(
                        egui::RichText::new("🎬")
                            .size(36.0)
                    );
                    
                    ui.label(
                        egui::RichText::new("RustFrame")
                            .size(22.0)
                            .color(Color32::WHITE)
                            .strong()
                    );
                    
                    ui.add_space(4.0);
                    
                    // Subtitle
                    ui.label(
                        egui::RichText::new("Drag to resize • Click to capture")
                            .size(11.0)
                            .color(Color32::from_rgb(130, 130, 140))
                    );
                    
                    ui.add_space(20.0);
                    
                    // Play button - larger and more prominent
                    let button_size = Vec2::new(72.0, 72.0);
                    let play_button = ui.add_sized(
                        button_size,
                        egui::Button::new(
                            egui::RichText::new("▶")
                                .size(36.0)
                                .color(Color32::WHITE)
                        )
                        .fill(RustFrameColors::PLAY_GREEN)
                        .corner_radius(CornerRadius::same(36))
                    );
                    
                    if play_button.clicked() {
                        response.start_capture = true;
                    }
                    
                    ui.add_space(12.0);
                    
                    // Settings button - more subtle
                    let settings_button = ui.add(
                        egui::Button::new(
                            egui::RichText::new("⚙  Settings")
                                .size(13.0)
                                .color(Color32::from_rgb(180, 180, 180))
                        )
                        .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 15))
                        .corner_radius(CornerRadius::same(8))
                    );
                    
                    if settings_button.clicked() {
                        response.open_settings = true;
                    }
                    
                    ui.add_space(8.0);
                    
                    // Keyboard hint
                    ui.label(
                        egui::RichText::new("ESC to exit")
                            .size(10.0)
                            .color(Color32::from_rgb(90, 90, 100))
                    );
                });
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
