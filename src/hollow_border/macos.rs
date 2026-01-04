//! macOS Hollow Border Implementation
//!
//! Creates a transparent window with a colored border using NSWindow and custom NSView.

use cocoa::appkit::{NSWindow, NSView, NSWindowStyleMask, NSBackingStoreType, NSColor, NSScreen};
use cocoa::base::{id, nil, YES, NO};
use cocoa::foundation::{NSRect, NSPoint, NSSize, NSAutoreleasePool};
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use objc::{msg_send, sel, sel_impl, class};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

// GCD dispatch functions for main thread execution
extern "C" {
    static _dispatch_main_q: std::ffi::c_void;
    
    fn dispatch_sync_f(
        queue: *const std::ffi::c_void,
        context: *mut std::ffi::c_void,
        work: extern "C" fn(*mut std::ffi::c_void),
    );
    
    fn pthread_main_np() -> i32;
}

static ESC_PRESSED: AtomicBool = AtomicBool::new(false);
static REGISTER_CLASS: Once = Once::new();
static PREVIEW_MODE: AtomicBool = AtomicBool::new(true);

const EDGE_LEFT: i32 = 1 << 0;
const EDGE_RIGHT: i32 = 1 << 1;
const EDGE_BOTTOM: i32 = 1 << 2;
const EDGE_TOP: i32 = 1 << 3;

/// Callback executed on main thread to create hollow border window
extern "C" fn create_border_on_main_thread(context: *mut std::ffi::c_void) {
    let ctx = unsafe { &mut *(context as *mut CreateBorderContext) };
    
    println!("[HOLLOW_BORDER] create_border_on_main_thread ENTERED");
    
    unsafe {
        let _pool = NSAutoreleasePool::new(nil);
        
        println!("[HOLLOW_BORDER] Getting main screen...");
        let screen: id = msg_send![class!(NSScreen), mainScreen];
        let screen_frame: NSRect = msg_send![screen, frame];
        let screen_height = screen_frame.size.height;
        
        let macos_y = screen_height - (ctx.y as f64) - (ctx.height as f64);
        
        println!("[HOLLOW_BORDER] Creating NSWindow at ({}, {}) size {}x{}",
            ctx.x, macos_y, ctx.width, ctx.height);
        
        let style_mask = NSWindowStyleMask::NSBorderlessWindowMask | NSWindowStyleMask::NSResizableWindowMask;
        
        let frame = NSRect::new(
            NSPoint::new(ctx.x as f64, macos_y),
            NSSize::new(ctx.width as f64, ctx.height as f64)
        );
        
        let window = NSWindow::alloc(nil).initWithContentRect_styleMask_backing_defer_(
            frame,
            style_mask,
            NSBackingStoreType::NSBackingStoreBuffered,
            NO,
        );
        
        println!("[HOLLOW_BORDER] NSWindow created: {:?}", window);
        
        if window == nil {
            println!("[HOLLOW_BORDER] ERROR: Failed to create NSWindow");
            ctx.result_window = None;
            return;
        }
        
        println!("[HOLLOW_BORDER] Configuring window...");
        window.setOpaque_(NO);
        PREVIEW_MODE.store(true, Ordering::SeqCst);
        let preview_bg: id = msg_send![class!(NSColor), colorWithRed:0.125 green:0.125 blue:0.125 alpha:0.15];
        window.setBackgroundColor_(preview_bg);
        let _: () = msg_send![window, setMovableByWindowBackground: YES];
        let _: () = msg_send![window, setLevel: 3i32];
        window.setIgnoresMouseEvents_(YES); // Click-through enabled
        
        println!("[HOLLOW_BORDER] Creating custom view...");
        register_border_view_class();
        
        let view_class = Class::get("HollowBorderView").expect("HollowBorderView class not registered");
        let view: id = msg_send![view_class, alloc];
        let view_frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(ctx.width as f64, ctx.height as f64)
        );
        let view: id = msg_send![view, initWithFrame: view_frame];
        
        let mut view_obj: &mut Object = &mut *(view as *mut Object);
        view_obj.set_ivar::<i32>("_isResizing", 0);
        view_obj.set_ivar::<i32>("_resizeEdgeMask", 0);
        view_obj.set_ivar::<NSPoint>("_initialMouseScreen", NSPoint::new(0.0, 0.0));
        view_obj.set_ivar::<NSRect>("_initialWindowFrame", NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0)));
        
        let r = ((ctx.border_color >> 16) & 0xFF) as f64 / 255.0;
        let g = ((ctx.border_color >> 8) & 0xFF) as f64 / 255.0;
        let b = (ctx.border_color & 0xFF) as f64 / 255.0;
        let ns_border_color: id = msg_send![class!(NSColor), colorWithRed:r green:g blue:b alpha:1.0];
        let _: () = msg_send![view, setBorderColor: ns_border_color];
        let _: () = msg_send![view, setBorderWidth: ctx.border_width as f64];
        
        let _: () = msg_send![window, setContentView: view];
        
        println!("[HOLLOW_BORDER] Showing window...");
        window.makeKeyAndOrderFront_(nil);
        
        println!("[HOLLOW_BORDER] Window created successfully");
        ctx.result_window = Some(window);
    }
    
    println!("[HOLLOW_BORDER] create_border_on_main_thread EXITED");
}

