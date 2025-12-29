//! Text Rendering for the Compositor
//!
//! GPU-accelerated text rendering using a glyph atlas approach.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐    ┌──────────────┐    ┌─────────────┐
//! │  TextLayout │ -> │  GlyphAtlas  │ -> │   WebGPU    │
//! │  (metrics)  │    │  (textures)  │    │  (render)   │
//! └─────────────┘    └──────────────┘    └─────────────┘
//! ```
//!
//! # Design
//!
//! Text rendering is split into:
//! 1. **Font metrics**: Character widths, heights, baselines
//! 2. **Glyph atlas**: Texture containing pre-rendered glyphs
//! 3. **Text layout**: Breaking text into positioned glyphs
//! 4. **Rendering**: Drawing glyphs using the atlas

use super::geometry::{Color, Point, Rect};
use std::collections::HashMap;

/// Font style for text rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FontStyle {
    /// Normal upright text
    #[default]
    Normal,
    /// Italic/oblique text
    Italic,
    /// Bold text
    Bold,
    /// Bold italic text
    BoldItalic,
}

/// Font weight (100-900)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontWeight(pub u16);

impl FontWeight {
    pub const THIN: FontWeight = FontWeight(100);
    pub const LIGHT: FontWeight = FontWeight(300);
    pub const NORMAL: FontWeight = FontWeight(400);
    pub const MEDIUM: FontWeight = FontWeight(500);
    pub const BOLD: FontWeight = FontWeight(700);
    pub const BLACK: FontWeight = FontWeight(900);
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::NORMAL
    }
}

/// Font metrics for a specific font size
#[derive(Debug, Clone, Copy)]
pub struct FontMetrics {
    /// Height of the font in pixels
    pub height: f64,
    /// Ascent above the baseline
    pub ascent: f64,
    /// Descent below the baseline
    pub descent: f64,
    /// Line gap (extra space between lines)
    pub line_gap: f64,
    /// Width of a space character
    pub space_width: f64,
    /// Width of the average character (for monospace estimation)
    pub average_width: f64,
}

impl FontMetrics {
    /// Create metrics for a monospace font at the given size
    pub fn monospace(size: f64) -> Self {
        // Typical monospace font metrics
        let char_width = size * 0.6; // 0.6 ratio is common for monospace
        Self {
            height: size,
            ascent: size * 0.8,
            descent: size * 0.2,
            line_gap: size * 0.1,
            space_width: char_width,
            average_width: char_width,
        }
    }

    /// Create metrics for a proportional font at the given size
    pub fn proportional(size: f64) -> Self {
        Self {
            height: size,
            ascent: size * 0.75,
            descent: size * 0.25,
            line_gap: size * 0.15,
            space_width: size * 0.25,
            average_width: size * 0.5,
        }
    }

    /// Total line height (including gap)
    pub fn line_height(&self) -> f64 {
        self.height + self.line_gap
    }
}

/// A positioned glyph ready for rendering
#[derive(Debug, Clone)]
pub struct PositionedGlyph {
    /// The character this glyph represents
    pub character: char,
    /// Position in the text layout
    pub position: Point,
    /// Size of the glyph
    pub size: Point,
    /// UV coordinates in the atlas (if using atlas)
    pub uv_rect: Option<Rect>,
}

/// Text alignment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    /// Left-aligned text
    #[default]
    Left,
    /// Center-aligned text
    Center,
    /// Right-aligned text
    Right,
}

/// Vertical text alignment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerticalAlign {
    /// Top-aligned text
    #[default]
    Top,
    /// Middle-aligned text
    Middle,
    /// Bottom-aligned text
    Bottom,
}

/// Text wrapping mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextWrap {
    /// No wrapping - single line
    None,
    /// Wrap at word boundaries
    #[default]
    Word,
    /// Wrap at character boundaries
    Character,
}

/// Options for text layout
#[derive(Debug, Clone)]
pub struct TextLayoutOptions {
    /// Font size in pixels
    pub font_size: f64,
    /// Font style
    pub font_style: FontStyle,
    /// Font weight
    pub font_weight: FontWeight,
    /// Text color
    pub color: Color,
    /// Horizontal alignment
    pub align: TextAlign,
    /// Vertical alignment
    pub vertical_align: VerticalAlign,
    /// Text wrapping mode
    pub wrap: TextWrap,
    /// Maximum width (for wrapping)
    pub max_width: Option<f64>,
    /// Line height multiplier
    pub line_height: f64,
    /// Letter spacing (additional space between characters)
    pub letter_spacing: f64,
}

