// capture/macos_sck.rs - macOS ScreenCaptureKit capture backend (macOS 12.3+)
//
// This module implements an async capture stream using ScreenCaptureKit so the
// system compositor can include the real cursor (showsCursor) without any
// manual overlay.
//
// IMPORTANT:
// - All Objective-C / ScreenCaptureKit interactions must happen on the main thread.
// - Keep CoreGraphics (screenshot) capture as a fallback in capture/macos.rs.

#![cfg(target_os = "macos")]

use anyhow::{anyhow, Result};
use block::ConcreteBlock;
use cocoa::base::{id, nil};
use cocoa::foundation::{NSArray, NSPoint, NSRect, NSSize, NSUInteger};
use objc::declare::ClassDecl;
use objc::rc::{autoreleasepool, StrongPtr};
use objc::runtime::{Class, Object, Sel};
use objc::{class, msg_send, sel, sel_impl};
use std::ffi::c_void;
use std::sync::{
	atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering},
	Arc, Mutex,
};

#[link(name = "ScreenCaptureKit", kind = "framework")]
extern "C" {}

#[link(name = "CoreMedia", kind = "framework")]
extern "C" {
	fn CMSampleBufferGetImageBuffer(sample_buffer: *const c_void) -> *mut c_void;
}

#[link(name = "CoreVideo", kind = "framework")]
extern "C" {
	fn CVPixelBufferLockBaseAddress(pixel_buffer: *mut c_void, lock_flags: u64) -> i32;
	fn CVPixelBufferUnlockBaseAddress(pixel_buffer: *mut c_void, lock_flags: u64) -> i32;
	fn CVPixelBufferGetBaseAddress(pixel_buffer: *mut c_void) -> *mut c_void;
	fn CVPixelBufferGetBytesPerRow(pixel_buffer: *mut c_void) -> usize;
	fn CVPixelBufferGetWidth(pixel_buffer: *mut c_void) -> usize;
	fn CVPixelBufferGetHeight(pixel_buffer: *mut c_void) -> usize;
	fn CVPixelBufferGetPixelFormatType(pixel_buffer: *mut c_void) -> u32;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
	fn CGMainDisplayID() -> u32;
	fn CGPreflightScreenCaptureAccess() -> bool;
}

// libdispatch
#[cfg(target_os = "macos")]
extern "C" {
	static _dispatch_main_q: c_void;

	fn dispatch_sync_f(
		queue: *const c_void,
		context: *mut c_void,
		work: extern "C" fn(*mut c_void),
	);
	fn pthread_main_np() -> i32;

	fn dispatch_get_global_queue(identifier: isize, flags: usize) -> *mut c_void;
	fn dispatch_semaphore_create(value: isize) -> *mut c_void;
	fn dispatch_semaphore_wait(sema: *mut c_void, timeout: u64) -> isize;
	fn dispatch_semaphore_signal(sema: *mut c_void) -> isize;
}

const DISPATCH_TIME_FOREVER: u64 = u64::MAX;
const DISPATCH_QUEUE_PRIORITY_DEFAULT: isize = 0;

const KCVPIXELFORMATTYPE_32BGRA: u32 = 0x42475241; // 'BGRA'
const KCVPIXELBUFFERLOCK_READONLY: u64 = 1;

// SCStreamOutputTypeScreen is 0 in current SDKs.
const SC_STREAM_OUTPUT_TYPE_SCREEN: NSUInteger = 0;

struct OutputState {
	latest: Mutex<Option<(Vec<u8>, u32, u32)>>,
	seq: AtomicU64,
	region_x: AtomicI32,
	region_y: AtomicI32,
	region_w: AtomicU32,
	region_h: AtomicU32,
	scale_milli: AtomicU32,
}

impl OutputState {
	fn new() -> Self {
		Self {
			latest: Mutex::new(None),
			seq: AtomicU64::new(0),
			region_x: AtomicI32::new(0),
			region_y: AtomicI32::new(0),
			region_w: AtomicU32::new(0),
			region_h: AtomicU32::new(0),
			scale_milli: AtomicU32::new(1000),
		}
	}

