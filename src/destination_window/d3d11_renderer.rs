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
use std::ffi::CString;
use std::ptr;
use std::slice;
use windows::core::{Interface, PCSTR};
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDeviceAndSwapChain, ID3D11BlendState, ID3D11Buffer, ID3D11Device,
    ID3D11DeviceContext, ID3D11InputLayout, ID3D11PixelShader, ID3D11RenderTargetView,
    ID3D11SamplerState, ID3D11Texture2D, ID3D11VertexShader, D3D11_BIND_CONSTANT_BUFFER,
    D3D11_BIND_INDEX_BUFFER, D3D11_BIND_VERTEX_BUFFER, D3D11_BLEND_DESC, D3D11_BLEND_INV_SRC_ALPHA,
    D3D11_BLEND_OP_ADD, D3D11_BLEND_SRC_ALPHA, D3D11_BLEND_ZERO, D3D11_BOX,
    D3D11_BUFFER_DESC, D3D11_COLOR_WRITE_ENABLE_ALL, D3D11_CPU_ACCESS_WRITE,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_FILTER_MIN_MAG_MIP_LINEAR, D3D11_INPUT_ELEMENT_DESC,
    D3D11_INPUT_PER_VERTEX_DATA, D3D11_MAP_WRITE_DISCARD,
    D3D11_RENDER_TARGET_BLEND_DESC, D3D11_SAMPLER_DESC, D3D11_SDK_VERSION, D3D11_SUBRESOURCE_DATA,
    D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_USAGE_DYNAMIC, D3D11_USAGE_DEFAULT, D3D11_VIEWPORT,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R32G32B32_FLOAT, DXGI_FORMAT_R32G32_FLOAT,
    DXGI_MODE_DESC, DXGI_MODE_SCALING_UNSPECIFIED, DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED,
    DXGI_RATIONAL, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    IDXGISwapChain, DXGI_PRESENT, DXGI_SWAP_CHAIN_DESC, DXGI_SWAP_CHAIN_FLAG,
    DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH, DXGI_SWAP_EFFECT_DISCARD,
    DXGI_USAGE_RENDER_TARGET_OUTPUT,
};
use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;

// ============================================================================
// Shader Code (HLSL)
// ============================================================================

const SHADER_CODE: &str = r#"
cbuffer ConstantBuffer : register(b0)
{
    float2 clickPos;
    float clickRadius;
    float clickAlpha;
    float4 clickColor;
    float2 offset;     // Texture crop offset
    float2 scale;      // Texture crop scale
};

struct VS_INPUT
{
    float3 Pos : POSITION;
    float2 Tex : TEXCOORD;
};

struct PS_INPUT
{
    float4 Pos : SV_POSITION;
    float2 Tex : TEXCOORD;
    float2 WorldPos : TEXCOORD1; // Screen coordinates for distance check
};

PS_INPUT VS(VS_INPUT input)
{
    PS_INPUT output;
    output.Pos = float4(input.Pos, 1.0);
    // Apply crop (scale and offset) to texture coordinates
    output.Tex = input.Tex * scale + offset;
    
    // Map -1..1 to 0..Width/Height is tricky without screen dim
    // But we can just use pixel shader for screen-space logic if we had SV_Position better
    // Actually, input.Pos is -1..1. Let's pass it through.
    return output;
}

Texture2D txDiffuse : register(t0);
SamplerState samLinear : register(s0);

float4 PS(PS_INPUT input) : SV_Target
{
    float4 pixelColor = txDiffuse.Sample(samLinear, input.Tex);
    
    // Simple circle drawing logic using SV_Position (Screen Pixel Coords)
    // input.Pos.xy is already in pixel coordinates (e.g. 500.5, 300.5) thanks to SV_POSITION
    
    if (clickAlpha > 0.0) {
        float dist = distance(input.Pos.xy, clickPos);
        
        // Anti-aliased circle edge
        float edge = fwidth(dist);
        float alpha = 1.0 - smoothstep(clickRadius - edge, clickRadius, dist);
        
        // Inner fade (optional ring effect)
        float innerRadius = clickRadius * 0.4;
        if (dist > innerRadius) {
            float fade = 1.0 - (dist - innerRadius) / (clickRadius - innerRadius);
             alpha *= fade;
        } else {
             // solid center
        }

        float4 finalClickColor = clickColor;
        finalClickColor.a *= clickAlpha * alpha;
        
        // Alpha blend manually (or let BlendState handle it if we output click color separately? 
        // No, we are drawing the whole texture here, so we must mix.)
        
        // Standard alpha blending: Src * SrcA + Dst * (1-SrcA)
        // Here "Dst" is the video pixel, "Src" is the click color
        
        return pixelColor * (1.0 - finalClickColor.a) + finalClickColor * finalClickColor.a;
        // Or cleaner: lerp(pixelColor, float4(finalClickColor.rgb, 1.0), finalClickColor.a);
    }
    
    return pixelColor;
}
"#;