impl Default for TextLayoutOptions {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            font_style: FontStyle::Normal,
            font_weight: FontWeight::NORMAL,
            color: Color::WHITE,
            align: TextAlign::Left,
            vertical_align: VerticalAlign::Top,
            wrap: TextWrap::Word,
            max_width: None,
            line_height: 1.2,
            letter_spacing: 0.0,
        }
    }
}

/// A laid out line of text
#[derive(Debug, Clone)]
pub struct TextLine {
    /// The text content of this line
    pub text: String,
    /// Starting position of the line
    pub position: Point,
    /// Width of the line
    pub width: f64,
    /// Glyphs in this line
    pub glyphs: Vec<PositionedGlyph>,
}

/// Result of laying out text
#[derive(Debug, Clone)]
pub struct TextLayout {
    /// The original text
    pub text: String,
    /// Lines of laid out text
    pub lines: Vec<TextLine>,
    /// Total bounds of the text
    pub bounds: Rect,
    /// Font metrics used
    pub metrics: FontMetrics,
}

impl TextLayout {
    /// Get the total height of the laid out text
    pub fn height(&self) -> f64 {
        self.bounds.height
    }

    /// Get the total width of the laid out text
    pub fn width(&self) -> f64 {
        self.bounds.width
    }

    /// Get the number of lines
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
}

/// Simple character width lookup for basic ASCII
fn char_width(c: char, metrics: &FontMetrics, is_monospace: bool) -> f64 {
    if is_monospace {
        metrics.average_width
    } else {
        match c {
            ' ' => metrics.space_width,
            'i' | 'l' | '!' | '|' | '\'' | '.' | ',' | ':' | ';' => metrics.average_width * 0.4,
            'm' | 'w' | 'M' | 'W' => metrics.average_width * 1.4,
            _ if c.is_uppercase() => metrics.average_width * 1.1,
            _ => metrics.average_width,
        }
    }
}

/// Layout text with the given options
pub fn layout_text(text: &str, bounds: Rect, options: &TextLayoutOptions) -> TextLayout {
    let metrics = FontMetrics::monospace(options.font_size);
    let line_height = metrics.line_height() * options.line_height;

    let mut lines = Vec::new();
    let mut current_y = bounds.y;

    // Split into lines first (handling explicit line breaks)
    for paragraph in text.split('\n') {
        if options.wrap == TextWrap::None || options.max_width.is_none() {
            // No wrapping - just add the line
            let width = measure_line(paragraph, &metrics, options.letter_spacing, true);
            let x = match options.align {
                TextAlign::Left => bounds.x,
                TextAlign::Center => bounds.x + (bounds.width - width) / 2.0,
                TextAlign::Right => bounds.x + bounds.width - width,
            };

            let glyphs = layout_line_glyphs(paragraph, x, current_y, &metrics, options, true);

            lines.push(TextLine {
                text: paragraph.to_string(),
                position: Point { x, y: current_y },
                width,
                glyphs,
            });
            current_y += line_height;
        } else {
            // Wrap text
            let max_width = options.max_width.unwrap_or(bounds.width);
            let wrapped = wrap_text(paragraph, max_width, &metrics, options);

            for line_text in wrapped {
                let width = measure_line(&line_text, &metrics, options.letter_spacing, true);
                let x = match options.align {
                    TextAlign::Left => bounds.x,
                    TextAlign::Center => bounds.x + (bounds.width - width) / 2.0,
                    TextAlign::Right => bounds.x + bounds.width - width,
                };

                let glyphs = layout_line_glyphs(&line_text, x, current_y, &metrics, options, true);

                lines.push(TextLine {
                    text: line_text,
                    position: Point { x, y: current_y },
                    width,
                    glyphs,
                });
                current_y += line_height;
            }
        }
    }

    // Calculate total bounds
    let total_height = current_y - bounds.y;
    let max_width = lines.iter().map(|l| l.width).fold(0.0f64, f64::max);

    // Apply vertical alignment
    let y_offset = match options.vertical_align {
        VerticalAlign::Top => 0.0,
        VerticalAlign::Middle => (bounds.height - total_height) / 2.0,
        VerticalAlign::Bottom => bounds.height - total_height,
    };

    // Adjust line positions for vertical alignment
    for line in &mut lines {
        line.position.y += y_offset;
        for glyph in &mut line.glyphs {
            glyph.position.y += y_offset;
        }
    }

    TextLayout {
        text: text.to_string(),
        lines,
        bounds: Rect {
            x: bounds.x,
            y: bounds.y + y_offset,
            width: max_width,
            height: total_height,
        },
        metrics,
    }
}