/// Context for creating hollow border on main thread
struct CreateBorderContext {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    border_width: i32,
    border_color: u32,
    result_window: Option<id>,
}

// Custom NSView subclass that draws the hollow border
extern "C" fn draw_rect(this: &Object, _cmd: Sel, _dirty_rect: NSRect) {
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
        
        // Preview mode: fill interior with a very transparent dark overlay (like Windows)
        if PREVIEW_MODE.load(Ordering::SeqCst) {
            let bg: id = msg_send![class!(NSColor), colorWithRed:0.125 green:0.125 blue:0.125 alpha:0.15];
            let _: () = msg_send![bg, set];
            let bg_path: id = msg_send![class!(NSBezierPath), bezierPathWithRect: outer_rect];
            let _: () = msg_send![bg_path, fill];
            // Restore border color for border fill
            let _: () = msg_send![border_color, set];
        }
        
        // Fill the border area (outer rect minus inner rect)
        let path: id = msg_send![class!(NSBezierPath), bezierPath];
        let _: () = msg_send![path, appendBezierPathWithRect: outer_rect];
        let _: () = msg_send![path, appendBezierPathWithRect: inner_rect];
        let _: () = msg_send![path, setWindingRule: 1]; // NSEvenOddWindingRule
        let _: () = msg_send![path, fill];

        // Draw thicker corner indicators (Windows parity)
        let corner_length = 16.0f64
            .min(bounds.size.width / 5.0)
            .min(bounds.size.height / 5.0)
            .max(8.0);
        let corner_thickness = (border_width + 1.0).max(4.0);

        let w = bounds.size.width;
        let h = bounds.size.height;

        // Top-left
        let tl_h = NSRect::new(
            NSPoint::new(0.0, h - corner_thickness),
            NSSize::new(corner_length, corner_thickness),
        );
        let tl_v = NSRect::new(
            NSPoint::new(0.0, h - corner_length),
            NSSize::new(corner_thickness, corner_length),
        );
        // Top-right
        let tr_h = NSRect::new(
            NSPoint::new(w - corner_length, h - corner_thickness),
            NSSize::new(corner_length, corner_thickness),
        );
        let tr_v = NSRect::new(
            NSPoint::new(w - corner_thickness, h - corner_length),
            NSSize::new(corner_thickness, corner_length),
        );
        // Bottom-left
        let bl_h = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(corner_length, corner_thickness),
        );
        let bl_v = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(corner_thickness, corner_length),
        );
        // Bottom-right
        let br_h = NSRect::new(
            NSPoint::new(w - corner_length, 0.0),
            NSSize::new(corner_length, corner_thickness),
        );
        let br_v = NSRect::new(
            NSPoint::new(w - corner_thickness, 0.0),
            NSSize::new(corner_thickness, corner_length),
        );

        let rects = [tl_h, tl_v, tr_h, tr_v, bl_h, bl_v, br_h, br_v];
        for r in rects {
            let p: id = msg_send![class!(NSBezierPath), bezierPathWithRect: r];
            let _: () = msg_send![p, fill];
        }
    }
}

