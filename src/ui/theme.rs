// ui/theme.rs - RustFrame Visual Theme
//
// Defines the visual style for all egui components.
// Colors match the original RustFrame design.

use egui::{Color32, CornerRadius, Stroke, Visuals};

/// Color palette for RustFrame
pub struct RustFrameColors;

impl RustFrameColors {
    // Primary colors
    pub const BLUE: Color32 = Color32::from_rgb(0, 168, 255);
    pub const BLUE_LIGHT: Color32 = Color32::from_rgb(0, 212, 255);
    pub const ORANGE: Color32 = Color32::from_rgb(255, 154, 60);
    pub const ORANGE_LIGHT: Color32 = Color32::from_rgb(255, 183, 102);
    
    // Background colors
    pub const BG_DARK: Color32 = Color32::from_rgb(24, 24, 24);
    pub const BG_PANEL: Color32 = Color32::from_rgba_premultiplied(30, 30, 30, 240);
    pub const BG_TRANSPARENT: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 16);
    
    // Text colors
    pub const TEXT_WHITE: Color32 = Color32::WHITE;
    pub const TEXT_GRAY: Color32 = Color32::from_rgb(176, 176, 176);
    pub const TEXT_GREEN: Color32 = Color32::from_rgb(0, 221, 0);
    pub const TEXT_RED: Color32 = Color32::from_rgb(255, 68, 68);
    pub const TEXT_YELLOW: Color32 = Color32::from_rgb(255, 204, 0);
    
    // UI element colors
    pub const PLAY_GREEN: Color32 = Color32::from_rgb(0, 255, 122);
    pub const STOP_RED: Color32 = Color32::from_rgb(255, 80, 80);
    pub const BORDER: Color32 = Color32::from_rgb(0, 168, 255);
    pub const BORDER_ACTIVE: Color32 = Color32::from_rgb(255, 154, 60);
}

/// Theme configuration for RustFrame
pub struct RustFrameTheme;

impl RustFrameTheme {
    /// Apply the RustFrame theme to an egui context
    pub fn apply(ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();
        
        // Dark theme base
        style.visuals = Visuals::dark();
        
        // Match the wgpu clear color exactly (RGB 20, 20, 25)
        let bg_color = Color32::from_rgb(20, 20, 25);
        
        // Customize colors - use matching background for overlay mode
        style.visuals.window_fill = bg_color;
        style.visuals.panel_fill = bg_color;
        style.visuals.extreme_bg_color = bg_color;
        style.visuals.faint_bg_color = bg_color;
        
        // Widget styling
        style.visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(40, 40, 40);
        style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, RustFrameColors::TEXT_GRAY);
        
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(50, 50, 50);
        style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, RustFrameColors::TEXT_WHITE);
        
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(70, 70, 70);
        style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, RustFrameColors::BLUE);
        
        style.visuals.widgets.active.bg_fill = RustFrameColors::BLUE;
        style.visuals.widgets.active.fg_stroke = Stroke::new(2.0, RustFrameColors::TEXT_WHITE);
        
        // Selection color
        style.visuals.selection.bg_fill = RustFrameColors::BLUE.gamma_multiply(0.5);
        style.visuals.selection.stroke = Stroke::new(1.0, RustFrameColors::BLUE);
        
        // Window styling
        style.visuals.window_stroke = Stroke::new(1.0, RustFrameColors::BLUE);
        
        // Button styling
        style.visuals.widgets.inactive.corner_radius = CornerRadius::same(4);
        style.visuals.widgets.hovered.corner_radius = CornerRadius::same(4);
        style.visuals.widgets.active.corner_radius = CornerRadius::same(4);
        
        // Spacing
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.window_margin = egui::Margin::same(12);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        
        ctx.set_style(style);
    }
    
    /// Get border color based on capture state
    pub fn border_color(is_capturing: bool) -> Color32 {
        if is_capturing {
            RustFrameColors::BORDER_ACTIVE
        } else {
            RustFrameColors::BORDER
        }
    }
    
    /// Get play/stop button color based on state
    pub fn action_button_color(is_capturing: bool) -> Color32 {
        if is_capturing {
            RustFrameColors::STOP_RED
        } else {
            RustFrameColors::PLAY_GREEN
        }
    }
}

/// Styled button widget
pub fn styled_button(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
    let button = egui::Button::new(
        egui::RichText::new(text)
            .color(Color32::WHITE)
            .strong()
    ).fill(color);
    
    ui.add(button)
}

/// Status indicator (colored dot + text)
pub fn status_indicator(ui: &mut egui::Ui, label: &str, enabled: bool) {
    ui.horizontal(|ui| {
        let color = if enabled {
            RustFrameColors::TEXT_GREEN
        } else {
            RustFrameColors::TEXT_RED
        };
        
        // Colored circle
        let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 4.0, color);
        
        // Label
        ui.label(egui::RichText::new(label).color(RustFrameColors::TEXT_GRAY));
    });
}
