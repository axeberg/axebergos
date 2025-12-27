//! WebGPU Surface for rendering
//!
//! Provides GPU-accelerated rendering using WebGPU.
//! The surface manages:
//! - GPU device and queue
//! - Render pipeline for drawing rectangles
//! - Vertex/index buffers for geometry
//! - Canvas context for presenting frames

use super::geometry::{Color, Rect};
use js_sys::{Array, Float32Array, Object, Reflect, Uint16Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    GpuAdapter, GpuBindGroup, GpuBuffer, GpuCanvasContext, GpuDevice, GpuQueue, GpuRenderPipeline,
    GpuTextureFormat, HtmlCanvasElement,
};

/// Maximum number of rectangles we can render in a single frame
const MAX_RECTS: usize = 1024;

/// Vertex data: position (2) + color (4) = 6 floats per vertex
/// 4 vertices per rectangle
const FLOATS_PER_VERTEX: usize = 6;
const VERTICES_PER_RECT: usize = 4;
const FLOATS_PER_RECT: usize = FLOATS_PER_VERTEX * VERTICES_PER_RECT;

// Buffer usage flags (from WebGPU spec)
const GPU_BUFFER_USAGE_VERTEX: u32 = 0x0020;
const GPU_BUFFER_USAGE_INDEX: u32 = 0x0010;
const GPU_BUFFER_USAGE_UNIFORM: u32 = 0x0040;
const GPU_BUFFER_USAGE_COPY_DST: u32 = 0x0008;

/// A rectangle to be rendered
#[derive(Debug, Clone, Copy)]
pub struct RenderRect {
    pub rect: Rect,
    pub color: Color,
}

impl RenderRect {
    pub fn new(rect: Rect, color: Color) -> Self {
        Self { rect, color }
    }
}

/// WebGPU rendering surface
pub struct Surface {
    canvas: HtmlCanvasElement,
    context: GpuCanvasContext,
    device: GpuDevice,
    queue: GpuQueue,
    pipeline: GpuRenderPipeline,
    vertex_buffer: GpuBuffer,
    index_buffer: GpuBuffer,
    uniform_buffer: GpuBuffer,
    bind_group: GpuBindGroup,
    format: GpuTextureFormat,
    width: u32,
    height: u32,
    /// Pending rectangles to render
    rects: Vec<RenderRect>,
}

impl Surface {
    /// Create a surface from a canvas element ID
    pub async fn from_canvas_id(id: &str) -> Result<Self, String> {
        let window = web_sys::window().ok_or("no window")?;
        let document = window.document().ok_or("no document")?;
        let canvas = document
            .get_element_by_id(id)
            .ok_or_else(|| format!("no element with id '{}'", id))?
            .dyn_into::<HtmlCanvasElement>()
            .map_err(|_| "element is not a canvas")?;

        Self::from_canvas(canvas).await
    }

    /// Create a surface from a canvas element
    pub async fn from_canvas(canvas: HtmlCanvasElement) -> Result<Self, String> {
        // Get WebGPU
        let gpu = get_gpu()?;

        // Request adapter
        let adapter = request_adapter(&gpu).await?;

        // Request device
        let device = request_device(&adapter).await?;
        let queue = device.queue();

        // Configure canvas context
        let context = canvas
            .get_context("webgpu")
            .map_err(|e| format!("failed to get webgpu context: {:?}", e))?
            .ok_or("no webgpu context")?
            .dyn_into::<GpuCanvasContext>()
            .map_err(|_| "context is not GpuCanvasContext")?;

        let format = gpu.get_preferred_canvas_format();
        configure_context(&context, &device, &format, canvas.width(), canvas.height());

        // Create shader module
        let shader = create_shader_module(&device)?;

        // Create pipeline
        let pipeline = create_render_pipeline(&device, &shader, &format)?;

        // Create buffers
        let vertex_buffer = create_vertex_buffer(&device)?;
        let index_buffer = create_index_buffer(&device, &queue)?;
        let uniform_buffer = create_uniform_buffer(&device)?;

        // Create bind group
        let bind_group = create_bind_group(&device, &pipeline, &uniform_buffer)?;

        Ok(Self {
            width: canvas.width(),
            height: canvas.height(),
            canvas,
            context,
            device,
            queue,
            pipeline,
            vertex_buffer,
            index_buffer,
            uniform_buffer,
            bind_group,
            format,
            rects: Vec::with_capacity(MAX_RECTS),
        })
    }

