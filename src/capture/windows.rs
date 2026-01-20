// capture/windows.rs - Windows Graphics Capture Implementation
//
// This module implements screen capture using the Windows Graphics Capture API (WGC).
// WGC is available on Windows 10 version 1903 (build 18362) and later.

use std::mem::size_of;

use anyhow::{anyhow, Context, Result};
use log::{debug, info, warn};
use windows::core::Interface;
use windows::Graphics::Capture::{
    Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_BOX,
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
    D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;

use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DrawIconEx, GetCursorInfo, GetIconInfo, CURSORINFO, DI_NORMAL, ICONINFO,
};

use super::{CaptureEngine, CaptureFrame};
use crate::capture::CaptureRect;
use crate::window_filter::WindowIdentifier;

// Global state for cursor filtering and include-only masking
lazy_static::lazy_static! {
    // Only set once at capture start: true if preview overlaps with capture region
    static ref SHOULD_FILTER_CURSOR: std::sync::Mutex<bool> = std::sync::Mutex::new(false);
    // Preview window bounds (for overlap detection)
    static ref PREVIEW_WINDOW_RECT: std::sync::Mutex<Option<(i32, i32, i32, i32)>> = std::sync::Mutex::new(None);
    // Included windows used to prevent black-masking over selected windows in include-only mode
    static ref INCLUDED_WINDOWS_FOR_MASK: std::sync::Mutex<Vec<WindowIdentifier>> = std::sync::Mutex::new(Vec::new());
}

/// Provide included windows to the masking layer (include-only mode)
pub fn set_included_windows_for_mask(list: Vec<WindowIdentifier>) {
    if let Ok(mut guard) = INCLUDED_WINDOWS_FOR_MASK.lock() {
        *guard = list;
    }
}

/// Clear included windows (on capture stop)
pub fn clear_included_windows_for_mask() {
    if let Ok(mut guard) = INCLUDED_WINDOWS_FOR_MASK.lock() {
        guard.clear();
    }
}

/// Set preview window bounds and check if it overlaps with capture region
/// This is called once at capture start, not per-frame
pub fn set_preview_bounds_and_check_overlap(px: i32, py: i32, pw: i32, ph: i32, cx: i32, cy: i32, cw: i32, ch: i32) {
    // Store preview bounds
    if let Ok(mut rect) = PREVIEW_WINDOW_RECT.lock() {
        *rect = Some((px, py, pw, ph));
    }
    
    // Check if preview overlaps capture region
    let overlaps = !(px + pw <= cx || px >= cx + cw || py + ph <= cy || py >= cy + ch);
    
    if let Ok(mut filter) = SHOULD_FILTER_CURSOR.lock() {
        *filter = overlaps;
        if overlaps {
            log::info!("🔄 Preview overlaps capture region - cursor filtering ENABLED");
        } else {
            log::info!("✓ No overlap - cursor filtering DISABLED");
        }
    }
}

/// Clear cursor filtering on capture stop
pub fn clear_cursor_filtering() {
    if let Ok(mut rect) = PREVIEW_WINDOW_RECT.lock() {
        *rect = None;
    }
    if let Ok(mut filter) = SHOULD_FILTER_CURSOR.lock() {
        *filter = false;
    }
}

/// Get current preview window bounds (for testing/debugging)
pub fn get_preview_window_rect() -> Option<(i32, i32, i32, i32)> {
    PREVIEW_WINDOW_RECT.lock().ok().and_then(|r| *r)
}

/// Windows GDI-based capture engine (RegionToShare-style)
///
/// Captures a screen region by copying pixels from the desktop DC (BitBlt/CopyFromScreen equivalent)
/// into a DIB section, then returning BGRA bytes.
pub struct WindowsGdiCopyCaptureEngine {
    capture_region: Option<CaptureRect>,
    is_active: bool,
    show_cursor: bool,
    excluded_windows: Vec<WindowIdentifier>,
}

impl WindowsGdiCopyCaptureEngine {
    pub fn new() -> Result<Self> {
        Ok(Self {
            capture_region: None,
            is_active: false,
            show_cursor: true,
            excluded_windows: Vec::new(),
        })
    }

    fn virtual_screen_bounds() -> (i32, i32, i32, i32) {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
            SM_YVIRTUALSCREEN,
        };

