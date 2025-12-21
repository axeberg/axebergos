//! WebGPU rendering surface
//!
//! Uses WebGPU for hardware-accelerated terminal rendering.
//! Falls back to... nothing. WebGPU or bust.

use super::{Color, Rect};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

/// A frame being rendered
pub struct Frame {
    /// Command encoder for this frame
    encoder: web_sys::GpuCommandEncoder,
    /// Current surface texture view
    view: web_sys::GpuTextureView,
    /// Queued rectangles to draw
    rects: Vec<RectInstance>,
    /// Queued text glyphs to draw
    glyphs: Vec<GlyphInstance>,
}

/// Instance data for a rectangle
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RectInstance {
    /// Position (x, y) in pixels
    pos: [f32; 2],
    /// Size (width, height) in pixels
    size: [f32; 2],
    /// Color (r, g, b, a)
    color: [f32; 4],
}

/// Instance data for a glyph
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GlyphInstance {
    /// Position (x, y) in pixels
    pos: [f32; 2],
    /// Glyph index in atlas (0-255)
    glyph: u32,
    /// Font size scale
    scale: f32,
    /// Color (r, g, b, a)
    color: [f32; 4],
}

/// The WebGPU rendering surface
pub struct Surface {
    device: web_sys::GpuDevice,
    queue: web_sys::GpuQueue,
    context: web_sys::GpuCanvasContext,
    canvas: web_sys::HtmlCanvasElement,
    _format: web_sys::GpuTextureFormat,
    width: u32,
    height: u32,

    // Pipelines
    rect_pipeline: web_sys::GpuRenderPipeline,
    glyph_pipeline: web_sys::GpuRenderPipeline,

    // Buffers
    rect_buffer: web_sys::GpuBuffer,
    glyph_buffer: web_sys::GpuBuffer,
    uniform_buffer: web_sys::GpuBuffer,

    // Font texture and bind group
    font_bind_group: web_sys::GpuBindGroup,
    uniform_bind_group: web_sys::GpuBindGroup,

    // Max instances
    max_rects: usize,
    max_glyphs: usize,
}

impl Surface {
    /// Create a new WebGPU surface
    pub async fn new(width: u32, height: u32) -> Result<Self, String> {
        // Get WebGPU adapter
        let window = web_sys::window().ok_or("No window")?;
        let navigator = window.navigator();
        let gpu = navigator.gpu();

        let adapter_opts = web_sys::GpuRequestAdapterOptions::new();
        adapter_opts.set_power_preference(web_sys::GpuPowerPreference::HighPerformance);

        let adapter: web_sys::GpuAdapter = JsFuture::from(gpu.request_adapter_with_options(&adapter_opts))
            .await
            .map_err(|e| format!("Failed to get adapter: {:?}", e))?
            .dyn_into()
            .map_err(|_| "Not an adapter")?;

        // Request device
        let device_desc = web_sys::GpuDeviceDescriptor::new();
        let device: web_sys::GpuDevice = JsFuture::from(adapter.request_device_with_descriptor(&device_desc))
            .await
            .map_err(|e| format!("Failed to get device: {:?}", e))?
            .dyn_into()
            .map_err(|_| "Not a device")?;

        let queue = device.queue();

        // Create canvas and configure surface
        let document = window.document().ok_or("No document")?;
        let canvas = document
            .create_element("canvas")
            .map_err(|_| "Failed to create canvas")?
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .map_err(|_| "Not a canvas")?;

        canvas.set_width(width);
        canvas.set_height(height);
        canvas.set_id("axeberg-canvas");

        // Style fullscreen
        let style = canvas.style();
        let _ = style.set_property("position", "fixed");
        let _ = style.set_property("top", "0");
        let _ = style.set_property("left", "0");
        let _ = style.set_property("width", "100%");
        let _ = style.set_property("height", "100%");

        document
            .body()
            .ok_or("No body")?
            .append_child(&canvas)
            .map_err(|_| "Failed to append canvas")?;

        // Get WebGPU context
        let context: web_sys::GpuCanvasContext = canvas
            .get_context("webgpu")
            .map_err(|_| "Failed to get webgpu context")?
            .ok_or("No webgpu context")?
            .dyn_into()
            .map_err(|_| "Not a GpuCanvasContext")?;

        // Configure the surface
        let format = navigator.gpu().get_preferred_canvas_format();
        let config = web_sys::GpuCanvasConfiguration::new(&device, format);
        let _ = context.configure(&config);

        // Create shaders
        let rect_shader = create_rect_shader(&device);
        let glyph_shader = create_glyph_shader(&device);

        // Create uniform buffer (screen size)
        let uniform_usage = web_sys::gpu_buffer_usage::UNIFORM | web_sys::gpu_buffer_usage::COPY_DST;
        let uniform_buffer = create_buffer(&device, 8, uniform_usage);

        // Create uniform bind group layout
        let uniform_layout = create_uniform_layout(&device);
        let uniform_bind_group = create_uniform_bind_group(&device, &uniform_layout, &uniform_buffer);

        // Create font texture and bind group
        let (font_texture, font_sampler) = create_font_texture(&device, &queue);
        let font_layout = create_font_layout(&device);
        let font_bind_group = create_font_bind_group(&device, &font_layout, &font_texture, &font_sampler);

        // Create pipelines
        let rect_pipeline = create_rect_pipeline(&device, &rect_shader, &uniform_layout, format);
        let glyph_pipeline = create_glyph_pipeline(&device, &glyph_shader, &uniform_layout, &font_layout, format);

        // Create instance buffers
        let max_rects = 1024;
        let max_glyphs = 16384;

        let vertex_usage = web_sys::gpu_buffer_usage::VERTEX | web_sys::gpu_buffer_usage::COPY_DST;
        let rect_buffer = create_buffer(
            &device,
            (max_rects * std::mem::size_of::<RectInstance>()) as u32,
            vertex_usage,
        );
        let glyph_buffer = create_buffer(
            &device,
            (max_glyphs * std::mem::size_of::<GlyphInstance>()) as u32,
            vertex_usage,
        );

        // Upload initial uniform data
        let uniform_data = [width as f32, height as f32];
        let uniform_array = js_sys::Float32Array::from(uniform_data.as_slice());
        let _ = queue.write_buffer_with_u32_and_buffer_source(&uniform_buffer, 0, &uniform_array);

        web_sys::console::log_1(&format!("[surface] WebGPU surface initialized: {}x{}", width, height).into());

        Ok(Self {
            device,
            queue,
            context,
            canvas,
            _format: format,
            width,
            height,
            rect_pipeline,
            glyph_pipeline,
            rect_buffer,
            glyph_buffer,
            uniform_buffer,
            font_bind_group,
            uniform_bind_group,
            max_rects,
            max_glyphs,
        })
    }

