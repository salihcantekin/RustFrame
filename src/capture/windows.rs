// capture/windows.rs - Windows Graphics Capture Implementation
//
// This module implements screen capture using the Windows Graphics Capture API (WGC).
// WGC is available on Windows 10 version 1903 (build 18362) and later.

use std::mem::size_of;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use log::{info, warn};
use windows::core::Interface;
use windows::Graphics::Capture::{
    Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
    D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING, D3D11_BOX,
    D3D11_MAPPED_SUBRESOURCE,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};

use crate::capture::CaptureRect;
use super::{CaptureEngine, CaptureFrame};

/// Windows-specific capture engine using Windows.Graphics.Capture API
pub struct WindowsCaptureEngine {
    // D3D11 resources
    d3d_device: Option<ID3D11Device>,
    d3d_context: Option<ID3D11DeviceContext>,
    direct3d_device: Option<IDirect3DDevice>,
    
    // Capture resources
    frame_pool: Option<Direct3D11CaptureFramePool>,
    capture_session: Option<GraphicsCaptureSession>,
    
    // State
    capture_region: Option<CaptureRect>,
    monitor_origin: (i32, i32),
    frame_ready: Arc<AtomicBool>,
    is_active: bool,
    show_cursor: bool,
}

impl WindowsCaptureEngine {
    /// Create a new Windows capture engine
    pub fn new() -> Result<Self> {
        info!("Creating WindowsCaptureEngine");
        
        // Note: COM initialization is done lazily in start() to avoid conflicts with winit
        
        Ok(Self {
            d3d_device: None,
            d3d_context: None,
            direct3d_device: None,
            frame_pool: None,
            capture_session: None,
            capture_region: None,
            monitor_origin: (0, 0),
            frame_ready: Arc::new(AtomicBool::new(false)),
            is_active: false,
            show_cursor: true,
        })
    }
    
    /// Create a Direct3D11 device
    fn create_d3d_device() -> Result<(ID3D11Device, ID3D11DeviceContext)> {
        let mut device = None;
        let mut context = None;

        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                windows::Win32::Foundation::HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .context("D3D11CreateDevice failed")?;
        }