        unsafe {
            let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            (x, y, x + w, y + h)
        }
    }

    fn clip_region(region: &CaptureRect) -> Option<(i32, i32, u32, u32)> {
        let (vx0, vy0, vx1, vy1) = Self::virtual_screen_bounds();

        let x0 = region.x.max(vx0);
        let y0 = region.y.max(vy0);
        let x1 = (region.x + region.width as i32).min(vx1);
        let y1 = (region.y + region.height as i32).min(vy1);

        if x0 >= x1 || y0 >= y1 {
            return None;
        }

        Some((x0, y0, (x1 - x0) as u32, (y1 - y0) as u32))
    }

    unsafe fn draw_cursor_if_needed(mem_dc: HDC, offset_x: i32, offset_y: i32) {
        let mut ci = CURSORINFO {
            cbSize: std::mem::size_of::<CURSORINFO>() as u32,
            ..Default::default()
        };

        if GetCursorInfo(&mut ci).is_err() {
            return;
        }

        // CURSOR_SHOWING = 0x00000001
        if (ci.flags.0 & 0x00000001) == 0 {
            return;
        }

        // Optimization: Only check preview overlap if filtering is enabled
        // This was set once at capture start, not per-frame
        if let Ok(filter) = SHOULD_FILTER_CURSOR.lock() {
            if *filter {
                if let Ok(rect) = PREVIEW_WINDOW_RECT.lock() {
                    if let Some((px, py, pw, ph)) = *rect {
                        let cursor_x = ci.ptScreenPos.x;
                        let cursor_y = ci.ptScreenPos.y;
                
                        // Debug logging (every 60 frames to avoid spam)
                        static mut FRAME_COUNT: u32 = 0;
                        unsafe {
                            FRAME_COUNT += 1;
                            if FRAME_COUNT % 60 == 0 {
                                log::debug!(
                                    "Cursor filter check: cursor=({}, {}), preview=({}, {}, {}x{})",
                                    cursor_x, cursor_y, px, py, pw, ph
                                );
                            }
                        }
                
                        // Check if cursor is within preview window bounds
                        if cursor_x >= px && cursor_x < px + pw &&
                           cursor_y >= py && cursor_y < py + ph {
                            log::debug!("🎯 Cursor filtered (inside preview window)");
                            return; // Skip drawing cursor over preview window
                        }
                    }
                }
            }
        }

        let mut icon_info = ICONINFO::default();
        if GetIconInfo(ci.hCursor.into(), &mut icon_info).is_err() {
            return;
        }

        // hotspot relative to cursor position
        let cursor_x = ci.ptScreenPos.x - offset_x - icon_info.xHotspot as i32;
        let cursor_y = ci.ptScreenPos.y - offset_y - icon_info.yHotspot as i32;

        let _ = DrawIconEx(
            mem_dc,
            cursor_x,
            cursor_y,
            ci.hCursor.into(),
            0,
            0,
            0,
            None,
            DI_NORMAL,
        );

        if !icon_info.hbmMask.is_invalid() {
            let _ = DeleteObject(icon_info.hbmMask.into());
        }
        if !icon_info.hbmColor.is_invalid() {
            let _ = DeleteObject(icon_info.hbmColor.into());
        }
    }
}

impl CaptureEngine for WindowsGdiCopyCaptureEngine {
    fn start(&mut self, region: CaptureRect, show_cursor: bool, _excluded_windows: Option<Vec<WindowIdentifier>>) -> Result<()> {
        self.capture_region = Some(region);
        self.is_active = true;
        self.show_cursor = show_cursor;
        self.excluded_windows = _excluded_windows.unwrap_or_default();
        Ok(())
    }

    fn stop(&mut self) {
        self.is_active = false;
        self.capture_region = None;
    }

    fn is_active(&self) -> bool {
        self.is_active
    }

    fn has_new_frame(&self) -> bool {
        self.is_active
    }

