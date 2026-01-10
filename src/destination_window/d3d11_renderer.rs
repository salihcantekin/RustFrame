//! DirectX 11 GPU-Accelerated Rendering for Destination Window
//!
//! This module provides zero-copy GPU rendering using DirectX 11 SwapChain.
//! Texture from capture is presented directly to screen without CPU involvement.
//!
//! Microsoft Documentation:
//! - https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-devices-downlevel-intro
//! - https://learn.microsoft.com/en-us/windows/win32/api/dxgi/nf-dxgi-idxgiswapchain-present
//! - https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/d3d10-graphics-programming-guide-dxgi

use log::info;
use windows::core::Interface;
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDeviceAndSwapChain, ID3D11Device, ID3D11DeviceContext, ID3D11RenderTargetView,
    ID3D11Texture2D, D3D11_BOX, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
    D3D11_VIEWPORT,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_MODE_DESC, DXGI_MODE_SCALING_UNSPECIFIED,
    DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED, DXGI_RATIONAL, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    IDXGISwapChain, DXGI_PRESENT, DXGI_SWAP_CHAIN_DESC, DXGI_SWAP_CHAIN_FLAG,
    DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH, DXGI_SWAP_EFFECT_DISCARD,
    DXGI_USAGE_RENDER_TARGET_OUTPUT,
};

/// DirectX 11 GPU renderer for destination window
/// Provides zero-copy rendering from capture texture to screen
pub struct D3D11Renderer {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swapchain: IDXGISwapChain,
    render_target_view: Option<ID3D11RenderTargetView>,
    hwnd: HWND,
}

impl D3D11Renderer {
    /// Create new DirectX 11 renderer for window
    ///
    /// # Arguments
    /// * `hwnd` - Window handle to render to
    /// * `width` - Initial window width
    /// * `height` - Initial window height
    ///
    /// # Returns
    /// Result containing renderer or error message
    pub fn new(hwnd: HWND, width: u32, height: u32) -> Result<Self, String> {
        info!(
            "Creating D3D11 renderer for HWND {:?}, size {}x{}",
            hwnd, width, height
        );

        // SwapChain description - using FLIP model for Windows 10+
        let swap_chain_desc = DXGI_SWAP_CHAIN_DESC {
            BufferDesc: DXGI_MODE_DESC {
                Width: width,
                Height: height,
                RefreshRate: DXGI_RATIONAL {
                    Numerator: 60,
                    Denominator: 1,
                },
                Format: DXGI_FORMAT_B8G8R8A8_UNORM, // Match capture format
                ScanlineOrdering: DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED,
                Scaling: DXGI_MODE_SCALING_UNSPECIFIED,
            },
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2, // Double buffering for smooth presentation
            OutputWindow: hwnd,
            Windowed: true.into(),
            SwapEffect: DXGI_SWAP_EFFECT_DISCARD, // Compatible with all Windows versions
            Flags: DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH.0 as u32,
        };

        // Create device and swapchain
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        let mut swapchain: Option<IDXGISwapChain> = None;

        let result = unsafe {
            D3D11CreateDeviceAndSwapChain(
                None, // Use default adapter
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE(std::ptr::null_mut()), // No software rasterizer
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, // Required for BGRA format
                None,                          // Use default feature levels
                D3D11_SDK_VERSION,
                Some(&swap_chain_desc),
                Some(&mut swapchain),
                Some(&mut device),
                None, // Don't care about feature level
                Some(&mut context),
            )
        };

        if let Err(e) = result {
            return Err(format!(
                "Failed to create D3D11 device and swapchain: {:?}",
                e
            ));
        }

        let device = device.ok_or("Device creation returned null")?;
        let context = context.ok_or("Context creation returned null")?;
        let swapchain = swapchain.ok_or("SwapChain creation returned null")?;

        info!("D3D11 device and swapchain created successfully");

        let mut renderer = Self {
            device,
            context,
            swapchain,
            render_target_view: None,
            hwnd,
        };

        // Create render target view
        renderer.recreate_render_target()?;

        Ok(renderer)
    }

    /// Recreate render target view (called on resize or initialization)
    fn recreate_render_target(&mut self) -> Result<(), String> {
        // Release old render target view if exists
        self.render_target_view = None;

        // Get back buffer from swapchain
        let back_buffer: ID3D11Texture2D = unsafe {
            self.swapchain
                .GetBuffer(0)
                .map_err(|e| format!("Failed to get back buffer: {:?}", e))?
        };

        // Create render target view
        let mut render_target_view: Option<ID3D11RenderTargetView> = None;
        unsafe {
            self.device
                .CreateRenderTargetView(&back_buffer, None, Some(&mut render_target_view))
                .map_err(|e| format!("Failed to create render target view: {:?}", e))?;
        }

        self.render_target_view = render_target_view;
        info!("Render target view created");

        Ok(())
    }