/// Measure the width of a line of text
fn measure_line(text: &str, metrics: &FontMetrics, letter_spacing: f64, is_monospace: bool) -> f64 {
    let mut width = 0.0;
    for (i, c) in text.chars().enumerate() {
        width += char_width(c, metrics, is_monospace);
        if i > 0 {
            width += letter_spacing;
        }
    }
    width
}

/// Wrap text to fit within max_width
fn wrap_text(
    text: &str,
    max_width: f64,
    metrics: &FontMetrics,
    options: &TextLayoutOptions,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0.0;

    match options.wrap {
        TextWrap::None => {
            lines.push(text.to_string());
        }
        TextWrap::Word => {
            for word in text.split_whitespace() {
                let word_width = measure_line(word, metrics, options.letter_spacing, true);
                let space_width = metrics.space_width + options.letter_spacing;

                if current_line.is_empty() {
                    current_line = word.to_string();
                    current_width = word_width;
                } else if current_width + space_width + word_width <= max_width {
                    current_line.push(' ');
                    current_line.push_str(word);
                    current_width += space_width + word_width;
                } else {
                    lines.push(current_line);
                    current_line = word.to_string();
                    current_width = word_width;
                }
            }
            if !current_line.is_empty() {
                lines.push(current_line);
            }
        }
        TextWrap::Character => {
            for c in text.chars() {
                let c_width = char_width(c, metrics, true);

                if current_width + c_width > max_width && !current_line.is_empty() {
                    lines.push(current_line);
                    current_line = String::new();
                    current_width = 0.0;
                }

                current_line.push(c);
                current_width += c_width + options.letter_spacing;
            }
            if !current_line.is_empty() {
                lines.push(current_line);
            }
        }
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

/// Layout individual glyphs for a line
fn layout_line_glyphs(
    text: &str,
    start_x: f64,
    start_y: f64,
    metrics: &FontMetrics,
    options: &TextLayoutOptions,
    is_monospace: bool,
) -> Vec<PositionedGlyph> {
    let mut glyphs = Vec::new();
    let mut x = start_x;

    for c in text.chars() {
        let width = char_width(c, metrics, is_monospace);

        glyphs.push(PositionedGlyph {
            character: c,
            position: Point { x, y: start_y },
            size: Point {
                x: width,
                y: metrics.height,
            },
            uv_rect: None, // Will be set when using atlas
        });

        x += width + options.letter_spacing;
    }

    glyphs
}

/// Glyph cache entry
#[derive(Debug, Clone)]
pub struct GlyphCacheEntry {
    /// Character this entry represents
    pub character: char,
    /// Font size
    pub font_size: f64,
    /// Font style
    pub font_style: FontStyle,
    /// UV rectangle in the atlas
    pub uv_rect: Rect,
    /// Glyph metrics
    pub advance: f64,
    /// Bearing (offset from origin)
    pub bearing: Point,
}

/// Glyph atlas for caching rendered glyphs
pub struct GlyphAtlas {
    /// Atlas width in pixels
    pub width: u32,
    /// Atlas height in pixels
    pub height: u32,
    /// Cache of glyph entries
    cache: HashMap<(char, u32, FontStyle), GlyphCacheEntry>,
    /// Next available position in the atlas
    next_x: u32,
    next_y: u32,
    /// Current row height
    row_height: u32,
}

impl GlyphAtlas {
    /// Create a new glyph atlas
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            cache: HashMap::new(),
            next_x: 0,
            next_y: 0,
            row_height: 0,
        }
    }

    /// Check if a glyph is in the cache
    pub fn get(&self, c: char, font_size: f64, style: FontStyle) -> Option<&GlyphCacheEntry> {
        let size_key = (font_size * 10.0) as u32;
        self.cache.get(&(c, size_key, style))
    }

    /// Insert a glyph into the atlas
    pub fn insert(
        &mut self,
        c: char,
        font_size: f64,
        style: FontStyle,
        glyph_width: u32,
        glyph_height: u32,
    ) -> Option<GlyphCacheEntry> {
        // Check if we need to move to next row
        if self.next_x + glyph_width > self.width {
            self.next_x = 0;
            self.next_y += self.row_height + 1; // +1 for padding
            self.row_height = 0;
        }

        // Check if we have space
        if self.next_y + glyph_height > self.height {
            return None; // Atlas is full
        }

        let uv_rect = Rect {
            x: self.next_x as f64 / self.width as f64,
            y: self.next_y as f64 / self.height as f64,
            width: glyph_width as f64 / self.width as f64,
            height: glyph_height as f64 / self.height as f64,
        };

        let entry = GlyphCacheEntry {
            character: c,
            font_size,
            font_style: style,
            uv_rect,
            advance: glyph_width as f64,
            bearing: Point { x: 0.0, y: 0.0 },
        };

        let size_key = (font_size * 10.0) as u32;
        self.cache.insert((c, size_key, style), entry.clone());

        self.next_x += glyph_width + 1; // +1 for padding
        self.row_height = self.row_height.max(glyph_height);

        Some(entry)
    }

    /// Clear the atlas
    pub fn clear(&mut self) {
        self.cache.clear();
        self.next_x = 0;
        self.next_y = 0;
        self.row_height = 0;
    }

    /// Get the number of cached glyphs
    pub fn glyph_count(&self) -> usize {
        self.cache.len()
    }

    /// Check if the atlas is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self::new(1024, 1024)
    }
}