    /// Get the canvas width
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Get the canvas height
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Resize the surface
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.canvas.set_width(width);
        self.canvas.set_height(height);

        // Reconfigure context
        configure_context(&self.context, &self.device, &self.format, width, height);

        // Update uniforms
        self.update_uniforms();
    }

    /// Update the uniform buffer with current dimensions
    fn update_uniforms(&self) {
        let uniforms = [self.width as f32, self.height as f32, 0.0, 0.0];
        let data = Float32Array::from(uniforms.as_slice());
        let _ =
            self.queue
                .write_buffer_with_f64_and_buffer_source(&self.uniform_buffer, 0.0, &data);
    }

    /// Clear pending rectangles
    pub fn clear(&mut self) {
        self.rects.clear();
    }

    /// Queue a rectangle for rendering
    pub fn draw_rect(&mut self, rect: Rect, color: Color) {
        if self.rects.len() < MAX_RECTS {
            self.rects.push(RenderRect::new(rect, color));
        }
    }

    /// Draw a filled rectangle with border
    pub fn draw_rect_with_border(
        &mut self,
        rect: Rect,
        fill_color: Color,
        border_color: Color,
        border_width: f64,
    ) {
        // Draw border (as 4 thin rectangles)
        let bw = border_width;

        // Top border
        self.draw_rect(Rect::new(rect.x, rect.y, rect.width, bw), border_color);
        // Bottom border
        self.draw_rect(
            Rect::new(rect.x, rect.y + rect.height - bw, rect.width, bw),
            border_color,
        );
        // Left border
        self.draw_rect(
            Rect::new(rect.x, rect.y + bw, bw, rect.height - 2.0 * bw),
            border_color,
        );
        // Right border
        self.draw_rect(
            Rect::new(
                rect.x + rect.width - bw,
                rect.y + bw,
                bw,
                rect.height - 2.0 * bw,
            ),
            border_color,
        );

        // Draw fill (inside border)
        let inner = rect.inset(bw);
        if !inner.is_empty() {
            self.draw_rect(inner, fill_color);
        }
    }

    /// Render all queued rectangles
    pub fn render(&mut self, clear_color: Color) {
        if self.rects.is_empty() && clear_color.a == 0.0 {
            return;
        }

        // Update uniforms
        self.update_uniforms();

        // Update vertex buffer
        let vertex_data = self.build_vertex_data();
        if !vertex_data.is_empty() {
            let data = Float32Array::from(vertex_data.as_slice());
            let _ =
                self.queue
                    .write_buffer_with_f64_and_buffer_source(&self.vertex_buffer, 0.0, &data);
        }

        // Get current texture
        let texture = match self.context.get_current_texture() {
            Ok(t) => t,
            Err(_) => return, // Skip frame if texture unavailable
        };
        let view = match texture.create_view() {
            Ok(v) => v,
            Err(_) => return, // Skip frame if view unavailable
        };

        // Create command encoder
        let encoder = self.device.create_command_encoder();

        // Begin render pass
        let color_attachment = create_color_attachment(&view, clear_color);
        let render_pass_desc = create_render_pass_descriptor(&color_attachment);
        let pass = match encoder.begin_render_pass(&render_pass_desc) {
            Ok(p) => p,
            Err(_) => return, // Skip frame on error
        };

        // Draw rectangles
        if !self.rects.is_empty() {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, Some(&self.bind_group));
            pass.set_vertex_buffer(0, Some(&self.vertex_buffer));
            pass.set_index_buffer(&self.index_buffer, web_sys::GpuIndexFormat::Uint16);

            let index_count = (self.rects.len() * 6) as u32;
            pass.draw_indexed(index_count);
        }

        pass.end();

        // Submit commands
        let command_buffer = encoder.finish();
        let commands = Array::of1(&command_buffer);
        self.queue.submit(&commands);
    }

    /// Build vertex data from queued rectangles
    fn build_vertex_data(&self) -> Vec<f32> {
        let mut data = Vec::with_capacity(self.rects.len() * FLOATS_PER_RECT);

        for render_rect in &self.rects {
            let rect = &render_rect.rect;
            let [r, g, b, a] = render_rect.color.to_array();

            // Convert to normalized device coordinates (-1 to 1)
            let x0 = (rect.x as f32 / self.width as f32) * 2.0 - 1.0;
            let y0 = 1.0 - (rect.y as f32 / self.height as f32) * 2.0;
            let x1 = ((rect.x + rect.width) as f32 / self.width as f32) * 2.0 - 1.0;
            let y1 = 1.0 - ((rect.y + rect.height) as f32 / self.height as f32) * 2.0;

            // 4 vertices: top-left, top-right, bottom-right, bottom-left
            // Top-left
            data.extend_from_slice(&[x0, y0, r, g, b, a]);
            // Top-right
            data.extend_from_slice(&[x1, y0, r, g, b, a]);
            // Bottom-right
            data.extend_from_slice(&[x1, y1, r, g, b, a]);
            // Bottom-left
            data.extend_from_slice(&[x0, y1, r, g, b, a]);
        }

        data
    }
}