    /// Resize the surface
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.canvas.set_width(width);
        self.canvas.set_height(height);

        // Update uniform buffer
        let uniform_data = [width as f32, height as f32];
        let uniform_array = js_sys::Float32Array::from(uniform_data.as_slice());
        let _ = self.queue.write_buffer_with_u32_and_buffer_source(&self.uniform_buffer, 0, &uniform_array);
    }

    /// Begin a new frame
    pub fn begin_frame(&mut self) -> Option<Frame> {
        let texture = match self.context.get_current_texture() {
            Ok(t) => t,
            Err(_) => return None,
        };
        let view = match texture.create_view() {
            Ok(v) => v,
            Err(_) => return None,
        };

        let encoder_desc = web_sys::GpuCommandEncoderDescriptor::new();
        let encoder = self.device.create_command_encoder_with_descriptor(&encoder_desc);

        Some(Frame {
            encoder,
            view,
            rects: Vec::new(),
            glyphs: Vec::new(),
        })
    }

    /// Clear the screen with a color (handled in end_frame)
    pub fn clear(&mut self, _frame: &Frame, _color: Color) {
        // Clear is done via the load operation in the render pass
    }

    /// Draw a filled rectangle
    pub fn draw_rect(&mut self, frame: &mut Frame, rect: Rect, color: Color) {
        if frame.rects.len() >= self.max_rects {
            return;
        }

        frame.rects.push(RectInstance {
            pos: [rect.x, rect.y],
            size: [rect.width, rect.height],
            color: [color.r, color.g, color.b, color.a],
        });
    }

    /// Draw text
    pub fn draw_text(
        &mut self,
        frame: &mut Frame,
        text: &str,
        x: f32,
        y: f32,
        color: Color,
        font_size: f32,
    ) {
        let scale = font_size / 16.0;
        let glyph_width = 8.0 * scale;

        for (i, c) in text.chars().enumerate() {
            if frame.glyphs.len() >= self.max_glyphs {
                break;
            }

            let glyph_index = c as u32;
            if glyph_index > 255 {
                continue;
            }

            frame.glyphs.push(GlyphInstance {
                pos: [x + (i as f32 * glyph_width), y],
                glyph: glyph_index,
                scale,
                color: [color.r, color.g, color.b, color.a],
            });
        }
    }

    /// Draw a rectangle outline
    #[allow(dead_code)]
    pub fn draw_rect_outline(
        &mut self,
        frame: &mut Frame,
        rect: Rect,
        color: Color,
        line_width: f32,
    ) {
        // Top
        self.draw_rect(frame, Rect::new(rect.x, rect.y, rect.width, line_width), color);
        // Bottom
        self.draw_rect(frame, Rect::new(rect.x, rect.y + rect.height - line_width, rect.width, line_width), color);
        // Left
        self.draw_rect(frame, Rect::new(rect.x, rect.y, line_width, rect.height), color);
        // Right
        self.draw_rect(frame, Rect::new(rect.x + rect.width - line_width, rect.y, line_width, rect.height), color);
    }

    /// End the frame and submit commands
    pub fn end_frame(&mut self, frame: Frame) {
        // Upload instance data
        if !frame.rects.is_empty() {
            let rect_data = bytemuck::cast_slice(&frame.rects);
            let array = js_sys::Uint8Array::from(rect_data);
            let _ = self.queue.write_buffer_with_u32_and_buffer_source(&self.rect_buffer, 0, &array);
        }

        if !frame.glyphs.is_empty() {
            let glyph_data = bytemuck::cast_slice(&frame.glyphs);
            let array = js_sys::Uint8Array::from(glyph_data);
            let _ = self.queue.write_buffer_with_u32_and_buffer_source(&self.glyph_buffer, 0, &array);
        }

        // Create render pass
        let color_attachment = web_sys::GpuRenderPassColorAttachment::new(
            web_sys::GpuLoadOp::Clear,
            web_sys::GpuStoreOp::Store,
            &frame.view,
        );

        // Set clear color (Tokyo Night background)
        let clear_color = web_sys::GpuColorDict::new(1.0, 0.15, 0.1, 0.1); // a, b, g, r
        color_attachment.set_clear_value(&clear_color);

        let color_attachments = js_sys::Array::new();
        color_attachments.push(&color_attachment);

        let pass_desc = web_sys::GpuRenderPassDescriptor::new(&color_attachments);
        let pass = match frame.encoder.begin_render_pass(&pass_desc) {
            Ok(p) => p,
            Err(_) => return,
        };

        // Draw rectangles
        if !frame.rects.is_empty() {
            pass.set_pipeline(&self.rect_pipeline);
            pass.set_bind_group(0, Some(&self.uniform_bind_group));
            pass.set_vertex_buffer(0, Some(&self.rect_buffer));
            pass.draw_with_instance_count(6, frame.rects.len() as u32);
        }

        // Draw glyphs
        if !frame.glyphs.is_empty() {
            pass.set_pipeline(&self.glyph_pipeline);
            pass.set_bind_group(0, Some(&self.uniform_bind_group));
            pass.set_bind_group(1, Some(&self.font_bind_group));
            pass.set_vertex_buffer(0, Some(&self.glyph_buffer));
            pass.draw_with_instance_count(6, frame.glyphs.len() as u32);
        }

        let _ = pass.end();

        // Submit
        let command_buffer = frame.encoder.finish();
        let commands = js_sys::Array::new();
        commands.push(&command_buffer);
        self.queue.submit(&commands);
    }

    /// Get dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// Create a GPU buffer
