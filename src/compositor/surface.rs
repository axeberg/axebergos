//! Canvas2D rendering surface
//!
//! Uses HTML Canvas 2D for rendering. Simple, reliable, works everywhere.
//! Can be upgraded to WebGPU later for better performance.

use super::{Color, Rect};
use wasm_bindgen::JsCast;

/// A frame being rendered (for Canvas2D this is just a marker)
pub struct Frame {
    _private: (),
}

/// The rendering surface
pub struct Surface {
    canvas: web_sys::HtmlCanvasElement,
    ctx: web_sys::CanvasRenderingContext2d,
    width: u32,
    height: u32,
}

impl Surface {
    /// Create a new Canvas2D surface
    pub async fn new(width: u32, height: u32) -> Result<Self, String> {
        // Get window and document
        let window = web_sys::window().ok_or("No window")?;
        let document = window.document().ok_or("No document")?;

        // Create canvas
        let canvas = document
            .create_element("canvas")
            .map_err(|_| "Failed to create canvas")?
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .map_err(|_| "Not a canvas")?;

        canvas.set_width(width);
        canvas.set_height(height);
        canvas.set_id("axeberg-canvas");

        // Style the canvas to fill the viewport
        let style = canvas.style();
        let _ = style.set_property("position", "fixed");
        let _ = style.set_property("top", "0");
        let _ = style.set_property("left", "0");
        let _ = style.set_property("width", "100%");
        let _ = style.set_property("height", "100%");
        let _ = style.set_property("z-index", "-1"); // Behind other content initially

        // Add to document
        document
            .body()
            .ok_or("No body")?
            .append_child(&canvas)
            .map_err(|_| "Failed to append canvas")?;

        // Get 2D context
        let ctx = canvas
            .get_context("2d")
            .map_err(|_| "Failed to get context")?
            .ok_or("No context")?
            .dyn_into::<web_sys::CanvasRenderingContext2d>()
            .map_err(|_| "Not a 2D context")?;

        Ok(Self {
            canvas,
            ctx,
            width,
            height,
        })
    }

    /// Resize the surface
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.canvas.set_width(width);
        self.canvas.set_height(height);
    }

    /// Begin a new frame
    pub fn begin_frame(&mut self) -> Option<Frame> {
        Some(Frame { _private: () })
    }

    /// Clear the screen with a color
    pub fn clear(&mut self, _frame: &Frame, color: Color) {
        self.ctx.set_fill_style_str(&color_to_css(color));
        self.ctx
            .fill_rect(0.0, 0.0, self.width as f64, self.height as f64);
    }

    /// Draw a filled rectangle
    pub fn draw_rect(&mut self, _frame: &Frame, rect: Rect, color: Color) {
        self.ctx.set_fill_style_str(&color_to_css(color));
        self.ctx.fill_rect(
            rect.x as f64,
            rect.y as f64,
            rect.width as f64,
            rect.height as f64,
        );
    }

    /// Draw text
    pub fn draw_text(
        &mut self,
        _frame: &Frame,
        text: &str,
        x: f32,
        y: f32,
        color: Color,
        font_size: f32,
    ) {
        self.ctx.set_fill_style_str(&color_to_css(color));
        self.ctx.set_font(&format!("{}px monospace", font_size));
        let _ = self.ctx.fill_text(text, x as f64, y as f64);
    }

    /// Draw a rectangle outline
    pub fn draw_rect_outline(
        &mut self,
        _frame: &Frame,
        rect: Rect,
        color: Color,
        line_width: f32,
    ) {
        self.ctx.set_stroke_style_str(&color_to_css(color));
        self.ctx.set_line_width(line_width as f64);
        self.ctx.stroke_rect(
            rect.x as f64,
            rect.y as f64,
            rect.width as f64,
            rect.height as f64,
        );
    }

    /// End the frame (no-op for Canvas2D, it's immediate mode)
    pub fn end_frame(&mut self, _frame: Frame) {
        // Nothing to do - Canvas2D is immediate mode
    }

    /// Get dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// Convert Color to CSS rgba string
fn color_to_css(color: Color) -> String {
    format!(
        "rgba({}, {}, {}, {})",
        (color.r * 255.0) as u8,
        (color.g * 255.0) as u8,
        (color.b * 255.0) as u8,
        color.a
    )
}
