//! macOS Hollow Border Implementation
//!
//! Creates a transparent window with a colored border using NSPanel and custom NSView.

use crate::traits::BorderWindow;
use cocoa::appkit::{NSBackingStoreType, NSColor, NSScreen, NSView, NSWindow, NSWindowStyleMask};
use cocoa::base::{id, nil, NO, YES};
use cocoa::foundation::{NSAutoreleasePool, NSPoint, NSRect, NSSize};
use lazy_static::lazy_static;
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use objc::{class, msg_send, sel, sel_impl};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;
use std::sync::{Arc, Mutex};
use std::thread;

// Global cache for border rectangle - updated by mouse events
// Allows lock-free reads in capture loop (60 FPS)
lazy_static! {
    static ref BORDER_RECT_CACHE: Arc<Mutex<(i32, i32, i32, i32)>> =
        Arc::new(Mutex::new((0, 0, 800, 600)));
}

// Flag indicating border is being dragged/resized
// When true, capture should be paused for performance
static BORDER_INTERACTING: AtomicBool = AtomicBool::new(false);

// Callback to notify when border interaction completes (mouseUp)
// Called once after drag/resize finishes - updates capture region and windows
type BorderInteractionCompleteCallback = Box<dyn Fn(i32, i32, i32, i32) + Send + Sync>;
lazy_static! {
    static ref BORDER_INTERACTION_COMPLETE_CALLBACK: Arc<Mutex<Option<BorderInteractionCompleteCallback>>> =
        Arc::new(Mutex::new(None));
}

/// Register callback to be notified when border drag/resize completes
/// Fires only on mouseUp - allows pausing capture during interaction for performance
pub fn set_border_interaction_complete_callback<F>(callback: F)
where
    F: Fn(i32, i32, i32, i32) + Send + Sync + 'static,
{
    if let Ok(mut cb) = BORDER_INTERACTION_COMPLETE_CALLBACK.lock() {
        *cb = Some(Box::new(callback));
    }
}

static MOUSE_POLL_RUNNING: AtomicBool = AtomicBool::new(false);

/// Check if border is currently being dragged or resized
pub fn is_border_interacting() -> bool {
    BORDER_INTERACTING.load(Ordering::SeqCst)
}

lazy_static! {
    static ref MOUSE_POLL_THREAD: Mutex<Option<thread::JoinHandle<()>>> = Mutex::new(None);
}

fn stop_mouse_poll() {
    MOUSE_POLL_RUNNING.store(false, Ordering::SeqCst);
    if let Ok(mut handle) = MOUSE_POLL_THREAD.lock() {
        if let Some(join) = handle.take() {
            // IMPORTANT: don't join from the main thread.
            // The poller uses dispatch_sync_f -> main queue, so joining on main can deadlock.
            let is_main = unsafe { pthread_main_np() } != 0;
            if is_main {
                std::thread::spawn(move || {
                    let _ = join.join();
                });
            } else {
                let _ = join.join();
            }
        }
    }
}