    fn get_frame(&mut self) -> Option<CaptureFrame> {
        if !self.is_active {
            return None;
        }

        let region = self.capture_region.as_ref()?;
        let (x, y, width, height) = Self::clip_region(region)?;

        let row_bytes = (width * 4) as usize;
        let mut data = vec![0u8; row_bytes * height as usize];

        unsafe {
            let screen_dc = GetDC(None);
            if screen_dc.is_invalid() {
                return None;
            }

            let mem_dc = CreateCompatibleDC(Some(screen_dc));
            if mem_dc.is_invalid() {
                let _ = ReleaseDC(None, screen_dc);
                return None;
            }

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let dib: windows::core::Result<HBITMAP> =
                CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0);

            let bitmap = match dib {
                Ok(b) => b,
                Err(_) => {
                    let _ = DeleteDC(mem_dc);
                    let _ = ReleaseDC(None, screen_dc);
                    return None;
                }
            };

            let old = SelectObject(mem_dc, bitmap.into());

            // Copy pixels from screen into our bitmap
            use windows::Win32::Graphics::Gdi::{BitBlt, SRCCOPY};
            let _ = BitBlt(
                mem_dc,
                0,
                0,
                width as i32,
                height as i32,
                Some(screen_dc),
                x,
                y,
                SRCCOPY,
            );

            // Optionally draw cursor on top
            if self.show_cursor {
                Self::draw_cursor_if_needed(mem_dc, x, y);
            }

            // Copy DIB memory to Vec<u8>
            if !bits.is_null() {
                std::ptr::copy_nonoverlapping(bits as *const u8, data.as_mut_ptr(), data.len());
            }

            // Cleanup
            SelectObject(mem_dc, old);
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);
        }

        // Note: Window exclusion is not supported on Windows due to OS limitations
        // See docs/technical/WINDOWS_LIMITATIONS.md for details

        Some(CaptureFrame {
            data,
            width,
            height,
            stride: row_bytes as u32,
            offset_x: x,
            offset_y: y,
            gpu_texture: None,
        })
    }

    fn set_cursor_visible(&mut self, visible: bool) -> Result<()> {
        self.show_cursor = visible;
        Ok(())
    }

    fn get_region(&self) -> Option<CaptureRect> {
        self.capture_region.clone()
    }

    fn update_region(&mut self, region: CaptureRect) -> Result<()> {
        self.capture_region = Some(region);
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// Window exclusion is not supported on Windows due to OS API limitations.
// See docs/technical/WINDOWS_LIMITATIONS.md for detailed explanation.
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
    monitor_size: (u32, u32), // Monitor width and height
    is_active: bool,
    show_cursor: bool,
    current_cursor_state: bool, // Track actual cursor state in session
    gpu_acceleration: bool, // Enable GPU texture passthrough (zero-copy)
    excluded_windows: Vec<WindowIdentifier>,
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
            monitor_size: (0, 0),
            is_active: false,
            show_cursor: true,
            current_cursor_state: true,
            gpu_acceleration: false, // TEMPORARILY DISABLED - Different D3D devices cause crash
            excluded_windows: Vec::new(),
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
            let module =
                LoadLibraryW(PCWSTR(dll_name.as_ptr())).context("Failed to load d3d11.dll")?;

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
                return Err(anyhow!(
                    "CreateDirect3D11DeviceFromDXGIDevice failed: {:?}",
                    hr
                ));
            }

            if result_ptr.is_null() {
                return Err(anyhow!(
                    "CreateDirect3D11DeviceFromDXGIDevice returned null"
                ));
            }

            Ok(IDirect3DDevice::from_raw(result_ptr))
        }
    }

    /// Create a GraphicsCaptureItem for the monitor containing the given point
    /// Returns: (capture_item, monitor_origin, monitor_size)
    fn create_capture_item_for_monitor(
        point: (i32, i32),
    ) -> Result<(GraphicsCaptureItem, (i32, i32), (u32, u32))> {
        let pt = POINT {
            x: point.0,
            y: point.1,
        };
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

        let origin = (monitor_info.rcMonitor.left, monitor_info.rcMonitor.top);
        let size = (
            (monitor_info.rcMonitor.right - monitor_info.rcMonitor.left) as u32,
            (monitor_info.rcMonitor.bottom - monitor_info.rcMonitor.top) as u32,
        );

        Ok((item, origin, size))
    }

    /// Get frame as GPU texture (zero-copy) - preferred for performance
    /// Returns texture handle for GPU-accelerated rendering
    /// Falls back to CPU copy if clicks need to be drawn
    fn get_frame_gpu(
        &self,
        source_texture: &ID3D11Texture2D,
        region: &CaptureRect,
    ) -> Option<CaptureFrame> {
        // Get texture description for dimensions
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { source_texture.GetDesc(&mut desc) };

        // Calculate monitor bounds in screen coordinates
        let monitor_left = self.monitor_origin.0;
        let monitor_top = self.monitor_origin.1;
        let monitor_right = monitor_left + self.monitor_size.0 as i32;
        let monitor_bottom = monitor_top + self.monitor_size.1 as i32;

        // Clip region to monitor bounds
        let clipped_left = region.x.max(monitor_left);
        let clipped_top = region.y.max(monitor_top);
        let clipped_right = (region.x + region.width as i32).min(monitor_right);
        let clipped_bottom = (region.y + region.height as i32).min(monitor_bottom);

        // Check if there's any visible region
        if clipped_left >= clipped_right || clipped_top >= clipped_bottom {
            warn!("Capture region entirely outside monitor bounds");
            return None;
        }

        // Calculate clipped dimensions
        let clipped_width = (clipped_right - clipped_left) as u32;
        let clipped_height = (clipped_bottom - clipped_top) as u32;

        // Calculate source position in texture coordinates (relative to monitor origin)
        let src_x = (clipped_left - monitor_left) as i32;
        let src_y = (clipped_top - monitor_top) as i32;

        // Clone texture COM pointer for safe cross-thread usage
        // SAFETY: Clone increments reference count (AddRef)
        // We use ManuallyDrop to prevent automatic Release - destination window will Release it
        let texture_ptr = {
            use std::mem::ManuallyDrop;
            let cloned_texture = ManuallyDrop::new(source_texture.clone()); // AddRef
            cloned_texture.as_raw() as usize
        };

        debug!(
            "GPU frame: {}x{} at screen({}, {}), texture crop({}, {}), ptr: 0x{:X}",
            clipped_width, clipped_height, clipped_left, clipped_top, src_x, src_y, texture_ptr
        );

        Some(CaptureFrame {
            data: Vec::new(), // No CPU data for GPU path
            width: clipped_width,
            height: clipped_height,
            stride: clipped_width * 4, // BGRA format
            offset_x: clipped_left,
            offset_y: clipped_top,
            gpu_texture: Some(super::GpuTextureHandle::D3D11 {
                texture_ptr,
                shared_handle: 0, // Not needed for same-process usage
                crop_x: src_x,
                crop_y: src_y,
                crop_width: clipped_width,
                crop_height: clipped_height,
            }),
        })
    }

    /// Copy texture to CPU-accessible staging texture and read pixels
    /// Clips region to monitor bounds and returns only the visible portion
    /// Used as fallback when GPU rendering is not available or clicks need to be drawn
    fn copy_frame_to_cpu(
        &self,
        source_texture: &ID3D11Texture2D,
        region: &CaptureRect,
    ) -> Option<CaptureFrame> {
        let d3d_device = self.d3d_device.as_ref()?;
        let d3d_context = self.d3d_context.as_ref()?;

        // Calculate monitor bounds in screen coordinates
        let monitor_left = self.monitor_origin.0;
        let monitor_top = self.monitor_origin.1;
        let monitor_right = monitor_left + self.monitor_size.0 as i32;
        let monitor_bottom = monitor_top + self.monitor_size.1 as i32;

        // Clip region to monitor bounds
        let clipped_left = region.x.max(monitor_left);
        let clipped_top = region.y.max(monitor_top);
        let clipped_right = (region.x + region.width as i32).min(monitor_right);
        let clipped_bottom = (region.y + region.height as i32).min(monitor_bottom);

        // Check if there's any visible region
        if clipped_left >= clipped_right || clipped_top >= clipped_bottom {
            // Entire region is outside monitor - return empty/minimal frame
            warn!("Capture region entirely outside monitor bounds");
            return None;
        }

        // Calculate clipped dimensions
        let clipped_width = (clipped_right - clipped_left) as u32;
        let clipped_height = (clipped_bottom - clipped_top) as u32;

        // Log once per 60 frames
        static COPY_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let count = COPY_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if count % 60 == 0 {
            info!(
                "Copying frame to CPU: {}x{} (clipped from {}x{}, frame #{})",
                clipped_width, clipped_height, region.width, region.height, count
            );
        }

        // Create staging texture for CPU read (using clipped dimensions)
        let staging_desc = D3D11_TEXTURE2D_DESC {
            Width: clipped_width,
            Height: clipped_height,
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
            if d3d_device
                .CreateTexture2D(&staging_desc, None, Some(&mut staging_texture))
                .is_err()
            {
                warn!("Failed to create staging texture");
                return None;
            }
        }

        let staging_texture = staging_texture?;

        // Calculate source position in texture coordinates (relative to monitor origin)
        let src_x = (clipped_left - monitor_left) as u32;
        let src_y = (clipped_top - monitor_top) as u32;

        // Copy region from source to staging
        let src_box = D3D11_BOX {
            left: src_x,
            top: src_y,
            front: 0,
            right: src_x + clipped_width,
            bottom: src_y + clipped_height,
            back: 1,
        };

        unsafe {
            d3d_context.CopySubresourceRegion(
                &staging_texture,
                0,
                0,
                0,
                0,
                source_texture,
                0,
                Some(&src_box),
            );
        }

        // Map the staging texture and read pixels
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            if d3d_context
                .Map(&staging_texture, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .is_err()
            {
                warn!("Failed to map staging texture");
                return None;
            }
        }

        let stride = mapped.RowPitch as usize;
        let row_bytes = (clipped_width * 4) as usize; // 4 bytes per pixel (BGRA)

        // Copy row by row, removing stride padding
        let mut data = vec![0u8; row_bytes * clipped_height as usize];

        unsafe {
            let src_ptr = mapped.pData as *const u8;
            for row in 0..clipped_height as usize {
                let src_row = src_ptr.add(row * stride);
                let dst_row = data.as_mut_ptr().add(row * row_bytes);
                std::ptr::copy_nonoverlapping(src_row, dst_row, row_bytes);
            }
            d3d_context.Unmap(&staging_texture, 0);
        }

        // Note: Window exclusion is not supported on Windows due to OS limitations
        // See docs/technical/WINDOWS_LIMITATIONS.md for details

        Some(CaptureFrame {
            data,
            width: clipped_width,
            height: clipped_height,
            stride: row_bytes as u32,
            offset_x: clipped_left,
            offset_y: clipped_top,
            gpu_texture: None,
        })
    }
}