// === Helper functions ===

fn get_gpu() -> Result<web_sys::Gpu, String> {
    let window = web_sys::window().ok_or("no window")?;
    let navigator = window.navigator();

    // Check if WebGPU is available
    let gpu = navigator.gpu();
    if gpu.is_undefined() {
        return Err("WebGPU is not supported in this browser".to_string());
    }

    Ok(gpu)
}

async fn request_adapter(gpu: &web_sys::Gpu) -> Result<GpuAdapter, String> {
    let options = web_sys::GpuRequestAdapterOptions::new();
    options.set_power_preference(web_sys::GpuPowerPreference::HighPerformance);

    let promise = gpu.request_adapter_with_options(&options);
    let result = JsFuture::from(promise)
        .await
        .map_err(|e| format!("failed to request adapter: {:?}", e))?;

    if result.is_null() || result.is_undefined() {
        return Err("no GPU adapter available".to_string());
    }

    result
        .dyn_into::<GpuAdapter>()
        .map_err(|_| "result is not GpuAdapter".to_string())
}

async fn request_device(adapter: &GpuAdapter) -> Result<GpuDevice, String> {
    let descriptor = web_sys::GpuDeviceDescriptor::new();

    let promise = adapter.request_device_with_descriptor(&descriptor);
    let result = JsFuture::from(promise)
        .await
        .map_err(|e| format!("failed to request device: {:?}", e))?;

    result
        .dyn_into::<GpuDevice>()
        .map_err(|_| "result is not GpuDevice".to_string())
}

fn configure_context(
    context: &GpuCanvasContext,
    device: &GpuDevice,
    format: &GpuTextureFormat,
    width: u32,
    height: u32,
) {
    let config = web_sys::GpuCanvasConfiguration::new(device, *format);
    // Set alpha mode to premultiplied for proper transparency
    config.set_alpha_mode(web_sys::GpuCanvasAlphaMode::Premultiplied);
    let _ = context.configure(&config);

    // Update canvas size
    if let Ok(canvas) = context.canvas().dyn_into::<HtmlCanvasElement>() {
        canvas.set_width(width);
        canvas.set_height(height);
    }
}

fn create_shader_module(device: &GpuDevice) -> Result<web_sys::GpuShaderModule, String> {
    let shader_source = include_str!("shaders/rect.wgsl");
    let descriptor = web_sys::GpuShaderModuleDescriptor::new(shader_source);
    Ok(device.create_shader_module(&descriptor))
}

