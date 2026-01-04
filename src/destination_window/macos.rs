//! macOS Destination Window Implementation
//!
//! Renders captured frames to a transparent overlay window using NSWindow and CoreGraphics.

use cocoa::appkit::{NSWindow, NSWindowStyleMask, NSBackingStoreType, NSColor};
use cocoa::base::{id, nil, YES, NO};
use cocoa::foundation::{NSRect, NSPoint, NSSize, NSAutoreleasePool};
use objc::{msg_send, sel, sel_impl};
use std::sync::{Arc, Mutex};

extern "C" {
    static _dispatch_main_q: std::ffi::c_void;
    fn dispatch_sync_f(
        queue: *const std::ffi::c_void,
        context: *mut std::ffi::c_void,
        work: extern "C" fn(*mut std::ffi::c_void),
    );
    fn pthread_main_np() -> i32; // Returns non-zero if on main thread
}

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

// Context struct for main thread creation
struct CreateDestWindowContext {
    width: u32,
    height: u32,
    config: DestinationWindowConfig,
    result_window: *mut id,
    result_view: *mut id,
}

unsafe impl Send for CreateDestWindowContext {}

extern "C" fn create_dest_window_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
    println!("[DEST_WINDOW] Executing create_dest_window_on_main_thread callback");
    
    let ctx = unsafe { &mut *(ctx_ptr as *mut CreateDestWindowContext) };
    
    unsafe {
        let _pool = NSAutoreleasePool::new(nil);
        
        // Create window style mask
        let style_mask = NSWindowStyleMask::NSBorderlessWindowMask;
        
        // Create window frame (positioned at top-left for now)
        let frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(ctx.width as f64, ctx.height as f64)
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
            println!("[DEST_WINDOW] ERROR: NSWindow creation failed");
            return;
        }
        
        // Configure window properties
        window.setOpaque_(NO);
        window.setBackgroundColor_(NSColor::clearColor(nil));
        
        // Set window title for screen sharing apps to detect
        let title = cocoa::foundation::NSString::alloc(nil);
        let title = cocoa::foundation::NSString::init_str(title, "RustFrame Preview");
        window.setTitle_(title);
        
        // Set window level for topmost behavior (default: true)
        let topmost = ctx.config.topmost.unwrap_or(true);
        if topmost {
            // Use NSFloatingWindowLevel (3) for topmost
            let _: () = msg_send![window, setLevel: 3i32];
        } else {
            // Use NSNormalWindowLevel (0) to be visible in screen sharing
            let _: () = msg_send![window, setLevel: 0i32];
        }
        
        // Set collection behavior to be visible in window lists
        // NSWindowCollectionBehaviorManaged = 1 << 2 (4)
        let _: () = msg_send![window, setCollectionBehavior: 4u64];
        
        // Configure click-through behavior (default: true in release)
        let click_through = ctx.config.click_through.unwrap_or(!cfg!(debug_assertions));
        if click_through {
            window.setIgnoresMouseEvents_(YES);
        }
        
        // Create content view
        let content_view = window.contentView();
        
        // Set alpha if specified
        if let Some(alpha) = ctx.config.alpha {
            window.setAlphaValue_((alpha as f64) / 255.0);
        }
        
        // Show the window
        window.makeKeyAndOrderFront_(nil);
        
        log::info!("macOS destination window created successfully");
        println!("[DEST_WINDOW] Window created successfully on main thread");
        
        // Store results
        *ctx.result_window = window;
        *ctx.result_view = content_view;
    }
}

impl DestinationWindow {
    pub fn new(width: u32, height: u32, config: DestinationWindowConfig) -> Option<Self> {
        log::info!("Creating macOS destination window {}x{} with config {:?}", width, height, config);
        println!("[DEST_WINDOW] Starting destination window creation");
        
        let is_main = unsafe { pthread_main_np() } != 0;
        println!("[DEST_WINDOW] Current thread is main: {}", is_main);
        
        let mut result_window: id = nil;
        let mut result_view: id = nil;
        
        let mut context = CreateDestWindowContext {
            width,
            height,
            config,
            result_window: &mut result_window,
            result_view: &mut result_view,
        };
        
        if !is_main {
            println!("[DEST_WINDOW] Not on main thread, dispatching to main queue");
            unsafe {
                dispatch_sync_f(
                    &_dispatch_main_q,
                    &mut context as *mut _ as *mut std::ffi::c_void,
                    create_dest_window_on_main_thread,
                );
            }
            println!("[DEST_WINDOW] Dispatch completed");
        } else {
            println!("[DEST_WINDOW] Already on main thread, creating directly");
            create_dest_window_on_main_thread(&mut context as *mut _ as *mut std::ffi::c_void);
        }
        
        if result_window == nil {
            log::error!("Failed to create destination window");
            println!("[DEST_WINDOW] ERROR: Window creation failed");
            return None;
        }
        
        println!("[DEST_WINDOW] Destination window created successfully");
        
        Some(Self {
            window: result_window,
            view: result_view,
            width,
            height,
        })
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
        println!("[DEST_WINDOW] Drop called");
        
        extern "C" fn close_window_on_main_thread(ctx_ptr: *mut std::ffi::c_void) {
            let window = ctx_ptr as id;
            unsafe {
                println!("[DEST_WINDOW] Hiding and closing window on main thread");
                let _: () = msg_send![window, orderOut: nil];
                let _: () = msg_send![window, close];
            }
        }
        
        unsafe {
            let is_main = pthread_main_np() != 0;
            println!("[DEST_WINDOW] Drop on main thread: {}", is_main);
            
            if !is_main {
                println!("[DEST_WINDOW] Dispatching window close to main thread");
                dispatch_sync_f(
                    &_dispatch_main_q,
                    self.window as *mut std::ffi::c_void,
                    close_window_on_main_thread,
                );
            } else {
                println!("[DEST_WINDOW] Closing window directly on main thread");
                let _: () = msg_send![self.window, orderOut: nil];
                let _: () = msg_send![self.window, close];
            }
        }
        
        println!("[DEST_WINDOW] Drop completed");
    }
}