// ============================================================================
// Structs
// ============================================================================

#[repr(C)]
struct Vertex {
    pos: [f32; 3],
    tex: [f32; 2],
}

#[repr(C)]
struct ConstantBufferData {
    click_pos: [f32; 2], // 8 bytes
    click_radius: f32,   // 4 bytes
    click_alpha: f32,    // 4 bytes
    click_color: [f32; 4], // 16 bytes
    offset: [f32; 2],    // 8 bytes (Crop X/Y)
    scale: [f32; 2],     // 8 bytes (Crop W/H)
    // Total: 8+4+4+16+8+8 = 48 bytes (Must be multiple of 16) -> Perfectly 48 (which is 3 * 16)
}

/// DirectX 11 GPU renderer for destination window
/// Provides zero-copy rendering from capture texture to screen with SHADERS
pub struct D3D11Renderer {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swapchain: IDXGISwapChain,
    render_target_view: Option<ID3D11RenderTargetView>,
    
    // Shader pipeline objects
    vertex_shader: Option<ID3D11VertexShader>,
    pixel_shader: Option<ID3D11PixelShader>,
    input_layout: Option<ID3D11InputLayout>,
    vertex_buffer: Option<ID3D11Buffer>,
    index_buffer: Option<ID3D11Buffer>,
    constant_buffer: Option<ID3D11Buffer>,
    sampler_state: Option<ID3D11SamplerState>,
    blend_state: Option<ID3D11BlendState>,
    
    hwnd: HWND,
}

unsafe impl Send for D3D11Renderer {}

impl D3D11Renderer {
    /// Create new DirectX 11 renderer for window
    pub fn new(hwnd: HWND, width: u32, height: u32) -> Result<Self, String> {
        info!(
            "Creating D3D11 renderer for HWND {:?}, size {}x{}",
            hwnd, width, height
        );

        // SwapChain description
        let swap_chain_desc = DXGI_SWAP_CHAIN_DESC {
            BufferDesc: DXGI_MODE_DESC {
                Width: width,
                Height: height,
                RefreshRate: DXGI_RATIONAL {
                    Numerator: 60,
                    Denominator: 1,
                },
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                ScanlineOrdering: DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED,
                Scaling: DXGI_MODE_SCALING_UNSPECIFIED,
            },
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            OutputWindow: hwnd,
            Windowed: true.into(),
            SwapEffect: DXGI_SWAP_EFFECT_DISCARD,
            Flags: DXGI_SWAP_CHAIN_FLAG_ALLOW_MODE_SWITCH.0 as u32,
        };

        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        let mut swapchain: Option<IDXGISwapChain> = None;

        unsafe {
            D3D11CreateDeviceAndSwapChain(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE(ptr::null_mut()),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&swap_chain_desc),
                Some(&mut swapchain),
                Some(&mut device),
                None,
                Some(&mut context),
            ).map_err(|e| format!("D3D11CreateDeviceAndSwapChain failed: {}", e))?;
        }

        let device = device.unwrap();
        let context = context.unwrap();
        let swapchain = swapchain.unwrap();

        let mut renderer = Self {
            device,
            context,
            swapchain,
            render_target_view: None,
            vertex_shader: None,
            pixel_shader: None,
            input_layout: None,
            vertex_buffer: None,
            index_buffer: None,
            constant_buffer: None,
            sampler_state: None,
            blend_state: None,
            hwnd,
        };

        renderer.recreate_render_target()?;
        renderer.init_pipeline()?;