fn create_buffer(device: &web_sys::GpuDevice, size: u32, usage: u32) -> web_sys::GpuBuffer {
    let desc = web_sys::GpuBufferDescriptor::new(size as f64, usage);
    device.create_buffer(&desc).expect("Failed to create buffer")
}

/// Create the rectangle shader module
fn create_rect_shader(device: &web_sys::GpuDevice) -> web_sys::GpuShaderModule {
    let code = r#"
        struct Uniforms {
            screen_size: vec2f,
        }

        @group(0) @binding(0) var<uniform> uniforms: Uniforms;

        struct RectInstance {
            @location(0) pos: vec2f,
            @location(1) size: vec2f,
            @location(2) color: vec4f,
        }

        struct VertexOutput {
            @builtin(position) position: vec4f,
            @location(0) color: vec4f,
        }

        @vertex
        fn vs_main(
            @builtin(vertex_index) vertex_index: u32,
            instance: RectInstance,
        ) -> VertexOutput {
            var positions = array<vec2f, 6>(
                vec2f(0.0, 0.0),
                vec2f(1.0, 0.0),
                vec2f(0.0, 1.0),
                vec2f(1.0, 0.0),
                vec2f(1.0, 1.0),
                vec2f(0.0, 1.0),
            );

            let local_pos = positions[vertex_index];
            let world_pos = instance.pos + local_pos * instance.size;
            let clip_pos = (world_pos / uniforms.screen_size) * 2.0 - 1.0;

            var output: VertexOutput;
            output.position = vec4f(clip_pos.x, -clip_pos.y, 0.0, 1.0);
            output.color = instance.color;
            return output;
        }

        @fragment
        fn fs_main(input: VertexOutput) -> @location(0) vec4f {
            return input.color;
        }
    "#;

    let desc = web_sys::GpuShaderModuleDescriptor::new(code);
    device.create_shader_module(&desc)
}

/// Create the glyph shader module
fn create_glyph_shader(device: &web_sys::GpuDevice) -> web_sys::GpuShaderModule {
    let code = r#"
        struct Uniforms {
            screen_size: vec2f,
        }

        @group(0) @binding(0) var<uniform> uniforms: Uniforms;
        @group(1) @binding(0) var font_texture: texture_2d<f32>;
        @group(1) @binding(1) var font_sampler: sampler;

        struct GlyphInstance {
            @location(0) pos: vec2f,
            @location(1) glyph: u32,
            @location(2) scale: f32,
            @location(3) color: vec4f,
        }

        struct VertexOutput {
            @builtin(position) position: vec4f,
            @location(0) uv: vec2f,
            @location(1) color: vec4f,
        }

        @vertex
        fn vs_main(
            @builtin(vertex_index) vertex_index: u32,
            instance: GlyphInstance,
        ) -> VertexOutput {
            let glyph_size = vec2f(8.0, 16.0) * instance.scale;

            var positions = array<vec2f, 6>(
                vec2f(0.0, 0.0),
                vec2f(1.0, 0.0),
                vec2f(0.0, 1.0),
                vec2f(1.0, 0.0),
                vec2f(1.0, 1.0),
                vec2f(0.0, 1.0),
            );

            let local_pos = positions[vertex_index];
            let world_pos = instance.pos + local_pos * glyph_size - vec2f(0.0, glyph_size.y);
            let clip_pos = (world_pos / uniforms.screen_size) * 2.0 - 1.0;

            let glyph_x = f32(instance.glyph % 16u);
            let glyph_y = f32(instance.glyph / 16u);
            let uv_base = vec2f(glyph_x / 16.0, glyph_y / 16.0);
            let uv_size = vec2f(1.0 / 16.0, 1.0 / 16.0);
            let uv = uv_base + local_pos * uv_size;

            var output: VertexOutput;
            output.position = vec4f(clip_pos.x, -clip_pos.y, 0.0, 1.0);
            output.uv = uv;
            output.color = instance.color;
            return output;
        }

        @fragment
        fn fs_main(input: VertexOutput) -> @location(0) vec4f {
            let alpha = textureSample(font_texture, font_sampler, input.uv).r;
            if (alpha < 0.5) {
                discard;
            }
            return vec4f(input.color.rgb, input.color.a * alpha);
        }
    "#;

    let desc = web_sys::GpuShaderModuleDescriptor::new(code);
    device.create_shader_module(&desc)
}