	fn set_region_points(&self, x: i32, y: i32, w: u32, h: u32) {
		self.region_x.store(x, Ordering::Relaxed);
		self.region_y.store(y, Ordering::Relaxed);
		self.region_w.store(w, Ordering::Relaxed);
		self.region_h.store(h, Ordering::Relaxed);
	}

	fn set_scale_milli(&self, scale_milli: u32) {
		self.scale_milli
			.store(scale_milli.max(1), Ordering::Relaxed);
	}

	fn set(&self, data: Vec<u8>, w: u32, h: u32) {
		if let Ok(mut g) = self.latest.lock() {
			*g = Some((data, w, h));
		}
		self.seq.fetch_add(1, Ordering::Relaxed);
	}

	fn get(&self) -> Option<(Vec<u8>, u32, u32, u64)> {
		let seq = self.seq.load(Ordering::Relaxed);
		self.latest
			.lock()
			.ok()
			.and_then(|g| g.clone())
			.map(|(d, w, h)| (d, w, h, seq))
	}

	fn seq_value(&self) -> u64 {
		self.seq.load(Ordering::Relaxed)
	}
}

fn responds(obj: id, selector: Sel) -> bool {
	unsafe {
		let ok: bool = msg_send![obj, respondsToSelector: selector];
		ok
	}
}

fn run_on_main_thread<F: FnOnce()>(f: F) {
	unsafe {
		if pthread_main_np() != 0 {
			f();
			return;
		}

		struct Ctx<F: FnOnce()>(Option<F>);
		extern "C" fn trampoline<F: FnOnce()>(ctx: *mut c_void) {
			let ctx = unsafe { &mut *(ctx as *mut Ctx<F>) };
			if let Some(f) = ctx.0.take() {
				f();
			}
		}

		let mut ctx = Ctx(Some(f));
		dispatch_sync_f(
			&_dispatch_main_q as *const c_void,
			&mut ctx as *mut _ as *mut c_void,
			trampoline::<F>,
		);
	}
}