        info!("D3D11 renderer initialized with Shader Pipeline");
        Ok(renderer)
    }

    fn init_pipeline(&mut self) -> Result<(), String> {
        let device = &self.device;

        unsafe {
            // 1. Compile Shaders
            let vs_blob = compile_shader(SHADER_CODE, "VS", "vs_4_0")?;
            let ps_blob = compile_shader(SHADER_CODE, "PS", "ps_4_0")?;

            // 2. Create Shaders
            let mut vs = None;
            device.CreateVertexShader(
                slice::from_raw_parts(vs_blob.GetBufferPointer() as *const u8, vs_blob.GetBufferSize()),
                None,
                Some(&mut vs)
            ).map_err(|e| format!("CreateVertexShader failed: {}", e))?;
            self.vertex_shader = vs;

            let mut ps = None;
            device.CreatePixelShader(
                slice::from_raw_parts(ps_blob.GetBufferPointer() as *const u8, ps_blob.GetBufferSize()),
                None,
                Some(&mut ps)
            ).map_err(|e| format!("CreatePixelShader failed: {}", e))?;
            self.pixel_shader = ps;

            // 3. Create Input Layout
            let input_elements = [
                D3D11_INPUT_ELEMENT_DESC {
                    SemanticName: PCSTR(b"POSITION\0".as_ptr()),
                    SemanticIndex: 0,
                    Format: DXGI_FORMAT_R32G32B32_FLOAT,
                    InputSlot: 0,
                    AlignedByteOffset: 0,
                    InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                    InstanceDataStepRate: 0,
                },
                D3D11_INPUT_ELEMENT_DESC {
                    SemanticName: PCSTR(b"TEXCOORD\0".as_ptr()),
                    SemanticIndex: 0,
                    Format: DXGI_FORMAT_R32G32_FLOAT, // u, v
                    InputSlot: 0,
                    AlignedByteOffset: 12, // after 3 floats
                    InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                    InstanceDataStepRate: 0,
                },
            ];

            let mut layout = None;
            device.CreateInputLayout(
                &input_elements,
                slice::from_raw_parts(vs_blob.GetBufferPointer() as *const u8, vs_blob.GetBufferSize()),
                Some(&mut layout)
            ).map_err(|e| format!("CreateInputLayout failed: {}", e))?;
            self.input_layout = layout;

            // 4. Create Vertex Buffer (Full Screen Quad)
            let vertices = [
                Vertex { pos: [-1.0,  1.0, 0.0], tex: [0.0, 0.0] }, // Top-Left
                Vertex { pos: [ 1.0,  1.0, 0.0], tex: [1.0, 0.0] }, // Top-Right
                Vertex { pos: [-1.0, -1.0, 0.0], tex: [0.0, 1.0] }, // Bottom-Left
                Vertex { pos: [ 1.0, -1.0, 0.0], tex: [1.0, 1.0] }, // Bottom-Right
            ];

            let vb_desc = D3D11_BUFFER_DESC {
                ByteWidth: (std::mem::size_of::<Vertex>() * vertices.len()) as u32,
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
                CPUAccessFlags: 0,
                MiscFlags: 0,
                StructureByteStride: 0,
            };

            let vb_data = D3D11_SUBRESOURCE_DATA {
                pSysMem: vertices.as_ptr() as *const _,
                SysMemPitch: 0,
                SysMemSlicePitch: 0,
            };

            let mut vb = None;
            device.CreateBuffer(&vb_desc, Some(&vb_data), Some(&mut vb)).map_err(|e| format!("CreateBuffer (Vertex) failed: {}", e))?;
            self.vertex_buffer = vb;

            // 5. Create Index Buffer
            let indices: [u16; 6] = [0, 1, 2, 2, 1, 3];
             let ib_desc = D3D11_BUFFER_DESC {
                ByteWidth: (std::mem::size_of::<u16>() * indices.len()) as u32,
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_INDEX_BUFFER.0 as u32,
                CPUAccessFlags: 0,
                MiscFlags: 0,
                StructureByteStride: 0,
            };

            let ib_data = D3D11_SUBRESOURCE_DATA {
                pSysMem: indices.as_ptr() as *const _,
                SysMemPitch: 0,
                SysMemSlicePitch: 0,
            };

            let mut ib = None;
            device.CreateBuffer(&ib_desc, Some(&ib_data), Some(&mut ib)).map_err(|e| format!("CreateBuffer (Index) failed: {}", e))?;
            self.index_buffer = ib;

            // 6. Create Constant Buffer
            let cb_desc = D3D11_BUFFER_DESC {
                ByteWidth: std::mem::size_of::<ConstantBufferData>() as u32,
                Usage: D3D11_USAGE_DYNAMIC, // CPU writable
                BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                MiscFlags: 0,
                StructureByteStride: 0,
            };

            let mut cb = None;
            device.CreateBuffer(&cb_desc, None, Some(&mut cb)).map_err(|e| format!("CreateBuffer (Constant) failed: {}", e))?;
            self.constant_buffer = cb;

            // 7. Create Sampler State
            let sampler_desc = D3D11_SAMPLER_DESC {
                Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                ComparisonFunc: windows::Win32::Graphics::Direct3D11::D3D11_COMPARISON_NEVER,
                MinLOD: 0.0,
                MaxLOD: D3D11_FLOAT_MAX, // defined locally or use standard float max
                ..Default::default()
            };
            
            let mut sampler = None;
            device.CreateSamplerState(&sampler_desc, Some(&mut sampler)).map_err(|e| format!("CreateSamplerState failed: {}", e))?;
            self.sampler_state = sampler;
            
            // 8. Create Blend State (Optional, but good for safety)
             let mut blend_desc = D3D11_BLEND_DESC::default();
            blend_desc.RenderTarget[0].BlendEnable = false.into(); // We do blending in shader
            blend_desc.RenderTarget[0].RenderTargetWriteMask = D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8;

            let mut blend = None;
            device.CreateBlendState(&blend_desc, Some(&mut blend)).map_err(|e| format!("CreateBlendState failed: {}", e))?;
            self.blend_state = blend;
        }

        Ok(())
    }

    fn recreate_render_target(&mut self) -> Result<(), String> {
        self.render_target_view = None;
        unsafe {
            let back_buffer: ID3D11Texture2D = self.swapchain.GetBuffer(0).map_err(|e| format!("GetBuffer failed: {}", e))?;
            let mut rtv = None;
            self.device.CreateRenderTargetView(&back_buffer, None, Some(&mut rtv)).map_err(|e| format!("CreateRenderTargetView failed: {}", e))?;
            self.render_target_view = rtv;
        }
        Ok(())
    }

    pub fn render_frame(
        &self,
        texture_ptr: usize,
        crop_rect: (i32, i32, u32, u32), // x, y, w, h
        click_data: (f32, f32, f32, f32, [f32; 4]), // x, y, radius, alpha, color
    ) -> Result<(), String> {
        let (crop_x, crop_y, crop_w, crop_h) = crop_rect;
        let (cx, cy, radius, alpha, color) = click_data;

        if let Some(rtv) = &self.render_target_view {
            // Reconstruct source texture
            let source_texture: &ID3D11Texture2D = unsafe { &*(texture_ptr as *const ID3D11Texture2D) };
            
            // Get dimensions
            let mut desc = Default::default();
            unsafe { source_texture.GetDesc(&mut desc); }
            
            unsafe {
                // 1. Setup OM
                self.context.OMSetRenderTargets(Some(&[Some(rtv.clone())]), None);
                let bg = [0.0, 0.0, 0.0, 1.0];
                self.context.ClearRenderTargetView(rtv, &bg);
                
                // 2. Setup Viewport
                // We use crop_w/h for viewport to fill the window with the cropped content
                // Actually the Window size might be different from Crop size (scaling).
                // Usually DestinationWindow resizes to match Crop. 
                // Let's assume Viewport matches Window Size (backbuffer size).
                 let mut back_desc = Default::default();
                let back_buffer: ID3D11Texture2D = self.swapchain.GetBuffer(0).unwrap();
                back_buffer.GetDesc(&mut back_desc);

                let viewport = D3D11_VIEWPORT {
                    TopLeftX: 0.0,
                    TopLeftY: 0.0,
                    Width: back_desc.Width as f32,
                    Height: back_desc.Height as f32,
                    MinDepth: 0.0,
                    MaxDepth: 1.0,
                };
                self.context.RSSetViewports(Some(&[viewport]));

                // 3. Update Constant Buffer
                let texture_width = desc.Width as f32;
                let texture_height = desc.Height as f32;
                
                // Calculate normalized offset/scale for TexCoords
                 let offset_x = (crop_x as f32) / texture_width;
                let offset_y = (crop_y as f32) / texture_height;
                let scale_x = (crop_w as f32) / texture_width;
                let scale_y = (crop_h as f32) / texture_height;

                let data = ConstantBufferData {
                    click_pos: [cx, cy],
                    click_radius: radius,
                    click_alpha: alpha,
                    click_color: color,
                    offset: [offset_x, offset_y],
                    scale: [scale_x, scale_y],
                };
                
                let mut mapped = Default::default();
                if self.context.Map(self.constant_buffer.as_ref().unwrap(), 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped)).is_ok() {
                    ptr::copy_nonoverlapping(&data, mapped.pData as *mut ConstantBufferData, 1);
                    self.context.Unmap(self.constant_buffer.as_ref().unwrap(), 0);
                }

                // 4. Bind Pipeline
                self.context.IASetInputLayout(self.input_layout.as_ref());
                self.context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                
                let stride = std::mem::size_of::<Vertex>() as u32;
                let offset = 0;
                self.context.IASetVertexBuffers(0, 1, Some(&self.vertex_buffer), Some(&stride), Some(&offset));
                self.context.IASetIndexBuffer(self.index_buffer.as_ref(), windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R16_UINT, 0);
                
                self.context.VSSetShader(self.vertex_shader.as_ref(), None);
                self.context.VSSetConstantBuffers(0, Some(&[Some(self.constant_buffer.as_ref().unwrap().clone())]));
                
                self.context.PSSetShader(self.pixel_shader.as_ref(), None);
                self.context.PSSetConstantBuffers(0, Some(&[Some(self.constant_buffer.as_ref().unwrap().clone())]));
                self.context.PSSetShaderResources(0, Some(&[Some(transmute_texture_srv(self.device.clone(), source_texture))])); // Need SRV
                self.context.PSSetSamplers(0, Some(&[Some(self.sampler_state.as_ref().unwrap().clone())]));

                // 5. Draw
                self.context.DrawIndexed(6, 0, 0);
                
                // 6. Presentation
                self.swapchain.Present(1, DXGI_PRESENT(0)).ok();
            }
        }
        Ok(())
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        self.render_target_view = None;
        unsafe {
            self.swapchain.ResizeBuffers(0, width, height, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SWAP_CHAIN_FLAG(0)).map_err(|e| format!("{:?}", e))?;
        }
        self.recreate_render_target()
    }
}