/// Text renderer state
pub struct TextRenderer {
    /// Glyph atlas
    atlas: GlyphAtlas,
    /// Default font size
    pub default_font_size: f64,
    /// Default text color
    pub default_color: Color,
}

impl TextRenderer {
    /// Create a new text renderer
    pub fn new() -> Self {
        Self {
            atlas: GlyphAtlas::default(),
            default_font_size: 14.0,
            default_color: Color::WHITE,
        }
    }

    /// Create with custom atlas size
    pub fn with_atlas_size(width: u32, height: u32) -> Self {
        Self {
            atlas: GlyphAtlas::new(width, height),
            default_font_size: 14.0,
            default_color: Color::WHITE,
        }
    }

    /// Layout text with default options
    pub fn layout(&self, text: &str, bounds: Rect) -> TextLayout {
        let options = TextLayoutOptions {
            font_size: self.default_font_size,
            color: self.default_color,
            ..Default::default()
        };
        layout_text(text, bounds, &options)
    }

    /// Layout text with custom options
    pub fn layout_with_options(
        &self,
        text: &str,
        bounds: Rect,
        options: &TextLayoutOptions,
    ) -> TextLayout {
        layout_text(text, bounds, options)
    }

    /// Get the glyph atlas
    pub fn atlas(&self) -> &GlyphAtlas {
        &self.atlas
    }

    /// Get mutable reference to the glyph atlas
    pub fn atlas_mut(&mut self) -> &mut GlyphAtlas {
        &mut self.atlas
    }

    /// Measure text without laying it out
    pub fn measure(&self, text: &str, font_size: f64) -> Point {
        let metrics = FontMetrics::monospace(font_size);
        let width = measure_line(text, &metrics, 0.0, true);
        let height = metrics.height;
        Point {
            x: width,
            y: height,
        }
    }

    /// Measure multi-line text
    pub fn measure_wrapped(&self, text: &str, font_size: f64, max_width: f64) -> Point {
        let options = TextLayoutOptions {
            font_size,
            max_width: Some(max_width),
            ..Default::default()
        };
        let layout = layout_text(
            text,
            Rect {
                x: 0.0,
                y: 0.0,
                width: max_width,
                height: f64::MAX,
            },
            &options,
        );
        Point {
            x: layout.width(),
            y: layout.height(),
        }
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_metrics_monospace() {
        let metrics = FontMetrics::monospace(16.0);
        assert_eq!(metrics.height, 16.0);
        assert!(metrics.average_width > 0.0);
        assert!(metrics.line_height() > metrics.height);
    }

    #[test]
    fn test_font_metrics_proportional() {
        let metrics = FontMetrics::proportional(16.0);
        assert_eq!(metrics.height, 16.0);
        assert!(metrics.average_width > 0.0);
    }

    #[test]
    fn test_text_layout_single_line() {
        let options = TextLayoutOptions::default();
        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        };

        let layout = layout_text("Hello", bounds, &options);

        assert_eq!(layout.line_count(), 1);
        assert_eq!(layout.lines[0].text, "Hello");
        assert!(layout.width() > 0.0);
        assert!(layout.height() > 0.0);
    }