fn ensure_output_class() -> *const Class {
	static mut CLS: *const Class = std::ptr::null();
	static ONCE: std::sync::Once = std::sync::Once::new();

	ONCE.call_once(|| unsafe {
		let superclass = class!(NSObject);
		let mut decl = ClassDecl::new("RustFrameSCStreamOutput", superclass)
			.expect("Failed to declare RustFrameSCStreamOutput");

		decl.add_ivar::<*mut c_void>("rustState");

		extern "C" fn did_output_sample_buffer(
			this: &Object,
			_cmd: Sel,
			_stream: *mut Object,
			sample_buffer: *mut c_void,
			_type: NSUInteger,
		) {
			unsafe {
				let state_ptr: *mut c_void = *this.get_ivar("rustState");
				if state_ptr.is_null() {
					return;
				}
				let state: &OutputState = &*(state_ptr as *const OutputState);

				let pixel_buffer = CMSampleBufferGetImageBuffer(sample_buffer as *const c_void);
				if pixel_buffer.is_null() {
					return;
				}

						// ScreenCaptureKit can deliver different pixel formats depending on configuration.
						// We only handle 32BGRA here; anything else is ignored (but must not crash).
						let pixel_format = CVPixelBufferGetPixelFormatType(pixel_buffer);
						if pixel_format != KCVPIXELFORMATTYPE_32BGRA {
							return;
						}

				if CVPixelBufferLockBaseAddress(pixel_buffer, KCVPIXELBUFFERLOCK_READONLY) != 0 {
					return;
				}

						let base = CVPixelBufferGetBaseAddress(pixel_buffer) as *const u8;
						let bytes_per_row = CVPixelBufferGetBytesPerRow(pixel_buffer);
				let full_w = CVPixelBufferGetWidth(pixel_buffer) as u32;
				let full_h = CVPixelBufferGetHeight(pixel_buffer) as u32;

						if base.is_null() || bytes_per_row == 0 || full_w == 0 || full_h == 0 {
					let _ = CVPixelBufferUnlockBaseAddress(pixel_buffer, KCVPIXELBUFFERLOCK_READONLY);
					return;
				}

						let min_row_bytes = (full_w as usize).saturating_mul(4);
						if bytes_per_row < min_row_bytes {
							let _ = CVPixelBufferUnlockBaseAddress(pixel_buffer, KCVPIXELBUFFERLOCK_READONLY);
							return;
						}

				let scale_m = state.scale_milli.load(Ordering::Relaxed).max(1) as i64;
				let rx_pt = state.region_x.load(Ordering::Relaxed) as i64;
				let ry_pt = state.region_y.load(Ordering::Relaxed) as i64;
				let rw_pt = state.region_w.load(Ordering::Relaxed) as i64;
				let rh_pt = state.region_h.load(Ordering::Relaxed) as i64;

				// Convert region from points (top-left origin) to pixels.
				let mut rx_px = ((rx_pt * scale_m) + 500) / 1000;
				let mut ry_px = ((ry_pt * scale_m) + 500) / 1000;
				let mut rw_px = ((rw_pt * scale_m) + 500) / 1000;
				let mut rh_px = ((rh_pt * scale_m) + 500) / 1000;

				if rw_px <= 0 || rh_px <= 0 {
					// Fallback: full frame
					rx_px = 0;
					ry_px = 0;
					rw_px = full_w as i64;
					rh_px = full_h as i64;
				}

				// Clamp to bounds.
				if rx_px < 0 {
					rw_px += rx_px;
					rx_px = 0;
				}
				if ry_px < 0 {
					rh_px += ry_px;
					ry_px = 0;
				}

				let max_w = ((full_w as i64) - rx_px).max(0);
				let max_h = ((full_h as i64) - ry_px).max(0);
				rw_px = rw_px.clamp(0, max_w);
				rh_px = rh_px.clamp(0, max_h);

						let out_w = rw_px as u32;
						let out_h = rh_px as u32;
						let rx_bytes = (rx_px as usize).saturating_mul(4);
						let out_row_bytes = (out_w as usize).saturating_mul(4);
						if rx_bytes.saturating_add(out_row_bytes) > bytes_per_row {
							let _ = CVPixelBufferUnlockBaseAddress(pixel_buffer, KCVPIXELBUFFERLOCK_READONLY);
							return;
						}
				if out_w == 0 || out_h == 0 {
					let _ = CVPixelBufferUnlockBaseAddress(pixel_buffer, KCVPIXELBUFFERLOCK_READONLY);
					return;
				}

						let mut rgba = vec![0u8; (out_w as usize) * (out_h as usize) * 4];
						let out_stride = (out_w as usize) * 4;
				for y in 0..(out_h as usize) {
					let src_y = (ry_px as usize) + y;
					let src_row = base.add(src_y * bytes_per_row).add(rx_bytes);
					let dst_row = &mut rgba[y * out_stride..(y + 1) * out_stride];
					for x in 0..(out_w as usize) {
						let si = x * 4;
						let di = x * 4;
						let b = *src_row.add(si);
						let g = *src_row.add(si + 1);
						let r = *src_row.add(si + 2);
						let a = *src_row.add(si + 3);
						dst_row[di] = r;
						dst_row[di + 1] = g;
						dst_row[di + 2] = b;
						dst_row[di + 3] = a;
					}
				}

				let _ = CVPixelBufferUnlockBaseAddress(pixel_buffer, KCVPIXELBUFFERLOCK_READONLY);
				state.set(rgba, out_w, out_h);
			}
		}

		decl.add_method(
			sel!(stream:didOutputSampleBuffer:ofType:),
			did_output_sample_buffer
				as extern "C" fn(&Object, Sel, *mut Object, *mut c_void, NSUInteger),
		);

		CLS = decl.register();
	});

	unsafe { CLS }
}

pub struct ScreenCaptureKitCapture {
	state: Arc<OutputState>,
	output_obj: Option<StrongPtr>,
	stream: Option<StrongPtr>,
	show_cursor: bool,
}

unsafe impl Send for ScreenCaptureKitCapture {}

impl Drop for ScreenCaptureKitCapture {
	fn drop(&mut self) {
		self.stop();
	}
}

impl ScreenCaptureKitCapture {
	pub fn is_available() -> bool {
		unsafe { Class::get("SCShareableContent").is_some() && Class::get("SCStream").is_some() }
	}

	pub fn new() -> Self {
		Self {
			state: Arc::new(OutputState::new()),
			output_obj: None,
			stream: None,
			show_cursor: true,
		}
	}