// Helper to create Shader Resource View from Texture
// NOTE: WGC textures usually have D3D11_BIND_SHADER_RESOURCE, so this works.
unsafe fn transmute_texture_srv(device: ID3D11Device, texture: &ID3D11Texture2D) -> windows::Win32::Graphics::Direct3D11::ID3D11ShaderResourceView {
    let mut srv = None;
    device.CreateShaderResourceView(texture, None, Some(&mut srv)).unwrap();
    srv.unwrap()
}

// Compile Shader Helper
fn compile_shader(src: &str, entry: &str, target: &str) -> Result<windows::Win32::Graphics::Direct3D::ID3DBlob, String> {
    unsafe {
        let mut flags = 0; // D3DCOMPILE_ENABLE_STRICTNESS
        #[cfg(debug_assertions)]
        { flags |= 1; } // D3DCOMPILE_DEBUG

        let mut code_blob = None;
        let mut error_blob = None;
        
        let src_cstr = CString::new(src).unwrap();
        let entry_cstr = CString::new(entry).unwrap();
        let target_cstr = CString::new(target).unwrap();

        let result = D3DCompile(
            src_cstr.as_ptr() as *const _,
            src.len(),
            PCSTR(ptr::null()), // source name
            None, // defines
            None, // include
            PCSTR(entry_cstr.as_ptr() as *const _),
            PCSTR(target_cstr.as_ptr() as *const _),
            flags,
            0,
            &mut code_blob,
            Some(&mut error_blob),
        );

        if let Some(err) = error_blob {
             let msg = String::from_utf8_lossy(slice::from_raw_parts(
                err.GetBufferPointer() as *const u8,
                err.GetBufferSize(),
            )).to_string();
            return Err(format!("Shader compile error: {}", msg));
        }
        
        if result.is_err() {
            return Err("Unknown compiler error".to_string());
        }

        Ok(code_blob.unwrap())
    }
}

const D3D11_FLOAT_MAX: f32 = 3.402823466e+38;