/// Create uniform bind group layout
fn create_uniform_layout(device: &web_sys::GpuDevice) -> web_sys::GpuBindGroupLayout {
    let entry = web_sys::GpuBindGroupLayoutEntry::new(0, web_sys::gpu_shader_stage::VERTEX);
    let buffer_layout = web_sys::GpuBufferBindingLayout::new();
    buffer_layout.set_type(web_sys::GpuBufferBindingType::Uniform);
    entry.set_buffer(&buffer_layout);

    let entries = js_sys::Array::new();
    entries.push(&entry);

    let desc = web_sys::GpuBindGroupLayoutDescriptor::new(&entries);
    device.create_bind_group_layout(&desc).expect("Failed to create uniform layout")
}

/// Create uniform bind group
fn create_uniform_bind_group(
    device: &web_sys::GpuDevice,
    layout: &web_sys::GpuBindGroupLayout,
    buffer: &web_sys::GpuBuffer,
) -> web_sys::GpuBindGroup {
    let buffer_binding = web_sys::GpuBufferBinding::new(buffer);
    let entry = web_sys::GpuBindGroupEntry::new(0, &buffer_binding);

    let entries = js_sys::Array::new();
    entries.push(&entry);

    let desc = web_sys::GpuBindGroupDescriptor::new(&entries, layout);
    device.create_bind_group(&desc)
}

/// Create font texture bind group layout
fn create_font_layout(device: &web_sys::GpuDevice) -> web_sys::GpuBindGroupLayout {
    let tex_entry = web_sys::GpuBindGroupLayoutEntry::new(0, web_sys::gpu_shader_stage::FRAGMENT);
    let tex_layout = web_sys::GpuTextureBindingLayout::new();
    tex_layout.set_sample_type(web_sys::GpuTextureSampleType::Float);
    tex_entry.set_texture(&tex_layout);

    let sampler_entry = web_sys::GpuBindGroupLayoutEntry::new(1, web_sys::gpu_shader_stage::FRAGMENT);
    let sampler_layout = web_sys::GpuSamplerBindingLayout::new();
    sampler_layout.set_type(web_sys::GpuSamplerBindingType::Filtering);
    sampler_entry.set_sampler(&sampler_layout);

    let entries = js_sys::Array::new();
    entries.push(&tex_entry);
    entries.push(&sampler_entry);

    let desc = web_sys::GpuBindGroupLayoutDescriptor::new(&entries);
    device.create_bind_group_layout(&desc).expect("Failed to create font layout")
}

/// Create font bind group
fn create_font_bind_group(
    device: &web_sys::GpuDevice,
    layout: &web_sys::GpuBindGroupLayout,
    texture: &web_sys::GpuTexture,
    sampler: &web_sys::GpuSampler,
) -> web_sys::GpuBindGroup {
    let view = texture.create_view().expect("Failed to create texture view");

    let tex_entry = web_sys::GpuBindGroupEntry::new(0, &view);
    let sampler_entry = web_sys::GpuBindGroupEntry::new(1, sampler);

    let entries = js_sys::Array::new();
    entries.push(&tex_entry);
    entries.push(&sampler_entry);

    let desc = web_sys::GpuBindGroupDescriptor::new(&entries, layout);
    device.create_bind_group(&desc)
}

/// Create the font texture (128x256 bitmap = 16x16 grid of 8x16 glyphs)
fn create_font_texture(
    device: &web_sys::GpuDevice,
    queue: &web_sys::GpuQueue,
) -> (web_sys::GpuTexture, web_sys::GpuSampler) {
    let width = 128u32;
    let height = 256u32;

    let size = web_sys::GpuExtent3dDict::new(width);
    size.set_height(height);

    let tex_usage = web_sys::gpu_texture_usage::TEXTURE_BINDING | web_sys::gpu_texture_usage::COPY_DST;
    let desc = web_sys::GpuTextureDescriptor::new(web_sys::GpuTextureFormat::R8unorm, &size, tex_usage);
    let texture = device.create_texture(&desc).expect("Failed to create font texture");

    let font_data = generate_font_bitmap();

    let copy_texture = web_sys::GpuImageCopyTexture::new(&texture);
    let data_layout = web_sys::GpuImageDataLayout::new();
    data_layout.set_bytes_per_row(width);
    data_layout.set_rows_per_image(height);

    // Create size array for write_texture
    let size_array = js_sys::Array::new();
    size_array.push(&wasm_bindgen::JsValue::from(width));
    size_array.push(&wasm_bindgen::JsValue::from(height));
    size_array.push(&wasm_bindgen::JsValue::from(1u32));

    let array = js_sys::Uint8Array::from(font_data.as_slice());
    let _ = queue.write_texture_with_buffer_source_and_u32_sequence(&copy_texture, &array, &data_layout, &size_array);

    let sampler_desc = web_sys::GpuSamplerDescriptor::new();
    sampler_desc.set_mag_filter(web_sys::GpuFilterMode::Nearest);
    sampler_desc.set_min_filter(web_sys::GpuFilterMode::Nearest);
    let sampler = device.create_sampler_with_descriptor(&sampler_desc);

    (texture, sampler)
}

/// Generate a simple bitmap font (ASCII 0-255 in 16x16 grid of 8x16 glyphs)
fn generate_font_bitmap() -> Vec<u8> {
    let mut data = vec![0u8; 128 * 256];

    for glyph in 32u8..=126 {
        let glyph_x = (glyph % 16) as usize * 8;
        let glyph_y = (glyph / 16) as usize * 16;
        render_glyph(&mut data, 128, glyph_x, glyph_y, glyph);
    }

    data
}

