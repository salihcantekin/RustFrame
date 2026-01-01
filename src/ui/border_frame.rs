// ui/border_frame.rs - Hollow Border Frame UI
//
// This module renders a transparent window with only a colored border.
// Used during capture to show the capture region without blocking content.

use egui::{Color32, CornerRadius, Pos2, Rect, Stroke};
use super::theme::RustFrameColors;
use winit::event_loop::EventLoopProxy;
use crate::{UserEvent, ResizeCorner};

/// Border frame UI for capture mode
pub struct BorderFrameUi {
    /// Border color
    border_color: Color32,
    /// Border width in pixels
    border_width: f32,
    /// Whether to show corner markers
    show_corners: bool,
    /// Animation pulse (0.0 to 1.0)
    pulse: f32,
    /// Pulse direction
    pulse_dir: bool,
    /// Event proxy to send events to main app
    event_proxy: Option<EventLoopProxy<UserEvent>>,
}

impl Default for BorderFrameUi {
    fn default() -> Self {
        Self::new()
    }
}

impl BorderFrameUi {
    pub fn new() -> Self {
        Self {
            border_color: RustFrameColors::ORANGE_LIGHT,
            border_width: 3.0,
            show_corners: true,
            pulse: 0.0,
            pulse_dir: true,
            event_proxy: None,
        }
    }
    
    /// Set event proxy for sending events to main app
    pub fn set_event_proxy(&mut self, proxy: EventLoopProxy<UserEvent>) {
        self.event_proxy = Some(proxy);
    }
    
    /// Set border color
    pub fn set_color(&mut self, color: Color32) {
        self.border_color = color;
    }
    
    /// Set border width
    pub fn set_width(&mut self, width: f32) {
        self.border_width = width;
    }
    