	pub fn update_region_points(&self, x: i32, y: i32, w: u32, h: u32) {
		self.state.set_region_points(x, y, w, h);
	}

	pub fn start(
		&mut self,
		region_x: i32,
		region_y: i32,
		region_w: u32,
		region_h: u32,
		show_cursor: bool,
	) -> Result<()> {
		log::info!("[SCK] start() called: region=({},{}) size={}x{} cursor={}", region_x, region_y, region_w, region_h, show_cursor);
		
		let has_perm = unsafe { CGPreflightScreenCaptureAccess() };
		log::info!("[SCK] Screen Recording permission check: {}", has_perm);
		if !has_perm {
			log::error!("[SCK] No Screen Recording permission - aborting");
			return Err(anyhow!(
				"Screen Recording permission is not granted. Enable it in System Settings > Privacy & Security > Screen Recording, then restart the app."
			));
		}

		self.show_cursor = show_cursor;
		self.state
			.set_region_points(region_x, region_y, region_w, region_h);

		if !Self::is_available() {
			log::error!("[SCK] ScreenCaptureKit classes not available (macOS < 12.3?)");
			return Err(anyhow!("ScreenCaptureKit classes are not available"));
		}
		
		log::info!("[SCK] ScreenCaptureKit is available, initializing stream...");

		let state_ptr: *const OutputState = Arc::as_ptr(&self.state);
		let show_cursor_local = self.show_cursor;
		let mut created_stream: Option<StrongPtr> = None;
		let mut created_output: Option<StrongPtr> = None;
		let mut start_err: Option<String> = None;

		log::info!("[SCK] Dispatching SCK initialization to main thread...");
		run_on_main_thread(|| unsafe {
			autoreleasepool(|| {
				log::info!("[SCK] Inside main thread autorelease pool");
				// Backing scale (points->pixels) for main screen.
				let screen: id = msg_send![class!(NSScreen), mainScreen];
				let scale: f64 = if screen != nil {
					let s: f64 = msg_send![screen, backingScaleFactor];
					if s > 0.0 { s } else { 1.0 }
				} else {
					1.0
				};
				let scale_milli = (scale * 1000.0).round().max(1.0) as u32;
				(*(state_ptr)).set_scale_milli(scale_milli);

				let main_display_id = CGMainDisplayID();

				log::info!("[SCK] Fetching shareable content...");
				let sema = dispatch_semaphore_create(0);
				if sema.is_null() {
					log::error!("[SCK] Failed to create dispatch semaphore");
					start_err = Some("Failed to create dispatch semaphore".to_string());
					return;
				}
				log::info!("[SCK] Semaphore created: {:p}", sema);

				let shareable_cls = class!(SCShareableContent);
				log::info!("[SCK] SCShareableContent class obtained: {:p}", shareable_cls);
				
				let result: Arc<Mutex<(id, id)>> = Arc::new(Mutex::new((nil, nil)));
				let result_cb = result.clone();
				let sema_cb = sema;
				
				log::info!("[SCK] Creating completion handler block...");
				let block = ConcreteBlock::new(move |content: id, error: id| {
					// CRITICAL: Retain the Objective-C objects before storing them
					// They need to survive beyond the autorelease pool of the callback
					let retained_content = if content != nil {
						let _: id = msg_send![content, retain];
						content
					} else {
						nil
					};
					let retained_error = if error != nil {
						let _: id = msg_send![error, retain];
						error
					} else {
						nil
					};
					
					if let Ok(mut g) = result_cb.lock() {
						*g = (retained_content, retained_error);
					}
					unsafe {
						dispatch_semaphore_signal(sema_cb);
					}
				})
				.copy();
				log::info!("[SCK] Block created, calling getShareableContentWithCompletionHandler...");

				let _: () = msg_send![shareable_cls, getShareableContentWithCompletionHandler: &*block];
				log::info!("[SCK] Waiting for shareable content...");
				let _ = dispatch_semaphore_wait(sema, DISPATCH_TIME_FOREVER);
				log::info!("[SCK] Shareable content received");
				let (shareable, shareable_error) = result
					.lock()
					.ok()
					.map(|guard| (guard.0, guard.1))
					.unwrap_or((nil, nil));

				if shareable_error != nil {
					let desc: id = msg_send![shareable_error, localizedDescription];
					let cstr: *const std::os::raw::c_char = msg_send![desc, UTF8String];
					let s = if !cstr.is_null() {
						std::ffi::CStr::from_ptr(cstr).to_string_lossy().to_string()
					} else {
						"Unknown ScreenCaptureKit error".to_string()
					};
					start_err = Some(format!("ScreenCaptureKit getShareableContent failed: {s}"));
					return;
				}
				if shareable == nil {
					log::error!("[SCK] Shareable content is nil");
					start_err = Some("ScreenCaptureKit returned null shareable content".to_string());
					return;
				}
				
				log::info!("[SCK] Shareable content valid, fetching displays...");
				let displays: id = msg_send![shareable, displays];
				if displays == nil {
					start_err = Some("ScreenCaptureKit shareable content has no displays".to_string());
					return;
				}

				let count: NSUInteger = msg_send![displays, count];
				log::info!("[SCK] Found {} displays", count);
				let mut target_display: id = nil;
				for i in 0..count {
					let d: id = msg_send![displays, objectAtIndex: i];
					if d == nil {
						continue;
					}
					let did: u32 = msg_send![d, displayID];
					if did == main_display_id {
						log::info!("[SCK] Found main display: {}", did);
						target_display = d;
						break;
					}
				}
				if target_display == nil {
					log::error!("[SCK] Main display {} not found in shareable displays", main_display_id);
					start_err = Some("ScreenCaptureKit could not find main display".to_string());
					return;
				}

				log::info!("[SCK] Creating content filter...");
				// Create filter and config.
				let empty: id = NSArray::array(nil);
				let filter: id = msg_send![class!(SCContentFilter), alloc];
				let filter: id = msg_send![filter, initWithDisplay: target_display excludingWindows: empty];
				if filter == nil {
					start_err = Some("Failed to create SCContentFilter".to_string());
					return;
				}

				let cfg: id = msg_send![class!(SCStreamConfiguration), new];
				if cfg == nil {
					start_err = Some("Failed to create SCStreamConfiguration".to_string());
					return;
				}

				// Capture full display; crop in Rust using the current border region.
				// Set width/height to avoid SCKit downscaling.
				if screen != nil {
					let frame: NSRect = msg_send![screen, frame];
					let display_w_px = (frame.size.width * scale).round().max(1.0) as u32;
					let display_h_px = (frame.size.height * scale).round().max(1.0) as u32;
					if responds(cfg, sel!(setWidth:)) {
						let _: () = msg_send![cfg, setWidth: display_w_px as NSUInteger];
					}
					if responds(cfg, sel!(setHeight:)) {
						let _: () = msg_send![cfg, setHeight: display_h_px as NSUInteger];
					}
				}
				if responds(cfg, sel!(setShowsCursor:)) {
					let _: () = msg_send![cfg, setShowsCursor: if show_cursor_local { 1 } else { 0 }];
				}
				if responds(cfg, sel!(setPixelFormat:)) {
					let _: () = msg_send![cfg, setPixelFormat: KCVPIXELFORMATTYPE_32BGRA];
				}
				if responds(cfg, sel!(setQueueDepth:)) {
					let _: () = msg_send![cfg, setQueueDepth: 2 as NSUInteger];
				}

				log::info!("[SCK] Creating SCStream...");
				let stream: id = msg_send![class!(SCStream), alloc];
				let stream: id = msg_send![stream, initWithFilter: filter configuration: cfg delegate: nil];
				if stream == nil {
					log::error!("[SCK] Failed to create SCStream");
					start_err = Some("Failed to create SCStream".to_string());
					return;
				}
				log::info!("[SCK] SCStream created successfully");

				let out_cls = ensure_output_class();
				if out_cls.is_null() {
					start_err = Some("Failed to create SCStreamOutput class".to_string());
					return;
				}
				let out: id = msg_send![out_cls, new];
				if out == nil {
					start_err = Some("Failed to create SCStreamOutput instance".to_string());
					return;
				}

				let out_obj: &mut Object = &mut *(out as *mut Object);
				out_obj.set_ivar("rustState", state_ptr as *mut c_void);

				log::info!("[SCK] Adding stream output...");
				let queue = dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_DEFAULT, 0);
				let mut err: id = nil;
				let ok: bool = msg_send![stream, addStreamOutput: out type: SC_STREAM_OUTPUT_TYPE_SCREEN sampleHandlerQueue: queue error: &mut err];
				log::info!("[SCK] addStreamOutput result: {}", ok);
				if !ok {
					let desc: id = if err != nil { msg_send![err, localizedDescription] } else { nil };
					let cstr: *const std::os::raw::c_char = if desc != nil { msg_send![desc, UTF8String] } else { std::ptr::null() };
					let s = if !cstr.is_null() {
						std::ffi::CStr::from_ptr(cstr).to_string_lossy().to_string()
					} else {
						"Unknown error".to_string()
					};
					start_err = Some(format!("SCStream addStreamOutput failed: {s}"));
					return;
				}

				log::info!("[SCK] Starting capture stream...");
				// Start capture.
				let sema2 = dispatch_semaphore_create(0);
				if sema2.is_null() {
					start_err = Some("Failed to create semaphore for startCapture".to_string());
					return;
				}
				let start_error: Arc<Mutex<id>> = Arc::new(Mutex::new(nil));
				let start_error_cb = start_error.clone();
				let sema2_cb = sema2;
				let block2 = ConcreteBlock::new(move |error: id| {
					if let Ok(mut g) = start_error_cb.lock() {
						*g = error;
					}
					unsafe {
						dispatch_semaphore_signal(sema2_cb);
					}
				})
				.copy();
				let _: () = msg_send![stream, startCaptureWithCompletionHandler: &*block2];
				log::info!("[SCK] Waiting for startCapture completion...");
				let _ = dispatch_semaphore_wait(sema2, DISPATCH_TIME_FOREVER);
				log::info!("[SCK] startCapture completed");
				let start_error_obj = start_error.lock().ok().map(|guard| *guard).unwrap_or(nil);
				if start_error_obj != nil {
					let desc: id = msg_send![start_error_obj, localizedDescription];
					let cstr: *const std::os::raw::c_char = msg_send![desc, UTF8String];
					let s = if !cstr.is_null() {
						std::ffi::CStr::from_ptr(cstr).to_string_lossy().to_string()
					} else {
						"Unknown error".to_string()
					};
					start_err = Some(format!("SCStream startCapture failed: {s}"));
					return;
				}

				log::info!("[SCK] Stream started successfully!");
				created_stream = Some(StrongPtr::new(stream));
				created_output = Some(StrongPtr::new(out));
				
				// Release the retained objects from completion handler
				if shareable != nil {
					let _: () = msg_send![shareable, release];
				}
				if shareable_error != nil {
					let _: () = msg_send![shareable_error, release];
				}
			});
		});