extern "C" fn hit_test(this: &Object, _cmd: Sel, point: NSPoint) -> id {
    unsafe {
        let window: id = msg_send![this, window];
        let preview_mode = PREVIEW_MODE.load(Ordering::SeqCst);
        
        // In preview mode, the whole window should be interactive.
        if preview_mode {
            if window != nil {
                let _: () = msg_send![window, setIgnoresMouseEvents: NO];
            }
            return this as *const _ as id;
        }

        // Capture mode: only border area (and top edge) should accept mouse.
        let bounds: NSRect = msg_send![this, bounds];
        let border_width: f64 = msg_send![this, borderWidth];

        let hit_margin = border_width.max(8.0);
        let on_left = point.x >= 0.0 && point.x < hit_margin;
        let on_right = point.x <= bounds.size.width && point.x > bounds.size.width - hit_margin;
        let on_bottom = point.y >= 0.0 && point.y < hit_margin;
        let on_top = point.y <= bounds.size.height && point.y > bounds.size.height - hit_margin;

        if on_left || on_right || on_bottom || on_top {
            // On border edge - make window interactive
            if window != nil {
                let _: () = msg_send![window, setIgnoresMouseEvents: NO];
            }
            return this as *const _ as id;
        }

        // Interior - make window click-through
        if window != nil {
            let _: () = msg_send![window, setIgnoresMouseEvents: YES];
        }
        nil
    }
}

extern "C" fn mouse_down(this: &mut Object, _cmd: Sel, event: id) {
    unsafe {
        let window: id = msg_send![this, window];
        if window == nil {
            return;
        }

        let loc_in_window: NSPoint = msg_send![event, locationInWindow];
        let point: NSPoint = msg_send![this, convertPoint: loc_in_window fromView: nil];
        let bounds: NSRect = msg_send![this, bounds];
        let border_width: f64 = msg_send![this, borderWidth];

        let border = border_width.max(8.0);
        let corner = (border * 2.0).max(20.0);

        let on_left = point.x >= 0.0 && point.x < corner;
        let on_right = point.x <= bounds.size.width && point.x > bounds.size.width - corner;
        let on_bottom = point.y >= 0.0 && point.y < corner;
        let on_top = point.y <= bounds.size.height && point.y > bounds.size.height - corner;

        let mut edge_mask: i32 = 0;
        if on_left { edge_mask |= EDGE_LEFT; }
        if on_right { edge_mask |= EDGE_RIGHT; }
        if on_bottom { edge_mask |= EDGE_BOTTOM; }
        if on_top { edge_mask |= EDGE_TOP; }

        // In capture mode, top edge should drag (like Windows).
        if !PREVIEW_MODE.load(Ordering::SeqCst) {
            let top_hit = point.y > bounds.size.height - border;
            if top_hit {
                let _: () = msg_send![window, performWindowDragWithEvent: event];
                return;
            }
        }

        if edge_mask != 0 {
            // Begin resize.
            this.set_ivar::<i32>("_isResizing", 1);
            this.set_ivar::<i32>("_resizeEdgeMask", edge_mask);

            let screen_point: NSPoint = msg_send![window, convertPointToScreen: loc_in_window];
            this.set_ivar::<NSPoint>("_initialMouseScreen", screen_point);
            let frame: NSRect = msg_send![window, frame];
            this.set_ivar::<NSRect>("_initialWindowFrame", frame);
            return;
        }

        // Otherwise, drag in preview mode.
        if PREVIEW_MODE.load(Ordering::SeqCst) {
            let _: () = msg_send![window, performWindowDragWithEvent: event];
        }
    }
}

