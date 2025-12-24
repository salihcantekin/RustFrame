// shader.wgsl - WGSL Shader for rendering captured textures
//
// This is a simple passthrough shader that:
// 1. Vertex shader: Transforms vertices from NDC to screen space
// 2. Fragment shader: Samples the captured texture and outputs the color
//
// WGSL is the WebGPU Shading Language, similar to GLSL or HLSL

// Vertex shader input
struct VertexInput {
    @location(0) position: vec2<f32>,    // 2D position in NDC
    @location(1) tex_coords: vec2<f32>,  // Texture coordinates (0-1)
};

// Vertex shader output / Fragment shader input
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>, // Position in clip space
    @location(0) tex_coords: vec2<f32>,          // Pass through texture coords
};

// The captured texture and sampler
@group(0) @binding(0)
var t_texture: texture_2d<f32>;

@group(0) @binding(1)
var t_sampler: sampler;

// Vertex shader
// Transforms 2D positions to 4D clip space positions
@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;

    // Convert 2D position to 4D (add z=0, w=1)
    output.clip_position = vec4<f32>(input.position, 0.0, 1.0);

    // Pass texture coordinates to fragment shader
    output.tex_coords = input.tex_coords;

    return output;
}

// Fragment shader
// Samples the texture and outputs the color
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Sample the texture at the given coordinates
    let color = textureSample(t_texture, t_sampler, input.tex_coords);

    // Return the sampled color
    return color;
}