fn start_mouse_poll(window: id) {
    // Only meaningful in capture mode.
    stop_mouse_poll();
    MOUSE_POLL_RUNNING.store(true, Ordering::SeqCst);

    let window_ptr = window as isize;
    if let Ok(mut handle) = MOUSE_POLL_THREAD.lock() {
        *handle = Some(thread::spawn(move || {
            struct PollCtx {
                window_ptr: isize,
                last_interactive: bool,
                last_cursor_kind: i32,
            }

            const CURSOR_NONE: i32 = 0;
            const CURSOR_CROSSHAIR: i32 = 1;
            const CURSOR_OPEN_HAND: i32 = 2;
            const CURSOR_RESIZE_LR: i32 = 3;
            const CURSOR_RESIZE_UD: i32 = 4;

            extern "C" fn poll_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
                let ctx = unsafe { &mut *(ctx_ptr as *mut PollCtx) };
                unsafe {
                    let window: id = ctx.window_ptr as *mut objc::runtime::Object;
                    if window == nil {
                        return;
                    }

                    let view: id = msg_send![window, contentView];

                    // Helper: make sure cursor rects are refreshed when we start accepting events.
                    let refresh_cursor_rects = |window: id, view: id| {
                        if view != nil {
                            let _: () = msg_send![window, enableCursorRects];
                            let _: () = msg_send![window, setAcceptsMouseMovedEvents: YES];
                            let _: () = msg_send![window, invalidateCursorRectsForView: view];
                            // Calling NSView::resetCursorRects directly is not always enough to
                            // immediately update the current cursor (especially after toggling
                            // ignoresMouseEvents). We still call it to rebuild rects.
                            let _: () = msg_send![view, resetCursorRects];
                        }
                    };

                    // Helper: set the cursor immediately based on current mouse position.
                    // This avoids requiring an initial click/resize before cursor feedback works.
                    let desired_cursor_kind =
                        |_window: id, view: id, x: f64, y: f64, width: f64, height: f64| -> i32 {
                            if view == nil {
                                return CURSOR_NONE;
                            }

                            let border_width: f64 = msg_send![view, borderWidth];
                            let border = border_width.max(8.0);
                            let corner = (border * 2.0).max(20.0);

                            let in_left = x >= 0.0 && x < border;
                            let in_right = x <= width && x > width - border;
                            let in_bottom = y >= 0.0 && y < border;
                            let in_top = y <= height && y > height - border;

                            let in_tl =
                                x >= 0.0 && x < corner && y <= height && y > height - corner;
                            let in_tr = x <= width
                                && x > width - corner
                                && y <= height
                                && y > height - corner;
                            let in_bl = x >= 0.0 && x < corner && y >= 0.0 && y < corner;
                            let in_br = x <= width && x > width - corner && y >= 0.0 && y < corner;

                            if in_tl || in_tr || in_bl || in_br {
                                CURSOR_CROSSHAIR
                            } else if in_top {
                                CURSOR_OPEN_HAND
                            } else if in_bottom {
                                CURSOR_RESIZE_UD
                            } else if in_left || in_right {
                                CURSOR_RESIZE_LR
                            } else {
                                CURSOR_NONE
                            }
                        };

                    let set_cursor_kind = |kind: i32| {
                        let cursor: id = match kind {
                            CURSOR_CROSSHAIR => msg_send![class!(NSCursor), crosshairCursor],
                            CURSOR_OPEN_HAND => msg_send![class!(NSCursor), openHandCursor],
                            CURSOR_RESIZE_LR => msg_send![class!(NSCursor), resizeLeftRightCursor],
                            CURSOR_RESIZE_UD => msg_send![class!(NSCursor), resizeUpDownCursor],
                            _ => nil,
                        };

                        if cursor != nil {
                            let _: () = msg_send![cursor, set];
                        }
                    };

                    // During interaction we must receive drag events.
                    if is_border_interacting() {
                        if !ctx.last_interactive {
                            let _: () = msg_send![window, setIgnoresMouseEvents: NO];
                            refresh_cursor_rects(window, view);
                            ctx.last_interactive = true;
                        }
                        return;
                    }

                    // If we're not in capture mode, keep interactive.
                    if PREVIEW_MODE.load(Ordering::SeqCst) {
                        if !ctx.last_interactive {
                            let _: () = msg_send![window, setIgnoresMouseEvents: NO];
                            refresh_cursor_rects(window, view);
                            ctx.last_interactive = true;
                        }
                        return;
                    }

                    let frame: NSRect = msg_send![window, frame];
                    let mouse: NSPoint = msg_send![class!(NSEvent), mouseLocation];

                    let x = mouse.x - frame.origin.x;
                    let y = mouse.y - frame.origin.y;

                    // If cursor isn't over our window, be fully click-through.
                    if x < 0.0 || y < 0.0 || x > frame.size.width || y > frame.size.height {
                        if ctx.last_interactive {
                            let _: () = msg_send![window, setIgnoresMouseEvents: YES];
                            ctx.last_interactive = false;
                            ctx.last_cursor_kind = CURSOR_NONE;
                        }
                        return;
                    }

                    let border_width: f64 = if view != nil {
                        let bw: f64 = msg_send![view, borderWidth];
                        bw
                    } else {
                        0.0
                    };
                    let hit_margin = border_width.max(8.0);

                    let on_left = x >= 0.0 && x < hit_margin;
                    let on_right = x <= frame.size.width && x > frame.size.width - hit_margin;
                    let on_bottom = y >= 0.0 && y < hit_margin;
                    let on_top = y <= frame.size.height && y > frame.size.height - hit_margin;

                    if on_left || on_right || on_bottom || on_top {
                        if !ctx.last_interactive {
                            let _: () = msg_send![window, setIgnoresMouseEvents: NO];
                            refresh_cursor_rects(window, view);
                            ctx.last_interactive = true;
                        }

                        // CURSOR CHANGE DISABLED: macOS doesn't support cursor change when window inactive
                        // Cursor will only change when user clicks and window becomes active
                        // This is macOS limitation, works fine on Windows
                        // let kind = desired_cursor_kind(window, view, x, y, frame.size.width, frame.size.height);
                        // if kind != ctx.last_cursor_kind {
                        //     set_cursor_kind(kind);
                        //     ctx.last_cursor_kind = kind;
                        // }
                    } else {
                        if ctx.last_interactive {
                            let _: () = msg_send![window, setIgnoresMouseEvents: YES];
                            ctx.last_interactive = false;
                            ctx.last_cursor_kind = CURSOR_NONE;
                        }
                    }
                }
            }

            // Polling thread for click-through management (still needed for capture mode)
            let mut ctx = PollCtx {
                window_ptr,
                last_interactive: false,
                last_cursor_kind: CURSOR_NONE,
            };
            while MOUSE_POLL_RUNNING.load(Ordering::SeqCst) {
                unsafe {
                    dispatch_sync_f(
                        &_dispatch_main_q,
                        &mut ctx as *mut _ as *mut std::ffi::c_void,
                        poll_on_main_thread,
                    );
                }
                thread::sleep(std::time::Duration::from_millis(16));
            }
        }));
    }
}

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

static REGISTER_CLASS: Once = Once::new();
static PREVIEW_MODE: AtomicBool = AtomicBool::new(true);

const EDGE_LEFT: i32 = 1 << 0;
const EDGE_RIGHT: i32 = 1 << 1;
const EDGE_BOTTOM: i32 = 1 << 2;
const EDGE_TOP: i32 = 1 << 3;