		if let Some(err) = start_err {
			log::error!("[SCK] start() failed: {}", err);
			return Err(anyhow!(err));
		}
		self.stream = created_stream;
		self.output_obj = created_output;
		log::info!("[SCK] start() completed successfully");
		Ok(())
	}

	pub fn stop(&mut self) {
		let stream = self.stream.take();
		let output_obj = self.output_obj.take();
		if stream.is_none() && output_obj.is_none() {
			return;
		}

		run_on_main_thread(|| unsafe {
			autoreleasepool(|| {
				if let Some(stream) = stream {
					let stream: id = *stream;
					let sema = dispatch_semaphore_create(0);
					if !sema.is_null() {
						let sema_for_block = sema;
						let block = ConcreteBlock::new(move |_error: *mut Object| {
							dispatch_semaphore_signal(sema_for_block);
						})
						.copy();
						let _: () = msg_send![stream, stopCaptureWithCompletionHandler: &*block];
						let _ = dispatch_semaphore_wait(sema, DISPATCH_TIME_FOREVER);
					}
				}

				// Ensure ObjC objects are released on the main thread.
				drop(output_obj);
			});
		});
	}

	pub fn latest_frame_rgba(&self) -> Option<(Vec<u8>, u32, u32, u64)> {
		self.state.get()
	}

	pub fn latest_seq(&self) -> u64 {
		self.state.seq_value()
	}
}