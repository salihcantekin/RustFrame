// capture.rs - Windows.Graphics.Capture API Implementation
//
// This module wraps the Windows.Graphics.Capture (WGC) API, which is the modern,
// GPU-accelerated way to capture screen content on Windows 10/11.
//
// WHY WGC instead of GDI/BitBlt?
// - GPU-accelerated (no CPU-side copy)
// - Works with modern Windows features (DPI scaling, HDR, multi-monitor)
// - Lower latency and higher performance
// - Supports capturing specific windows or monitors directly
//
// ARCHITECTURE:
// 1. Create a Direct3D11 device (COM object)
// 2. Create a GraphicsCaptureItem (the thing we're capturing - screen, window, etc.)
// 3. Create a Direct3D11CaptureFramePool (manages texture buffers)
// 4. Create a GraphicsCaptureSession and start it
// 5. Handle FrameArrived events to get new frames

use anyhow::{Result, anyhow, Context};
use log::{info, warn};
use windows::{
    Foundation::TypedEventHandler,
    Graphics::{
        Capture::{
            Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
        },
        DirectX::{
            Direct3D11::{IDirect3DDevice, IDirect3DSurface},
            DirectXPixelFormat,
        },
    },
    Win32::{
        Foundation::RECT,
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::{
                D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
            },
            Dxgi::IDXGIDevice,
            Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTOPRIMARY},
        },
        System::{
            WinRT::Graphics::Capture::IGraphicsCaptureItemInterop,
            Com::{CoInitializeEx, COINIT_MULTITHREADED},
        },
        UI::WindowsAndMessaging::GetDesktopWindow,
    },
};
use std::sync::Arc;

/// Represents a rectangular region on the screen
#[derive(Debug, Clone, Copy)]
pub struct CaptureRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Settings for the capture session
#[derive(Debug, Clone)]
pub struct CaptureSettings {
    /// Whether to show the mouse cursor in the capture
    pub show_cursor: bool,
    /// Whether to show window border after capture starts
    pub show_border: bool,
    /// Border width in pixels (only used if show_border is true)
    pub border_width: u32,
    /// Whether to exclude destination from screen capture (prevents infinite mirror)
    /// Note: If true, Google Meet "window share" will show black. Use "screen share" instead.
    pub exclude_from_capture: bool,
}

impl Default for CaptureSettings {
    /// Default settings for PRODUCTION mode
    fn default() -> Self {
        Self {
            show_cursor: true,
            show_border: true,
            border_width: crate::constants::capture::DEFAULT_BORDER_WIDTH,
            exclude_from_capture: true,
        }
    }
}

impl CaptureSettings {
    /// Development mode settings - destination window visible beside overlay
    pub fn for_development() -> Self {
        Self {
            show_cursor: true,
            show_border: true,
            border_width: crate::constants::capture::DEFAULT_BORDER_WIDTH,
            exclude_from_capture: false,
        }
    }
}

impl From<CaptureRect> for RECT {
    fn from(rect: CaptureRect) -> Self {
        RECT {
            left: rect.x,
            top: rect.y,
            right: rect.x + rect.width as i32,
            bottom: rect.y + rect.height as i32,
        }
    }
}

/// The main capture engine that wraps Windows.Graphics.Capture
pub struct CaptureEngine {
    /// Direct3D11 device (COM object) - this is the GPU device
    /// SAFETY: Must be kept alive for the entire capture session
    d3d_device: ID3D11Device,

    /// Direct3D11 device context - used for GPU operations
    d3d_context: ID3D11DeviceContext,

    /// WinRT wrapper around our D3D11 device (needed for WGC API)
    /// This bridges Win32 D3D11 and WinRT APIs
    #[allow(dead_code)]
    direct3d_device: IDirect3DDevice,

    /// The item we're capturing (could be a monitor, window, etc.)
    #[allow(dead_code)]
    capture_item: GraphicsCaptureItem,

    /// The frame pool that manages texture buffers for captured frames
    /// This is like a ring buffer of textures
    #[allow(dead_code)]
    frame_pool: Direct3D11CaptureFramePool,

    /// The active capture session
    /// IMPORTANT: Dropping this stops the capture!
    capture_session: GraphicsCaptureSession,

    /// The region we want to capture (cropping rectangle)
    capture_region: CaptureRect,

    /// Monitor origin (top-left) in virtual screen coordinates, used for cropping
    monitor_origin: (i32, i32),

    /// Flag indicating a new frame is ready
    frame_ready: Arc<std::sync::atomic::AtomicBool>,
}