/// Callback executed on main thread to create hollow border window
extern "C" fn create_border_on_main_thread(context: *mut std::ffi::c_void) {
    let ctx = unsafe { &mut *(context as *mut CreateBorderContext) };

    unsafe {
        let _pool = NSAutoreleasePool::new(nil);
        let screen: id = msg_send![class!(NSScreen), mainScreen];
        let screen_frame: NSRect = msg_send![screen, frame];
        let screen_height = screen_frame.size.height;

        let macos_y = screen_height - (ctx.y as f64) - (ctx.height as f64);

        // Use NSPanel instead of NSWindow - panels don't activate the application
        // NSNonactivatingPanelMask (1 << 7) = 128 prevents application activation
        const NS_NONACTIVATING_PANEL_MASK: u64 = 1 << 7;
        // IMPORTANT: do NOT include NSResizableWindowMask.
        // We implement manual resize in mouseDragged; native resize conflicts with top-edge drag.
        let style_mask = NSWindowStyleMask::NSBorderlessWindowMask;
        let style_mask_raw = style_mask.bits() | NS_NONACTIVATING_PANEL_MASK;

        let frame = NSRect::new(
            NSPoint::new(ctx.x as f64, macos_y),
            NSSize::new(ctx.width as f64, ctx.height as f64),
        );

        // Register custom classes first
        register_border_view_class();

        // Create as custom NSPanel (utility window type) that can become key
        let panel_class =
            Class::get("HollowBorderPanel").expect("HollowBorderPanel class not registered");
        let window: id = msg_send![panel_class, alloc];
        let window: id = msg_send![window,
            initWithContentRect:frame
            styleMask:style_mask_raw
            backing:NSBackingStoreType::NSBackingStoreBuffered as u64
            defer:NO
        ];

        if window == nil {
            log::error!("Failed to create hollow border NSWindow");
            ctx.result_window = None;
            return;
        }

        window.setOpaque_(NO);
        PREVIEW_MODE.store(true, Ordering::SeqCst);
        let preview_bg: id =
            msg_send![class!(NSColor), colorWithRed:0.125 green:0.125 blue:0.125 alpha:0.15];
        window.setBackgroundColor_(preview_bg);
        let _: () = msg_send![window, setMovableByWindowBackground: YES];

        // NSPanel-specific settings
        // Note: Removed setBecomesKeyOnlyIfNeeded to allow immediate key window activation
        let _: () = msg_send![window, setWorksWhenModal: YES];

        // Set window level to floating (3) - always on top
        let _: () = msg_send![window, setLevel: 3i32];

        // Configure collection behavior to prevent affecting main window
        // - Stay in all spaces (CanJoinAllSpaces)
        // - Don't show in Cmd+Tab (IgnoresCycle)
        // Note: Removed Transient and Stationary to allow main window activation
        let collection_behavior = (1u64 << 0) | // CanJoinAllSpaces
            (1u64 << 6); // IgnoresCycle
        let _: () = msg_send![window, setCollectionBehavior: collection_behavior];

        // Start in click-through mode; capture mode will dynamically enable events on the border.
        window.setIgnoresMouseEvents_(YES);

        // Accept mouse events without becoming key window
        // This allows interaction without activating the main application window
        let _: () = msg_send![window, setAcceptsMouseMovedEvents: YES];

        // Note: register_border_view_class() already called before window creation

        let view_class =
            Class::get("HollowBorderView").expect("HollowBorderView class not registered");
        let view: id = msg_send![view_class, alloc];
        let view_frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(ctx.width as f64, ctx.height as f64),
        );
        let view: id = msg_send![view, initWithFrame: view_frame];

        let mut view_obj: &mut Object = &mut *(view as *mut Object);
        view_obj.set_ivar::<i32>("_isResizing", 0);
        view_obj.set_ivar::<i32>("_resizeEdgeMask", 0);
        view_obj.set_ivar::<NSPoint>("_initialMouseScreen", NSPoint::new(0.0, 0.0));
        view_obj.set_ivar::<NSRect>(
            "_initialWindowFrame",
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0)),
        );

        let r = ((ctx.border_color >> 16) & 0xFF) as f64 / 255.0;
        let g = ((ctx.border_color >> 8) & 0xFF) as f64 / 255.0;
        let b = (ctx.border_color & 0xFF) as f64 / 255.0;
        let ns_border_color: id =
            msg_send![class!(NSColor), colorWithRed:r green:g blue:b alpha:1.0];
        let _: () = msg_send![view, setBorderColor: ns_border_color];
        let _: () = msg_send![view, setBorderWidth: ctx.border_width as f64];

        let _: () = msg_send![window, setContentView: view];

        // Enable cursor rect tracking
        let _: () = msg_send![window, invalidateCursorRectsForView: view];

        // Initialize tracking areas for mouse movement detection
        let _: () = msg_send![view, updateTrackingAreas];

        // Show window WITHOUT making it key window
        // This prevents the border from activating the main application window
        let _: () = msg_send![window, orderFront: nil];

        // Initialize cache with window's initial position
        update_border_cache_from_window(window);

        log::info!("Border window created and shown (non-key)");
        ctx.result_window = Some(window);
    }

    tracing::trace!("create_border_on_main_thread callback exited");
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
            let bg: id =
                msg_send![class!(NSColor), colorWithRed:0.125 green:0.125 blue:0.125 alpha:0.15];
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

        // REC indicator removed from border view to prevent screen sharing capture
        // Now handled by separate overlay window with NSWindowSharingNone
    }
}

// draw_rec_indicator function removed - REC now in separate window