        Ok((
            device.ok_or_else(|| anyhow!("Device creation returned null"))?,
            context.ok_or_else(|| anyhow!("Context creation returned null"))?,
        ))
    }
    
    /// Create a WinRT Direct3D device from a D3D11 device
    fn create_direct3d_device(d3d_device: &ID3D11Device) -> Result<IDirect3DDevice> {
        use windows::core::PCWSTR;
        use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

        let dxgi_device: IDXGIDevice = d3d_device
            .cast()
            .context("Failed to cast ID3D11Device to IDXGIDevice")?;

        unsafe {
            let dll_name = windows::core::w!("d3d11.dll");
            let module = LoadLibraryW(PCWSTR(dll_name.as_ptr()))
                .context("Failed to load d3d11.dll")?;

            let func_name = windows::core::s!("CreateDirect3D11DeviceFromDXGIDevice");
            let func_ptr = GetProcAddress(module, windows::core::PCSTR(func_name.as_ptr()))
                .ok_or_else(|| anyhow!("CreateDirect3D11DeviceFromDXGIDevice not found"))?;

            type CreateFn = unsafe extern "system" fn(
                dxgi_device: *mut std::ffi::c_void,
                result: *mut *mut std::ffi::c_void,
            ) -> windows::core::HRESULT;

            let create_fn: CreateFn = std::mem::transmute(func_ptr);

            let mut result_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
            let hr = create_fn(dxgi_device.as_raw() as *mut _, &mut result_ptr);

            if hr.is_err() {
                return Err(anyhow!("CreateDirect3D11DeviceFromDXGIDevice failed: {:?}", hr));
            }

            if result_ptr.is_null() {
                return Err(anyhow!("CreateDirect3D11DeviceFromDXGIDevice returned null"));
            }

            Ok(IDirect3DDevice::from_raw(result_ptr))
        }
    }
    
    /// Create a GraphicsCaptureItem for the monitor containing the given point
    fn create_capture_item_for_monitor(point: (i32, i32)) -> Result<(GraphicsCaptureItem, (i32, i32))> {
        let pt = POINT { x: point.0, y: point.1 };
        let monitor = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };

        if monitor.is_invalid() {
            return Err(anyhow!("Failed to get monitor for point {:?}", point));
        }
        
        info!("Detected monitor for point {:?}", point);

        let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;

        let mut monitor_info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };

        let info_ok = unsafe { GetMonitorInfoW(monitor, &mut monitor_info) }.as_bool();
        if !info_ok {
            return Err(anyhow!("GetMonitorInfoW failed"));
        }

        let item = unsafe { interop.CreateForMonitor(monitor)? };

        Ok((item, (monitor_info.rcMonitor.left, monitor_info.rcMonitor.top)))
    }
    
    /// Copy texture to CPU-accessible staging texture and read pixels
    fn copy_frame_to_cpu(
        &self,
        source_texture: &ID3D11Texture2D,
        region: &CaptureRect,
    ) -> Option<CaptureFrame> {
        let d3d_device = self.d3d_device.as_ref()?;
        let d3d_context = self.d3d_context.as_ref()?;
        
        let width = region.width as u32;
        let height = region.height as u32;
        
        // Log once per 60 frames
        static COPY_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let count = COPY_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if count % 60 == 0 {
            info!("Copying frame to CPU: {}x{} (frame #{})", width, height, count);
        }
        
        // Create staging texture for CPU read
        let staging_desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        
        let mut staging_texture = None;
        unsafe {
            if d3d_device.CreateTexture2D(&staging_desc, None, Some(&mut staging_texture)).is_err() {
                warn!("Failed to create staging texture");
                return None;
            }
        }
        
        let staging_texture = staging_texture?;
        
        // Calculate source region (offset by monitor origin)
        let src_x = (region.x - self.monitor_origin.0) as u32;
        let src_y = (region.y - self.monitor_origin.1) as u32;
        
        // Copy region from source to staging
        let src_box = D3D11_BOX {
            left: src_x,
            top: src_y,
            front: 0,
            right: src_x + width,
            bottom: src_y + height,
            back: 1,
        };
        
        unsafe {
            d3d_context.CopySubresourceRegion(
                &staging_texture,
                0,
                0, 0, 0,
                source_texture,
                0,
                Some(&src_box),
            );
        }
        
        // Map the staging texture and read pixels
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            if d3d_context.Map(&staging_texture, 0, D3D11_MAP_READ, 0, Some(&mut mapped)).is_err() {
                warn!("Failed to map staging texture");
                return None;
            }
        }
        
        let stride = mapped.RowPitch as usize;
        let row_bytes = (width * 4) as usize;  // 4 bytes per pixel (BGRA)
        
        // Copy row by row, removing stride padding
        let mut data = vec![0u8; row_bytes * height as usize];
        
        unsafe {
            let src_ptr = mapped.pData as *const u8;
            for row in 0..height as usize {
                let src_row = src_ptr.add(row * stride);
                let dst_row = data.as_mut_ptr().add(row * row_bytes);
                std::ptr::copy_nonoverlapping(src_row, dst_row, row_bytes);
            }
            d3d_context.Unmap(&staging_texture, 0);
        }
        
        Some(CaptureFrame {
            data,
            width,
            height,
            stride: row_bytes as u32,
        })
    }
}

impl CaptureEngine for WindowsCaptureEngine {
    fn start(&mut self, region: CaptureRect, show_cursor: bool) -> Result<()> {
        info!("Starting capture for region: {:?}", region);
        
        // Create D3D11 device
        let (d3d_device, d3d_context) = Self::create_d3d_device()?;
        info!("Created D3D11 device");
        
        // Create WinRT Direct3D device
        let direct3d_device = Self::create_direct3d_device(&d3d_device)?;
        info!("Created WinRT Direct3D device");
        
        // Create capture item for monitor
        let center_point = (
            region.x + (region.width as i32) / 2, 
            region.y + (region.height as i32) / 2
        );
        let (capture_item, monitor_origin) = Self::create_capture_item_for_monitor(center_point)?;
        info!("Created capture item for monitor at origin {:?}", monitor_origin);
        
        // Get capture size
        let size = capture_item.Size()?;
        info!("Monitor size: {}x{}", size.Width, size.Height);
        
        // Create frame pool
        let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &direct3d_device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2,  // Double buffering
            size,
        )?;
        info!("Created frame pool");
        
