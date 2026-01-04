//! macOS Destination Window Implementation
//!
//! Renders captured frames to a transparent overlay window using NSWindow and CoreGraphics.

use cocoa::appkit::{NSWindow, NSWindowStyleMask, NSBackingStoreType, NSColor};
use cocoa::base::{id, nil, YES, NO};
use cocoa::foundation::{NSRect, NSPoint, NSSize, NSAutoreleasePool};
use objc::{msg_send, sel, sel_impl};
use std::sync::{Arc, Mutex};

lazy_static::lazy_static! {
    /// Global frame buffer - stores the latest frame to render
    static ref FRAME_BUFFER: Arc<Mutex<Option<FrameData>>> = Arc::new(Mutex::new(None));
}

struct FrameData {
    data: Vec<u8>,
    width: u32,
    height: u32,
}

pub struct DestinationWindow {
    window: id,
    view: id,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DestinationWindowConfig {
    pub alpha: Option<u8>,
    pub topmost: Option<bool>,
    pub click_through: Option<bool>,
    pub toolwindow: Option<bool>,
    pub layered: Option<bool>,
    pub appwindow: Option<bool>,
    pub noactivate: Option<bool>,
    pub overlapped: Option<bool>,
}

unsafe impl Send for DestinationWindow {}
unsafe impl Sync for DestinationWindow {}

impl DestinationWindow {
    pub fn new(width: u32, height: u32, config: DestinationWindowConfig) -> Option<Self> {
        log::info!("Creating macOS destination window {}x{} with config {:?}", width, height, config);
        
        unsafe {
            let _pool = NSAutoreleasePool::new(nil);
            
            // Create window style mask
            let style_mask = NSWindowStyleMask::NSBorderlessWindowMask;
            
            // Create window frame (positioned at top-left for now)
            let frame = NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(width as f64, height as f64)
            );
            
            // Create the window
            let window = NSWindow::alloc(nil).initWithContentRect_styleMask_backing_defer_(
                frame,
                style_mask,
                NSBackingStoreType::NSBackingStoreBuffered,
                NO,
            );
            
            if window == nil {
                log::error!("Failed to create NSWindow");
                return None;
            }
            
            // Configure window properties
            window.setOpaque_(NO);
            window.setBackgroundColor_(NSColor::clearColor(nil));
            
            // Set window level for topmost behavior (default: true)
            let topmost = config.topmost.unwrap_or(true);
            if topmost {
                // NSFloatingWindowLevel = 3
                let _: () = msg_send![window, setLevel: 3i32];
            }
            
            // Configure click-through behavior (default: true in release)
            let click_through = config.click_through.unwrap_or(!cfg!(debug_assertions));
            if click_through {
                window.setIgnoresMouseEvents_(YES);
            }
            
            // Create content view
            let content_view = window.contentView();
            
            // Set alpha if specified
            if let Some(alpha) = config.alpha {
                window.setAlphaValue_((alpha as f64) / 255.0);
            }
            
            // Show the window
            window.makeKeyAndOrderFront_(nil);
            
            log::info!("macOS destination window created successfully");
            
            Some(Self {
                window,
                view: content_view,
                width,
                height,
            })
        }
    }

    pub fn hwnd_value(&self) -> isize {
        self.window as isize
    }

    pub fn update_frame(&self, data: Vec<u8>, width: u32, height: u32) {
        // Store frame data for rendering
        let mut buffer = FRAME_BUFFER.lock().unwrap();
        *buffer = Some(FrameData { data, width, height });
        
        // Trigger redraw
        unsafe {
            let _: () = msg_send![self.view, setNeedsDisplay: YES];
        }
    }

    pub fn render(&mut self, pixels: &[u8], width: u32, height: u32) {
        self.update_frame(pixels.to_vec(), width, height);
    }

    pub fn set_pos(&mut self, x: i32, y: i32) {
        unsafe {
            // macOS uses bottom-left origin, need to convert from top-left
            let screen_frame: NSRect = msg_send![self.window, screen];
            let screen_height = screen_frame.size.height;
            
            let origin = NSPoint::new(
                x as f64,
                screen_height - (y as f64) - (self.height as f64)
            );
            
            let _: () = msg_send![self.window, setFrameOrigin: origin];
        }
    }
}

impl Drop for DestinationWindow {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![self.window, close];
        }
    }
}