extern "C" fn hit_test(this: &Object, _cmd: Sel, point: NSPoint) -> id {
    unsafe {
        let preview_mode = PREVIEW_MODE.load(Ordering::SeqCst);

        // In preview mode, the whole window should be interactive.
        if preview_mode {
            return this as *const _ as id;
        }

        // Capture mode: only border area should respond to hit test
        // Interior clicks pass through to underlying windows
        let bounds: NSRect = msg_send![this, bounds];
        let border_width: f64 = msg_send![this, borderWidth];

        let hit_margin = border_width.max(8.0);
        let on_left = point.x >= 0.0 && point.x < hit_margin;
        let on_right = point.x <= bounds.size.width && point.x > bounds.size.width - hit_margin;
        let on_bottom = point.y >= 0.0 && point.y < hit_margin;
        let on_top = point.y <= bounds.size.height && point.y > bounds.size.height - hit_margin;

        if on_left || on_right || on_bottom || on_top {
            // CRITICAL: Force window to accept events at hit test time
            let window: id = msg_send![this, window];
            if window != nil {
                let current_ignores: bool = msg_send![window, ignoresMouseEvents];
                if current_ignores {
                    tracing::warn!("ignoresMouseEvents was true, setting to false");
                    let _: () = msg_send![window, setIgnoresMouseEvents: NO];
                }

                // CRITICAL: Make window key to receive mouse events (required for trackpad)
                let is_key: bool = msg_send![window, isKeyWindow];
                if !is_key {
                    tracing::trace!("Making window key to receive mouse events");
                    let _: () = msg_send![window, makeKeyWindow];
                    // Also make view first responder to ensure it gets events
                    let success: bool = msg_send![window, makeFirstResponder: this];
                    tracing::trace!(success, "makeFirstResponder result");
                }
            }
            // On border edge - return view to handle events
            return this as *const _ as id;
        }

        tracing::trace!(x = point.x, y = point.y, "Interior click (click-through)");
        // Interior must be true click-through: the window must ignore mouse events.
        let window: id = msg_send![this, window];
        if window != nil {
            let _: () = msg_send![window, setIgnoresMouseEvents: YES];
        }
        // Return nil so the system can send the click to underlying windows.
        nil
    }
}

extern "C" fn mouse_down(this: &mut Object, _cmd: Sel, event: id) {
    tracing::debug!("Mouse down event received");
    unsafe {
        let window: id = msg_send![this, window];
        if window == nil {
            tracing::error!("Window is nil in mouse_down");
            return;
        }

        let loc_in_window: NSPoint = msg_send![event, locationInWindow];
        let point: NSPoint = msg_send![this, convertPoint: loc_in_window fromView: nil];
        let bounds: NSRect = msg_send![this, bounds];
        let border_width: f64 = msg_send![this, borderWidth];

        let hit_margin = border_width.max(8.0);
        let corner = (hit_margin * 2.0).max(20.0);

        // Edge detection - use hit_margin for consistency with hit_test
        let on_left = point.x >= 0.0 && point.x < hit_margin;
        let on_right = point.x <= bounds.size.width && point.x > bounds.size.width - hit_margin;
        let on_bottom = point.y >= 0.0 && point.y < hit_margin;

        // Top edge: draggable (except corners)
        let on_top = point.y > bounds.size.height - hit_margin;
        let on_top_left_corner = on_top && point.x < corner;
        let on_top_right_corner = on_top && point.x > bounds.size.width - corner;
        let on_top_edge_drag = on_top && !on_top_left_corner && !on_top_right_corner;

        tracing::debug!(
            x = point.x,
            y = point.y,
            bounds_w = bounds.size.width,
            bounds_h = bounds.size.height,
            hit_margin,
            on_left,
            on_right,
            on_bottom,
            on_top_edge = on_top_edge_drag,
            on_corner = on_top_left_corner || on_top_right_corner,
            "Mouse down on border"
        );
        // Store initial mouse position for manual drag
        let screen_point: NSPoint = msg_send![window, convertPointToScreen: loc_in_window];
        this.set_ivar::<NSPoint>("_initialMouseScreen", screen_point);
        let frame: NSRect = msg_send![window, frame];
        this.set_ivar::<NSRect>("_initialWindowFrame", frame);

        // In capture mode:
        // - top edge drags (move)
        // - top corners resize
        // - left/right/bottom edges resize

        // First handle drag-on-top-edge (excluding corners)
        if on_top_edge_drag {
            tracing::debug!("Starting drag interaction");
            BORDER_INTERACTING.store(true, Ordering::SeqCst);
            this.set_ivar::<i32>("_isResizing", 0);
            this.set_ivar::<i32>("_resizeEdgeMask", -1); // -1 = dragging mode

            // Hide REC indicator during drag (only in capture mode, not preview)
            let preview_mode = PREVIEW_MODE.load(Ordering::SeqCst);
            if !preview_mode {
                if let Ok(rec_lock) = crate::REC_INDICATOR.try_lock() {
                    if let Some(rec) = rec_lock.as_ref() {
                        rec.hide();
                    }
                }
            }
            return;
        }

        // Resize mask: left/right/bottom edges, plus top only for the top corners.
        let mut edge_mask: i32 = 0;
        if on_left {
            edge_mask |= EDGE_LEFT;
        }
        if on_right {
            edge_mask |= EDGE_RIGHT;
        }
        if on_bottom {
            edge_mask |= EDGE_BOTTOM;
        }
        if on_top_left_corner || on_top_right_corner {
            edge_mask |= EDGE_TOP;
        }

        if edge_mask != 0 {
            tracing::debug!(edge_mask, "Starting resize interaction");
            BORDER_INTERACTING.store(true, Ordering::SeqCst);
            this.set_ivar::<i32>("_isResizing", 1);
            this.set_ivar::<i32>("_resizeEdgeMask", edge_mask);

            // Hide REC indicator during resize (only in capture mode, not preview)
            let preview_mode = PREVIEW_MODE.load(Ordering::SeqCst);
            if !preview_mode {
                if let Ok(rec_lock) = crate::REC_INDICATOR.try_lock() {
                    if let Some(rec) = rec_lock.as_ref() {
                        rec.hide();
                    }
                }
            }
            return;
        }

        // Preview mode: interior drag (manual implementation)
        if PREVIEW_MODE.load(Ordering::SeqCst) && !on_left && !on_right && !on_bottom {
            tracing::debug!("Starting drag interaction (preview mode)");
            BORDER_INTERACTING.store(true, Ordering::SeqCst);
            this.set_ivar::<i32>("_isResizing", 0);
            this.set_ivar::<i32>("_resizeEdgeMask", -1);
            return;
        }

        tracing::trace!("Mouse down ignored - not on interactive edge");
    }
}