impl CaptureEngine for WindowsCaptureEngine {
    fn start(&mut self, region: CaptureRect, show_cursor: bool, excluded_windows: Option<Vec<WindowIdentifier>>) -> Result<()> {
        info!("Starting capture for region: {:?}", region);

        if let Some(windows) = &excluded_windows {
            info!("Received {} windows to exclude from capture", windows.len());
        }

        // Create D3D11 device
        let (d3d_device, d3d_context) = Self::create_d3d_device()?;
        info!("Created D3D11 device");

        // Create WinRT Direct3D device
        let direct3d_device = Self::create_direct3d_device(&d3d_device)?;
        info!("Created WinRT Direct3D device");

        // Create capture item for monitor
        let center_point = (
            region.x + (region.width as i32) / 2,
            region.y + (region.height as i32) / 2,
        );
        let (capture_item, monitor_origin, monitor_size) =
            Self::create_capture_item_for_monitor(center_point)?;
        info!(
            "Created capture item for monitor at origin {:?}, size {:?}",
            monitor_origin, monitor_size
        );

        // Store active excludes for potential usage
        if let Some(ref wins) = excluded_windows {
            self.excluded_windows = wins.clone();
        }

        // Get capture size
        let size = capture_item.Size()?;
        info!("Monitor size: {}x{}", size.Width, size.Height);

        // Create frame pool
        let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &direct3d_device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2, // Double buffering
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
        self.monitor_size = monitor_size;
        self.show_cursor = show_cursor;
        self.current_cursor_state = show_cursor;
        self.is_active = true;
        // excluded_windows already stored above, no need to set again

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

        // Dynamic Cursor Filtering Logic - DISABLED
        // With separation layer in place, preview window no longer causes infinite mirror.
        // Simply use the user's show_cursor setting without any filtering.
        if let Some(session) = &self.capture_session {
            // Apply cursor setting if it differs from current state
            if self.show_cursor != self.current_cursor_state {
                match session.SetIsCursorCaptureEnabled(self.show_cursor) {
                    Ok(_) => {
                        self.current_cursor_state = self.show_cursor;
                        log::debug!("Cursor capture setting applied: {}", self.show_cursor);
                    },
                    Err(e) => log::warn!("Failed to set cursor capture: {:?}", e),
                }
            }
        }

        let frame_pool = self.frame_pool.as_ref()?;
        let region = self.capture_region.as_ref()?;

        // Try to get frame from pool (non-blocking)
        let frame = match frame_pool.TryGetNextFrame() {
            Ok(f) => {
                //debug!("Got frame from pool!");
                f
            }
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

        // Choose GPU or CPU path based on gpu_acceleration setting
        // GPU path: Return texture handle for zero-copy rendering (fast)
        // CPU path: Copy to system memory for compatibility (slower)
        // If GPU path is enabled but we need to apply exclusions, fall back to CPU path
        if self.gpu_acceleration && self.excluded_windows.is_empty() {
            // Try GPU path first
            let result = self.get_frame_gpu(&texture, region);
            if result.is_some() {
                return result;
            }
            // Fallback to CPU if GPU path fails
            warn!("GPU path failed, falling back to CPU");
        }

        // CPU fallback path
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

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl WindowsCaptureEngine {
    /// Get the current monitor origin (for monitor change detection)
    pub fn get_monitor_origin(&self) -> (i32, i32) {
        self.monitor_origin
    }
}

// SAFETY: COM objects in WGC are thread-safe
unsafe impl Send for WindowsCaptureEngine {}
unsafe impl Sync for WindowsCaptureEngine {}
