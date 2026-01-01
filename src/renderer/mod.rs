// renderer/mod.rs - wgpu + egui Rendering Pipeline
//
// This module combines wgpu rendering with egui for the UI.
// It handles both the captured frame display and the UI overlay.

mod egui_renderer;

pub use egui_renderer::EguiRenderer;

use std::sync::Arc;
use anyhow::{anyhow, Context, Result};
use log::info;
use winit::window::Window;

/// Combined wgpu + egui renderer
pub struct Renderer {
    /// wgpu instance
    instance: wgpu::Instance,
    /// wgpu surface for the window
    surface: wgpu::Surface<'static>,
    /// wgpu device
    device: wgpu::Device,
    /// wgpu queue
    queue: wgpu::Queue,
    /// Surface configuration
    config: wgpu::SurfaceConfiguration,
    /// egui renderer
    egui_renderer: EguiRenderer,
    /// Window reference
    window: Arc<Window>,
    /// Whether this renderer uses transparent background
    transparent: bool,
    /// Frame buffer for reading pixels (for layered windows)
    frame_buffer: Option<wgpu::Buffer>,
}

impl Renderer {
    /// Create a new renderer for the given window
    pub fn new(window: Arc<Window>) -> Result<Self> {
        Self::new_with_options(window, false)
    }
    
    /// Create a new renderer with transparency option
    pub fn new_transparent(window: Arc<Window>) -> Result<Self> {
        Self::new_with_options(window, true)
    }
    
    /// Create a new renderer with options
    fn new_with_options(window: Arc<Window>, transparent: bool) -> Result<Self> {
        info!("Initializing wgpu + egui renderer");
        
        // Create wgpu instance
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        
        // Create surface
        let surface = instance
            .create_surface(window.clone())
            .context("Failed to create surface")?;
        
        // Get adapter
        let adapter_future = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        });
        let adapter = pollster::block_on(adapter_future)
            .context("Failed to find suitable GPU adapter")?;
        
        info!("Using adapter: {:?}", adapter.get_info());
        
        // Create device and queue
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("RustFrame Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            },
        )).context("Failed to create device")?;
        
        // Configure surface
        let size = window.inner_size();
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps.formats.iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);
        
        // Choose alpha mode based on transparency setting
        let alpha_mode = if transparent {
            // Prefer PreMultiplied for transparent windows
            if surface_caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
                wgpu::CompositeAlphaMode::PreMultiplied
            } else if surface_caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PostMultiplied) {
                wgpu::CompositeAlphaMode::PostMultiplied
            } else {
                surface_caps.alpha_modes[0]
            }
        } else {
            surface_caps.alpha_modes[0]
        };
        
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        
        // Create egui renderer
        let egui_renderer = EguiRenderer::new(&device, surface_format, &window);
        
        info!("Renderer initialized successfully (transparent={})", transparent);
        
        Ok(Self {
            instance,
            surface,
            device,
            queue,
            config,
            egui_renderer,
            window,
            transparent,
            frame_buffer: None,
        })
    }
    
    /// Handle window resize
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            
            // Recreate frame buffer if needed
            if self.transparent {
                self.create_frame_buffer();
            }
        }
    }
    
    /// Get the egui context for UI rendering
    pub fn egui_ctx(&self) -> &egui::Context {
        self.egui_renderer.context()
    }
    
    /// Handle a winit window event for egui
    pub fn handle_event(&mut self, event: &winit::event::WindowEvent) -> bool {
        self.egui_renderer.handle_event(&self.window, event)
    }
    
    /// Begin a new frame
    pub fn begin_frame(&mut self) {
        self.egui_renderer.begin_frame(&self.window);
    }
    
    /// End the frame and render
    pub fn end_frame(&mut self) -> Result<()> {
        // Get surface texture
        let output = self.surface.get_current_texture()
            .context("Failed to get surface texture")?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        
        // End egui frame and get paint jobs
        let full_output = self.egui_renderer.end_frame();
        let paint_jobs = self.egui_renderer.context().tessellate(
            full_output.shapes,
            full_output.pixels_per_point,
        );
        
        // Update textures
        for (id, delta) in &full_output.textures_delta.set {
            self.egui_renderer.update_texture(&self.device, &self.queue, *id, delta);
        }
        
        // Create encoder
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });
        
        // Update buffers
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point: self.window.scale_factor() as f32,
        };
        
        let _ = self.egui_renderer.prepare(
            &self.device,
            &self.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );
        
        // Render
        {
            // For border window using region API, we can use a normal background color
            // since the center is literally cut out (no pixels there at all)
            let clear_color = if self.transparent {
                wgpu::Color { r: 0.1, g: 0.1, b: 0.1, a: 1.0 } // Dark background for border
            } else {
                wgpu::Color::BLACK
            };
            
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            
            let mut render_pass = render_pass.forget_lifetime();
            self.egui_renderer.render(&mut render_pass, &paint_jobs, &screen_descriptor);
        }
        
        // Free textures
        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }
        
        // Submit
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        
        Ok(())
    }
    
    /// Get current surface size
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }
    
    /// Get the wgpu device
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }
    
    /// Get the wgpu queue  
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }
    
    /// Create frame buffer for reading pixels
    fn create_frame_buffer(&mut self) {
        if !self.transparent {
            return;
        }
        
        let buffer_size = (self.config.width * self.config.height * 4) as u64;
        
        self.frame_buffer = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Frame Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        }));
    }
    
    /// Read current frame buffer as RGBA bytes
    pub async fn read_frame_buffer(&mut self) -> Result<Vec<u8>> {
        // For simplicity, we'll skip the complex frame buffer reading for now
        // The magenta color key approach should work without needing this
        Err(anyhow!("Frame buffer reading not implemented yet"))
    }
}
