// ui/settings.rs - Enhanced Settings Dialog
//
// Modal dialog with tabbed interface for all application settings.

use egui::{Color32, CornerRadius, RichText, Stroke, Vec2};
use crate::app::{AppState, CaptureSettings, WindowPreset, PositionPreset, CaptureQuality};
use super::theme::RustFrameColors;

/// Settings tab selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Window,
    Capture,
    Advanced,
}

impl SettingsTab {
    fn all() -> &'static [SettingsTab] {
        &[
            SettingsTab::General,
            SettingsTab::Window,
            SettingsTab::Capture,
            SettingsTab::Advanced,
        ]
    }

    fn icon(&self) -> &'static str {
        match self {
            SettingsTab::General => "⚙",
            SettingsTab::Window => "🪟",
            SettingsTab::Capture => "🎥",
            SettingsTab::Advanced => "🔧",
        }
    }

    fn name(&self) -> &'static str {
        match self {
            SettingsTab::General => "General",
            SettingsTab::Window => "Window",
            SettingsTab::Capture => "Capture",
            SettingsTab::Advanced => "Advanced",
        }
    }
}

/// Settings dialog state
pub struct SettingsDialog {
    /// Whether the dialog is open
    pub is_open: bool,
    /// Current selected tab
    current_tab: SettingsTab,
    /// Temporary settings (applied on OK)
    temp_settings: CaptureSettings,
    /// Border width slider value
    border_width_value: f32,
    /// Custom width input
    custom_width_str: String,
    /// Custom height input
    custom_height_str: String,
    /// Custom X input
    custom_x_str: String,
    /// Custom Y input
    custom_y_str: String,
    /// Target FPS slider value
    target_fps_value: f32,
    /// Maximum FPS (based on monitor refresh rate)
    max_fps: u32,
    /// Show validation error
    validation_error: Option<String>,
}

impl Default for SettingsDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsDialog {
    pub fn new() -> Self {
        // Get monitor refresh rate for max FPS
        #[cfg(target_os = "windows")]
        let max_fps = crate::platform::windows::get_monitor_refresh_rate();
        #[cfg(not(target_os = "windows"))]
        let max_fps = 60;
        
        Self {
            is_open: false,
            current_tab: SettingsTab::General,
            temp_settings: CaptureSettings::default(),
            border_width_value: 3.0,
            custom_width_str: "800".to_string(),
            custom_height_str: "600".to_string(),
            custom_x_str: "100".to_string(),
            custom_y_str: "100".to_string(),
            target_fps_value: 60.0,
            max_fps,
            validation_error: None,
        }
    }