fn create_render_pipeline(
    device: &GpuDevice,
    shader: &web_sys::GpuShaderModule,
    format: &GpuTextureFormat,
) -> Result<GpuRenderPipeline, String> {
    // Vertex state
    let vertex_attributes = Array::new();

    // Position attribute
    let pos_attr = Object::new();
    Reflect::set(&pos_attr, &"format".into(), &"float32x2".into()).unwrap();
    Reflect::set(&pos_attr, &"offset".into(), &0.into()).unwrap();
    Reflect::set(&pos_attr, &"shaderLocation".into(), &0.into()).unwrap();
    vertex_attributes.push(&pos_attr);

    // Color attribute
    let color_attr = Object::new();
    Reflect::set(&color_attr, &"format".into(), &"float32x4".into()).unwrap();
    Reflect::set(&color_attr, &"offset".into(), &8.into()).unwrap(); // 2 floats * 4 bytes
    Reflect::set(&color_attr, &"shaderLocation".into(), &1.into()).unwrap();
    vertex_attributes.push(&color_attr);

    let vertex_buffer_layout = Object::new();
    Reflect::set(&vertex_buffer_layout, &"arrayStride".into(), &24.into()).unwrap(); // 6 floats * 4 bytes
    Reflect::set(&vertex_buffer_layout, &"stepMode".into(), &"vertex".into()).unwrap();
    Reflect::set(
        &vertex_buffer_layout,
        &"attributes".into(),
        &vertex_attributes,
    )
    .unwrap();

    let vertex_buffers = Array::of1(&vertex_buffer_layout);

    let vertex_state = Object::new();
    Reflect::set(&vertex_state, &"module".into(), shader).unwrap();
    Reflect::set(&vertex_state, &"entryPoint".into(), &"vs_main".into()).unwrap();
    Reflect::set(&vertex_state, &"buffers".into(), &vertex_buffers).unwrap();

    // Fragment state
    let blend_component = Object::new();
    Reflect::set(&blend_component, &"srcFactor".into(), &"src-alpha".into()).unwrap();
    Reflect::set(
        &blend_component,
        &"dstFactor".into(),
        &"one-minus-src-alpha".into(),
    )
    .unwrap();
    Reflect::set(&blend_component, &"operation".into(), &"add".into()).unwrap();

    let blend = Object::new();
    Reflect::set(&blend, &"color".into(), &blend_component).unwrap();
    Reflect::set(&blend, &"alpha".into(), &blend_component).unwrap();

    let color_target = Object::new();
    Reflect::set(&color_target, &"format".into(), &JsValue::from(*format)).unwrap();
    Reflect::set(&color_target, &"blend".into(), &blend).unwrap();

    let color_targets = Array::of1(&color_target);

    let fragment_state = Object::new();
    Reflect::set(&fragment_state, &"module".into(), shader).unwrap();
    Reflect::set(&fragment_state, &"entryPoint".into(), &"fs_main".into()).unwrap();
    Reflect::set(&fragment_state, &"targets".into(), &color_targets).unwrap();

    // Primitive state
    let primitive_state = Object::new();
    Reflect::set(
        &primitive_state,
        &"topology".into(),
        &"triangle-list".into(),
    )
    .unwrap();

    // Pipeline descriptor
    let pipeline_desc = Object::new();
    Reflect::set(&pipeline_desc, &"vertex".into(), &vertex_state).unwrap();
    Reflect::set(&pipeline_desc, &"fragment".into(), &fragment_state).unwrap();
    Reflect::set(&pipeline_desc, &"primitive".into(), &primitive_state).unwrap();
    Reflect::set(&pipeline_desc, &"layout".into(), &"auto".into()).unwrap();

    let pipeline_desc: web_sys::GpuRenderPipelineDescriptor = pipeline_desc.unchecked_into();
    device
        .create_render_pipeline(&pipeline_desc)
        .map_err(|e| format!("failed to create render pipeline: {:?}", e))
}

fn create_vertex_buffer(device: &GpuDevice) -> Result<GpuBuffer, String> {
    let size = (MAX_RECTS * FLOATS_PER_RECT * 4) as f64; // 4 bytes per float

    let descriptor = web_sys::GpuBufferDescriptor::new(
        size,
        GPU_BUFFER_USAGE_VERTEX | GPU_BUFFER_USAGE_COPY_DST,
    );

    device
        .create_buffer(&descriptor)
        .map_err(|e| format!("failed to create vertex buffer: {:?}", e))
}