extern "C" fn mouse_dragged(this: &Object, _cmd: Sel, event: id) {
    unsafe {
        let window: id = msg_send![this, window];
        if window == nil {
            return;
        }

        let edge_mask = *this.get_ivar::<i32>("_resizeEdgeMask");

        // edge_mask == 0 means no operation
        if edge_mask == 0 {
            return;
        }

        let initial_mouse = *this.get_ivar::<NSPoint>("_initialMouseScreen");
        let initial_frame = *this.get_ivar::<NSRect>("_initialWindowFrame");

        let loc_in_window: NSPoint = msg_send![event, locationInWindow];
        let current_mouse: NSPoint = msg_send![window, convertPointToScreen: loc_in_window];

        let dx = current_mouse.x - initial_mouse.x;
        let dy = current_mouse.y - initial_mouse.y;

        // Log drag calculations only in capture mode (not preview)
        let preview_mode = PREVIEW_MODE.load(Ordering::SeqCst);
        if !preview_mode {
            tracing::trace!(
                initial_mouse_x = initial_mouse.x,
                initial_mouse_y = initial_mouse.y,
                current_mouse_x = current_mouse.x,
                current_mouse_y = current_mouse.y,
                dx,
                dy,
                initial_frame_x = initial_frame.origin.x,
                initial_frame_y = initial_frame.origin.y,
                "Mouse dragged delta calculation"
            );
        }

        let mut new_frame = initial_frame;

        // edge_mask == -1 means dragging (move window)
        if edge_mask == -1 {
            new_frame.origin.x += dx;
            new_frame.origin.y += dy;
        } else {
            // Resizing
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

            // Top edge: change height without moving origin
            // (origin is bottom-left in Cocoa coordinates)
            if (edge_mask & EDGE_TOP) != 0 {
                new_frame.size.height += dy;
            }

            if new_frame.size.width < min_w {
                new_frame.size.width = min_w;
                if (edge_mask & EDGE_LEFT) != 0 {
                    new_frame.origin.x =
                        initial_frame.origin.x + (initial_frame.size.width - min_w);
                }
            }
            if new_frame.size.height < min_h {
                new_frame.size.height = min_h;
                if (edge_mask & EDGE_BOTTOM) != 0 {
                    new_frame.origin.y =
                        initial_frame.origin.y + (initial_frame.size.height - min_h);
                }
            }
        }

        let _: () = msg_send![window, setFrame: new_frame display: YES];
        let _: () = msg_send![this, setNeedsDisplay: YES];

        let preview_mode = PREVIEW_MODE.load(Ordering::SeqCst);

        // In capture mode: force immediate updates for screen sharing apps
        if !preview_mode {
            let _: () = msg_send![window, displayIfNeeded];
            let _: () = msg_send![window, flushWindow]; // Force backing store flush for Meet

            // Update cursor rects after frame changes
            let _: () = msg_send![window, invalidateCursorRectsForView: this];
        }

        // Update global cache for capture loop (not needed in preview)
        if !preview_mode {
            let screen: id = msg_send![class!(NSScreen), mainScreen];
            let screen_frame: NSRect = msg_send![screen, frame];
            let screen_height = screen_frame.size.height;

            let x = new_frame.origin.x as i32;
            let y = (screen_height - new_frame.origin.y - new_frame.size.height) as i32;
            let width = new_frame.size.width as i32;

            // Update cache for render thread to read
            if let Ok(mut cache) = BORDER_RECT_CACHE.try_lock() {
                *cache = (x, y, width, new_frame.size.height as i32);
            }

            // Update REC indicator position during dragging (if moving, not resizing)
            if edge_mask == -1 {
                if let Ok(rec_lock) = crate::REC_INDICATOR.try_lock() {
                    if let Some(rec) = rec_lock.as_ref() {
                        // Get border_width for update_position
                        let border_width: f64 = msg_send![this, borderWidth];
                        rec.update_position(x, y, width, border_width as i32);
                    }
                }
            }
        }
    }
}