extern "C" fn mouse_dragged(this: &Object, _cmd: Sel, event: id) {
    unsafe {
        let window: id = msg_send![this, window];
        if window == nil {
            return;
        }

        let is_resizing = *this.get_ivar::<i32>("_isResizing") != 0;
        if !is_resizing {
            return;
        }

        let edge_mask = *this.get_ivar::<i32>("_resizeEdgeMask");
        let initial_mouse = *this.get_ivar::<NSPoint>("_initialMouseScreen");
        let initial_frame = *this.get_ivar::<NSRect>("_initialWindowFrame");

        let loc_in_window: NSPoint = msg_send![event, locationInWindow];
        let current_mouse: NSPoint = msg_send![window, convertPointToScreen: loc_in_window];

        let dx = current_mouse.x - initial_mouse.x;
        let dy = current_mouse.y - initial_mouse.y;

        let mut new_frame = initial_frame;
        let min_w: f64 = 80.0;
        let min_h: f64 = 60.0;

        if (edge_mask & EDGE_LEFT) != 0 {
            new_frame.origin.x += dx;
            new_frame.size.width -= dx;
        }
        if (edge_mask & EDGE_RIGHT) != 0 {
            new_frame.size.width += dx;
        }
        if (edge_mask & EDGE_BOTTOM) != 0 {
            new_frame.origin.y += dy;
            new_frame.size.height -= dy;
        }
        if (edge_mask & EDGE_TOP) != 0 {
            new_frame.size.height += dy;
        }

        if new_frame.size.width < min_w {
            new_frame.size.width = min_w;
            if (edge_mask & EDGE_LEFT) != 0 {
                new_frame.origin.x = initial_frame.origin.x + (initial_frame.size.width - min_w);
            }
        }
        if new_frame.size.height < min_h {
            new_frame.size.height = min_h;
            if (edge_mask & EDGE_BOTTOM) != 0 {
                new_frame.origin.y = initial_frame.origin.y + (initial_frame.size.height - min_h);
            }
        }

        let _: () = msg_send![window, setFrame: new_frame display: YES];
        let _: () = msg_send![this, setNeedsDisplay: YES];
    }
}