/// Render a single glyph to the bitmap
fn render_glyph(data: &mut [u8], stride: usize, x: usize, y: usize, c: u8) {
    let font = get_embedded_font();

    if c < 32 || c > 126 {
        return;
    }

    let glyph_index = (c - 32) as usize;
    let glyph_data = &font[glyph_index * 16..(glyph_index + 1) * 16];

    for row in 0..16 {
        let byte = glyph_data[row];
        for col in 0..8 {
            if byte & (0x80 >> col) != 0 {
                data[(y + row) * stride + x + col] = 255;
            }
        }
    }
}

/// Embedded 8x16 VGA font (printable ASCII 32-126)
fn get_embedded_font() -> &'static [u8] {
    static FONT: &[u8] = &[
        // Space (32)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        // ! (33)
        0x00, 0x00, 0x18, 0x3c, 0x3c, 0x3c, 0x18, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00,
        // " (34)
        0x00, 0x66, 0x66, 0x66, 0x24, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        // # (35)
        0x00, 0x00, 0x00, 0x6c, 0x6c, 0xfe, 0x6c, 0x6c, 0x6c, 0xfe, 0x6c, 0x6c, 0x00, 0x00, 0x00, 0x00,
        // $ (36)
        0x18, 0x18, 0x7c, 0xc6, 0xc2, 0xc0, 0x7c, 0x06, 0x06, 0x86, 0xc6, 0x7c, 0x18, 0x18, 0x00, 0x00,
        // % (37)
        0x00, 0x00, 0x00, 0x00, 0xc2, 0xc6, 0x0c, 0x18, 0x30, 0x60, 0xc6, 0x86, 0x00, 0x00, 0x00, 0x00,
        // & (38)
        0x00, 0x00, 0x38, 0x6c, 0x6c, 0x38, 0x76, 0xdc, 0xcc, 0xcc, 0xcc, 0x76, 0x00, 0x00, 0x00, 0x00,
        // ' (39)
        0x00, 0x30, 0x30, 0x30, 0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        // ( (40)
        0x00, 0x00, 0x0c, 0x18, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x18, 0x0c, 0x00, 0x00, 0x00, 0x00,
        // ) (41)
        0x00, 0x00, 0x30, 0x18, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x18, 0x30, 0x00, 0x00, 0x00, 0x00,
        // * (42)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x66, 0x3c, 0xff, 0x3c, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        // + (43)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x7e, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        // , (44)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x18, 0x30, 0x00, 0x00, 0x00,
        // - (45)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xfe, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        // . (46)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00,
        // / (47)
        0x00, 0x00, 0x00, 0x00, 0x02, 0x06, 0x0c, 0x18, 0x30, 0x60, 0xc0, 0x80, 0x00, 0x00, 0x00, 0x00,
        // 0 (48)
        0x00, 0x00, 0x7c, 0xc6, 0xc6, 0xce, 0xde, 0xf6, 0xe6, 0xc6, 0xc6, 0x7c, 0x00, 0x00, 0x00, 0x00,
        // 1 (49)
        0x00, 0x00, 0x18, 0x38, 0x78, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x7e, 0x00, 0x00, 0x00, 0x00,
        // 2 (50)
        0x00, 0x00, 0x7c, 0xc6, 0x06, 0x0c, 0x18, 0x30, 0x60, 0xc0, 0xc6, 0xfe, 0x00, 0x00, 0x00, 0x00,
        // 3 (51)
        0x00, 0x00, 0x7c, 0xc6, 0x06, 0x06, 0x3c, 0x06, 0x06, 0x06, 0xc6, 0x7c, 0x00, 0x00, 0x00, 0x00,
        // 4 (52)
        0x00, 0x00, 0x0c, 0x1c, 0x3c, 0x6c, 0xcc, 0xfe, 0x0c, 0x0c, 0x0c, 0x1e, 0x00, 0x00, 0x00, 0x00,
        // 5 (53)
        0x00, 0x00, 0xfe, 0xc0, 0xc0, 0xc0, 0xfc, 0x06, 0x06, 0x06, 0xc6, 0x7c, 0x00, 0x00, 0x00, 0x00,
        // 6 (54)
        0x00, 0x00, 0x38, 0x60, 0xc0, 0xc0, 0xfc, 0xc6, 0xc6, 0xc6, 0xc6, 0x7c, 0x00, 0x00, 0x00, 0x00,
        // 7 (55)
        0x00, 0x00, 0xfe, 0xc6, 0x06, 0x06, 0x0c, 0x18, 0x30, 0x30, 0x30, 0x30, 0x00, 0x00, 0x00, 0x00,
        // 8 (56)
        0x00, 0x00, 0x7c, 0xc6, 0xc6, 0xc6, 0x7c, 0xc6, 0xc6, 0xc6, 0xc6, 0x7c, 0x00, 0x00, 0x00, 0x00,
        // 9 (57)
        0x00, 0x00, 0x7c, 0xc6, 0xc6, 0xc6, 0x7e, 0x06, 0x06, 0x06, 0x0c, 0x78, 0x00, 0x00, 0x00, 0x00,
        // : (58)
        0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00,
        // ; (59)
        0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x18, 0x18, 0x30, 0x00, 0x00, 0x00, 0x00,
        // < (60)
        0x00, 0x00, 0x00, 0x06, 0x0c, 0x18, 0x30, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x00, 0x00, 0x00, 0x00,
        // = (61)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x7e, 0x00, 0x00, 0x7e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        // > (62)
        0x00, 0x00, 0x00, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x0c, 0x18, 0x30, 0x60, 0x00, 0x00, 0x00, 0x00,
        // ? (63)
        0x00, 0x00, 0x7c, 0xc6, 0xc6, 0x0c, 0x18, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00,
        // @ (64)
        0x00, 0x00, 0x7c, 0xc6, 0xc6, 0xc6, 0xde, 0xde, 0xde, 0xdc, 0xc0, 0x7c, 0x00, 0x00, 0x00, 0x00,
        // A (65)
        0x00, 0x00, 0x10, 0x38, 0x6c, 0xc6, 0xc6, 0xfe, 0xc6, 0xc6, 0xc6, 0xc6, 0x00, 0x00, 0x00, 0x00,
        // B (66)
        0x00, 0x00, 0xfc, 0x66, 0x66, 0x66, 0x7c, 0x66, 0x66, 0x66, 0x66, 0xfc, 0x00, 0x00, 0x00, 0x00,
        // C (67)
        0x00, 0x00, 0x3c, 0x66, 0xc2, 0xc0, 0xc0, 0xc0, 0xc0, 0xc2, 0x66, 0x3c, 0x00, 0x00, 0x00, 0x00,
        // D (68)
        0x00, 0x00, 0xf8, 0x6c, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x6c, 0xf8, 0x00, 0x00, 0x00, 0x00,
        // E (69)
        0x00, 0x00, 0xfe, 0x66, 0x62, 0x68, 0x78, 0x68, 0x60, 0x62, 0x66, 0xfe, 0x00, 0x00, 0x00, 0x00,
        // F (70)
        0x00, 0x00, 0xfe, 0x66, 0x62, 0x68, 0x78, 0x68, 0x60, 0x60, 0x60, 0xf0, 0x00, 0x00, 0x00, 0x00,
        // G (71)
        0x00, 0x00, 0x3c, 0x66, 0xc2, 0xc0, 0xc0, 0xde, 0xc6, 0xc6, 0x66, 0x3a, 0x00, 0x00, 0x00, 0x00,
        // H (72)
        0x00, 0x00, 0xc6, 0xc6, 0xc6, 0xc6, 0xfe, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0x00, 0x00, 0x00, 0x00,
        // I (73)
        0x00, 0x00, 0x3c, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3c, 0x00, 0x00, 0x00, 0x00,
        // J (74)
        0x00, 0x00, 0x1e, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0xcc, 0xcc, 0xcc, 0x78, 0x00, 0x00, 0x00, 0x00,
        // K (75)
        0x00, 0x00, 0xe6, 0x66, 0x66, 0x6c, 0x78, 0x78, 0x6c, 0x66, 0x66, 0xe6, 0x00, 0x00, 0x00, 0x00,
        // L (76)
        0x00, 0x00, 0xf0, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x62, 0x66, 0xfe, 0x00, 0x00, 0x00, 0x00,
        // M (77)
        0x00, 0x00, 0xc6, 0xee, 0xfe, 0xfe, 0xd6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0x00, 0x00, 0x00, 0x00,
        // N (78)
        0x00, 0x00, 0xc6, 0xe6, 0xf6, 0xfe, 0xde, 0xce, 0xc6, 0xc6, 0xc6, 0xc6, 0x00, 0x00, 0x00, 0x00,
        // O (79)
        0x00, 0x00, 0x7c, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0x7c, 0x00, 0x00, 0x00, 0x00,
        // P (80)
        0x00, 0x00, 0xfc, 0x66, 0x66, 0x66, 0x7c, 0x60, 0x60, 0x60, 0x60, 0xf0, 0x00, 0x00, 0x00, 0x00,
        // Q (81)
        0x00, 0x00, 0x7c, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xd6, 0xde, 0x7c, 0x0c, 0x0e, 0x00, 0x00,
        // R (82)
        0x00, 0x00, 0xfc, 0x66, 0x66, 0x66, 0x7c, 0x6c, 0x66, 0x66, 0x66, 0xe6, 0x00, 0x00, 0x00, 0x00,
        // S (83)
        0x00, 0x00, 0x7c, 0xc6, 0xc6, 0x60, 0x38, 0x0c, 0x06, 0xc6, 0xc6, 0x7c, 0x00, 0x00, 0x00, 0x00,
        // T (84)
        0x00, 0x00, 0x7e, 0x7e, 0x5a, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3c, 0x00, 0x00, 0x00, 0x00,
        // U (85)
        0x00, 0x00, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0x7c, 0x00, 0x00, 0x00, 0x00,
        // V (86)
        0x00, 0x00, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0x6c, 0x38, 0x10, 0x00, 0x00, 0x00, 0x00,
        // W (87)
        0x00, 0x00, 0xc6, 0xc6, 0xc6, 0xc6, 0xd6, 0xd6, 0xd6, 0xfe, 0xee, 0x6c, 0x00, 0x00, 0x00, 0x00,
        // X (88)
        0x00, 0x00, 0xc6, 0xc6, 0x6c, 0x7c, 0x38, 0x38, 0x7c, 0x6c, 0xc6, 0xc6, 0x00, 0x00, 0x00, 0x00,
        // Y (89)
        0x00, 0x00, 0x66, 0x66, 0x66, 0x66, 0x3c, 0x18, 0x18, 0x18, 0x18, 0x3c, 0x00, 0x00, 0x00, 0x00,
        // Z (90)
        0x00, 0x00, 0xfe, 0xc6, 0x86, 0x0c, 0x18, 0x30, 0x60, 0xc2, 0xc6, 0xfe, 0x00, 0x00, 0x00, 0x00,
        // [ (91)
        0x00, 0x00, 0x3c, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x3c, 0x00, 0x00, 0x00, 0x00,
        // \ (92)
        0x00, 0x00, 0x00, 0x80, 0xc0, 0xe0, 0x70, 0x38, 0x1c, 0x0e, 0x06, 0x02, 0x00, 0x00, 0x00, 0x00,
        // ] (93)
        0x00, 0x00, 0x3c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x3c, 0x00, 0x00, 0x00, 0x00,
        // ^ (94)
        0x10, 0x38, 0x6c, 0xc6, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        // _ (95)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x00, 0x00,
        // ` (96)
        0x30, 0x30, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        // a (97)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x78, 0x0c, 0x7c, 0xcc, 0xcc, 0xcc, 0x76, 0x00, 0x00, 0x00, 0x00,
        // b (98)
        0x00, 0x00, 0xe0, 0x60, 0x60, 0x78, 0x6c, 0x66, 0x66, 0x66, 0x66, 0x7c, 0x00, 0x00, 0x00, 0x00,
        // c (99)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x7c, 0xc6, 0xc0, 0xc0, 0xc0, 0xc6, 0x7c, 0x00, 0x00, 0x00, 0x00,
        // d (100)
        0x00, 0x00, 0x1c, 0x0c, 0x0c, 0x3c, 0x6c, 0xcc, 0xcc, 0xcc, 0xcc, 0x76, 0x00, 0x00, 0x00, 0x00,
        // e (101)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x7c, 0xc6, 0xfe, 0xc0, 0xc0, 0xc6, 0x7c, 0x00, 0x00, 0x00, 0x00,
        // f (102)
        0x00, 0x00, 0x38, 0x6c, 0x64, 0x60, 0xf0, 0x60, 0x60, 0x60, 0x60, 0xf0, 0x00, 0x00, 0x00, 0x00,
        // g (103)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x76, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0x7c, 0x0c, 0xcc, 0x78, 0x00,
        // h (104)
        0x00, 0x00, 0xe0, 0x60, 0x60, 0x6c, 0x76, 0x66, 0x66, 0x66, 0x66, 0xe6, 0x00, 0x00, 0x00, 0x00,
        // i (105)
        0x00, 0x00, 0x18, 0x18, 0x00, 0x38, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3c, 0x00, 0x00, 0x00, 0x00,
        // j (106)
        0x00, 0x00, 0x06, 0x06, 0x00, 0x0e, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x66, 0x66, 0x3c, 0x00,
        // k (107)
        0x00, 0x00, 0xe0, 0x60, 0x60, 0x66, 0x6c, 0x78, 0x78, 0x6c, 0x66, 0xe6, 0x00, 0x00, 0x00, 0x00,
        // l (108)
        0x00, 0x00, 0x38, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3c, 0x00, 0x00, 0x00, 0x00,
        // m (109)
        0x00, 0x00, 0x00, 0x00, 0x00, 0xec, 0xfe, 0xd6, 0xd6, 0xd6, 0xd6, 0xc6, 0x00, 0x00, 0x00, 0x00,
        // n (110)
        0x00, 0x00, 0x00, 0x00, 0x00, 0xdc, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x00, 0x00, 0x00, 0x00,
        // o (111)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x7c, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0x7c, 0x00, 0x00, 0x00, 0x00,
        // p (112)
        0x00, 0x00, 0x00, 0x00, 0x00, 0xdc, 0x66, 0x66, 0x66, 0x66, 0x66, 0x7c, 0x60, 0x60, 0xf0, 0x00,
        // q (113)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x76, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0x7c, 0x0c, 0x0c, 0x1e, 0x00,
        // r (114)
        0x00, 0x00, 0x00, 0x00, 0x00, 0xdc, 0x76, 0x66, 0x60, 0x60, 0x60, 0xf0, 0x00, 0x00, 0x00, 0x00,
        // s (115)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x7c, 0xc6, 0x60, 0x38, 0x0c, 0xc6, 0x7c, 0x00, 0x00, 0x00, 0x00,
        // t (116)
        0x00, 0x00, 0x10, 0x30, 0x30, 0xfc, 0x30, 0x30, 0x30, 0x30, 0x36, 0x1c, 0x00, 0x00, 0x00, 0x00,
        // u (117)
        0x00, 0x00, 0x00, 0x00, 0x00, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0x76, 0x00, 0x00, 0x00, 0x00,
        // v (118)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3c, 0x18, 0x00, 0x00, 0x00, 0x00,
        // w (119)
        0x00, 0x00, 0x00, 0x00, 0x00, 0xc6, 0xc6, 0xd6, 0xd6, 0xd6, 0xfe, 0x6c, 0x00, 0x00, 0x00, 0x00,
        // x (120)
        0x00, 0x00, 0x00, 0x00, 0x00, 0xc6, 0x6c, 0x38, 0x38, 0x38, 0x6c, 0xc6, 0x00, 0x00, 0x00, 0x00,
        // y (121)
        0x00, 0x00, 0x00, 0x00, 0x00, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0x7e, 0x06, 0x0c, 0xf8, 0x00,
        // z (122)
        0x00, 0x00, 0x00, 0x00, 0x00, 0xfe, 0xcc, 0x18, 0x30, 0x60, 0xc6, 0xfe, 0x00, 0x00, 0x00, 0x00,
        // { (123)
        0x00, 0x00, 0x0e, 0x18, 0x18, 0x18, 0x70, 0x18, 0x18, 0x18, 0x18, 0x0e, 0x00, 0x00, 0x00, 0x00,
        // | (124)
        0x00, 0x00, 0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00,
        // } (125)
        0x00, 0x00, 0x70, 0x18, 0x18, 0x18, 0x0e, 0x18, 0x18, 0x18, 0x18, 0x70, 0x00, 0x00, 0x00, 0x00,
        // ~ (126)
        0x00, 0x00, 0x76, 0xdc, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    FONT
}

