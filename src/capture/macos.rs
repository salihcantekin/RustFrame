// capture/macos.rs - macOS Screen Capture Implementation (Stub)
//
// TODO: Implement using ScreenCaptureKit (macOS 12.3+) or CGWindowListCreateImage

use anyhow::{anyhow, Result};
use crate::app::CaptureRect;
use super::{CaptureEngine, CaptureFrame};

/// macOS capture engine (stub)
pub struct MacOSCaptureEngine {
    is_active: bool,
    region: Option<CaptureRect>,
}

impl MacOSCaptureEngine {
    pub fn new() -> Result<Self> {
        Ok(Self {
            is_active: false,
            region: None,
        })
    }
}

impl CaptureEngine for MacOSCaptureEngine {
    fn start(&mut self, region: CaptureRect, _show_cursor: bool) -> Result<()> {
        // TODO: Implement using ScreenCaptureKit
        // For macOS 12.3+: SCStreamConfiguration, SCContentFilter, SCStream
        // For older versions: CGWindowListCreateImage
        self.region = Some(region);
        self.is_active = true;
        Err(anyhow!("macOS capture not yet implemented"))
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
}
