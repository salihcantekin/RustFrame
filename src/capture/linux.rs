// capture/linux.rs - Linux Screen Capture Implementation (Stub)
//
// TODO: Implement using PipeWire (modern) or X11/XComposite (legacy)

use super::{CaptureEngine, CaptureFrame, CaptureRect};
use crate::window_filter::WindowIdentifier;
use anyhow::{anyhow, Result};

/// Linux capture engine (stub)
pub struct LinuxCaptureEngine {
    is_active: bool,
    region: Option<CaptureRect>,
}

impl LinuxCaptureEngine {
    pub fn new() -> Result<Self> {
        Ok(Self {
            is_active: false,
            region: None,
        })
    }
}

impl CaptureEngine for LinuxCaptureEngine {
    fn start(&mut self, region: CaptureRect, _show_cursor: bool, _excluded_windows: Option<Vec<WindowIdentifier>>) -> Result<()> {
        // TODO: Implement using:
        // - PipeWire Portal API (for Wayland and modern Gnome/KDE)
        // - X11 XShm extension (for X11 systems)
        // - XComposite (for window capture on X11)
        self.region = Some(region);
        self.is_active = true;
        Err(anyhow!("Linux capture not yet implemented"))
    }

    fn stop(&mut self) {
        self.is_active = false;
    }

    fn is_active(&self) -> bool {
        self.is_active
    }

    fn has_new_frame(&self) -> bool {
        false
    }

    fn get_frame(&mut self) -> Option<CaptureFrame> {
        None
    }

    fn set_cursor_visible(&mut self, _visible: bool) -> Result<()> {
        Ok(())
    }

    fn get_region(&self) -> Option<CaptureRect> {
        self.region
    }

    fn update_region(&mut self, region: CaptureRect) -> Result<()> {
        self.region = Some(region);
        Ok(())
    }
}