impl CaptureEngine {
    /// Create a new capture engine for a specific screen region
    pub fn new(region: CaptureRect, settings: &CaptureSettings) -> Result<Self> {
        info!("Initializing CaptureEngine for region: {:?}", region);
        info!("Capture settings: show_cursor={}, exclude_from_capture={}", 
              settings.show_cursor, settings.exclude_from_capture);

        // STEP 1: Initialize COM (Component Object Model)
        // This is REQUIRED before using any Windows COM APIs (including D3D11 and WGC)
        // COINIT_MULTITHREADED allows COM calls from any thread
        unsafe {
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            // S_OK (0) or S_FALSE (1) means success
            // RPC_E_CHANGED_MODE (0x80010106) means COM already initialized, which is OK
            if hr.is_err() {
                let code = hr.0;
                // Ignore RPC_E_CHANGED_MODE - COM already initialized
                if code != 0x80010106u32 as i32 {
                    return Err(anyhow!("Failed to initialize COM: HRESULT 0x{:08X}", code as u32));
                }
                info!("COM already initialized (different apartment type)");
            } else {
                info!("COM initialized");
            }
        }

        // STEP 2: Create Direct3D11 Device
        // This is the GPU device that will handle all graphics operations
        let (d3d_device, d3d_context) = Self::create_d3d_device()?;
        info!("D3D11 device created");

        // STEP 3: Create WinRT Direct3D device wrapper
        // WGC is a WinRT API, so we need to wrap our Win32 D3D11 device
        let direct3d_device = Self::create_direct3d_device(&d3d_device)?;
        info!("WinRT Direct3D device created");

        // STEP 4: Create GraphicsCaptureItem for the primary monitor
        // In a full implementation, you'd want to capture a specific window
        // or allow the user to pick. For now, we capture the entire primary monitor.
        let (capture_item, monitor_origin) = Self::create_capture_item_for_monitor()?;
        info!("GraphicsCaptureItem created for primary monitor");

        // STEP 5: Create the frame pool
        // This allocates GPU textures that will hold captured frames
        // We use a small pool (2 buffers) for double-buffering
        let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &direct3d_device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized, // Standard BGRA format
            2, // Number of buffers (2 = double buffering)
            capture_item.Size()?, // Size of the capture
        )?;
        info!("Frame pool created with 2 buffers");

        // STEP 6: Create the capture session
        let capture_session = frame_pool.CreateCaptureSession(&capture_item)?;
        info!("Capture session created");

        // Configure cursor visibility
        // IsCursorCaptureEnabled controls whether the mouse cursor appears in the capture
        capture_session.SetIsCursorCaptureEnabled(settings.show_cursor)?;
        info!("Cursor capture enabled: {}", settings.show_cursor);

        // STEP 7: Set up frame arrival event handler
        // This is called every time a new frame is ready
        let frame_ready = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let frame_ready_clone = Arc::clone(&frame_ready);

        frame_pool.FrameArrived(&TypedEventHandler::new(
            move |_pool, _args| {
                frame_ready_clone.store(true, std::sync::atomic::Ordering::Release);
                Ok(())
            },
        ))?;
        info!("Frame arrival event handler registered");

        // STEP 8: Start the capture!
        capture_session.StartCapture()?;
        info!("Capture started successfully");

