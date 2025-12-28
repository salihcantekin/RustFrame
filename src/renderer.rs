// renderer.rs - wgpu Rendering Pipeline for Captured Frames
//
// This module handles rendering captured D3D11 textures to the destination window
// using wgpu (a modern, cross-platform graphics API built on top of DirectX/Vulkan/Metal)
//
// RENDERING PIPELINE:
// 1. Get a D3D11 texture from the capture engine
// 2. Import it into wgpu (texture sharing between D3D11 and wgpu)
// 3. Render it to the destination window's swapchain
// 4. Handle cropping (only show the selected region)
//
// WHY wgpu?
// - Modern, safe Rust API
// - Cross-platform (could work on Linux/macOS with different capture backends)
// - Efficient GPU rendering
// - Easy integration with winit

use anyhow::{Result, Context, anyhow};
use log::{info, warn};
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::window::Window;
use windows::Win32::Graphics::Direct3D11::*;

use crate::capture::{CaptureEngine, CaptureRect};

/// The renderer that displays captured frames in the destination window
pub struct Renderer {
    /// The wgpu surface (represents the window's drawable area)
    surface: wgpu::Surface<'static>,

    /// The GPU device (wgpu's abstraction over D3D11/D3D12/Vulkan)
    device: wgpu::Device,

    /// Command queue for submitting GPU commands
    queue: wgpu::Queue,

    /// Surface configuration (format, size, etc.)
    config: wgpu::SurfaceConfiguration,

    /// Render pipeline (vertex/fragment shaders and state)
    render_pipeline: wgpu::RenderPipeline,

    /// Bind group layout (describes texture bindings)
    bind_group_layout: wgpu::BindGroupLayout,

    /// Sampler for texture sampling
    sampler: wgpu::Sampler,

    /// Vertex buffer (two triangles forming a quad)
    vertex_buffer: wgpu::Buffer,

    /// Current window size
    window_size: (u32, u32),
    
    /// Frame counter for debugging
    frame_count: u32,
}

