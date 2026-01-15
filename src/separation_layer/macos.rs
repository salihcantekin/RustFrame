//! macOS Separation Layer Window
//! Opaque window inserted between border and preview. Kept behind user windows.

use cocoa::appkit::{NSBackingStoreType, NSColor, NSWindow, NSWindowStyleMask};
use cocoa::base::{id, nil, YES, NO};
use cocoa::foundation::{NSAutoreleasePool, NSPoint, NSRect, NSSize};
use objc::{class, msg_send, sel, sel_impl};

const NS_NORMAL_WINDOW_LEVEL: i32 = 0;

unsafe impl Send for SeparationLayer {}
unsafe impl Sync for SeparationLayer {}

pub struct SeparationLayer {
    window: id,
}

extern "C" {
    static _dispatch_main_q: std::ffi::c_void;
    fn dispatch_sync_f(
        queue: *const std::ffi::c_void,
        context: *mut std::ffi::c_void,
        work: extern "C" fn(*mut std::ffi::c_void),
    );
    fn pthread_main_np() -> i32;
}

impl SeparationLayer {
    pub fn new(x: i32, y: i32, width: i32, height: i32, color: u32) -> Option<Self> {
        let mut result_window: id = nil;

        #[repr(C)]
        struct CreateCtx {
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            color: u32,
            out_window: *mut id,
        }

        extern "C" fn create_on_main(ctx_ptr: *mut std::ffi::c_void) {
            unsafe {
                let ctx = &mut *(ctx_ptr as *mut CreateCtx);
                let _pool = NSAutoreleasePool::new(nil);

                // macOS uses bottom-left origin, need to convert from top-left
                let screen: id = msg_send![class!(NSScreen), mainScreen];
                let screen_frame: NSRect = msg_send![screen, frame];
                let screen_height = screen_frame.size.height;
                
                // Convert y from top-left to bottom-left origin
                let cocoa_y = screen_height - (ctx.y as f64) - (ctx.height as f64);

                let frame = NSRect::new(
                    NSPoint::new(ctx.x as f64, cocoa_y),
                    NSSize::new(ctx.width as f64, ctx.height as f64),
                );

                let window = NSWindow::alloc(nil).initWithContentRect_styleMask_backing_defer_(
                    frame,
                    NSWindowStyleMask::NSBorderlessWindowMask,
                    NSBackingStoreType::NSBackingStoreBuffered,
                    NO,
                );
                if window == nil {
                    return;
                }

                // Solid fill color
                let r = ((ctx.color >> 16) & 0xFF) as f64 / 255.0;
                let g = ((ctx.color >> 8) & 0xFF) as f64 / 255.0;
                let b = (ctx.color & 0xFF) as f64 / 255.0;
                let fill: id = msg_send![class!(NSColor), colorWithRed:r green:g blue:b alpha:1.0];
                window.setBackgroundColor_(fill);
                window.setOpaque_(YES);

                // Make it non-interactive so kullanıcı tıklayınca fokus almaz
                window.setIgnoresMouseEvents_(YES);

                // CRITICAL: Sharing type NONE so separation layer is HIDDEN from screen sharing pickers
                // Only destination window should be visible in Meet/Zoom/Discord picker
                let sharing: u64 = 0; // NSWindowSharingNone
                let _: () = msg_send![window, setSharingType: sharing];

                // CRITICAL: Collection behavior MUST match destination window
                // - MANAGED (1 << 2): Participates in window management
                // - MOVE_TO_ACTIVE_SPACE (1 << 1): Moves with active space (hides on desktop view)
                // - FULL_SCREEN_AUXILIARY (1 << 8): Can be shown alongside fullscreen windows  
                // - IGNORES_CYCLE (1 << 6): Hidden from Dock/Cmd+Tab
                // Do NOT use CAN_JOIN_ALL_SPACES (1 << 0) - it conflicts with MOVE_TO_ACTIVE_SPACE!
                let behavior = (1u64 << 2) /*managed*/
                    | (1u64 << 1) /*move to active space - CRITICAL for desktop hiding*/
                    | (1u64 << 8) /*full screen auxiliary*/
                    | (1u64 << 6); /*ignores cycle*/
                let _: () = msg_send![window, setCollectionBehavior: behavior];

                // CRITICAL: Use NORMAL level (0) for screen sharing visibility
                // Desktop level windows are filtered by Meet/Zoom/Teams
                let _: () = msg_send![window, setLevel: NS_NORMAL_WINDOW_LEVEL];

                // CRITICAL: Window will be positioned and shown via update_position() and show()
                // Do NOT call orderOut/orderBack here - let z-order restoration handle it

                *ctx.out_window = window;
            }
        }

        let mut ctx = CreateCtx {
            x,
            y,
            width,
            height,
            color,
            out_window: &mut result_window,
        };

        unsafe {
            if pthread_main_np() != 0 {
                create_on_main(&mut ctx as *mut _ as *mut std::ffi::c_void);
            } else {
                dispatch_sync_f(
                    &_dispatch_main_q as *const _ as *const std::ffi::c_void,
                    &mut ctx as *mut _ as *mut std::ffi::c_void,
                    create_on_main,
                );
            }
        }

        if result_window == nil {
            None
        } else {
            Some(Self { window: result_window })
        }
    }