    /// Render the border frame with resize/move handling
    pub fn show(&mut self, ctx: &egui::Context) {
        // Update pulse animation
        self.update_pulse();
        
        let screen_rect = ctx.screen_rect();
        
        // Create frame - since we use Window Region API, the center is literally cut out
        // No need for color-key transparency
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(30, 30, 30))) // Dark background
            .show(ctx, |ui| {
                // Full-window area for mouse interaction
                let response = ui.allocate_response(ui.available_size(), egui::Sense::click_and_drag());
                
                // Handle mouse interaction for resize/move
                self.handle_mouse_interaction(&response, screen_rect);
            });
        
        // Draw border elements on top
        self.draw_border(ctx, screen_rect);
        
        // Request continuous repaint for animation
        ctx.request_repaint();
    }
    
    /// Handle mouse interaction for window resize/move
    fn handle_mouse_interaction(&mut self, response: &egui::Response, window_rect: egui::Rect) {
        if let Some(pos) = response.hover_pos() {
            // Check if mouse is over a corner or edge for resize/move
            let corner_size = 20.0;
            let border_width = 15.0; // Increased for easier grabbing
            
            let in_top_left_corner = pos.x <= window_rect.left() + corner_size && 
                                   pos.y <= window_rect.top() + corner_size;
            let in_top_right_corner = pos.x >= window_rect.right() - corner_size && 
                                    pos.y <= window_rect.top() + corner_size;
            let in_bottom_left_corner = pos.x <= window_rect.left() + corner_size && 
                                      pos.y >= window_rect.bottom() - corner_size;
            let in_bottom_right_corner = pos.x >= window_rect.right() - corner_size && 
                                       pos.y >= window_rect.bottom() - corner_size;
            
            let in_top_edge = !in_top_left_corner && !in_top_right_corner &&
                            pos.y <= window_rect.top() + border_width;
            let in_bottom_edge = !in_bottom_left_corner && !in_bottom_right_corner &&
                               pos.y >= window_rect.bottom() - border_width;
            let in_left_edge = !in_top_left_corner && !in_bottom_left_corner &&
                             pos.x <= window_rect.left() + border_width;
            let in_right_edge = !in_top_right_corner && !in_bottom_right_corner &&
                              pos.x >= window_rect.right() - border_width;
            
            // Set cursor using Win32 API for reliable cursor changes
            #[cfg(target_os = "windows")]
            {
                use crate::platform::windows::{set_cursor_for_direction, ResizeDirection};
                
                let direction = if in_top_left_corner {
                    ResizeDirection::TopLeft
                } else if in_top_right_corner {
                    ResizeDirection::TopRight
                } else if in_bottom_left_corner {
                    ResizeDirection::BottomLeft
                } else if in_bottom_right_corner {
                    ResizeDirection::BottomRight
                } else if in_top_edge {
                    ResizeDirection::Move // Top edge = move
                } else if in_bottom_edge {
                    ResizeDirection::Bottom
                } else if in_left_edge {
                    ResizeDirection::Left
                } else if in_right_edge {
                    ResizeDirection::Right
                } else {
                    ResizeDirection::None
                };
                
                set_cursor_for_direction(direction);
            }
            
            // Handle drag operations
            if response.dragged() {
                let delta = response.drag_delta();
                if delta.length() > 0.0 {
                    if let Some(ref proxy) = self.event_proxy {
                        // Determine action based on mouse position
                        if in_top_left_corner {
                            let _ = proxy.send_event(UserEvent::ResizeBorderWindow { 
                                delta_x: delta.x, 
                                delta_y: delta.y,
                                from_corner: ResizeCorner::TopLeft 
                            });
                        } else if in_top_right_corner {
                            let _ = proxy.send_event(UserEvent::ResizeBorderWindow { 
                                delta_x: delta.x, 
                                delta_y: delta.y,
                                from_corner: ResizeCorner::TopRight 
                            });
                        } else if in_bottom_left_corner {
                            let _ = proxy.send_event(UserEvent::ResizeBorderWindow { 
                                delta_x: delta.x, 
                                delta_y: delta.y,
                                from_corner: ResizeCorner::BottomLeft 
                            });
                        } else if in_bottom_right_corner {
                            let _ = proxy.send_event(UserEvent::ResizeBorderWindow { 
                                delta_x: delta.x, 
                                delta_y: delta.y,
                                from_corner: ResizeCorner::BottomRight 
                            });
                        } else if in_top_edge {
                            // Top edge = move window (like title bar)
                            let _ = proxy.send_event(UserEvent::MoveBorderWindow { 
                                delta_x: delta.x, 
                                delta_y: delta.y 
                            });
                        } else if in_bottom_edge {
                            let _ = proxy.send_event(UserEvent::ResizeBorderWindow { 
                                delta_x: delta.x, 
                                delta_y: delta.y,
                                from_corner: ResizeCorner::BottomEdge 
                            });
                        } else if in_left_edge {
                            let _ = proxy.send_event(UserEvent::ResizeBorderWindow { 
                                delta_x: delta.x, 
                                delta_y: delta.y,
                                from_corner: ResizeCorner::LeftEdge 
                            });
                        } else if in_right_edge {
                            let _ = proxy.send_event(UserEvent::ResizeBorderWindow { 
                                delta_x: delta.x, 
                                delta_y: delta.y,
                                from_corner: ResizeCorner::RightEdge 
                            });
                        }
                    }
                }
            }
            
            // When drag stops, notify main app to sync destination window and update region
            if response.drag_stopped() {
                if let Some(ref proxy) = self.event_proxy {
                    let _ = proxy.send_event(UserEvent::BorderDragEnded);
                }
            }
        }
    }
    
    /// Update pulse animation
    fn update_pulse(&mut self) {
        let speed = 0.02;
        if self.pulse_dir {
            self.pulse += speed;
            if self.pulse >= 1.0 {
                self.pulse = 1.0;
                self.pulse_dir = false;
            }
        } else {
            self.pulse -= speed;
            if self.pulse <= 0.0 {
                self.pulse = 0.0;
                self.pulse_dir = true;
            }
        }
    }
    
    /// Draw the border frame
    fn draw_border(&self, ctx: &egui::Context, rect: Rect) {
        let painter = ctx.layer_painter(egui::LayerId::background());
        
        // Calculate animated border color (subtle pulse)
        let pulse_factor = 0.8 + 0.2 * self.pulse;
        let border_color = Color32::from_rgba_unmultiplied(
            (self.border_color.r() as f32 * pulse_factor) as u8,
            (self.border_color.g() as f32 * pulse_factor) as u8,
            (self.border_color.b() as f32 * pulse_factor) as u8,
            self.border_color.a(),
        );
        
        // Draw outer border stroke only (no fill)
        painter.rect_stroke(
            rect.shrink(self.border_width / 2.0),
            CornerRadius::ZERO,
            Stroke::new(self.border_width, border_color),
            egui::StrokeKind::Outside,
        );
        
        // Draw corner markers
        if self.show_corners {
            let corner_size = 24.0;
            let corner_width = 4.0;
            let corner_color = Color32::WHITE;
            
            // Top-left corner
            self.draw_corner(&painter, rect.left_top(), corner_size, corner_width, corner_color, true, true);
            // Top-right corner
            self.draw_corner(&painter, rect.right_top(), corner_size, corner_width, corner_color, false, true);
            // Bottom-left corner
            self.draw_corner(&painter, rect.left_bottom(), corner_size, corner_width, corner_color, true, false);
            // Bottom-right corner
            self.draw_corner(&painter, rect.right_bottom(), corner_size, corner_width, corner_color, false, false);
        }
        
        // Draw "REC" indicator in top-right
        self.draw_recording_indicator(&painter, rect);
    }
    
    /// Draw a corner marker (L-shaped)
    fn draw_corner(
        &self,
        painter: &egui::Painter,
        pos: Pos2,
        size: f32,
        width: f32,
        color: Color32,
        left: bool,
        top: bool,
    ) {
        let stroke = Stroke::new(width, color);
        let dx = if left { size } else { -size };
        let dy = if top { size } else { -size };
        
        // Horizontal line
        painter.line_segment([pos, Pos2::new(pos.x + dx, pos.y)], stroke);
        // Vertical line
        painter.line_segment([pos, Pos2::new(pos.x, pos.y + dy)], stroke);
    }
    
    /// Draw recording indicator
    fn draw_recording_indicator(&self, painter: &egui::Painter, rect: Rect) {
        let indicator_pos = Pos2::new(rect.right() - 60.0, rect.top() + 20.0);
        
        // Pulsing red circle
        let alpha = (180.0 + 75.0 * self.pulse) as u8;
        let red = Color32::from_rgba_unmultiplied(255, 60, 60, alpha);
        
        painter.circle_filled(indicator_pos, 6.0, red);
        
        // "REC" text
        painter.text(
            Pos2::new(indicator_pos.x + 14.0, indicator_pos.y),
            egui::Align2::LEFT_CENTER,
            "REC",
            egui::FontId::proportional(12.0),
            Color32::from_rgba_unmultiplied(255, 255, 255, alpha),
        );
    }
}
