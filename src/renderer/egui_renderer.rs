// renderer/egui_renderer.rs - egui Integration with wgpu
//
// Wraps egui_wgpu and egui_winit for easy integration.

use egui::Context;
use egui_wgpu::ScreenDescriptor;
use winit::window::Window;

/// Wrapper around egui rendering components
pub struct EguiRenderer {
    /// egui context
    ctx: Context,
    /// egui-winit state
    state: egui_winit::State,
    /// egui-wgpu renderer
    renderer: egui_wgpu::Renderer,
}

impl EguiRenderer {
    /// Create a new egui renderer
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, window: &Window) -> Self {
        let ctx = Context::default();
        
        let state = egui_winit::State::new(
            ctx.clone(),
            ctx.viewport_id(),
            window,
            None,
            None,
            None,
        );
        
        let renderer = egui_wgpu::Renderer::new(
            device,
            format,
            None,
            1,
            false,
        );
        
        Self {
            ctx,
            state,
            renderer,
        }
    }
    
    /// Get the egui context
    pub fn context(&self) -> &Context {
        &self.ctx
    }
    
    /// Handle a winit window event
    /// Returns true if egui consumed the event
    pub fn handle_event(&mut self, window: &Window, event: &winit::event::WindowEvent) -> bool {
        self.state.on_window_event(window, event).consumed
    }
    
    /// Begin a new egui frame
    pub fn begin_frame(&mut self, window: &Window) {
        let raw_input = self.state.take_egui_input(window);
        self.ctx.begin_pass(raw_input);
    }
    
    /// End the egui frame and return the output
    pub fn end_frame(&mut self) -> egui::FullOutput {
        self.ctx.end_pass()
    }
    
    /// Update an egui texture
    pub fn update_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: egui::TextureId,
        delta: &egui::epaint::ImageDelta,
    ) {
        self.renderer.update_texture(device, queue, id, delta);
    }
    
    /// Prepare rendering (update buffers)
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        paint_jobs: &[egui::ClippedPrimitive],
        screen_descriptor: &ScreenDescriptor,
    ) -> Vec<wgpu::CommandBuffer> {
        self.renderer.update_buffers(device, queue, encoder, paint_jobs, screen_descriptor)
    }
    
    /// Render egui
    pub fn render<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'static>,
        paint_jobs: &[egui::ClippedPrimitive],
        screen_descriptor: &ScreenDescriptor,
    ) {
        self.renderer.render(render_pass, paint_jobs, screen_descriptor);
    }
    
    /// Free an egui texture
    pub fn free_texture(&mut self, id: &egui::TextureId) {
        self.renderer.free_texture(id);
    }
    
    /// Handle platform output (clipboard, cursor, etc.)
    pub fn handle_platform_output(&mut self, window: &Window, output: egui::PlatformOutput) {
        self.state.handle_platform_output(window, output);
    }
}