    /// Open the dialog with current settings
    pub fn open(&mut self, current_settings: &CaptureSettings) {
        self.temp_settings = current_settings.clone();
        self.border_width_value = current_settings.border_width as f32;
        self.custom_width_str = current_settings.custom_width.to_string();
        self.custom_height_str = current_settings.custom_height.to_string();
        self.custom_x_str = current_settings.custom_x.to_string();
        self.custom_y_str = current_settings.custom_y.to_string();
        self.target_fps_value = current_settings.target_fps as f32;
        self.validation_error = None;
        self.current_tab = SettingsTab::General;
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
                    Color32::from_rgba_unmultiplied(0, 0, 0, 200),
                );
            });

        // Settings window - calculate available size dynamically
        let screen = ctx.screen_rect();
        let available_height = (screen.height() - 100.0).max(200.0);
        let available_width = (screen.width() - 80.0).max(300.0).min(600.0);
        let scroll_height = (available_height - 180.0).max(100.0); // Reserve space for tabs and buttons
        
        egui::Window::new("⚙ Settings")
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_size([available_width, available_height.min(480.0)])
            .min_width(400.0)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(RustFrameColors::BG_DARK)
                    .stroke(Stroke::new(2.0, RustFrameColors::BLUE))
            )
            .show(ctx, |ui| {
                // Use full width of the window
                ui.set_min_width(ui.available_width());
                
                // Tab bar - wrap if needed
                ui.horizontal_wrapped(|ui| {
                    for tab in SettingsTab::all() {
                        let is_selected = self.current_tab == *tab;
                        let text = format!("{} {}", tab.icon(), tab.name());

                        let button = if is_selected {
                            egui::Button::new(RichText::new(text).strong())
                                .fill(RustFrameColors::BLUE)
                        } else {
                            egui::Button::new(text)
                                .fill(Color32::from_rgb(50, 50, 50))
                        };

                        if ui.add(button).clicked() {
                            self.current_tab = *tab;
                        }
                    }
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(12.0);

                // Tab content with dynamic height - use full width
                egui::ScrollArea::vertical()
                    .max_height(scroll_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        match self.current_tab {
                            SettingsTab::General => self.show_general_tab(ui),
                            SettingsTab::Window => self.show_window_tab(ui),
                            SettingsTab::Capture => self.show_capture_tab(ui, state),
                            SettingsTab::Advanced => self.show_advanced_tab(ui),
                        }
                    });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                // Validation error
                if let Some(error) = &self.validation_error {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("⚠ {}", error)).color(Color32::from_rgb(255, 100, 100)));
                    });
                    ui.add_space(8.0);
                }

                // Bottom buttons
                ui.horizontal(|ui| {
                    // Reset to defaults button
                    if ui.button("Reset to Defaults").clicked() {
                        self.temp_settings = CaptureSettings::default();
                        self.sync_from_temp_settings();
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Cancel button
                        if ui.button("Cancel").clicked() {
                            self.is_open = false;
                            response.cancelled = true;
                        }

                        ui.add_space(8.0);

                        // OK button
                        let ok_button = egui::Button::new(
                            RichText::new("  Save  ").strong()
                        ).fill(RustFrameColors::BLUE);

                        if ui.add(ok_button).clicked() {
                            if self.validate_and_apply() {
                                state.settings = self.temp_settings.clone();
                                // Save to file
                                if let Err(e) = state.settings.save() {
                                    log::warn!("Failed to save settings: {}", e);
                                }
                                self.is_open = false;
                                response.applied = true;
                            }
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

    /// General settings tab
    fn show_general_tab(&mut self, ui: &mut egui::Ui) {
        Self::section_header(ui, "🖱 Mouse Settings");

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.temp_settings.show_cursor, "");
            ui.label("Show mouse cursor in capture");
        });

        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.temp_settings.highlight_clicks, "");
            ui.label("Highlight mouse clicks");
        });

        if self.temp_settings.highlight_clicks {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.label("Color:");
                let mut color = Color32::from_rgba_unmultiplied(
                    self.temp_settings.click_highlight_color[0],
                    self.temp_settings.click_highlight_color[1],
                    self.temp_settings.click_highlight_color[2],
                    self.temp_settings.click_highlight_color[3],
                );
                if ui.color_edit_button_srgba(&mut color).changed() {
                    self.temp_settings.click_highlight_color = [color.r(), color.g(), color.b(), color.a()];
                }
            });
            
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.label("Duration:");
                let mut duration = self.temp_settings.click_highlight_duration_ms as f32;
                ui.add(egui::Slider::new(&mut duration, 100.0..=2000.0)
                    .suffix(" ms")
                    .step_by(50.0));
                self.temp_settings.click_highlight_duration_ms = duration as u32;
            });
        }

        ui.add_space(16.0);
        Self::section_header(ui, "🎨 Border Settings");

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.temp_settings.show_border, "");
            ui.label("Show border frame during capture");
        });

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

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.label("Border color:");
                let mut color = Color32::from_rgba_unmultiplied(
                    self.temp_settings.border_color[0],
                    self.temp_settings.border_color[1],
                    self.temp_settings.border_color[2],
                    self.temp_settings.border_color[3],
                );
                if ui.color_edit_button_srgba(&mut color).changed() {
                    self.temp_settings.border_color = [color.r(), color.g(), color.b(), color.a()];
                }
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(24.0);
                ui.checkbox(&mut self.temp_settings.show_rec_indicator, "");
                ui.label("Show REC indicator");
            });
            
            if self.temp_settings.show_rec_indicator {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space(48.0);
                    ui.label("Size:");
                    
                    let size_labels = ["Small", "Medium", "Large"];
                    for (i, label) in size_labels.iter().enumerate() {
                        let size_val = (i + 1) as u32;
                        if ui.selectable_label(
                            self.temp_settings.rec_indicator_size == size_val,
                            *label
                        ).clicked() {
                            self.temp_settings.rec_indicator_size = size_val;
                        }
                    }
                });
            }
        }

        ui.add_space(16.0);
        Self::section_header(ui, "💡 UI Settings");

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.temp_settings.show_shortcuts, "");
            ui.label("Show keyboard shortcuts hints");
        });

        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.temp_settings.remember_region, "");
            ui.label("Remember last capture region");
        });

        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.temp_settings.auto_start, "");
            ui.label("Auto-start capture after region selection");
        });
    }

    /// Window settings tab
    fn show_window_tab(&mut self, ui: &mut egui::Ui) {
        Self::section_header(ui, "📐 Window Size");

        ui.label("Select a preset or enter custom dimensions:");
        ui.add_space(8.0);

        // Size presets grid
        egui::Grid::new("size_presets")
            .num_columns(3)
            .spacing([8.0, 8.0])
            .show(ui, |ui| {
                for (i, preset) in WindowPreset::all().iter().enumerate() {
                    let is_selected = self.temp_settings.size_preset == *preset;
                    let dims = preset.dimensions();
                    let text = if *preset == WindowPreset::Custom {
                        preset.short_name().to_string()
                    } else {
                        format!("{}\n{}×{}", preset.short_name(), dims.0, dims.1)
                    };

                    let button = if is_selected {
                        egui::Button::new(RichText::new(text).strong())
                            .fill(RustFrameColors::BLUE)
                            .min_size(Vec2::new(100.0, 45.0))
                    } else {
                        egui::Button::new(text)
                            .fill(Color32::from_rgb(50, 50, 50))
                            .min_size(Vec2::new(100.0, 45.0))
                    };

                    if ui.add(button).clicked() {
                        self.temp_settings.size_preset = *preset;
                        if *preset != WindowPreset::Custom {
                            let (w, h) = preset.dimensions();
                            self.custom_width_str = w.to_string();
                            self.custom_height_str = h.to_string();
                        }
                    }

                    if (i + 1) % 3 == 0 {
                        ui.end_row();
                    }
                }
            });

        // Custom size inputs
        if self.temp_settings.size_preset == WindowPreset::Custom {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.label("Width:");
                ui.add(egui::TextEdit::singleline(&mut self.custom_width_str).desired_width(80.0));
                ui.label("px");

                ui.add_space(16.0);

                ui.label("Height:");
                ui.add(egui::TextEdit::singleline(&mut self.custom_height_str).desired_width(80.0));
                ui.label("px");
            });
        }

        ui.add_space(20.0);
        Self::section_header(ui, "📍 Window Position");

        ui.label("Select a position preset or enter custom coordinates:");
        ui.add_space(8.0);

        // Position presets - use grid for better wrapping
        egui::Grid::new("position_presets")
            .num_columns(3)
            .spacing([8.0, 8.0])
            .show(ui, |ui| {
                for (i, preset) in PositionPreset::all().iter().enumerate() {
                    let is_selected = self.temp_settings.position_preset == *preset;
                    let text = preset.display_name();

                    let button = if is_selected {
                        egui::Button::new(RichText::new(text).strong())
                            .fill(RustFrameColors::BLUE)
                            .min_size(Vec2::new(90.0, 28.0))
                    } else {
                        egui::Button::new(text)
                            .fill(Color32::from_rgb(50, 50, 50))
                            .min_size(Vec2::new(90.0, 28.0))
                    };

                    if ui.add(button).clicked() {
                        self.temp_settings.position_preset = *preset;
                    }

                    if (i + 1) % 3 == 0 {
                        ui.end_row();
                    }
                }
            });

        // Custom position inputs
        if self.temp_settings.position_preset == PositionPreset::Custom {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.label("X:");
                ui.add(egui::TextEdit::singleline(&mut self.custom_x_str).desired_width(80.0));
                ui.label("px");

                ui.add_space(16.0);

                ui.label("Y:");
                ui.add(egui::TextEdit::singleline(&mut self.custom_y_str).desired_width(80.0));
                ui.label("px");
            });
        }

        ui.add_space(16.0);

        // Info box
        Self::info_box(ui, "💡 Tip: Window position is calculated when capture starts.\nFor custom position, enter screen coordinates.");
    }

    /// Capture settings tab
    fn show_capture_tab(&mut self, ui: &mut egui::Ui, state: &AppState) {
        Self::section_header(ui, "🎬 Capture Quality");

        ui.horizontal(|ui| {
            ui.label("Quality:");
            egui::ComboBox::from_id_salt("quality_combo")
                .selected_text(self.temp_settings.quality.display_name())
                .show_ui(ui, |ui| {
                    for quality in [CaptureQuality::Low, CaptureQuality::Medium, CaptureQuality::High, CaptureQuality::Maximum] {
                        ui.selectable_value(&mut self.temp_settings.quality, quality, quality.display_name());
                    }
                });
        });

        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Target FPS:");
            ui.add(
                egui::Slider::new(&mut self.target_fps_value, 1.0..=(self.max_fps as f32))
                    .integer()
                    .suffix(" fps")
            );
        });
        self.temp_settings.target_fps = self.target_fps_value as u32;

        // Show monitor refresh rate info
        ui.label(
            RichText::new(format!("Monitor: {} Hz", self.max_fps))
                .size(11.0)
                .color(RustFrameColors::TEXT_GRAY)
        );

        // FPS presets - only show values <= max_fps
        let available_presets: Vec<u32> = [30, 60, 120, 144]
            .iter()
            .copied()
            .filter(|&fps| fps <= self.max_fps)
            .collect();
        
        if !available_presets.is_empty() {
            egui::Grid::new("fps_presets")
                .num_columns(available_presets.len())
                .spacing([4.0, 4.0])
                .show(ui, |ui| {
                    for fps in available_presets {
                        if ui.small_button(format!("{}", fps)).clicked() {
                            self.target_fps_value = fps as f32;
                            self.temp_settings.target_fps = fps;
                        }
                    }
                });
        }

        ui.add_space(16.0);
        Self::section_header(ui, "🔒 Capture Options");

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.temp_settings.exclude_from_capture, "");
            ui.vertical(|ui| {
                ui.label("Exclude destination from capture");
                ui.label(
                    RichText::new("Prevents infinite mirror effect")
                        .size(11.0)
                        .color(RustFrameColors::TEXT_GRAY)
                );
            });
        });

        // Production mode section - only visible in dev mode
        if state.dev_mode {
            ui.add_space(16.0);
            Self::section_header(ui, "🛠 Development Options");
            
            Self::info_box(ui, "⚠ Development mode is active.\nDestination window is visible for debugging.");
        }
    }

    /// Advanced settings tab
    fn show_advanced_tab(&mut self, ui: &mut egui::Ui) {
        Self::section_header(ui, "💾 Settings Storage");

        ui.label("Settings file:");
        let config_path = CaptureSettings::config_path_display();
        ui.label(RichText::new(&config_path).size(11.0).color(RustFrameColors::TEXT_GRAY));

        ui.add_space(8.0);

        ui.horizontal_wrapped(|ui| {
            if ui.button("Open Folder").clicked() {
                if let Some(path) = CaptureSettings::config_dir() {
                    let _ = std::process::Command::new("explorer").arg(path).spawn();
                }
            }

            if ui.button("Export").clicked() {
                // TODO: File save dialog
            }

            if ui.button("Import").clicked() {
                // TODO: File open dialog
            }
        });

        ui.add_space(20.0);
        Self::section_header(ui, "ℹ About");

        ui.label(RichText::new("RustFrame").strong().size(16.0));
        ui.label(format!("Version: {}", env!("CARGO_PKG_VERSION")));
        ui.add_space(4.0);
        ui.label(RichText::new(env!("CARGO_PKG_DESCRIPTION")).size(11.0).color(RustFrameColors::TEXT_GRAY));

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.hyperlink_to("GitHub Repository", env!("CARGO_PKG_REPOSITORY"));
        });

        ui.add_space(16.0);
        Self::section_header(ui, "⌨ Keyboard Shortcuts");

        egui::Grid::new("shortcuts_grid")
            .num_columns(2)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                Self::shortcut_row(ui, "ESC", "Stop capture / Close dialog");
                Self::shortcut_row(ui, "Space", "Start/Stop capture");
                Self::shortcut_row(ui, "C", "Toggle cursor visibility");
                Self::shortcut_row(ui, "B", "Toggle border visibility");
                Self::shortcut_row(ui, "R", "Reset capture region");
            });
    }

    /// Helper: Section header
    fn section_header(ui: &mut egui::Ui, text: &str) {
        ui.label(RichText::new(text).strong().size(14.0));
        ui.add_space(4.0);
    }

    /// Helper: Info box
    fn info_box(ui: &mut egui::Ui, text: &str) {
        egui::Frame::new()
            .fill(Color32::from_rgb(40, 50, 60))
            .corner_radius(CornerRadius::same(4))
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(text)
                        .size(12.0)
                        .color(RustFrameColors::TEXT_GRAY)
                );
            });
    }

    /// Helper: Shortcut row
    fn shortcut_row(ui: &mut egui::Ui, key: &str, description: &str) {
        ui.label(RichText::new(key).strong().monospace());
        ui.label(description);
        ui.end_row();
    }

    /// Sync string inputs from temp_settings
    fn sync_from_temp_settings(&mut self) {
        self.border_width_value = self.temp_settings.border_width as f32;
        self.custom_width_str = self.temp_settings.custom_width.to_string();
        self.custom_height_str = self.temp_settings.custom_height.to_string();
        self.custom_x_str = self.temp_settings.custom_x.to_string();
        self.custom_y_str = self.temp_settings.custom_y.to_string();
        self.target_fps_value = self.temp_settings.target_fps as f32;
    }

    /// Validate inputs and apply to temp_settings
    fn validate_and_apply(&mut self) -> bool {
        self.validation_error = None;

        // Validate custom width
        if self.temp_settings.size_preset == WindowPreset::Custom {
            match self.custom_width_str.parse::<u32>() {
                Ok(w) if w >= 100 && w <= 7680 => self.temp_settings.custom_width = w,
                _ => {
                    self.validation_error = Some("Width must be between 100 and 7680".to_string());
                    return false;
                }
            }

            match self.custom_height_str.parse::<u32>() {
                Ok(h) if h >= 100 && h <= 4320 => self.temp_settings.custom_height = h,
                _ => {
                    self.validation_error = Some("Height must be between 100 and 4320".to_string());
                    return false;
                }
            }
        }

        // Validate custom position
        if self.temp_settings.position_preset == PositionPreset::Custom {
            match self.custom_x_str.parse::<i32>() {
                Ok(x) if x >= -10000 && x <= 10000 => self.temp_settings.custom_x = x,
                _ => {
                    self.validation_error = Some("X must be between -10000 and 10000".to_string());
                    return false;
                }
            }

            match self.custom_y_str.parse::<i32>() {
                Ok(y) if y >= -10000 && y <= 10000 => self.temp_settings.custom_y = y,
                _ => {
                    self.validation_error = Some("Y must be between -10000 and 10000".to_string());
                    return false;
                }
            }
        }

        true
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

// Extension methods for CaptureSettings display helpers
impl CaptureSettings {
    /// Get config path for display
    pub fn config_path_display() -> String {
        Self::config_path().display().to_string()
    }

    /// Get config directory
    pub fn config_dir() -> Option<std::path::PathBuf> {
        Self::config_path().parent().map(|p| p.to_path_buf())
    }
}