extern "C" fn mouse_up(this: &mut Object, _cmd: Sel, _event: id) {
    unsafe {
        let edge_mask = *this.get_ivar::<i32>("_resizeEdgeMask");
        let was_interacting = edge_mask != 0;

        tracing::debug!(was_interacting, "Mouse up event");

        // Always reset state to avoid stuck interactions
        this.set_ivar::<i32>("_isResizing", 0);
        this.set_ivar::<i32>("_resizeEdgeMask", 0);

        // Update cache and force redraw after interaction completes
        let window: id = msg_send![this, window];
        if window != nil {
            update_border_cache_from_window(window);
            // Force display update for screen sharing apps
            let _: () = msg_send![this, setNeedsDisplay: YES];
            let _: () = msg_send![window, displayIfNeeded];

            // Notify callback: interaction complete - update capture region and windows
            let frame: NSRect = msg_send![window, frame];
            let screen: id = msg_send![class!(NSScreen), mainScreen];
            let screen_frame: NSRect = msg_send![screen, frame];
            let screen_height = screen_frame.size.height;

            let x = frame.origin.x as i32;
            let y = (screen_height - frame.origin.y - frame.size.height) as i32;
            let width = frame.size.width as i32;
            let height = frame.size.height as i32;

            // Get border_width from the view
            let border_width: f64 = msg_send![this, borderWidth];
            let border_width_i32 = border_width as i32;

            // Only call callback and show REC if NOT in preview mode
            let preview_mode = PREVIEW_MODE.load(Ordering::SeqCst);
            if !preview_mode {
                if let Ok(cb_lock) = BORDER_INTERACTION_COMPLETE_CALLBACK.try_lock() {
                    if let Some(ref callback) = *cb_lock {
                        tracing::info!(
                            x,
                            y,
                            width,
                            height,
                            "Interaction complete, notifying callback"
                        );
                        callback(x, y, width, height);
                    }
                }

                // Show REC indicator after interaction completes (at new position)
                if let Ok(rec_lock) = crate::REC_INDICATOR.try_lock() {
                    if let Some(rec) = rec_lock.as_ref() {
                        rec.show(x, y, width, border_width_i32);
                    }
                }
            }
        }

        // Interaction complete
        if was_interacting {
            if edge_mask == -1 {
                tracing::debug!("Drag interaction ended");
            } else {
                tracing::debug!(edge_mask, "Resize interaction ended");
            }
            BORDER_INTERACTING.store(false, Ordering::SeqCst);

            // After interactions, return to click-through; the mouse poller will re-enable
            // events when the cursor is back on the border.
            // BUT: In preview mode, keep mouse events enabled so user can interact
            let preview_mode = PREVIEW_MODE.load(Ordering::SeqCst);
            if !preview_mode {
                let window: id = msg_send![this, window];
                if window != nil {
                    let _: () = msg_send![window, setIgnoresMouseEvents: YES];
                }
            }
        }
    }
}

/// Helper to update border cache from window frame
fn update_border_cache_from_window(window: id) {
    unsafe {
        let frame: NSRect = msg_send![window, frame];
        let screen: id = msg_send![class!(NSScreen), mainScreen];
        let screen_frame: NSRect = msg_send![screen, frame];
        let screen_height = screen_frame.size.height;

        let x = frame.origin.x as i32;
        let y = (screen_height - frame.origin.y - frame.size.height) as i32;
        let width = frame.size.width as i32;
        let height = frame.size.height as i32;

        if let Ok(mut cache) = BORDER_RECT_CACHE.try_lock() {
            *cache = (x, y, width, height);
        }
    }
}

// Set cursor for different regions
extern "C" fn reset_cursor_rects(this: &Object, _cmd: Sel) {
    unsafe {
        // Clear old cursor rects first
        let _: () = msg_send![this, discardCursorRects];

        let bounds: NSRect = msg_send![this, bounds];
        let border_width: f64 = msg_send![this, borderWidth];
        let border = border_width.max(8.0);

        // Corner resize cursors
        let corner = (border * 2.0).max(20.0);

        // Top-left and bottom-right: diagonal resize
        let tl_rect = NSRect::new(
            NSPoint::new(0.0, bounds.size.height - corner),
            NSSize::new(corner, corner),
        );
        let br_rect = NSRect::new(
            NSPoint::new(bounds.size.width - corner, 0.0),
            NSSize::new(corner, corner),
        );
        // Note: macOS doesn't have nwseResizeCursor, use crosshair or pointingHand
        let resize_cursor: id = msg_send![class!(NSCursor), crosshairCursor];
        let _: () = msg_send![this, addCursorRect:tl_rect cursor:resize_cursor];
        let _: () = msg_send![this, addCursorRect:br_rect cursor:resize_cursor];

        // Top-right and bottom-left: other diagonal
        let tr_rect = NSRect::new(
            NSPoint::new(bounds.size.width - corner, bounds.size.height - corner),
            NSSize::new(corner, corner),
        );
        let bl_rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(corner, corner));
        let _: () = msg_send![this, addCursorRect:tr_rect cursor:resize_cursor];
        let _: () = msg_send![this, addCursorRect:bl_rect cursor:resize_cursor];

        // Top edge (excluding corners): open hand cursor (dragging)
        let top_rect = NSRect::new(
            NSPoint::new(corner, bounds.size.height - border),
            NSSize::new((bounds.size.width - 2.0 * corner).max(0.0), border),
        );
        let open_hand_cursor: id = msg_send![class!(NSCursor), openHandCursor];
        let _: () = msg_send![this, addCursorRect:top_rect cursor:open_hand_cursor];

        // Left and right edges: horizontal resize
        let left_rect = NSRect::new(
            NSPoint::new(0.0, corner),
            NSSize::new(border, bounds.size.height - 2.0 * corner),
        );
        let right_rect = NSRect::new(
            NSPoint::new(bounds.size.width - border, corner),
            NSSize::new(border, bounds.size.height - 2.0 * corner),
        );
        let h_resize: id = msg_send![class!(NSCursor), resizeLeftRightCursor];
        let _: () = msg_send![this, addCursorRect:left_rect cursor:h_resize];
        let _: () = msg_send![this, addCursorRect:right_rect cursor:h_resize];

        // Bottom edge: vertical resize
        let bottom_rect = NSRect::new(
            NSPoint::new(corner, 0.0),
            NSSize::new(bounds.size.width - 2.0 * corner, border),
        );
        // Try resizeUpDown first, fallback to resizeUpDownCursor
        let v_resize: id = msg_send![class!(NSCursor), resizeUpDownCursor];
        let _: () = msg_send![this, addCursorRect:bottom_rect cursor:v_resize];
    }
}