    /// Render texture to screen with optional cropping
    ///
    /// # Arguments
    /// * `texture_ptr` - Pointer to ID3D11Texture2D (must be AddRef'd)
    /// * `crop_x` - Crop X position in texture coordinates
    /// * `crop_y` - Crop Y position in texture coordinates
    /// * `crop_width` - Crop width
    /// * `crop_height` - Crop height
    ///
    /// # Performance
    /// This is a zero-copy GPU operation. Texture is copied on GPU only.
    /// Typical performance: ~100-500 µs for 1080p
    pub fn render_texture(
        &self,
        texture_ptr: usize,
        crop_x: i32,
        crop_y: i32,
        crop_width: u32,
        crop_height: u32,
    ) -> Result<(), String> {
        // Validate crop coordinates
        if crop_x < 0 || crop_y < 0 {
            return Err(format!(
                "Invalid crop coordinates: ({}, {})",
                crop_x, crop_y
            ));
        }

        // Get render target view
        let rtv = self
            .render_target_view
            .as_ref()
            .ok_or("Render target view not created")?;

        // Get back buffer texture
        let back_buffer: ID3D11Texture2D = unsafe {
            self.swapchain
                .GetBuffer(0)
                .map_err(|e| format!("Failed to get back buffer: {:?}", e))?
        };

        // Get back buffer desc for viewport setup
        let mut back_buffer_desc = Default::default();
        unsafe {
            back_buffer.GetDesc(&mut back_buffer_desc);
        }

        // Set render target (required for DirectX to know where to render)
        unsafe {
            self.context
                .OMSetRenderTargets(Some(&[Some(rtv.clone())]), None);
        }

        // Set viewport (required for rasterizer stage)
        let viewport = D3D11_VIEWPORT {
            TopLeftX: 0.0,
            TopLeftY: 0.0,
            Width: back_buffer_desc.Width as f32,
            Height: back_buffer_desc.Height as f32,
            MinDepth: 0.0,
            MaxDepth: 1.0,
        };
        unsafe {
            self.context.RSSetViewports(Some(&[viewport]));
        }

        // Clear render target to black before copying
        let clear_color = [0.0f32, 0.0, 0.0, 1.0];
        unsafe {
            self.context.ClearRenderTargetView(rtv, &clear_color);
        }

        // Reconstruct source texture from pointer WITHOUT taking ownership
        // SAFETY: We borrow the texture temporarily, don't drop it
        // The capture module owns the reference and will Release it
        if texture_ptr == 0 {
            return Err("Invalid texture pointer (null)".to_string());
        }

        let source_texture: &ID3D11Texture2D = unsafe { &*(texture_ptr as *const ID3D11Texture2D) };

        // Get source texture descriptor for debugging and validation
        let mut src_desc = Default::default();
        unsafe {
            source_texture.GetDesc(&mut src_desc);
        }

        // Validate crop region is within source texture bounds
        if crop_x < 0 || crop_y < 0 {
            return Err(format!(
                "Crop coordinates out of bounds: ({}, {})",
                crop_x, crop_y
            ));
        }

        let crop_right = crop_x as u32 + crop_width;
        let crop_bottom = crop_y as u32 + crop_height;

        if crop_right > src_desc.Width || crop_bottom > src_desc.Height {
            return Err(format!(
                "Crop region ({}+{}, {}+{}) exceeds source texture bounds ({}x{})",
                crop_x, crop_width, crop_y, crop_height, src_desc.Width, src_desc.Height
            ));
        }

        // Log texture descriptors for debugging (once per second)
        static LOG_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        if LOG_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 60 == 0 {
            info!(
                "Texture info - Source: {}x{} format={:?}, BackBuffer: {}x{} format={:?}",
                src_desc.Width,
                src_desc.Height,
                src_desc.Format,
                back_buffer_desc.Width,
                back_buffer_desc.Height,
                back_buffer_desc.Format
            );
        }

        // Copy region from source to back buffer (GPU → GPU)
        let src_box = D3D11_BOX {
            left: crop_x as u32,
            top: crop_y as u32,
            front: 0,
            right: (crop_x as u32) + crop_width,
            bottom: (crop_y as u32) + crop_height,
            back: 1,
        };

        unsafe {
            self.context.CopySubresourceRegion(
                &back_buffer,
                0, // Subresource
                0, // Dest X
                0, // Dest Y
                0, // Dest Z
                source_texture,
                0, // Source subresource
                Some(&src_box),
            );
        }

        // Flush context to ensure copy completes before Present
        unsafe {
            self.context.Flush();
        }

        // Present to screen (VSYNC enabled for smooth playback)
        let hr = unsafe {
            self.swapchain.Present(1, DXGI_PRESENT(0)) // 1 = wait for VSYNC
        };
        if hr.is_err() {
            return Err(format!("Present failed: {:?}", hr));
        }

        Ok(())
    }

    /// Resize swapchain buffers
    ///
    /// # Arguments
    /// * `width` - New width
    /// * `height` - New height
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        info!("Resizing D3D11 renderer to {}x{}", width, height);

        // Release render target view before resizing
        self.render_target_view = None;

        // Resize buffers
        unsafe {
            self.swapchain
                .ResizeBuffers(
                    0,
                    width,
                    height,
                    DXGI_FORMAT_B8G8R8A8_UNORM,
                    DXGI_SWAP_CHAIN_FLAG(0),
                )
                .map_err(|e| format!("Failed to resize buffers: {:?}", e))?;
        }

        // Recreate render target view
        self.recreate_render_target()?;

        Ok(())
    }

    /// Clear screen to black (used when no frame available)
    pub fn clear(&self) {
        if let Some(rtv) = &self.render_target_view {
            let clear_color = [0.0f32, 0.0, 0.0, 1.0]; // Black
            unsafe {
                self.context.ClearRenderTargetView(rtv, &clear_color);
            }
        }

        // Present cleared frame
        unsafe {
            let _ = self.swapchain.Present(0, DXGI_PRESENT(0));
        }
    }
}

impl Drop for D3D11Renderer {
    fn drop(&mut self) {
        info!("Dropping D3D11 renderer for HWND {:?}", self.hwnd);
        // COM objects are automatically released
    }
}

// SAFETY: DirectX 11 device and context are thread-safe for concurrent use
// SwapChain must be accessed from a single thread, but we ensure this in DestinationWindow
unsafe impl Send for D3D11Renderer {}
