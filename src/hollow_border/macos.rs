//! macOS Hollow Border Implementation
//!
//! Creates a transparent window with a colored border using NSWindow and custom NSView.

use cocoa::appkit::{NSWindow, NSView, NSWindowStyleMask, NSBackingStoreType, NSColor};
use cocoa::base::{id, nil, YES, NO};
use cocoa::foundation::{NSRect, NSPoint, NSSize, NSAutoreleasePool, NSString};
use core_graphics::display::CGDisplay;
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use objc::{msg_send, sel, sel_impl, class};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

static ESC_PRESSED: AtomicBool = AtomicBool::new(false);
static REGISTER_CLASS: Once = Once::new();

// Custom NSView subclass that draws the hollow border
extern "C" fn draw_rect(this: &Object, _cmd: Sel, dirty_rect: NSRect) {
    unsafe {
        let border_color: id = msg_send![this, borderColor];
        let border_width: f64 = msg_send![this, borderWidth];
        
        if border_color == nil {
            return;
        }
        
        // Set the stroke color
        let _: () = msg_send![border_color, set];
        
        // Get the bounds
        let bounds: NSRect = msg_send![this, bounds];
        
        // Create the outer rect (full bounds)
        let outer_rect = bounds;
        
        // Create the inner rect (inset by border width)
        let inner_rect = NSRect::new(
            NSPoint::new(border_width, border_width),
            NSSize::new(
                bounds.size.width - 2.0 * border_width,
                bounds.size.height - 2.0 * border_width,
            ),
        );
        
        // Get the current graphics context
        let context: id = msg_send![class!(NSGraphicsContext), currentContext];
        let cg_context: id = msg_send![context, CGContext];
        
        // Fill the border area (outer rect minus inner rect)
        let path: id = msg_send![class!(NSBezierPath), bezierPath];
        let _: () = msg_send![path, appendBezierPathWithRect: outer_rect];
        let _: () = msg_send![path, appendBezierPathWithRect: inner_rect];
        let _: () = msg_send![path, setWindingRule: 1]; // NSEvenOddWindingRule
        let _: () = msg_send![path, fill];
    }
}

extern "C" fn border_color(this: &Object, _cmd: Sel) -> id {
    unsafe {
        let ivar = this.get_ivar::<id>("_borderColor");
        *ivar
    }
}

extern "C" fn set_border_color(this: &mut Object, _cmd: Sel, color: id) {
    unsafe {
        this.set_ivar("_borderColor", color);
        let _: () = msg_send![this, setNeedsDisplay: YES];
    }
}

extern "C" fn border_width(this: &Object, _cmd: Sel) -> f64 {
    unsafe {
        let ivar = this.get_ivar::<f64>("_borderWidth");
        *ivar
    }
}

extern "C" fn set_border_width(this: &mut Object, _cmd: Sel, width: f64) {
    unsafe {
        this.set_ivar("_borderWidth", width);
        let _: () = msg_send![this, setNeedsDisplay: YES];
    }
}

fn register_border_view_class() {
    REGISTER_CLASS.call_once(|| {
        let superclass = class!(NSView);
        let mut decl = ClassDecl::new("HollowBorderView", superclass).unwrap();
        
        // Add ivars
        decl.add_ivar::<id>("_borderColor");
        decl.add_ivar::<f64>("_borderWidth");
        
        // Add methods
        unsafe {
            decl.add_method(
                sel!(drawRect:),
                draw_rect as extern "C" fn(&Object, Sel, NSRect),
            );
            decl.add_method(
                sel!(borderColor),
                border_color as extern "C" fn(&Object, Sel) -> id,
            );
            decl.add_method(
                sel!(setBorderColor:),
                set_border_color as extern "C" fn(&mut Object, Sel, id),
            );
            decl.add_method(
                sel!(borderWidth),
                border_width as extern "C" fn(&Object, Sel) -> f64,
            );
            decl.add_method(
                sel!(setBorderWidth:),
                set_border_width as extern "C" fn(&mut Object, Sel, f64),
            );
        }
        
        decl.register();
    });
}