// Update tracking areas when view bounds change
extern "C" fn update_tracking_areas(this: &Object, _cmd: Sel) {
    unsafe {
        // Call super
        let superclass = class!(NSView);
        let _: () = msg_send![super(this, superclass), updateTrackingAreas];

        // Remove old tracking areas
        let tracking_areas: id = msg_send![this, trackingAreas];
        let count: usize = msg_send![tracking_areas, count];
        for i in 0..count {
            let area: id = msg_send![tracking_areas, objectAtIndex: i];
            let _: () = msg_send![this, removeTrackingArea: area];
        }

        // Add new tracking area for entire view
        let bounds: NSRect = msg_send![this, bounds];

        // NSTrackingMouseEnteredAndExited | NSTrackingMouseMoved | NSTrackingCursorUpdate | NSTrackingActiveAlways | NSTrackingInVisibleRect
        // NSTrackingCursorUpdate (1 << 2) is CRITICAL for cursor rects to work!
        let options: u64 = (1 << 0) | (1 << 1) | (1 << 2) | (1 << 7) | (1 << 9);

        let tracking_area_class = class!(NSTrackingArea);
        let tracking_area: id = msg_send![tracking_area_class, alloc];
        let tracking_area: id = msg_send![tracking_area,
            initWithRect: bounds
            options: options
            owner: this
            userInfo: nil
        ];
        let _: () = msg_send![this, addTrackingArea: tracking_area];
    }
}

// Called when mouse moves within the view - no action needed, hit_test handles click-through
extern "C" fn mouse_moved(_this: &Object, _cmd: Sel, _event: id) {
    // hit_test automatically provides click-through behavior
    // No need to toggle ignoresMouseEvents - that breaks event delivery
}

// Called when mouse exits the view
extern "C" fn mouse_exited(_this: &Object, _cmd: Sel, _event: id) {
    // hit_test automatically provides click-through behavior
    // No need to change ignoresMouseEvents
}

// Accept first mouse click without activating window
extern "C" fn accepts_first_mouse(_this: &Object, _cmd: Sel, _event: id) -> bool {
    tracing::trace!("acceptsFirstMouse called, returning true");
    true
}

// Accept first responder to receive mouse events
extern "C" fn accepts_first_responder(_this: &Object, _cmd: Sel) -> bool {
    tracing::trace!("acceptsFirstResponder called, returning true");
    true
}

// Called when mouse enters the view
extern "C" fn mouse_entered(this: &Object, _cmd: Sel, event: id) {
    // Just trigger the mouse_moved logic
    mouse_moved(this, _cmd, event);
}

