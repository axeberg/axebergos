// Rectangle rendering shader for axeberg compositor
//
// Renders filled rectangles with per-vertex colors.
// Uses indexed drawing for efficient quad rendering.

// Uniform buffer containing screen dimensions
struct Uniforms {
    screen_size: vec2<f32>,
    _padding: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

// Vertex input
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
}

// Vertex output / Fragment input
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

// Vertex shader
// Transforms 2D positions (already in NDC) to clip space
@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;

    // Position is already in normalized device coordinates (-1 to 1)
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    output.color = input.color;

    return output;
}

// Fragment shader
// Simply outputs the interpolated color
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Premultiply alpha for correct blending
    let color = input.color;
    return vec4<f32>(color.rgb * color.a, color.a);
}