    #[test]
    fn test_text_layout_multi_line() {
        let options = TextLayoutOptions::default();
        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        };

        let layout = layout_text("Line 1\nLine 2\nLine 3", bounds, &options);

        assert_eq!(layout.line_count(), 3);
        assert_eq!(layout.lines[0].text, "Line 1");
        assert_eq!(layout.lines[1].text, "Line 2");
        assert_eq!(layout.lines[2].text, "Line 3");
    }

    #[test]
    fn test_text_layout_word_wrap() {
        let options = TextLayoutOptions {
            max_width: Some(50.0),
            wrap: TextWrap::Word,
            ..Default::default()
        };
        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 200.0,
        };

        let layout = layout_text("This is a long text that should wrap", bounds, &options);

        assert!(layout.line_count() > 1);
    }

    #[test]
    fn test_text_alignment_center() {
        let options = TextLayoutOptions {
            align: TextAlign::Center,
            ..Default::default()
        };
        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        };

        let layout = layout_text("Hi", bounds, &options);
        let line = &layout.lines[0];

        // Line should be centered
        assert!(line.position.x > 0.0);
        assert!(line.position.x + line.width < bounds.width);
    }

    #[test]
    fn test_text_alignment_right() {
        let options = TextLayoutOptions {
            align: TextAlign::Right,
            ..Default::default()
        };
        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        };

        let layout = layout_text("Hi", bounds, &options);
        let line = &layout.lines[0];

        // Line should be right-aligned
        assert!((line.position.x + line.width - bounds.width).abs() < 0.01);
    }

    #[test]
    fn test_glyph_atlas_insert() {
        let mut atlas = GlyphAtlas::new(256, 256);

        let entry = atlas.insert('A', 14.0, FontStyle::Normal, 10, 14);
        assert!(entry.is_some());

        let entry = entry.unwrap();
        assert_eq!(entry.character, 'A');
        assert!(atlas.glyph_count() == 1);
    }

    #[test]
    fn test_glyph_atlas_get() {
        let mut atlas = GlyphAtlas::new(256, 256);

        atlas.insert('A', 14.0, FontStyle::Normal, 10, 14);

        let entry = atlas.get('A', 14.0, FontStyle::Normal);
        assert!(entry.is_some());

        let entry = atlas.get('B', 14.0, FontStyle::Normal);
        assert!(entry.is_none());
    }

    #[test]
    fn test_glyph_atlas_clear() {
        let mut atlas = GlyphAtlas::new(256, 256);

        atlas.insert('A', 14.0, FontStyle::Normal, 10, 14);
        atlas.insert('B', 14.0, FontStyle::Normal, 10, 14);

        assert_eq!(atlas.glyph_count(), 2);

        atlas.clear();

        assert_eq!(atlas.glyph_count(), 0);
        assert!(atlas.is_empty());
    }

    #[test]
    fn test_text_renderer() {
        let renderer = TextRenderer::new();

        let size = renderer.measure("Hello", 14.0);
        assert!(size.x > 0.0);
        assert!(size.y > 0.0);
    }

    #[test]
    fn test_positioned_glyph() {
        let options = TextLayoutOptions::default();
        let bounds = Rect {
            x: 10.0,
            y: 20.0,
            width: 200.0,
            height: 100.0,
        };

        let layout = layout_text("AB", bounds, &options);

        assert_eq!(layout.lines[0].glyphs.len(), 2);

        let glyph_a = &layout.lines[0].glyphs[0];
        let glyph_b = &layout.lines[0].glyphs[1];

        assert_eq!(glyph_a.character, 'A');
        assert_eq!(glyph_b.character, 'B');
        assert!(glyph_b.position.x > glyph_a.position.x);
    }

    #[test]
    fn test_vertical_align_middle() {
        let options = TextLayoutOptions {
            vertical_align: VerticalAlign::Middle,
            ..Default::default()
        };
        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        };

        let layout = layout_text("Hi", bounds, &options);

        // Text should be vertically centered
        assert!(layout.bounds.y > 0.0);
    }

    #[test]
    fn test_font_weight() {
        assert_eq!(FontWeight::NORMAL.0, 400);
        assert_eq!(FontWeight::BOLD.0, 700);
    }
}
