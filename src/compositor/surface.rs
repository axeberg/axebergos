//! WebGPU rendering surface using wgpu
//!
//! Uses wgpu for cross-platform WebGPU abstraction.
//! On WASM, this uses WebGPU via wgpu's web backend.

use super::{Color, Rect};
use wasm_bindgen::JsCast;

/// A frame being rendered
pub struct Frame {
    /// Surface texture for this frame
    texture: wgpu::SurfaceTexture,
    /// Texture view for rendering
    view: wgpu::TextureView,
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

/// The wgpu rendering surface
pub struct Surface {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    canvas: web_sys::HtmlCanvasElement,

    /// Physical pixel width (CSS width * DPR)
    width: u32,
    /// Physical pixel height (CSS height * DPR)
    height: u32,
    /// Device pixel ratio for high-DPI displays
    device_pixel_ratio: f64,

    // Pipelines
    rect_pipeline: wgpu::RenderPipeline,
    glyph_pipeline: wgpu::RenderPipeline,

    // Buffers
    rect_buffer: wgpu::Buffer,
    glyph_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,

    // Bind groups
    uniform_bind_group: wgpu::BindGroup,
    font_bind_group: wgpu::BindGroup,

    // Max instances
    max_rects: usize,
    max_glyphs: usize,
}

impl Surface {
    /// Create a new wgpu surface
    ///
    /// width/height are CSS pixel dimensions. They will be scaled by device pixel ratio
    /// for high-DPI displays.
    pub async fn new(width: u32, height: u32) -> Result<Self, String> {
        // Get device pixel ratio
        let window = web_sys::window().ok_or("No window")?;
        let device_pixel_ratio = window.device_pixel_ratio();
        let physical_width = ((width as f64) * device_pixel_ratio).round() as u32;
        let physical_height = ((height as f64) * device_pixel_ratio).round() as u32;

        web_sys::console::log_1(
            &format!(
                "[surface] DPR: {}, CSS: {}x{}, Physical: {}x{}",
                device_pixel_ratio, width, height, physical_width, physical_height
            )
            .into(),
        );

        // Create canvas
        let document = window.document().ok_or("No document")?;
        let canvas = document
            .create_element("canvas")
            .map_err(|_| "Failed to create canvas")?
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .map_err(|_| "Not a canvas")?;

        canvas.set_width(physical_width);
        canvas.set_height(physical_height);
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

        // Create wgpu instance
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
            ..Default::default()
        });