impl Renderer {
    /// Create a new renderer for the destination window
    pub fn new(window: &Arc<Window>) -> Result<Self> {
        info!("Initializing wgpu renderer");

        // STEP 1: Create wgpu instance
        // This is the entry point to wgpu, similar to creating a D3D11 device
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12, // Use DirectX 12 on Windows
            ..Default::default()
        });
        info!("wgpu instance created (using DX12 backend)");

        // STEP 2: Create surface
        // The surface represents the window we're rendering to
        // SAFETY: We're passing a valid window handle from winit
        let surface = instance.create_surface(window.clone())
            .context("Failed to create surface")?;
        info!("Surface created");

        // STEP 3: Request adapter
        // The adapter represents a physical GPU
        let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })) {
            Ok(adapter) => adapter,
            Err(e) => return Err(anyhow!("Failed to find suitable GPU adapter: {:?}", e)),
        };

        info!("Adapter acquired: {:?}", adapter.get_info());

        // STEP 4: Request device and queue
        // The device is our interface to the GPU, the queue submits commands
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("RustFrame Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                experimental_features: Default::default(),
                trace: Default::default(),
            },
        )).context("Failed to create device and queue")?;

        info!("Device and queue created");

        // STEP 5: Configure surface
        let window_size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface.get_capabilities(&adapter).formats[0], // Use native format
            width: window_size.width,
            height: window_size.height,
            present_mode: wgpu::PresentMode::Fifo, // VSync (or use Mailbox for lower latency)
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        info!("Surface configured: {}x{}", config.width, config.height);

        // STEP 6: Create shader module
        // This is a simple passthrough shader that renders a textured quad
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("RustFrame Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        info!("Shader module created");

        // STEP 7: Create bind group layout
        // This describes what resources (textures, samplers) the shader needs
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Texture Bind Group Layout"),
            entries: &[
                // Texture binding
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Sampler binding
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // STEP 8: Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: Default::default(),
        });

        // STEP 9: Create render pipeline
        // This combines shaders, vertex layout, and render state
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });
        info!("Render pipeline created");

        // STEP 10: Create sampler
        // This controls how textures are sampled (linear filtering, clamping, etc.)
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Texture Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear, // Smooth scaling
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        // STEP 11: Create vertex buffer
        // Two triangles forming a full-screen quad
        let vertices = QUAD_VERTICES;
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        info!("Vertex buffer created");

        Ok(Self {
            surface,
            device,
            queue,
            config,
            render_pipeline,
            bind_group_layout,
            sampler,
            vertex_buffer,
            window_size: (window_size.width, window_size.height),
            frame_count: 0,
        })
    }

    /// Resize the renderer (called when window is resized)
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            info!("Resizing renderer to {}x{}", width, height);
            self.window_size = (width, height);
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// Render a frame from the capture engine
    pub fn render(&mut self, capture: &mut CaptureEngine) -> Result<()> {
        // STEP 1: Get the latest captured frame surface from WGC
        let frame_surface = match capture.get_latest_frame_surface() {
            Some(surf) => surf,
            None => {
                // No new frame available - don't clear to black!
                // Just skip this render cycle and keep the previous frame displayed
                // This prevents the rapid about_to_wait loop from overwriting good frames
                return Ok(());
            }
        };

        // STEP 2: Convert the WinRT IDirect3DSurface to COM ID3D11Texture2D
        // Use DXGI as the bridge between WinRT and COM interfaces
        let d3d11_texture: ID3D11Texture2D = match self.cast_surface_to_texture(&frame_surface) {
            Ok(tex) => tex,
            Err(e) => {
                warn!("Failed to cast surface to D3D11 texture: {:?}. Rendering clear color.", e);
                return self.render_clear();
            }
        };

        // STEP 3: Get the current surface texture (what we're rendering to)
        let output = self.surface.get_current_texture()
            .context("Failed to get surface texture")?;

        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        // STEP 4: Copy D3D11 texture to wgpu texture
        // This uses CPU-side copying via staging texture
        let (_texture, texture_view) = self.copy_d3d11_texture_to_wgpu(
            &d3d11_texture,
            capture.get_d3d_device(),
            capture.get_d3d_context(),
            capture.get_capture_region(),
            capture.get_monitor_origin(),
        )?;

        // STEP 5: Create bind group for this frame
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Texture Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        // STEP 6: Create command encoder and render pass
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Draw the quad with the captured texture
            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.draw(0..6, 0..1); // 6 vertices (2 triangles)
        }

        // STEP 7: Submit commands and present
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        // Log every 60 frames to confirm rendering is working
        self.frame_count += 1;
        if self.frame_count % 60 == 0 {
            info!("Rendered frame #{}", self.frame_count);
        }

        Ok(())
    }

    /// Cast WinRT IDirect3DSurface to COM ID3D11Texture2D using DXGI as bridge
    /// This properly handles the WinRT↔COM interface conversion
    fn cast_surface_to_texture(&self, surface: &windows::Graphics::DirectX::Direct3D11::IDirect3DSurface) -> Result<ID3D11Texture2D> {
        use windows::core::Interface;
        use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;
        
        // The correct way to get the underlying DXGI/D3D11 interface from a WinRT IDirect3DSurface
        // is through IDirect3DDxgiInterfaceAccess::GetInterface()
        unsafe {
            // Cast the WinRT surface to the interop interface
            let interop: IDirect3DDxgiInterfaceAccess = surface.cast()
                .context("Failed to cast IDirect3DSurface to IDirect3DDxgiInterfaceAccess")?;
            
            // Get the underlying D3D11 texture
            let texture: ID3D11Texture2D = interop.GetInterface()
                .context("Failed to get ID3D11Texture2D from IDirect3DDxgiInterfaceAccess")?;
            
            Ok(texture)
        }
    }

    /// Render a clear frame (black screen)
    fn render_clear(&mut self) -> Result<()> {
        let output = self.surface.get_current_texture()
            .context("Failed to get surface texture")?;

        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Clear Encoder"),
        });

        {
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    /// Copy a D3D11 texture to a wgpu texture
    ///
    /// This is the bridge between Windows.Graphics.Capture (D3D11) and wgpu (DX12/Vulkan).
    /// The process:
    /// 1. Create a staging texture in D3D11 (CPU-readable)
    /// 2. Copy the captured texture to the staging texture
    /// 3. Map the staging texture and read pixel data to CPU
    /// 4. Create a wgpu texture and upload the pixel data
    ///
    /// WHY: wgpu and D3D11 don't share memory directly without using HAL (Hardware Abstraction Layer)
    /// This is the simplest approach but involves a CPU roundtrip.
    ///
    /// PERFORMANCE: This is not ideal for real-time capture (adds latency and CPU overhead)
    /// For production, you'd want to use:
    /// - Direct3D12 interop with wgpu's DX12 backend
    /// - wgpu HAL for zero-copy texture sharing
    fn copy_d3d11_texture_to_wgpu(
        &self,
        d3d11_texture: &ID3D11Texture2D,
        d3d_device: &ID3D11Device,
        d3d_context: &ID3D11DeviceContext,
        crop_region: CaptureRect,
        monitor_origin: (i32, i32),
    ) -> Result<(wgpu::Texture, wgpu::TextureView)> {
        // STEP 1: Get the texture description
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe {
            d3d11_texture.GetDesc(&mut desc);
        }

        // STEP 2: Create a staging texture (CPU-readable)
        // This is necessary because the captured texture is on the GPU
        let staging_desc = D3D11_TEXTURE2D_DESC {
            Width: desc.Width,
            Height: desc.Height,
            MipLevels: 1,
            ArraySize: 1,
            Format: desc.Format, // Keep same format (should be BGRA8)
            SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING, // Staging = CPU-readable
            BindFlags: D3D11_BIND_FLAG(0).0 as u32,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32, // Allow CPU reads
            MiscFlags: D3D11_RESOURCE_MISC_FLAG(0).0 as u32,
        };

        let mut staging_texture: Option<ID3D11Texture2D> = None;
        unsafe {
            d3d_device
                .CreateTexture2D(&staging_desc, None, Some(&mut staging_texture))
                .context("Failed to create staging texture")?;
        }

        let staging_texture = staging_texture
            .ok_or_else(|| anyhow!("Staging texture creation returned null"))?;

        // STEP 3: Copy from captured texture to staging texture (GPU -> GPU)
        unsafe {
            d3d_context.CopyResource(&staging_texture, d3d11_texture);
        }

        // STEP 4: Map the staging texture (GPU -> CPU)
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            d3d_context
                .Map(
                    &staging_texture,
                    0, // Subresource index
                    D3D11_MAP_READ, // Read-only access
                    0, // No flags
                    Some(&mut mapped),
                )
                .context("Failed to map staging texture")?;
        }

        // STEP 5: Read pixel data from CPU memory
        let row_pitch = mapped.RowPitch as usize;

        // Compute crop window relative to captured surface (monitor origin aware)
        let origin_x = (crop_region.x - monitor_origin.0).max(0) as usize;
        let origin_y = (crop_region.y - monitor_origin.1).max(0) as usize;

        let crop_width = crop_region
            .width
            .min(desc.Width.saturating_sub(origin_x as u32)) as usize;
        let crop_height = crop_region
            .height
            .min(desc.Height.saturating_sub(origin_y as u32)) as usize;

        // Log crop calculation for debugging (only first frame)
        if self.frame_count == 0 {
            info!("Crop calculation: monitor_origin=({},{}), crop_region=({},{} {}x{})",
                monitor_origin.0, monitor_origin.1,
                crop_region.x, crop_region.y, crop_region.width, crop_region.height);
            info!("Computed: origin=({},{}) size={}x{}, texture_size={}x{}",
                origin_x, origin_y, crop_width, crop_height, desc.Width, desc.Height);
        }

        if crop_width == 0 || crop_height == 0 {
            unsafe { d3d_context.Unmap(&staging_texture, 0); }
            return Err(anyhow!("Computed zero-sized crop region; check overlay position"));
        }

        // Allocate buffer for pixel data
        // Assuming BGRA8 format (4 bytes per pixel)
        let mut pixel_data = vec![0u8; crop_width * crop_height * 4];

        unsafe {
            let src_ptr = mapped.pData as *const u8;

            // Copy only the cropped region row by row (texture rows may have padding)
            for y in 0..crop_height {
                let src_offset = (origin_y + y) * row_pitch + origin_x * 4;
                let dst_offset = y * crop_width * 4;

                std::ptr::copy_nonoverlapping(
                    src_ptr.add(src_offset),
                    pixel_data.as_mut_ptr().add(dst_offset),
                    crop_width * 4,
                );
            }

            // STEP 6: Unmap the staging texture
            d3d_context.Unmap(&staging_texture, 0);
        }

        // STEP 7: Create wgpu texture and upload data
        // Use Bgra8UnormSrgb to match the surface format and get correct colors
        // The captured data is already in sRGB color space from the desktop
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Captured Frame Texture"),
            size: wgpu::Extent3d {
                width: crop_width as u32,
                height: crop_height as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8UnormSrgb, // sRGB to match surface format
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Upload pixel data to GPU
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixel_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(crop_width as u32 * 4),
                rows_per_image: Some(crop_height as u32),
            },
            wgpu::Extent3d {
                width: crop_width as u32,
                height: crop_height as u32,
                depth_or_array_layers: 1,
            },
        );

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Ok((texture, texture_view))
    }
}

// Vertex structure for our full-screen quad
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2], // 2D position (x, y)
    tex_coords: [f32; 2], // Texture coordinates (u, v)
}

impl Vertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // Position
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // Texture coordinates
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

// Full-screen quad vertices (two triangles)
// Coordinates are in NDC (Normalized Device Coordinates): -1 to 1
const QUAD_VERTICES: &[Vertex] = &[
    // First triangle
    Vertex {
        position: [-1.0, -1.0], // Bottom-left
        tex_coords: [0.0, 1.0],
    },
    Vertex {
        position: [1.0, -1.0], // Bottom-right
        tex_coords: [1.0, 1.0],
    },
    Vertex {
        position: [1.0, 1.0], // Top-right
        tex_coords: [1.0, 0.0],
    },
    // Second triangle
    Vertex {
        position: [-1.0, -1.0], // Bottom-left
        tex_coords: [0.0, 1.0],
    },
    Vertex {
        position: [1.0, 1.0], // Top-right
        tex_coords: [1.0, 0.0],
    },
    Vertex {
        position: [-1.0, 1.0], // Top-left
        tex_coords: [0.0, 0.0],
    },
];
