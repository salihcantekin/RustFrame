//! Separation Layer Window
//!
//! A hidden window placed between the border and preview window in z-order.
//! This creates the RegionToShare-style masking effect where users see a solid
//! color instead of the preview window when border is over desktop.

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "windows")]
pub use windows::SeparationLayer;

#[cfg(not(target_os = "windows"))]
pub struct SeparationLayer;

#[cfg(not(target_os = "windows"))]
impl SeparationLayer {
    pub fn new(_x: i32, _y: i32, _width: i32, _height: i32, _color: u32) -> Option<Self> {
        None
    }

    pub fn update_position(&self, _x: i32, _y: i32, _width: i32, _height: i32) {}
    pub fn show(&self) {}
    pub fn hide(&self) {}
    pub fn hwnd_value(&self) -> isize {
        0
    }
}