        // Create capture session
        let capture_session = frame_pool.CreateCaptureSession(&capture_item)?;
        
        // Configure cursor capture
        capture_session.SetIsCursorCaptureEnabled(show_cursor)?;
        
        // Try to disable border (Windows 11+)
        if let Err(_) = capture_session.SetIsBorderRequired(false) {
            info!("SetIsBorderRequired not supported (pre-Windows 11)");
        }
        
        // Note: We don't use FrameArrived event because it requires a DispatcherQueue
        // Instead, we poll TryGetNextFrame directly in get_frame()
        
        // Start capturing
        capture_session.StartCapture()?;
        info!("Capture started");
        
        // Store resources
        self.d3d_device = Some(d3d_device);
        self.d3d_context = Some(d3d_context);
        self.direct3d_device = Some(direct3d_device);
        self.frame_pool = Some(frame_pool);
        self.capture_session = Some(capture_session);
        self.capture_region = Some(region);
        self.monitor_origin = monitor_origin;
        self.show_cursor = show_cursor;
        self.is_active = true;
        
        Ok(())
    }
    
    fn stop(&mut self) {
        info!("Stopping capture");
        
        // Close session
        if let Some(session) = self.capture_session.take() {
            let _ = session.Close();
        }
        
        // Close frame pool
        if let Some(pool) = self.frame_pool.take() {
            let _ = pool.Close();
        }
        
        // Clear other resources
        self.direct3d_device = None;
        self.d3d_context = None;
        self.d3d_device = None;
        self.capture_region = None;
        self.is_active = false;
        
        info!("Capture stopped");
    }
    
    fn is_active(&self) -> bool {
        self.is_active
    }
    
    fn has_new_frame(&self) -> bool {
        // Always return true when active - we'll poll in get_frame
        self.is_active
    }
    
    fn get_frame(&mut self) -> Option<CaptureFrame> {
        if !self.is_active {
            return None;
        }
        
        let frame_pool = self.frame_pool.as_ref()?;
        let region = self.capture_region.as_ref()?;
        
        // Try to get frame from pool (non-blocking)
        let frame = match frame_pool.TryGetNextFrame() {
            Ok(f) => {
                info!("Got frame from pool!");
                f
            },
            Err(e) => {
                // Log only occasionally to avoid spam
                static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if count % 60 == 0 {
                    warn!("No frame available (attempt {}): {:?}", count, e);
                }
                return None;
            }
        };
        
        // Get surface from frame
        let surface = match frame.Surface() {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to get surface: {:?}", e);
                return None;
            }
        };
        
        // Get the D3D11 texture from the surface
        use windows::Win32::Graphics::Direct3D11::ID3D11Resource;
        use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;
        
        let access: IDirect3DDxgiInterfaceAccess = match surface.cast() {
            Ok(a) => a,
            Err(e) => {
                warn!("Failed to cast surface: {:?}", e);
                return None;
            }
        };
        
        let texture: ID3D11Texture2D = match unsafe { access.GetInterface() } {
            Ok(t) => t,
            Err(e) => {
                warn!("Failed to get texture interface: {:?}", e);
                return None;
            }
        };
        
        // Copy to CPU and return
        self.copy_frame_to_cpu(&texture, region)
    }
    
    fn set_cursor_visible(&mut self, visible: bool) -> Result<()> {
        self.show_cursor = visible;
        if let Some(session) = &self.capture_session {
            session.SetIsCursorCaptureEnabled(visible)?;
        }
        Ok(())
    }
    
    fn get_region(&self) -> Option<CaptureRect> {
        self.capture_region.clone()
    }
    
    fn update_region(&mut self, region: CaptureRect) -> Result<()> {
        // Just update the capture region - the frame pool will continue to capture
        // from the same monitor, but we'll crop to the new region in copy_frame_to_cpu
        info!("Updating capture region to: {:?}", region);
        self.capture_region = Some(region);
        Ok(())
    }
}

// SAFETY: COM objects in WGC are thread-safe
unsafe impl Send for WindowsCaptureEngine {}
unsafe impl Sync for WindowsCaptureEngine {}