    pub fn update_position(&self, x: i32, y: i32, width: i32, height: i32) {
        #[repr(C)]
        struct PosCtx {
            window: id,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
        }

        extern "C" fn move_on_main(ctx_ptr: *mut std::ffi::c_void) {
            unsafe {
                let ctx = &*(ctx_ptr as *const PosCtx);
                
                // macOS uses bottom-left origin, need to convert from top-left
                let screen: id = msg_send![class!(NSScreen), mainScreen];
                let screen_frame: NSRect = msg_send![screen, frame];
                let screen_height = screen_frame.size.height;
                
                // Convert y from top-left to bottom-left origin
                let cocoa_y = screen_height - (ctx.y as f64) - (ctx.height as f64);
                
                let new_frame = NSRect::new(
                    NSPoint::new(ctx.x as f64, cocoa_y),
                    NSSize::new(ctx.width as f64, ctx.height as f64),
                );
                let _: () = msg_send![ctx.window, setFrame:new_frame display:NO animate:NO];
                // NOTE: Do NOT call orderOut/orderBack here - causes flashing!
                // Z-order is restored separately in callback when interaction completes
            }
        }

        let mut ctx = PosCtx {
            window: self.window,
            x,
            y,
            width,
            height,
        };

        unsafe {
            if pthread_main_np() != 0 {
                move_on_main(&mut ctx as *mut _ as *mut std::ffi::c_void);
            } else {
                dispatch_sync_f(
                    &_dispatch_main_q as *const _ as *const std::ffi::c_void,
                    &mut ctx as *mut _ as *mut std::ffi::c_void,
                    move_on_main,
                );
            }
        }
    }

    pub fn show(&self) {
        unsafe {
            let is_main = pthread_main_np() != 0;
            let window = self.window;
            extern "C" fn show_on_main(window_ptr: *mut std::ffi::c_void) {
                unsafe {
                    let window = window_ptr as id;
                    // CRITICAL: Do NOT use orderOut - causes flashing!
                    // Just order to back to ensure proper layering
                    let _: () = msg_send![window, orderBack: nil];
                }
            }
            if is_main {
                show_on_main(window as *mut _);
            } else {
                dispatch_sync_f(
                    &_dispatch_main_q as *const _ as *const std::ffi::c_void,
                    window as *mut _ as *mut std::ffi::c_void,
                    show_on_main,
                );
            }
        }
    }

    pub fn hide(&self) {
        unsafe {
            let is_main = pthread_main_np() != 0;
            let window = self.window;
            extern "C" fn hide_on_main(window_ptr: *mut std::ffi::c_void) {
                unsafe {
                    let window = window_ptr as id;
                    let _: () = msg_send![window, orderOut: nil];
                }
            }
            if is_main {
                hide_on_main(window as *mut _);
            } else {
                dispatch_sync_f(
                    &_dispatch_main_q as *const _ as *const std::ffi::c_void,
                    window as *mut _ as *mut std::ffi::c_void,
                    hide_on_main,
                );
            }
        }
    }

    pub fn hwnd_value(&self) -> isize {
        self.window as isize
    }

    /// Get raw NSWindow pointer for z-order operations
    pub fn get_window(&self) -> id {
        self.window
    }

    /// Get current window position and size for debugging/verification
    pub fn get_rect(&self) -> Option<(i32, i32, i32, i32)> {
        unsafe {
            let frame: NSRect = msg_send![self.window, frame];
            let screen: id = msg_send![class!(NSScreen), mainScreen];
            let screen_frame: NSRect = msg_send![screen, frame];
            let screen_height = screen_frame.size.height;

            let x = frame.origin.x as i32;
            let y = (screen_height - frame.origin.y - frame.size.height) as i32;
            let width = frame.size.width as i32;
            let height = frame.size.height as i32;

            Some((x, y, width, height))
        }
    }
}

impl Drop for SeparationLayer {
    fn drop(&mut self) {
        unsafe {
            let window = self.window;
            extern "C" fn close_on_main(window_ptr: *mut std::ffi::c_void) {
                unsafe {
                    let window = window_ptr as id;
                    let _: () = msg_send![window, orderOut: nil];
                    let _: () = msg_send![window, close];
                }
            }
            if pthread_main_np() != 0 {
                close_on_main(window as *mut _);
            } else {
                dispatch_sync_f(
                    &_dispatch_main_q as *const _ as *const std::ffi::c_void,
                    window as *mut _ as *mut std::ffi::c_void,
                    close_on_main,
                );
            }
        }
    }
}