        Ok(Self {
            d3d_device,
            d3d_context,
            direct3d_device,
            capture_item,
            frame_pool,
            capture_session,
            capture_region: region,
            monitor_origin,
            frame_ready,
        })
    }

    /// Create a Direct3D11 device
    /// This is the GPU device that will handle all rendering and capture
    fn create_d3d_device() -> Result<(ID3D11Device, ID3D11DeviceContext)> {
        let mut device = None;
        let mut context = None;

        // SAFETY: This is a standard D3D11 device creation call
        // We're using hardware acceleration (GPU) and BGRA support for better compatibility
        unsafe {
            D3D11CreateDevice(
                None, // Use default adapter (primary GPU)
                D3D_DRIVER_TYPE_HARDWARE, // Use hardware acceleration
                windows::Win32::Foundation::HMODULE::default(), // No software rasterizer
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, // Enable BGRA format (needed for WGC)
                None, // Use default feature levels
                D3D11_SDK_VERSION, // SDK version
                Some(&mut device), // Output device
                None, // Don't care about feature level
                Some(&mut context), // Output context
            )
            .context("D3D11CreateDevice failed")?;
        }

        Ok((
            device.ok_or_else(|| anyhow!("Device creation returned null"))?,
            context.ok_or_else(|| anyhow!("Context creation returned null"))?,
        ))
    }

    /// Create a WinRT Direct3D device from a D3D11 device
    /// This bridges Win32 D3D11 and WinRT APIs
    fn create_direct3d_device(d3d_device: &ID3D11Device) -> Result<IDirect3DDevice> {
        use windows::core::Interface;
        use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
        use windows::core::PCWSTR;
        
        // Cast the D3D11 device to a DXGI device
        let dxgi_device: IDXGIDevice = d3d_device
            .cast()
            .context("Failed to cast ID3D11Device to IDXGIDevice")?;

        // Manually load CreateDirect3D11DeviceFromDXGIDevice from d3d11.dll
        // This function is not exposed in windows 0.58 but exists in the DLL
        unsafe {
            // Load d3d11.dll
            let dll_name = windows::core::w!("d3d11.dll");
            let module = LoadLibraryW(PCWSTR(dll_name.as_ptr()))
                .context("Failed to load d3d11.dll")?;
            
            // Get the function pointer (ANSI name for GetProcAddress)
            let func_name = windows::core::s!("CreateDirect3D11DeviceFromDXGIDevice");
            let func_ptr = GetProcAddress(module, windows::core::PCSTR(func_name.as_ptr()))
                .ok_or_else(|| anyhow!("CreateDirect3D11DeviceFromDXGIDevice not found in d3d11.dll"))?;


            // Define the function signature
            type CreateDirect3D11DeviceFromDXGIDeviceFn = unsafe extern "system" fn(
                dxgi_device: *mut std::ffi::c_void,
                result: *mut *mut std::ffi::c_void,
            ) -> windows::core::HRESULT;
            
            let create_fn: CreateDirect3D11DeviceFromDXGIDeviceFn = 
                std::mem::transmute(func_ptr);
            
            // Call the function
            let mut result_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
            let hr = create_fn(
                dxgi_device.as_raw() as *mut _,
                &mut result_ptr,
            );
            
            if hr.is_err() {
                return Err(anyhow!(
                    "CreateDirect3D11DeviceFromDXGIDevice failed: HRESULT 0x{:08X}",
                    hr.0 as u32
                ));
            }
            
            if result_ptr.is_null() {
                return Err(anyhow!("CreateDirect3D11DeviceFromDXGIDevice returned null"));
            }
            
            // Wrap the result in IDirect3DDevice
            Ok(IDirect3DDevice::from_raw(result_ptr))
        }
    }

    /// Create a GraphicsCaptureItem for the primary monitor
    ///
    /// NOTE: In a production app, you might want to:
    /// - Let the user pick a window/monitor using GraphicsCapturePicker
    /// - Capture a specific HWND
    /// - Support multi-monitor setups
    fn create_capture_item_for_monitor() -> Result<(GraphicsCaptureItem, (i32, i32))> {
        // Get the primary monitor
        // SAFETY: These are standard Win32 API calls
        let hwnd = unsafe { GetDesktopWindow() };
        let monitor = unsafe {
            MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY)
        };

        if monitor.is_invalid() {
            return Err(anyhow!("Failed to get primary monitor"));
        }

        // Create a GraphicsCaptureItem from the monitor
        // SAFETY: This uses the IGraphicsCaptureItemInterop COM interface
        // which is the official way to create capture items from HWNDs/monitors
        let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;

        // Query monitor origin for cropping math
        let mut monitor_info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };

        let info_ok = unsafe { GetMonitorInfoW(monitor, &mut monitor_info) }.as_bool();
        if !info_ok {
            return Err(anyhow!("GetMonitorInfoW failed for primary monitor"));
        }

        let item = unsafe { interop.CreateForMonitor(monitor)? };

        Ok((item, (monitor_info.rcMonitor.left, monitor_info.rcMonitor.top)))
    }

    /// Update the capture region (when the overlay window is moved/resized)
    pub fn update_region(&mut self, new_region: CaptureRect) -> Result<()> {
        info!("Updating capture region to {:?}", new_region);
        self.capture_region = new_region;

        // Note: WGC captures the entire item (monitor/window)
        // Cropping happens in the rendering stage
        // If you wanted to reduce GPU load, you'd need to recreate the capture session
        // with a new item (e.g., a specific window instead of the full monitor)

        Ok(())
    }
    
    /// Update cursor visibility in the capture
    pub fn update_cursor_visibility(&self, show_cursor: bool) -> Result<()> {
        info!("Updating cursor visibility to: {}", show_cursor);
        self.capture_session.SetIsCursorCaptureEnabled(show_cursor)?;
        Ok(())
    }

    /// Get the D3D11 device (needed by renderer)
    pub fn get_d3d_device(&self) -> &ID3D11Device {
        &self.d3d_device
    }

    /// Get the D3D11 device context (needed by renderer)
    pub fn get_d3d_context(&self) -> &ID3D11DeviceContext {
        &self.d3d_context
    }

    /// Get the latest captured frame surface directly from the pool
    /// This pulls from the frame pool synchronously
    pub fn get_latest_frame_surface(&self) -> Option<IDirect3DSurface> {
        if self.frame_ready.load(std::sync::atomic::Ordering::Acquire) {
            // Try to get the next frame from the pool
            match self.frame_pool.TryGetNextFrame() {
                Ok(frame) => {
                    match frame.Surface() {
                        Ok(surface) => {
                            self.frame_ready.store(false, std::sync::atomic::Ordering::Release);
                            return Some(surface);
                        }
                        Err(e) => {
                            warn!("Failed to get surface from frame: {}", e);
                        }
                    }
                }
                Err(_e) => {
                    // No frame ready
                }
            }
        }
        None
    }

    /// Get the capture region (for cropping in the renderer)
    pub fn get_capture_region(&self) -> CaptureRect {
        self.capture_region
    }

    /// Get monitor origin (top-left) in virtual screen coordinates
    pub fn get_monitor_origin(&self) -> (i32, i32) {
        self.monitor_origin
    }
}

// SAFETY: These are COM objects that are thread-safe
// We need to implement Send to use CaptureEngine across threads
unsafe impl Send for CaptureEngine {}
unsafe impl Sync for CaptureEngine {}
