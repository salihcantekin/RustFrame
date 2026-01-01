// examples/egui_test.rs - Simple egui integration test
// 
// This example demonstrates egui integration with winit and wgpu.
// Run with: cargo run --example egui_test

use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

struct EguiTestApp {
    window: Option<Arc<Window>>,
    state: Option<egui_winit::State>,
    renderer: Option<egui_wgpu::Renderer>,
    device: Option<wgpu::Device>,
    queue: Option<wgpu::Queue>,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    
    // Test UI state
    slider_value: f32,
    checkbox_value: bool,
    text_input: String,
    selected_option: usize,
}

impl EguiTestApp {
    fn new() -> Self {
        Self {
            window: None,
            state: None,
            renderer: None,
            device: None,
            queue: None,
            surface: None,
            surface_config: None,
            
            slider_value: 50.0,
            checkbox_value: true,
            text_input: String::from("Hello, egui!"),
            selected_option: 0,
        }
    }
    
    fn init_wgpu(&mut self, window: Arc<Window>) {
        // Create wgpu instance
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        
        // Create surface
        let surface = instance.create_surface(window.clone()).expect("Failed to create surface");
        
        // Get adapter
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })).expect("Failed to get adapter");
        
        // Create device and queue
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("egui Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            },
        )).expect("Failed to create device");
        
        // Configure surface
        let size = window.inner_size();
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps.formats.iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);
            
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);
        
        // Initialize egui
        let ctx = egui::Context::default();
        let state = egui_winit::State::new(
            ctx.clone(),
            ctx.viewport_id(),
            &window,
            None,
            None,
            None,
        );
        
        // Create egui renderer
        let renderer = egui_wgpu::Renderer::new(
            &device,
            surface_format,
            None,
            1,
            false,
        );
        
        self.window = Some(window);
        self.state = Some(state);
        self.renderer = Some(renderer);
        self.device = Some(device);
        self.queue = Some(queue);
        self.surface = Some(surface);
        self.surface_config = Some(surface_config);
    }
    
    fn render(&mut self) {
        let Some(window) = &self.window else { return };
        let Some(state) = &mut self.state else { return };
        let Some(renderer) = &mut self.renderer else { return };
        let Some(device) = &self.device else { return };
        let Some(queue) = &self.queue else { return };
        let Some(surface) = &self.surface else { return };
        let Some(surface_config) = &self.surface_config else { return };
        
        // Get surface texture
        let output = match surface.get_current_texture() {
            Ok(output) => output,
            Err(_) => return,
        };
        
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        
        // Begin egui frame
        let raw_input = state.take_egui_input(window.as_ref());
        let ctx = state.egui_ctx().clone();
        let full_output = ctx.run(raw_input, |ctx| {
            // Build egui UI
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("🎨 egui Test Window");
                ui.separator();
                
                ui.horizontal(|ui| {
                    ui.label("This is a simple egui test");
                    if ui.button("Click me!").clicked() {
                        println!("Button clicked!");
                    }
                });
                
                ui.separator();
                
                ui.add(egui::Slider::new(&mut self.slider_value, 0.0..=100.0).text("Slider"));
                
                ui.checkbox(&mut self.checkbox_value, "Enable feature");
                
                ui.horizontal(|ui| {
                    ui.label("Text input:");
                    ui.text_edit_singleline(&mut self.text_input);
                });
                
                ui.separator();
                
                ui.horizontal(|ui| {
                    ui.label("Select an option:");
                    egui::ComboBox::from_label("")
                        .selected_text(format!("Option {}", self.selected_option + 1))
                        .show_ui(ui, |ui| {
                            for i in 0..5 {
                                ui.selectable_value(&mut self.selected_option, i, format!("Option {}", i + 1));
                            }
                        });
                });
                
                ui.separator();
                
                ui.collapsing("Advanced Settings", |ui| {
                    ui.label("This section can be collapsed");
                    ui.label("More content here...");
                });
                
                ui.separator();
                
                ui.label(format!("Slider value: {:.1}", self.slider_value));
                ui.label(format!("Checkbox: {}", self.checkbox_value));
                ui.label(format!("Text: {}", self.text_input));
            });
        });
        
        // Handle egui output
        state.handle_platform_output(window.as_ref(), full_output.platform_output);
        
        // Prepare egui primitives
        let paint_jobs = ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        
        // Update textures
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [surface_config.width, surface_config.height],
            pixels_per_point: window.scale_factor() as f32,
        };
        
        for (id, image_delta) in &full_output.textures_delta.set {
            renderer.update_texture(device, queue, *id, image_delta);
        }
        
        // Create encoder
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("egui Encoder"),
        });
        
        // Render egui directly - collect all in one pass
        {
            // Update buffers first (this prepares the vertex/index data)
            let _ = renderer.update_buffers(
                device,
                queue,
                &mut encoder,
                &paint_jobs,
                &screen_descriptor,
            );
        }
        
        // Now render with a new borrow scope
        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.1,
                            b: 0.15,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            
            // Convert to 'static lifetime as required by egui-wgpu
            let mut render_pass = render_pass.forget_lifetime();
            renderer.render(&mut render_pass, &paint_jobs, &screen_descriptor);
        }
        
        // Free textures
        for id in &full_output.textures_delta.free {
            renderer.free_texture(id);
        }
        
        // Submit
        queue.submit(std::iter::once(encoder.finish()));
        output.present();
        
        // Always request redraw for continuous rendering
        window.request_redraw();
    }
}

impl ApplicationHandler for EguiTestApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        
        let window_attributes = Window::default_attributes()
            .with_title("egui Integration Test")
            .with_inner_size(PhysicalSize::new(600, 400))
            .with_resizable(true);
            
        let window = Arc::new(
            event_loop.create_window(window_attributes).expect("Failed to create window")
        );
        
        self.init_wgpu(window);
    }
    
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        // Let egui handle the event
        if let Some(state) = &mut self.state {
            if let Some(window) = &self.window {
                let response = state.on_window_event(window.as_ref(), &event);
                if response.consumed {
                    if response.repaint {
                        window.request_redraw();
                    }
                    return;
                }
            }
        }
        
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let (Some(surface), Some(device), Some(config)) = 
                    (&self.surface, &self.device, &mut self.surface_config) {
                    config.width = size.width.max(1);
                    config.height = size.height.max(1);
                    surface.configure(device, config);
                }
            }
            WindowEvent::RedrawRequested => {
                self.render();
            }
            _ => {}
        }
    }
}

fn main() {
    // Initialize logging
    env_logger::init();
    
    println!("Starting egui test...");
    println!("This window demonstrates egui UI components:");
    println!("- Buttons, sliders, checkboxes");
    println!("- Text input, combo boxes");
    println!("- Collapsible sections");
    println!();
    
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    
    let mut app = EguiTestApp::new();
    event_loop.run_app(&mut app).expect("Event loop error");
}