extern "C" fn mouse_up(this: &mut Object, _cmd: Sel, _event: id) {
    unsafe {
        this.set_ivar::<i32>("_isResizing", 0);
        this.set_ivar::<i32>("_resizeEdgeMask", 0);
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
        decl.add_ivar::<i32>("_isResizing");
        decl.add_ivar::<i32>("_resizeEdgeMask");
        decl.add_ivar::<NSPoint>("_initialMouseScreen");
        decl.add_ivar::<NSRect>("_initialWindowFrame");
        
        // Add methods
        unsafe {
            decl.add_method(
                sel!(drawRect:),
                draw_rect as extern "C" fn(&Object, Sel, NSRect),
            );
            decl.add_method(
                sel!(hitTest:),
                hit_test as extern "C" fn(&Object, Sel, NSPoint) -> id,
            );
            decl.add_method(
                sel!(mouseDown:),
                mouse_down as extern "C" fn(&mut Object, Sel, id),
            );
            decl.add_method(
                sel!(mouseDragged:),
                mouse_dragged as extern "C" fn(&Object, Sel, id),
            );
            decl.add_method(
                sel!(mouseUp:),
                mouse_up as extern "C" fn(&mut Object, Sel, id),
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
        println!("[HOLLOW_BORDER] HollowBorder::new called at ({}, {}) size {}x{}", x, y, width, height);
        
        log::info!(
            "Creating macOS hollow border at ({}, {}) size {}x{}, border_width={}, color={:06x}",
            x,
            y,
            width,
            height,
            border_width,
            border_color
        );
        
        // Check if we're on main thread
        let is_main = unsafe { pthread_main_np() } != 0;
        println!("[HOLLOW_BORDER] Current thread is main: {}", is_main);
        
        let mut ctx = CreateBorderContext {
            x,
            y,
            width,
            height,
            border_width,
            border_color,
            result_window: None,
        };
        
        if is_main {
            // Already on main thread
            println!("[HOLLOW_BORDER] Already on main thread, calling directly");
            create_border_on_main_thread(&mut ctx as *mut CreateBorderContext as *mut std::ffi::c_void);
        } else {
            // Dispatch to main thread
            println!("[HOLLOW_BORDER] Dispatching to main thread via dispatch_sync_f");
            unsafe {
                let main_queue = &_dispatch_main_q as *const std::ffi::c_void;
                dispatch_sync_f(
                    main_queue,
                    &mut ctx as *mut CreateBorderContext as *mut std::ffi::c_void,
                    create_border_on_main_thread,
                );
            }
            println!("[HOLLOW_BORDER] dispatch_sync_f returned");
        }
        
        let window = ctx.result_window?;
        
        println!("[HOLLOW_BORDER] HollowBorder created successfully");
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
        
        struct UpdateRectContext {
            window: id,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
        }
        
        extern "C" fn update_rect_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
            let ctx = unsafe { &*(ctx_ptr as *const UpdateRectContext) };
            unsafe {
                let screen: id = msg_send![class!(NSScreen), mainScreen];
                let screen_frame: NSRect = msg_send![screen, frame];
                let screen_height = screen_frame.size.height;
                let macos_y = screen_height - (ctx.y as f64) - (ctx.height as f64);
                
                let new_frame = NSRect::new(
                    NSPoint::new(ctx.x as f64, macos_y),
                    NSSize::new(ctx.width as f64, ctx.height as f64)
                );
                
                let _: () = msg_send![ctx.window, setFrame:new_frame display:YES];
            }
        }
        
        let mut context = UpdateRectContext {
            window: self.window,
            x,
            y,
            width,
            height,
        };
        
        unsafe {
            let is_main = pthread_main_np() != 0;
            if !is_main {
                dispatch_sync_f(
                    &_dispatch_main_q,
                    &mut context as *mut _ as *mut std::ffi::c_void,
                    update_rect_on_main_thread,
                );
            } else {
                update_rect_on_main_thread(&mut context as *mut _ as *mut std::ffi::c_void);
            }
        }
    }

    pub fn update_color(&self, color: u32) {
        log::info!("Updating hollow border color: {:06x}", color);
        
        struct UpdateColorContext {
            window: id,
            color: u32,
        }
        
        extern "C" fn update_color_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
            let ctx = unsafe { &*(ctx_ptr as *const UpdateColorContext) };
            unsafe {
                let r = ((ctx.color >> 16) & 0xFF) as f64 / 255.0;
                let g = ((ctx.color >> 8) & 0xFF) as f64 / 255.0;
                let b = (ctx.color & 0xFF) as f64 / 255.0;
                
                let ns_color: id = msg_send![class!(NSColor), colorWithRed:r green:g blue:b alpha:1.0];
                let view: id = msg_send![ctx.window, contentView];
                if view != nil {
                    let _: () = msg_send![view, setBorderColor: ns_color];
                }
            }
        }
        
        let mut context = UpdateColorContext {
            window: self.window,
            color,
        };
        
        unsafe {
            let is_main = pthread_main_np() != 0;
            if !is_main {
                dispatch_sync_f(
                    &_dispatch_main_q,
                    &mut context as *mut _ as *mut std::ffi::c_void,
                    update_color_on_main_thread,
                );
            } else {
                update_color_on_main_thread(&mut context as *mut _ as *mut std::ffi::c_void);
            }
        }
    }

    pub fn update_style(&self, width: i32, color: u32) {
        log::info!(
            "Updating hollow border style: width={}, color={:06x}",
            width,
            color
        );
        
        struct UpdateStyleContext {
            window: id,
            width: i32,
            color: u32,
        }
        
        extern "C" fn update_style_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
            let ctx = unsafe { &*(ctx_ptr as *const UpdateStyleContext) };
            unsafe {
                let view: id = msg_send![ctx.window, contentView];
                if view != nil {
                    let _: () = msg_send![view, setBorderWidth: ctx.width as f64];
                    
                    let r = ((ctx.color >> 16) & 0xFF) as f64 / 255.0;
                    let g = ((ctx.color >> 8) & 0xFF) as f64 / 255.0;
                    let b = (ctx.color & 0xFF) as f64 / 255.0;
                    let ns_color: id = msg_send![class!(NSColor), colorWithRed:r green:g blue:b alpha:1.0];
                    let _: () = msg_send![view, setBorderColor: ns_color];
                }
            }
        }
        
        let mut context = UpdateStyleContext {
            window: self.window,
            width,
            color,
        };
        
        unsafe {
            let is_main = pthread_main_np() != 0;
            if !is_main {
                dispatch_sync_f(
                    &_dispatch_main_q,
                    &mut context as *mut _ as *mut std::ffi::c_void,
                    update_style_on_main_thread,
                );
            } else {
                update_style_on_main_thread(&mut context as *mut _ as *mut std::ffi::c_void);
            }
        }
    }

    pub fn hide(&self) {
        println!("[HOLLOW_BORDER] hide() called");
        log::info!("Hiding macOS hollow border");
        
        extern "C" fn hide_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
            let window = ctx_ptr as id;
            unsafe {
                let _: () = msg_send![window, orderOut: nil];
            }
        }
        
        unsafe {
            let is_main = pthread_main_np() != 0;
            if !is_main {
                dispatch_sync_f(
                    &_dispatch_main_q,
                    self.window as *mut std::ffi::c_void,
                    hide_on_main_thread,
                );
            } else {
                let _: () = msg_send![self.window, orderOut: nil];
            }
        }
    }

    pub fn show(&self) {
        println!("[HOLLOW_BORDER] show() called");
        log::info!("Showing macOS hollow border");
        
        extern "C" fn show_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
            let window = ctx_ptr as id;
            unsafe {
                let _: () = msg_send![window, makeKeyAndOrderFront: nil];
            }
        }
        
        unsafe {
            let is_main = pthread_main_np() != 0;
            if !is_main {
                dispatch_sync_f(
                    &_dispatch_main_q,
                    self.window as *mut std::ffi::c_void,
                    show_on_main_thread,
                );
            } else {
                let _: () = msg_send![self.window, makeKeyAndOrderFront: nil];
            }
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

    /// Set capture mode: interior click-through, only border is interactive
    pub fn set_capture_mode(&mut self) {
        println!("[HOLLOW_BORDER] set_capture_mode() called");
        log::info!("Setting hollow border to capture mode (click-through)");
        PREVIEW_MODE.store(false, Ordering::SeqCst);
        
        struct SetCaptureModeContext {
            window: id,
        }
        
        extern "C" fn set_capture_mode_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
            let ctx = unsafe { &*(ctx_ptr as *const SetCaptureModeContext) };
            unsafe {
                // Disable moving window from anywhere except edges
                let _: () = msg_send![ctx.window, setMovableByWindowBackground: NO];
                let _: () = msg_send![ctx.window, setIgnoresMouseEvents: NO];
                let _: () = msg_send![ctx.window, setBackgroundColor: NSColor::clearColor(nil)];

                let view: id = msg_send![ctx.window, contentView];
                if view != nil {
                    let _: () = msg_send![view, setNeedsDisplay: YES];
                }
            }
        }
        
        let mut context = SetCaptureModeContext {
            window: self.window,
        };
        
        unsafe {
            let is_main = pthread_main_np() != 0;
            if !is_main {
                println!("[HOLLOW_BORDER] set_capture_mode dispatching to main");
                dispatch_sync_f(
                    &_dispatch_main_q,
                    &mut context as *mut _ as *mut std::ffi::c_void,
                    set_capture_mode_on_main_thread,
                );
            } else {
                println!("[HOLLOW_BORDER] set_capture_mode on main thread");
                set_capture_mode_on_main_thread(&mut context as *mut _ as *mut std::ffi::c_void);
            }
        }
    }

    /// Set preview mode: interior is draggable (not click-through)
    pub fn set_preview_mode(&mut self) {
        log::info!("Setting hollow border to preview mode (draggable)");
        PREVIEW_MODE.store(true, Ordering::SeqCst);
        unsafe {
            let _: () = msg_send![self.window, setIgnoresMouseEvents: NO];
            let preview_bg: id = msg_send![class!(NSColor), colorWithRed:0.125 green:0.125 blue:0.125 alpha:0.15];
            let _: () = msg_send![self.window, setBackgroundColor: preview_bg];

            let view: id = msg_send![self.window, contentView];
            if view != nil {
                let _: () = msg_send![view, setNeedsDisplay: YES];
            }
        }
    }
}

impl Drop for HollowBorder {
    fn drop(&mut self) {
        println!("[HOLLOW_BORDER] Drop called");
        
        extern "C" fn close_window_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
            let window = ctx_ptr as id;
            unsafe {
                println!("[HOLLOW_BORDER] Hiding and closing window on main thread");
                let _: () = msg_send![window, orderOut: nil];
                let _: () = msg_send![window, close];
            }
        }
        
        unsafe {
            let is_main = pthread_main_np() != 0;
            println!("[HOLLOW_BORDER] Drop on main thread: {}", is_main);
            
            if !is_main {
                println!("[HOLLOW_BORDER] Dispatching window close to main thread");
                dispatch_sync_f(
                    &_dispatch_main_q,
                    self.window as *mut std::ffi::c_void,
                    close_window_on_main_thread,
                );
            } else {
                println!("[HOLLOW_BORDER] Closing window directly on main thread");
                let _: () = msg_send![self.window, orderOut: nil];
                let _: () = msg_send![self.window, close];
            }
        }
        
        println!("[HOLLOW_BORDER] Drop completed");
    }
}