pub struct HollowBorder {
    window: id,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    border_width: i32,
    border_color: u32,
}

unsafe impl Send for HollowBorder {}
unsafe impl Sync for HollowBorder {}

impl HollowBorder {
    pub fn new(
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        border_width: i32,
        border_color: u32,
    ) -> Option<Self> {
        log::info!(
            "Creating macOS hollow border at ({}, {}) size {}x{}, border_width={}, color={:06x}",
            x,
            y,
            width,
            height,
            border_width,
            border_color
        );
        
        unsafe {
            let _pool = NSAutoreleasePool::new(nil);
            
            // Create borderless window
            let style_mask = NSWindowStyleMask::NSBorderlessWindowMask;
            
            // Create window frame
            let frame = NSRect::new(
                NSPoint::new(x as f64, y as f64),
                NSSize::new(width as f64, height as f64)
            );
            
            let window = NSWindow::alloc(nil).initWithContentRect_styleMask_backing_defer_(
                frame,
                style_mask,
                NSBackingStoreType::NSBackingStoreBuffered,
                NO,
            );
            
            if window == nil {
                log::error!("Failed to create NSWindow for hollow border");
                return None;
            }
            
            // Configure window
            window.setOpaque_(NO);
            window.setBackgroundColor_(NSColor::clearColor(nil));
            
            // Set window level to float above other windows
            let _: () = msg_send![window, setLevel: 3i32]; // NSFloatingWindowLevel
            
            // Make it click-through
            window.setIgnoresMouseEvents_(YES);
            
            // Show the window
            window.makeKeyAndOrderFront_(nil);
            
            log::info!("macOS hollow border created successfully");
            
            Some(Self {
                window,
                x,
                y,
                width,
                height,
                border_width,
                border_color,
            })
        }
    }

    pub fn get_rect(&self) -> (i32, i32, i32, i32) {
        (self.x, self.y, self.width, self.height)
    }

    pub fn get_inner_rect(&self) -> (i32, i32, i32, i32) {
        let bw = self.border_width;
        (
            self.x + bw,
            self.y + bw,
            self.width - 2 * bw,
            self.height - 2 * bw,
        )
    }

    pub fn update_rect(&self, x: i32, y: i32, width: i32, height: i32) {
        log::info!(
            "Updating hollow border rect: ({}, {}) {}x{}",
            x,
            y,
            width,
            height
        );
        
        unsafe {
            let new_frame = NSRect::new(
                NSPoint::new(x as f64, y as f64),
                NSSize::new(width as f64, height as f64)
            );
            
            let _: () = msg_send![self.window, setFrame:new_frame display:YES];
        }
    }

    pub fn update_color(&self, color: u32) {
        log::info!("Updating hollow border color: {:06x}", color);
        
        unsafe {
            let r = ((color >> 16) & 0xFF) as f64 / 255.0;
            let g = ((color >> 8) & 0xFF) as f64 / 255.0;
            let b = (color & 0xFF) as f64 / 255.0;
            
            let ns_color: id = msg_send![class!(NSColor), colorWithRed:r green:g blue:b alpha:1.0];
            let _: () = msg_send![self.window, setBackgroundColor: ns_color];
        }
    }

    pub fn update_style(&self, width: i32, color: u32) {
        log::info!(
            "Updating hollow border style: width={}, color={:06x}",
            width,
            color
        );
        self.update_color(color);
    }

    pub fn hide(&self) {
        log::info!("Hiding macOS hollow border");
        unsafe {
            let _: () = msg_send![self.window, orderOut: nil];
        }
    }

    pub fn show(&self) {
        log::info!("Showing macOS hollow border");
        unsafe {
            self.window.makeKeyAndOrderFront_(nil);
        }
    }

    pub fn hwnd_value(&self) -> isize {
        self.window as isize
    }

    pub fn was_esc_pressed() -> bool {
        ESC_PRESSED.load(Ordering::Relaxed)
    }

    pub fn stop(&mut self) {
        log::info!("Stopping macOS hollow border");
        self.hide();
    }
}

impl Drop for HollowBorder {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![self.window, close];
        }
    }
}