fn create_index_buffer(device: &GpuDevice, queue: &GpuQueue) -> Result<GpuBuffer, String> {
    // 6 indices per rectangle (2 triangles)
    let mut indices: Vec<u16> = Vec::with_capacity(MAX_RECTS * 6);

    for i in 0..MAX_RECTS {
        let base = (i * 4) as u16;
        // First triangle: 0, 1, 2
        indices.push(base);
        indices.push(base + 1);
        indices.push(base + 2);
        // Second triangle: 0, 2, 3
        indices.push(base);
        indices.push(base + 2);
        indices.push(base + 3);
    }

    let size = (indices.len() * 2) as f64; // 2 bytes per u16
    let descriptor =
        web_sys::GpuBufferDescriptor::new(size, GPU_BUFFER_USAGE_INDEX | GPU_BUFFER_USAGE_COPY_DST);

    let buffer = device
        .create_buffer(&descriptor)
        .map_err(|e| format!("failed to create index buffer: {:?}", e))?;

    // Upload index data
    let data = Uint16Array::from(indices.as_slice());
    let _ = queue.write_buffer_with_f64_and_buffer_source(&buffer, 0.0, &data);

    Ok(buffer)
}

fn create_uniform_buffer(device: &GpuDevice) -> Result<GpuBuffer, String> {
    // Screen dimensions: width, height, padding (16-byte aligned)
    let size = 16.0;

    let descriptor = web_sys::GpuBufferDescriptor::new(
        size,
        GPU_BUFFER_USAGE_UNIFORM | GPU_BUFFER_USAGE_COPY_DST,
    );

    device
        .create_buffer(&descriptor)
        .map_err(|e| format!("failed to create uniform buffer: {:?}", e))
}

fn create_bind_group(
    device: &GpuDevice,
    pipeline: &GpuRenderPipeline,
    uniform_buffer: &GpuBuffer,
) -> Result<GpuBindGroup, String> {
    let layout = pipeline.get_bind_group_layout(0);

    let buffer_binding = Object::new();
    Reflect::set(&buffer_binding, &"buffer".into(), uniform_buffer).unwrap();

    let entry = Object::new();
    Reflect::set(&entry, &"binding".into(), &0.into()).unwrap();
    Reflect::set(&entry, &"resource".into(), &buffer_binding).unwrap();

    let entries = Array::of1(&entry);

    let descriptor = Object::new();
    Reflect::set(&descriptor, &"layout".into(), &layout).unwrap();
    Reflect::set(&descriptor, &"entries".into(), &entries).unwrap();

    let descriptor: web_sys::GpuBindGroupDescriptor = descriptor.unchecked_into();
    Ok(device.create_bind_group(&descriptor))
}

fn create_color_attachment(
    view: &web_sys::GpuTextureView,
    clear_color: Color,
) -> web_sys::GpuRenderPassColorAttachment {
    let clear_value = Object::new();
    Reflect::set(&clear_value, &"r".into(), &(clear_color.r as f64).into()).unwrap();
    Reflect::set(&clear_value, &"g".into(), &(clear_color.g as f64).into()).unwrap();
    Reflect::set(&clear_value, &"b".into(), &(clear_color.b as f64).into()).unwrap();
    Reflect::set(&clear_value, &"a".into(), &(clear_color.a as f64).into()).unwrap();

    let attachment = web_sys::GpuRenderPassColorAttachment::new(
        web_sys::GpuLoadOp::Clear,
        web_sys::GpuStoreOp::Store,
        view,
    );
    attachment.set_clear_value(&clear_value.into());

    attachment
}

fn create_render_pass_descriptor(
    color_attachment: &web_sys::GpuRenderPassColorAttachment,
) -> web_sys::GpuRenderPassDescriptor {
    let color_attachments = Array::of1(color_attachment);
    web_sys::GpuRenderPassDescriptor::new(&color_attachments)
}