        // Create surface from canvas using SurfaceTarget::Canvas (web-only variant)
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|e| format!("Failed to create surface: {:?}", e))?;

        // Request adapter
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| format!("Failed to get adapter: {:?}", e))?;

        web_sys::console::log_1(&format!("[surface] Adapter: {:?}", adapter.get_info()).into());

        // Request device
        let (device, queue): (wgpu::Device, wgpu::Queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| format!("Failed to get device: {:?}", e))?;

        // Configure surface
        let surface_caps = surface.get_capabilities(&adapter);
        let format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: physical_width,
            height: physical_height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Create shaders
        let rect_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Rect Shader"),
            source: wgpu::ShaderSource::Wgsl(RECT_SHADER.into()),
        });

        let glyph_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Glyph Shader"),
            source: wgpu::ShaderSource::Wgsl(GLYPH_SHADER.into()),
        });

        // Create uniform buffer
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: 8,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Upload initial uniform data
        queue.write_buffer(
            &uniform_buffer,
            0,
            bytemuck::cast_slice(&[physical_width as f32, physical_height as f32]),
        );

        // Create uniform bind group layout
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Uniform Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform Bind Group"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Create font texture
        let font_data = generate_font_bitmap();
        let font_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Font Texture"),
            size: wgpu::Extent3d {
                width: 128,
                height: 256,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &font_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &font_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(128),
                rows_per_image: Some(256),
            },
            wgpu::Extent3d {
                width: 128,
                height: 256,
                depth_or_array_layers: 1,
            },
        );

        let font_view = font_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let font_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create font bind group layout
        let font_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Font Layout"),
            entries: &[
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
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let font_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Font Bind Group"),
            layout: &font_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&font_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&font_sampler),
                },
            ],
        });

        // Create pipelines
        let rect_pipeline = create_rect_pipeline(&device, &rect_shader, &uniform_layout, format);
        let glyph_pipeline =
            create_glyph_pipeline(&device, &glyph_shader, &uniform_layout, &font_layout, format);

        // Create instance buffers
        let max_rects = 1024;
        let max_glyphs = 16384;

        let rect_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Rect Buffer"),
            size: (max_rects * std::mem::size_of::<RectInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let glyph_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Glyph Buffer"),
            size: (max_glyphs * std::mem::size_of::<GlyphInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        web_sys::console::log_1(
            &format!(
                "[surface] wgpu surface initialized: {}x{} (physical), DPR: {}",
                physical_width, physical_height, device_pixel_ratio
            )
            .into(),
        );

        Ok(Self {
            device,
            queue,
            surface,
            config,
            canvas,
            width: physical_width,
            height: physical_height,
            device_pixel_ratio,
            rect_pipeline,
            glyph_pipeline,
            rect_buffer,
            glyph_buffer,
            uniform_buffer,
            uniform_bind_group,
            font_bind_group,
            max_rects,
            max_glyphs,
        })
    }

    /// Resize the surface
    pub fn resize(&mut self, width: u32, height: u32) {
        if let Some(window) = web_sys::window() {
            self.device_pixel_ratio = window.device_pixel_ratio();
        }

        let physical_width = ((width as f64) * self.device_pixel_ratio).round() as u32;
        let physical_height = ((height as f64) * self.device_pixel_ratio).round() as u32;

        if physical_width == self.width && physical_height == self.height {
            return;
        }

        self.width = physical_width;
        self.height = physical_height;
        self.canvas.set_width(physical_width);
        self.canvas.set_height(physical_height);

        self.config.width = physical_width;
        self.config.height = physical_height;
        self.surface.configure(&self.device, &self.config);

        // Update uniform buffer
        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[physical_width as f32, physical_height as f32]),
        );
    }

    /// Begin a new frame
    pub fn begin_frame(&mut self) -> Option<Frame> {
        let texture = self.surface.get_current_texture().ok()?;
        let view = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        Some(Frame {
            texture,
            view,
            rects: Vec::new(),
            glyphs: Vec::new(),
        })
    }

    /// Clear the screen (handled in end_frame)
    pub fn clear(&mut self, _frame: &Frame, _color: Color) {
        // Clear is done via the load operation in the render pass
    }

    /// Draw a filled rectangle (coordinates in CSS pixels)
    pub fn draw_rect(&mut self, frame: &mut Frame, rect: Rect, color: Color) {
        if frame.rects.len() >= self.max_rects {
            return;
        }

        let dpr = self.device_pixel_ratio as f32;
        frame.rects.push(RectInstance {
            pos: [rect.x * dpr, rect.y * dpr],
            size: [rect.width * dpr, rect.height * dpr],
            color: [color.r, color.g, color.b, color.a],
        });
    }

    /// Draw text (coordinates and font_size in CSS pixels)
    pub fn draw_text(
        &mut self,
        frame: &mut Frame,
        text: &str,
        x: f32,
        y: f32,
        color: Color,
        font_size: f32,
    ) {
        let dpr = self.device_pixel_ratio as f32;
        let physical_font_size = font_size * dpr;
        let scale = physical_font_size / 16.0;
        let glyph_width = 8.0 * scale;
        let physical_x = x * dpr;
        let physical_y = y * dpr;

        for (i, c) in text.chars().enumerate() {
            if frame.glyphs.len() >= self.max_glyphs {
                break;
            }

            let glyph_index = c as u32;
            if glyph_index > 255 {
                continue;
            }

            frame.glyphs.push(GlyphInstance {
                pos: [physical_x + (i as f32 * glyph_width), physical_y],
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
        self.draw_rect(
            frame,
            Rect::new(rect.x, rect.y, rect.width, line_width),
            color,
        );
        self.draw_rect(
            frame,
            Rect::new(
                rect.x,
                rect.y + rect.height - line_width,
                rect.width,
                line_width,
            ),
            color,
        );
        self.draw_rect(
            frame,
            Rect::new(rect.x, rect.y, line_width, rect.height),
            color,
        );
        self.draw_rect(
            frame,
            Rect::new(
                rect.x + rect.width - line_width,
                rect.y,
                line_width,
                rect.height,
            ),
            color,
        );
    }

    /// End the frame and submit commands
    pub fn end_frame(&mut self, frame: Frame) {
        // Upload instance data
        if !frame.rects.is_empty() {
            self.queue
                .write_buffer(&self.rect_buffer, 0, bytemuck::cast_slice(&frame.rects));
        }

        if !frame.glyphs.is_empty() {
            self.queue
                .write_buffer(&self.glyph_buffer, 0, bytemuck::cast_slice(&frame.glyphs));
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &frame.view,
                    resolve_target: None,
                    depth_slice: None,
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
                multiview_mask: None,
            });

            // Draw rectangles
            if !frame.rects.is_empty() {
                render_pass.set_pipeline(&self.rect_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.rect_buffer.slice(..));
                render_pass.draw(0..6, 0..frame.rects.len() as u32);
            }

            // Draw glyphs
            if !frame.glyphs.is_empty() {
                render_pass.set_pipeline(&self.glyph_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_bind_group(1, &self.font_bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.glyph_buffer.slice(..));
                render_pass.draw(0..6, 0..frame.glyphs.len() as u32);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.texture.present();
    }

    /// Get dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// Rectangle shader
const RECT_SHADER: &str = r#"
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

/// Glyph shader
const GLYPH_SHADER: &str = r#"
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

/// Create rect pipeline
fn create_rect_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    uniform_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Rect Pipeline Layout"),
        bind_group_layouts: &[uniform_layout],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Rect Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: 32,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 8,
                        shader_location: 1,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: 16,
                        shader_location: 2,
                    },
                ],
            }],
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Create glyph pipeline
fn create_glyph_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    uniform_layout: &wgpu::BindGroupLayout,
    font_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Glyph Pipeline Layout"),
        bind_group_layouts: &[uniform_layout, font_layout],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Glyph Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: 32,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Uint32,
                        offset: 8,
                        shader_location: 1,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32,
                        offset: 12,
                        shader_location: 2,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: 16,
                        shader_location: 3,
                    },
                ],
            }],
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        multiview_mask: None,
        cache: None,
    })
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
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, // ! (33)
        0x00, 0x00, 0x18, 0x3c, 0x3c, 0x3c, 0x18, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00,
        0x00, // " (34)
        0x00, 0x66, 0x66, 0x66, 0x24, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, // # (35)
        0x00, 0x00, 0x00, 0x6c, 0x6c, 0xfe, 0x6c, 0x6c, 0x6c, 0xfe, 0x6c, 0x6c, 0x00, 0x00, 0x00,
        0x00, // $ (36)
        0x18, 0x18, 0x7c, 0xc6, 0xc2, 0xc0, 0x7c, 0x06, 0x06, 0x86, 0xc6, 0x7c, 0x18, 0x18, 0x00,
        0x00, // % (37)
        0x00, 0x00, 0x00, 0x00, 0xc2, 0xc6, 0x0c, 0x18, 0x30, 0x60, 0xc6, 0x86, 0x00, 0x00, 0x00,
        0x00, // & (38)
        0x00, 0x00, 0x38, 0x6c, 0x6c, 0x38, 0x76, 0xdc, 0xcc, 0xcc, 0xcc, 0x76, 0x00, 0x00, 0x00,
        0x00, // ' (39)
        0x00, 0x30, 0x30, 0x30, 0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, // ( (40)
        0x00, 0x00, 0x0c, 0x18, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x18, 0x0c, 0x00, 0x00, 0x00,
        0x00, // ) (41)
        0x00, 0x00, 0x30, 0x18, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x18, 0x30, 0x00, 0x00, 0x00,
        0x00, // * (42)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x66, 0x3c, 0xff, 0x3c, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, // + (43)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x7e, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, // , (44)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x18, 0x30, 0x00, 0x00,
        0x00, // - (45)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xfe, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, // . (46)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00,
        0x00, // / (47)
        0x00, 0x00, 0x00, 0x00, 0x02, 0x06, 0x0c, 0x18, 0x30, 0x60, 0xc0, 0x80, 0x00, 0x00, 0x00,
        0x00, // 0 (48)
        0x00, 0x00, 0x7c, 0xc6, 0xc6, 0xce, 0xde, 0xf6, 0xe6, 0xc6, 0xc6, 0x7c, 0x00, 0x00, 0x00,
        0x00, // 1 (49)
        0x00, 0x00, 0x18, 0x38, 0x78, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x7e, 0x00, 0x00, 0x00,
        0x00, // 2 (50)
        0x00, 0x00, 0x7c, 0xc6, 0x06, 0x0c, 0x18, 0x30, 0x60, 0xc0, 0xc6, 0xfe, 0x00, 0x00, 0x00,
        0x00, // 3 (51)
        0x00, 0x00, 0x7c, 0xc6, 0x06, 0x06, 0x3c, 0x06, 0x06, 0x06, 0xc6, 0x7c, 0x00, 0x00, 0x00,
        0x00, // 4 (52)
        0x00, 0x00, 0x0c, 0x1c, 0x3c, 0x6c, 0xcc, 0xfe, 0x0c, 0x0c, 0x0c, 0x1e, 0x00, 0x00, 0x00,
        0x00, // 5 (53)
        0x00, 0x00, 0xfe, 0xc0, 0xc0, 0xc0, 0xfc, 0x06, 0x06, 0x06, 0xc6, 0x7c, 0x00, 0x00, 0x00,
        0x00, // 6 (54)
        0x00, 0x00, 0x38, 0x60, 0xc0, 0xc0, 0xfc, 0xc6, 0xc6, 0xc6, 0xc6, 0x7c, 0x00, 0x00, 0x00,
        0x00, // 7 (55)
        0x00, 0x00, 0xfe, 0xc6, 0x06, 0x06, 0x0c, 0x18, 0x30, 0x30, 0x30, 0x30, 0x00, 0x00, 0x00,
        0x00, // 8 (56)
        0x00, 0x00, 0x7c, 0xc6, 0xc6, 0xc6, 0x7c, 0xc6, 0xc6, 0xc6, 0xc6, 0x7c, 0x00, 0x00, 0x00,
        0x00, // 9 (57)
        0x00, 0x00, 0x7c, 0xc6, 0xc6, 0xc6, 0x7e, 0x06, 0x06, 0x06, 0x0c, 0x78, 0x00, 0x00, 0x00,
        0x00, // : (58)
        0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00,
        0x00, // ; (59)
        0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00, 0x18, 0x18, 0x30, 0x00, 0x00, 0x00,
        0x00, // < (60)
        0x00, 0x00, 0x00, 0x06, 0x0c, 0x18, 0x30, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x00, 0x00, 0x00,
        0x00, // = (61)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x7e, 0x00, 0x00, 0x7e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, // > (62)
        0x00, 0x00, 0x00, 0x60, 0x30, 0x18, 0x0c, 0x06, 0x0c, 0x18, 0x30, 0x60, 0x00, 0x00, 0x00,
        0x00, // ? (63)
        0x00, 0x00, 0x7c, 0xc6, 0xc6, 0x0c, 0x18, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00, 0x00,
        0x00, // @ (64)
        0x00, 0x00, 0x7c, 0xc6, 0xc6, 0xc6, 0xde, 0xde, 0xde, 0xdc, 0xc0, 0x7c, 0x00, 0x00, 0x00,
        0x00, // A (65)
        0x00, 0x00, 0x10, 0x38, 0x6c, 0xc6, 0xc6, 0xfe, 0xc6, 0xc6, 0xc6, 0xc6, 0x00, 0x00, 0x00,
        0x00, // B (66)
        0x00, 0x00, 0xfc, 0x66, 0x66, 0x66, 0x7c, 0x66, 0x66, 0x66, 0x66, 0xfc, 0x00, 0x00, 0x00,
        0x00, // C (67)
        0x00, 0x00, 0x3c, 0x66, 0xc2, 0xc0, 0xc0, 0xc0, 0xc0, 0xc2, 0x66, 0x3c, 0x00, 0x00, 0x00,
        0x00, // D (68)
        0x00, 0x00, 0xf8, 0x6c, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x6c, 0xf8, 0x00, 0x00, 0x00,
        0x00, // E (69)
        0x00, 0x00, 0xfe, 0x66, 0x62, 0x68, 0x78, 0x68, 0x60, 0x62, 0x66, 0xfe, 0x00, 0x00, 0x00,
        0x00, // F (70)
        0x00, 0x00, 0xfe, 0x66, 0x62, 0x68, 0x78, 0x68, 0x60, 0x60, 0x60, 0xf0, 0x00, 0x00, 0x00,
        0x00, // G (71)
        0x00, 0x00, 0x3c, 0x66, 0xc2, 0xc0, 0xc0, 0xde, 0xc6, 0xc6, 0x66, 0x3a, 0x00, 0x00, 0x00,
        0x00, // H (72)
        0x00, 0x00, 0xc6, 0xc6, 0xc6, 0xc6, 0xfe, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0x00, 0x00, 0x00,
        0x00, // I (73)
        0x00, 0x00, 0x3c, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3c, 0x00, 0x00, 0x00,
        0x00, // J (74)
        0x00, 0x00, 0x1e, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0xcc, 0xcc, 0xcc, 0x78, 0x00, 0x00, 0x00,
        0x00, // K (75)
        0x00, 0x00, 0xe6, 0x66, 0x66, 0x6c, 0x78, 0x78, 0x6c, 0x66, 0x66, 0xe6, 0x00, 0x00, 0x00,
        0x00, // L (76)
        0x00, 0x00, 0xf0, 0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x62, 0x66, 0xfe, 0x00, 0x00, 0x00,
        0x00, // M (77)
        0x00, 0x00, 0xc6, 0xee, 0xfe, 0xfe, 0xd6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0x00, 0x00, 0x00,
        0x00, // N (78)
        0x00, 0x00, 0xc6, 0xe6, 0xf6, 0xfe, 0xde, 0xce, 0xc6, 0xc6, 0xc6, 0xc6, 0x00, 0x00, 0x00,
        0x00, // O (79)
        0x00, 0x00, 0x7c, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0x7c, 0x00, 0x00, 0x00,
        0x00, // P (80)
        0x00, 0x00, 0xfc, 0x66, 0x66, 0x66, 0x7c, 0x60, 0x60, 0x60, 0x60, 0xf0, 0x00, 0x00, 0x00,
        0x00, // Q (81)
        0x00, 0x00, 0x7c, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xd6, 0xde, 0x7c, 0x0c, 0x0e, 0x00,
        0x00, // R (82)
        0x00, 0x00, 0xfc, 0x66, 0x66, 0x66, 0x7c, 0x6c, 0x66, 0x66, 0x66, 0xe6, 0x00, 0x00, 0x00,
        0x00, // S (83)
        0x00, 0x00, 0x7c, 0xc6, 0xc6, 0x60, 0x38, 0x0c, 0x06, 0xc6, 0xc6, 0x7c, 0x00, 0x00, 0x00,
        0x00, // T (84)
        0x00, 0x00, 0x7e, 0x7e, 0x5a, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3c, 0x00, 0x00, 0x00,
        0x00, // U (85)
        0x00, 0x00, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0x7c, 0x00, 0x00, 0x00,
        0x00, // V (86)
        0x00, 0x00, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0x6c, 0x38, 0x10, 0x00, 0x00, 0x00,
        0x00, // W (87)
        0x00, 0x00, 0xc6, 0xc6, 0xc6, 0xc6, 0xd6, 0xd6, 0xd6, 0xfe, 0xee, 0x6c, 0x00, 0x00, 0x00,
        0x00, // X (88)
        0x00, 0x00, 0xc6, 0xc6, 0x6c, 0x7c, 0x38, 0x38, 0x7c, 0x6c, 0xc6, 0xc6, 0x00, 0x00, 0x00,
        0x00, // Y (89)
        0x00, 0x00, 0x66, 0x66, 0x66, 0x66, 0x3c, 0x18, 0x18, 0x18, 0x18, 0x3c, 0x00, 0x00, 0x00,
        0x00, // Z (90)
        0x00, 0x00, 0xfe, 0xc6, 0x86, 0x0c, 0x18, 0x30, 0x60, 0xc2, 0xc6, 0xfe, 0x00, 0x00, 0x00,
        0x00, // [ (91)
        0x00, 0x00, 0x3c, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x3c, 0x00, 0x00, 0x00,
        0x00, // \ (92)
        0x00, 0x00, 0x00, 0x80, 0xc0, 0xe0, 0x70, 0x38, 0x1c, 0x0e, 0x06, 0x02, 0x00, 0x00, 0x00,
        0x00, // ] (93)
        0x00, 0x00, 0x3c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x3c, 0x00, 0x00, 0x00,
        0x00, // ^ (94)
        0x10, 0x38, 0x6c, 0xc6, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, // _ (95)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x00,
        0x00, // ` (96)
        0x30, 0x30, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, // a (97)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x78, 0x0c, 0x7c, 0xcc, 0xcc, 0xcc, 0x76, 0x00, 0x00, 0x00,
        0x00, // b (98)
        0x00, 0x00, 0xe0, 0x60, 0x60, 0x78, 0x6c, 0x66, 0x66, 0x66, 0x66, 0x7c, 0x00, 0x00, 0x00,
        0x00, // c (99)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x7c, 0xc6, 0xc0, 0xc0, 0xc0, 0xc6, 0x7c, 0x00, 0x00, 0x00,
        0x00, // d (100)
        0x00, 0x00, 0x1c, 0x0c, 0x0c, 0x3c, 0x6c, 0xcc, 0xcc, 0xcc, 0xcc, 0x76, 0x00, 0x00, 0x00,
        0x00, // e (101)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x7c, 0xc6, 0xfe, 0xc0, 0xc0, 0xc6, 0x7c, 0x00, 0x00, 0x00,
        0x00, // f (102)
        0x00, 0x00, 0x38, 0x6c, 0x64, 0x60, 0xf0, 0x60, 0x60, 0x60, 0x60, 0xf0, 0x00, 0x00, 0x00,
        0x00, // g (103)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x76, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0x7c, 0x0c, 0xcc, 0x78,
        0x00, // h (104)
        0x00, 0x00, 0xe0, 0x60, 0x60, 0x6c, 0x76, 0x66, 0x66, 0x66, 0x66, 0xe6, 0x00, 0x00, 0x00,
        0x00, // i (105)
        0x00, 0x00, 0x18, 0x18, 0x00, 0x38, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3c, 0x00, 0x00, 0x00,
        0x00, // j (106)
        0x00, 0x00, 0x06, 0x06, 0x00, 0x0e, 0x06, 0x06, 0x06, 0x06, 0x06, 0x06, 0x66, 0x66, 0x3c,
        0x00, // k (107)
        0x00, 0x00, 0xe0, 0x60, 0x60, 0x66, 0x6c, 0x78, 0x78, 0x6c, 0x66, 0xe6, 0x00, 0x00, 0x00,
        0x00, // l (108)
        0x00, 0x00, 0x38, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3c, 0x00, 0x00, 0x00,
        0x00, // m (109)
        0x00, 0x00, 0x00, 0x00, 0x00, 0xec, 0xfe, 0xd6, 0xd6, 0xd6, 0xd6, 0xc6, 0x00, 0x00, 0x00,
        0x00, // n (110)
        0x00, 0x00, 0x00, 0x00, 0x00, 0xdc, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x00, 0x00, 0x00,
        0x00, // o (111)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x7c, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0x7c, 0x00, 0x00, 0x00,
        0x00, // p (112)
        0x00, 0x00, 0x00, 0x00, 0x00, 0xdc, 0x66, 0x66, 0x66, 0x66, 0x66, 0x7c, 0x60, 0x60, 0xf0,
        0x00, // q (113)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x76, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0x7c, 0x0c, 0x0c, 0x1e,
        0x00, // r (114)
        0x00, 0x00, 0x00, 0x00, 0x00, 0xdc, 0x76, 0x66, 0x60, 0x60, 0x60, 0xf0, 0x00, 0x00, 0x00,
        0x00, // s (115)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x7c, 0xc6, 0x60, 0x38, 0x0c, 0xc6, 0x7c, 0x00, 0x00, 0x00,
        0x00, // t (116)
        0x00, 0x00, 0x10, 0x30, 0x30, 0xfc, 0x30, 0x30, 0x30, 0x30, 0x36, 0x1c, 0x00, 0x00, 0x00,
        0x00, // u (117)
        0x00, 0x00, 0x00, 0x00, 0x00, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0x76, 0x00, 0x00, 0x00,
        0x00, // v (118)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3c, 0x18, 0x00, 0x00, 0x00,
        0x00, // w (119)
        0x00, 0x00, 0x00, 0x00, 0x00, 0xc6, 0xc6, 0xd6, 0xd6, 0xd6, 0xfe, 0x6c, 0x00, 0x00, 0x00,
        0x00, // x (120)
        0x00, 0x00, 0x00, 0x00, 0x00, 0xc6, 0x6c, 0x38, 0x38, 0x38, 0x6c, 0xc6, 0x00, 0x00, 0x00,
        0x00, // y (121)
        0x00, 0x00, 0x00, 0x00, 0x00, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0xc6, 0x7e, 0x06, 0x0c, 0xf8,
        0x00, // z (122)
        0x00, 0x00, 0x00, 0x00, 0x00, 0xfe, 0xcc, 0x18, 0x30, 0x60, 0xc6, 0xfe, 0x00, 0x00, 0x00,
        0x00, // { (123)
        0x00, 0x00, 0x0e, 0x18, 0x18, 0x18, 0x70, 0x18, 0x18, 0x18, 0x18, 0x0e, 0x00, 0x00, 0x00,
        0x00, // | (124)
        0x00, 0x00, 0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x00, 0x00,
        0x00, // } (125)
        0x00, 0x00, 0x70, 0x18, 0x18, 0x18, 0x0e, 0x18, 0x18, 0x18, 0x18, 0x70, 0x00, 0x00, 0x00,
        0x00, // ~ (126)
        0x00, 0x00, 0x76, 0xdc, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00,
    ];

    FONT
}