/// Create rect pipeline
fn create_rect_pipeline(
    device: &web_sys::GpuDevice,
    shader: &web_sys::GpuShaderModule,
    uniform_layout: &web_sys::GpuBindGroupLayout,
    format: web_sys::GpuTextureFormat,
) -> web_sys::GpuRenderPipeline {
    let attrs = js_sys::Array::new();
    attrs.push(&create_vertex_attr(web_sys::GpuVertexFormat::Float32x2, 0, 0));
    attrs.push(&create_vertex_attr(web_sys::GpuVertexFormat::Float32x2, 8, 1));
    attrs.push(&create_vertex_attr(web_sys::GpuVertexFormat::Float32x4, 16, 2));

    let buffer_layout = web_sys::GpuVertexBufferLayout::new(32.0, &attrs);
    buffer_layout.set_step_mode(web_sys::GpuVertexStepMode::Instance);

    let buffers = js_sys::Array::new();
    buffers.push(&buffer_layout);

    let vertex = web_sys::GpuVertexState::new(shader);
    vertex.set_entry_point("vs_main");
    vertex.set_buffers(&buffers);

    let blend_component = web_sys::GpuBlendComponent::new();
    blend_component.set_src_factor(web_sys::GpuBlendFactor::SrcAlpha);
    blend_component.set_dst_factor(web_sys::GpuBlendFactor::OneMinusSrcAlpha);

    let blend = web_sys::GpuBlendState::new(&blend_component, &blend_component);

    let target = web_sys::GpuColorTargetState::new(format);
    target.set_blend(&blend);

    let targets = js_sys::Array::new();
    targets.push(&target);

    let fragment = web_sys::GpuFragmentState::new(shader, &targets);
    fragment.set_entry_point("fs_main");

    let layouts = js_sys::Array::new();
    layouts.push(uniform_layout);

    let layout_desc = web_sys::GpuPipelineLayoutDescriptor::new(&layouts);
    let layout = device.create_pipeline_layout(&layout_desc);

    let primitive = web_sys::GpuPrimitiveState::new();
    primitive.set_topology(web_sys::GpuPrimitiveTopology::TriangleList);

    let desc = web_sys::GpuRenderPipelineDescriptor::new(&layout, &vertex);
    desc.set_fragment(&fragment);
    desc.set_primitive(&primitive);

    device.create_render_pipeline(&desc).expect("Failed to create rect pipeline")
}

