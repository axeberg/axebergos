//! Geometry types for the compositor
//!
//! Provides basic 2D geometry primitives used throughout the compositor.

/// A 2D point
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    /// Create a new point
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Origin point (0, 0)
    pub const fn origin() -> Self {
        Self { x: 0.0, y: 0.0 }
    }

    /// Distance to another point
    pub fn distance(&self, other: &Point) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

/// A rectangle with position and size
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    /// Create a new rectangle
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Create a rectangle from two points (top-left and bottom-right)
    pub fn from_points(p1: Point, p2: Point) -> Self {
        let x = p1.x.min(p2.x);
        let y = p1.y.min(p2.y);
        let width = (p2.x - p1.x).abs();
        let height = (p2.y - p1.y).abs();
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Get the top-left corner
    pub fn top_left(&self) -> Point {
        Point::new(self.x, self.y)
    }

    /// Get the top-right corner
    pub fn top_right(&self) -> Point {
        Point::new(self.x + self.width, self.y)
    }

    /// Get the bottom-left corner
    pub fn bottom_left(&self) -> Point {
        Point::new(self.x, self.y + self.height)
    }

    /// Get the bottom-right corner
    pub fn bottom_right(&self) -> Point {
        Point::new(self.x + self.width, self.y + self.height)
    }

    /// Get the center point
    pub fn center(&self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Check if a point is inside the rectangle
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    /// Check if a point is inside the rectangle
    pub fn contains_point(&self, p: &Point) -> bool {
        self.contains(p.x, p.y)
    }

    /// Check if this rectangle intersects with another
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }

    /// Get the intersection of two rectangles, if any
    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        if !self.intersects(other) {
            return None;
        }

        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);

        Some(Rect::new(x, y, right - x, bottom - y))
    }

    /// Split horizontally at a ratio (0.0 to 1.0)
    /// Returns (left, right)
    pub fn split_horizontal(&self, ratio: f32) -> (Rect, Rect) {
        let split_x = self.x + self.width * ratio as f64;
        let left = Rect::new(self.x, self.y, split_x - self.x, self.height);
        let right = Rect::new(split_x, self.y, self.x + self.width - split_x, self.height);
        (left, right)
    }

    /// Split vertically at a ratio (0.0 to 1.0)
    /// Returns (top, bottom)
    pub fn split_vertical(&self, ratio: f32) -> (Rect, Rect) {
        let split_y = self.y + self.height * ratio as f64;
        let top = Rect::new(self.x, self.y, self.width, split_y - self.y);
        let bottom = Rect::new(self.x, split_y, self.width, self.y + self.height - split_y);
        (top, bottom)
    }

    /// Inset the rectangle by a margin on all sides
    pub fn inset(&self, margin: f64) -> Rect {
        Rect::new(
            self.x + margin,
            self.y + margin,
            (self.width - 2.0 * margin).max(0.0),
            (self.height - 2.0 * margin).max(0.0),
        )
    }

    /// Expand the rectangle by a margin on all sides
    pub fn expand(&self, margin: f64) -> Rect {
        Rect::new(
            self.x - margin,
            self.y - margin,
            self.width + 2.0 * margin,
            self.height + 2.0 * margin,
        )
    }

    /// Get the area
    pub fn area(&self) -> f64 {
        self.width * self.height
    }

    /// Check if the rectangle has zero or negative area
    pub fn is_empty(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
}

/// A color in RGBA format (0.0 to 1.0 per channel)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    /// Create a new color
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Create a color from RGB (alpha = 1.0)
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Create a color from a hex string like "#1a1a2e" or "#1a1a2eff"
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        let len = hex.len();

        if len != 6 && len != 8 {
            return None;
        }

        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        let a = if len == 8 {
            u8::from_str_radix(&hex[6..8], 16).ok()?
        } else {
            255
        };

        Some(Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        })
    }

    /// Convert to array for GPU
    pub fn to_array(&self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    // Common colors
    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);
    pub const RED: Self = Self::rgb(1.0, 0.0, 0.0);
    pub const GREEN: Self = Self::rgb(0.0, 1.0, 0.0);
    pub const BLUE: Self = Self::rgb(0.0, 0.0, 1.0);
    pub const TRANSPARENT: Self = Self::new(0.0, 0.0, 0.0, 0.0);
}

impl Default for Color {
    fn default() -> Self {
        Self::BLACK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_distance() {
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(3.0, 4.0);
        assert!((p1.distance(&p2) - 5.0).abs() < 0.0001);
    }

    #[test]
    fn test_rect_contains() {
        let rect = Rect::new(10.0, 10.0, 100.0, 100.0);
        assert!(rect.contains(50.0, 50.0));
        assert!(rect.contains(10.0, 10.0));
        assert!(!rect.contains(110.0, 110.0)); // Edge is exclusive
        assert!(!rect.contains(5.0, 50.0));
    }

    #[test]
    fn test_rect_split_horizontal() {
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        let (left, right) = rect.split_horizontal(0.5);

        assert_eq!(left.width, 50.0);
        assert_eq!(right.width, 50.0);
        assert_eq!(left.x, 0.0);
        assert_eq!(right.x, 50.0);
    }

    #[test]
    fn test_rect_split_vertical() {
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        let (top, bottom) = rect.split_vertical(0.5);

        assert_eq!(top.height, 50.0);
        assert_eq!(bottom.height, 50.0);
        assert_eq!(top.y, 0.0);
        assert_eq!(bottom.y, 50.0);
    }

    #[test]
    fn test_rect_intersection() {
        let r1 = Rect::new(0.0, 0.0, 100.0, 100.0);
        let r2 = Rect::new(50.0, 50.0, 100.0, 100.0);
        let r3 = Rect::new(200.0, 200.0, 50.0, 50.0);

        assert!(r1.intersects(&r2));
        assert!(!r1.intersects(&r3));

        let intersection = r1.intersection(&r2).unwrap();
        assert_eq!(intersection.x, 50.0);
        assert_eq!(intersection.y, 50.0);
        assert_eq!(intersection.width, 50.0);
        assert_eq!(intersection.height, 50.0);
    }

    #[test]
    fn test_rect_inset() {
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        let inset = rect.inset(10.0);

        assert_eq!(inset.x, 10.0);
        assert_eq!(inset.y, 10.0);
        assert_eq!(inset.width, 80.0);
        assert_eq!(inset.height, 80.0);
    }

    #[test]
    fn test_color_from_hex() {
        let color = Color::from_hex("#1a1a2e").unwrap();
        assert!((color.r - 0.102).abs() < 0.01);
        assert!((color.g - 0.102).abs() < 0.01);
        assert!((color.b - 0.180).abs() < 0.01);
        assert_eq!(color.a, 1.0);

        let color_with_alpha = Color::from_hex("#1a1a2e80").unwrap();
        assert!((color_with_alpha.a - 0.502).abs() < 0.01);
    }
}