// Called when cursor needs to be updated based on position
extern "C" fn cursor_update(this: &Object, _cmd: Sel, event: id) {
    unsafe {
        // IMPORTANT: call super. The default implementation applies cursor rects.
        let superclass = class!(NSView);
        let _: () = msg_send![super(this, superclass), cursorUpdate: event];
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
        // Register custom NSPanel subclass that can become key window
        let panel_superclass = class!(NSPanel);
        let mut panel_decl = ClassDecl::new("HollowBorderPanel", panel_superclass).unwrap();

        extern "C" fn can_become_key_window(_this: &Object, _cmd: Sel) -> bool {
            true // No logging - called frequently
        }

        unsafe {
            panel_decl.add_method(
                sel!(canBecomeKeyWindow),
                can_become_key_window as extern "C" fn(&Object, Sel) -> bool,
            );
            // No sendEvent: override - let standard Cocoa event chain work
            // hit_test in view controls click-through behavior
        }

        panel_decl.register();

        // Register view class
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
                sel!(resetCursorRects),
                reset_cursor_rects as extern "C" fn(&Object, Sel),
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
            decl.add_method(
                sel!(updateTrackingAreas),
                update_tracking_areas as extern "C" fn(&Object, Sel),
            );
            decl.add_method(
                sel!(mouseMoved:),
                mouse_moved as extern "C" fn(&Object, Sel, id),
            );
            decl.add_method(
                sel!(mouseEntered:),
                mouse_entered as extern "C" fn(&Object, Sel, id),
            );
            decl.add_method(
                sel!(mouseExited:),
                mouse_exited as extern "C" fn(&Object, Sel, id),
            );
            decl.add_method(
                sel!(cursorUpdate:),
                cursor_update as extern "C" fn(&Object, Sel, id),
            );
            decl.add_method(
                sel!(acceptsFirstMouse:),
                accepts_first_mouse as extern "C" fn(&Object, Sel, id) -> bool,
            );
            decl.add_method(
                sel!(acceptsFirstResponder),
                accepts_first_responder as extern "C" fn(&Object, Sel) -> bool,
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
    // Thread-safe cached position updated by mouse events
    // Avoids blocking dispatch_sync_f in get_rect() called at 60 FPS
    cached_rect: Arc<Mutex<(i32, i32, i32, i32)>>,
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
        tracing::debug!(x, y, width, height, "Creating HollowBorder");

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
        tracing::debug!(
            is_main_thread = is_main,
            "HollowBorder creation thread check"
        );

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
            tracing::debug!("Already on main thread, creating border directly");
            create_border_on_main_thread(
                &mut ctx as *mut CreateBorderContext as *mut std::ffi::c_void,
            );
        } else {
            // Dispatch to main thread
            tracing::debug!("Dispatching border creation to main thread");
            unsafe {
                let main_queue = &_dispatch_main_q as *const std::ffi::c_void;
                dispatch_sync_f(
                    main_queue,
                    &mut ctx as *mut CreateBorderContext as *mut std::ffi::c_void,
                    create_border_on_main_thread,
                );
            }
            tracing::trace!("dispatch_sync_f returned");
        }

        let window = ctx.result_window?;

        tracing::info!("HollowBorder created successfully");
        log::info!("macOS hollow border created successfully");

        // Initialize global cache
        *BORDER_RECT_CACHE.lock().unwrap() = (x, y, width, height);

        let cached_rect = BORDER_RECT_CACHE.clone();

        Some(Self {
            window,
            x,
            y,
            width,
            height,
            border_width,
            border_color,
            cached_rect,
        })
    }

    pub fn get_rect(&self) -> (i32, i32, i32, i32) {
        // Return cached values to avoid blocking dispatch_sync_f
        // Cache is updated by mouse events on main thread
        *self.cached_rect.lock().unwrap()
    }

    pub fn get_inner_rect(&self) -> (i32, i32, i32, i32) {
        // Get current window frame (may have been moved/resized by user)
        let (x, y, width, height) = self.get_rect();
        let bw = self.border_width;
        (x + bw, y + bw, width - 2 * bw, height - 2 * bw)
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
                    NSSize::new(ctx.width as f64, ctx.height as f64),
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

                let ns_color: id =
                    msg_send![class!(NSColor), colorWithRed:r green:g blue:b alpha:1.0];
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
                    let ns_color: id =
                        msg_send![class!(NSColor), colorWithRed:r green:g blue:b alpha:1.0];
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
        log::info!("Showing macOS hollow border");

        extern "C" fn show_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
            let window = ctx_ptr as id;
            unsafe {
                // Use orderFront instead of makeKeyAndOrderFront
                // to prevent activating the main application window
                let _: () = msg_send![window, orderFront: nil];
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

    pub fn stop(&mut self) {
        log::info!("Stopping macOS hollow border");
        stop_mouse_poll();
        self.hide();
    }

    /// Set capture mode: interior click-through, only border is interactive
    pub fn set_capture_mode(&mut self) {
        tracing::debug!("set_capture_mode() called");
        PREVIEW_MODE.store(false, Ordering::SeqCst);

        struct SetCaptureModeContext {
            window: id,
        }

        extern "C" fn set_capture_mode_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
            let ctx = unsafe { &*(ctx_ptr as *const SetCaptureModeContext) };
            unsafe {
                // Disable moving window from anywhere except edges
                let _: () = msg_send![ctx.window, setMovableByWindowBackground: NO];
                // Real click-through requires ignoresMouseEvents=YES.
                // We dynamically enable events on the border via a mouse-location poller.
                let _: () = msg_send![ctx.window, setIgnoresMouseEvents: YES];
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
                dispatch_sync_f(
                    &_dispatch_main_q,
                    &mut context as *mut _ as *mut std::ffi::c_void,
                    set_capture_mode_on_main_thread,
                );
            } else {
                set_capture_mode_on_main_thread(&mut context as *mut _ as *mut std::ffi::c_void);
            }
        }

        // Start polling for border hover to temporarily accept mouse events.
        start_mouse_poll(self.window);
    }

    /// Set preview mode: interior is draggable (not click-through)
    pub fn set_preview_mode(&mut self) {
        log::info!("Setting hollow border to preview mode (draggable)");
        PREVIEW_MODE.store(true, Ordering::SeqCst);
        stop_mouse_poll();
        unsafe {
            // Disable native drag - we handle it manually in mouse events
            let _: () = msg_send![self.window, setMovableByWindowBackground: NO];
            let _: () = msg_send![self.window, setIgnoresMouseEvents: NO];
            let preview_bg: id =
                msg_send![class!(NSColor), colorWithRed:0.125 green:0.125 blue:0.125 alpha:0.15];
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
        stop_mouse_poll();
        extern "C" fn close_window_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
            let window = ctx_ptr as id;
            unsafe {
                let _: () = msg_send![window, orderOut: nil];
                let _: () = msg_send![window, close];
            }
        }

        unsafe {
            let is_main = pthread_main_np() != 0;

            if !is_main {
                dispatch_sync_f(
                    &_dispatch_main_q,
                    self.window as *mut std::ffi::c_void,
                    close_window_on_main_thread,
                );
            } else {
                let _: () = msg_send![self.window, orderOut: nil];
                let _: () = msg_send![self.window, close];
            }
        }
    }
}

// Implement cross-platform BorderWindow trait
impl BorderWindow for HollowBorder {
    fn new(
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        border_width: i32,
        border_color: u32,
    ) -> Option<Self> {
        HollowBorder::new(x, y, width, height, border_width, border_color)
    }

    fn get_rect(&self) -> (i32, i32, i32, i32) {
        self.get_rect()
    }

    fn get_inner_rect(&self) -> (i32, i32, i32, i32) {
        self.get_inner_rect()
    }

    fn update_rect(&self, x: i32, y: i32, width: i32, height: i32) {
        self.update_rect(x, y, width, height)
    }

    fn update_color(&self, color: u32) {
        self.update_color(color)
    }

    fn update_style(&self, width: i32, color: u32) {
        self.update_style(width, color)
    }

    fn hide(&self) {
        self.hide()
    }

    fn show(&self) {
        self.show()
    }

    fn hwnd_value(&self) -> isize {
        self.hwnd_value()
    }

    fn set_capture_mode(&mut self) {
        self.set_capture_mode()
    }

    fn set_preview_mode(&mut self) {
        self.set_preview_mode()
    }

    fn stop(&mut self) {
        self.stop()
    }
}