/// Create glyph pipeline
fn create_glyph_pipeline(
    device: &web_sys::GpuDevice,
    shader: &web_sys::GpuShaderModule,
    uniform_layout: &web_sys::GpuBindGroupLayout,
    font_layout: &web_sys::GpuBindGroupLayout,
    format: web_sys::GpuTextureFormat,
) -> web_sys::GpuRenderPipeline {
    let attrs = js_sys::Array::new();
    attrs.push(&create_vertex_attr(web_sys::GpuVertexFormat::Float32x2, 0, 0));
    attrs.push(&create_vertex_attr(web_sys::GpuVertexFormat::Uint32, 8, 1));
    attrs.push(&create_vertex_attr(web_sys::GpuVertexFormat::Float32, 12, 2));
    attrs.push(&create_vertex_attr(web_sys::GpuVertexFormat::Float32x4, 16, 3));

    let buffer_layout = web_sys::GpuVertexBufferLayout::new(32.0, &attrs);
    buffer_layout.set_step_mode(web_sys::GpuVertexStepMode::Instance);

    let buffers = js_sys::Array::new();
    buffers.push(&buffer_layout);

    let vertex = web_sys::GpuVertexState::new(shader);
    vertex.set_entry_point("vs_main");
    vertex.set_buffers(&buffers);

    let blend_component = web_sys::GpuBlendComponent::new();
    blend_component.set_src_factor(web_sys::GpuBlendFactor::SrcAlpha);
    blend_component.set_dst_factor(web_sys::GpuBlendFactor::OneMinusSrcAlpha);

    let blend = web_sys::GpuBlendState::new(&blend_component, &blend_component);

    let target = web_sys::GpuColorTargetState::new(format);
    target.set_blend(&blend);

    let targets = js_sys::Array::new();
    targets.push(&target);

    let fragment = web_sys::GpuFragmentState::new(shader, &targets);
    fragment.set_entry_point("fs_main");

    let layouts = js_sys::Array::new();
    layouts.push(uniform_layout);
    layouts.push(font_layout);

    let layout_desc = web_sys::GpuPipelineLayoutDescriptor::new(&layouts);
    let layout = device.create_pipeline_layout(&layout_desc);

    let primitive = web_sys::GpuPrimitiveState::new();
    primitive.set_topology(web_sys::GpuPrimitiveTopology::TriangleList);

    let desc = web_sys::GpuRenderPipelineDescriptor::new(&layout, &vertex);
    desc.set_fragment(&fragment);
    desc.set_primitive(&primitive);

    device.create_render_pipeline(&desc).expect("Failed to create glyph pipeline")
}

/// Create a vertex attribute
fn create_vertex_attr(
    format: web_sys::GpuVertexFormat,
    offset: u32,
    shader_location: u32,
) -> web_sys::GpuVertexAttribute {
    web_sys::GpuVertexAttribute::new(format, offset as f64, shader_location)
}
