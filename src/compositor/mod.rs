//! Compositor - Window management and rendering
//!
//! The compositor manages windows and renders the GUI using WebGPU.
//! It provides:
//! - Window creation and management
//! - Tiling layout using Binary Space Partition (BSP)
//! - Focus management and input routing
//! - GPU-accelerated rendering via WebGPU
//!
//! Architecture:
//! ```text
//! ┌──────────────────────────────────────────┐
//! │              Compositor                  │
//! │  ┌─────────────┐  ┌─────────────────┐   │
//! │  │   Layout    │  │     Surface     │   │
//! │  │ (Tiling BSP)│  │    (WebGPU)     │   │
//! │  └──────┬──────┘  └────────┬────────┘   │
//! │         │                  │            │
//! │  ┌──────▼──────────────────▼──────┐     │
//! │  │          Windows               │     │
//! │  │  ┌────────┐  ┌────────┐       │     │
//! │  │  │Terminal│  │ Files  │  ...  │     │
//! │  │  └────────┘  └────────┘       │     │
//! │  └────────────────────────────────┘     │
//! └──────────────────────────────────────────┘
//! ```

mod geometry;
mod layout;
mod text;
mod window;

// Surface module requires web_sys, only available on wasm32
#[cfg(target_arch = "wasm32")]
mod surface;

pub use geometry::{Color, Point, Rect};
pub use layout::{LayoutNode, SplitDirection, TilingLayout};
pub use text::{
    FontMetrics, FontStyle, FontWeight, GlyphAtlas, GlyphCacheEntry, PositionedGlyph, TextAlign,
    TextLayout, TextLayoutOptions, TextLine, TextRenderer, TextWrap, VerticalAlign, layout_text,
};
pub use window::{Window, WindowId};

#[cfg(target_arch = "wasm32")]
pub use surface::Surface;

use crate::kernel::TaskId;
use std::cell::RefCell;
use std::collections::HashMap;

/// Theme colors for the compositor
#[derive(Debug, Clone)]
pub struct Theme {
    /// Background color for the desktop
    pub background: Color,
    /// Background color for windows
    pub window_bg: Color,
    /// Title bar background color
    pub titlebar_bg: Color,
    /// Title bar text color
    pub titlebar_fg: Color,
    /// Focused window border color
    pub focus_border: Color,
    /// Unfocused window border color
    pub unfocus_border: Color,
    /// Border width in pixels
    pub border_width: f64,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// Dark theme (default)
    pub fn dark() -> Self {
        Self {
            background: Color::from_hex("#0f0f1a").unwrap_or(Color::BLACK),
            window_bg: Color::from_hex("#1a1a2e").unwrap_or(Color::BLACK),
            titlebar_bg: Color::from_hex("#16213e").unwrap_or(Color::BLACK),
            titlebar_fg: Color::WHITE,
            focus_border: Color::from_hex("#00ff88").unwrap_or(Color::GREEN),
            unfocus_border: Color::from_hex("#333333").unwrap_or(Color::BLACK),
            border_width: 2.0,
        }
    }

    /// Light theme
    pub fn light() -> Self {
        Self {
            background: Color::from_hex("#e8e8e8").unwrap_or(Color::WHITE),
            window_bg: Color::from_hex("#ffffff").unwrap_or(Color::WHITE),
            titlebar_bg: Color::from_hex("#d4d4d4").unwrap_or(Color::WHITE),
            titlebar_fg: Color::BLACK,
            focus_border: Color::from_hex("#0066cc").unwrap_or(Color::BLUE),
            unfocus_border: Color::from_hex("#aaaaaa").unwrap_or(Color::BLACK),
            border_width: 2.0,
        }
    }

    /// High contrast dark theme
    pub fn high_contrast_dark() -> Self {
        Self {
            background: Color::BLACK,
            window_bg: Color::from_hex("#1a1a1a").unwrap_or(Color::BLACK),
            titlebar_bg: Color::from_hex("#2a2a2a").unwrap_or(Color::BLACK),
            titlebar_fg: Color::from_hex("#ffff00").unwrap_or(Color::WHITE),
            focus_border: Color::from_hex("#ffff00").unwrap_or(Color::WHITE),
            unfocus_border: Color::from_hex("#666666").unwrap_or(Color::BLACK),
            border_width: 3.0,
        }
    }

    /// Monokai-inspired theme
    pub fn monokai() -> Self {
        Self {
            background: Color::from_hex("#272822").unwrap_or(Color::BLACK),
            window_bg: Color::from_hex("#1e1f1c").unwrap_or(Color::BLACK),
            titlebar_bg: Color::from_hex("#3e3d32").unwrap_or(Color::BLACK),
            titlebar_fg: Color::from_hex("#f8f8f2").unwrap_or(Color::WHITE),
            focus_border: Color::from_hex("#a6e22e").unwrap_or(Color::GREEN),
            unfocus_border: Color::from_hex("#75715e").unwrap_or(Color::BLACK),
            border_width: 2.0,
        }
    }

    /// Nord-inspired theme
    pub fn nord() -> Self {
        Self {
            background: Color::from_hex("#2e3440").unwrap_or(Color::BLACK),
            window_bg: Color::from_hex("#3b4252").unwrap_or(Color::BLACK),
            titlebar_bg: Color::from_hex("#434c5e").unwrap_or(Color::BLACK),
            titlebar_fg: Color::from_hex("#eceff4").unwrap_or(Color::WHITE),
            focus_border: Color::from_hex("#88c0d0").unwrap_or(Color::BLUE),
            unfocus_border: Color::from_hex("#4c566a").unwrap_or(Color::BLACK),
            border_width: 2.0,
        }
    }

    /// Get a theme by name
    pub fn by_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "dark" => Some(Self::dark()),
            "light" => Some(Self::light()),
            "high-contrast" | "high_contrast" | "highcontrast" => Some(Self::high_contrast_dark()),
            "monokai" => Some(Self::monokai()),
            "nord" => Some(Self::nord()),
            _ => None,
        }
    }

    /// List available theme names
    pub fn available_themes() -> &'static [&'static str] {
        &["dark", "light", "high-contrast", "monokai", "nord"]
    }
}

// ============================================================================
// Animation Support
// ============================================================================

/// Easing function type
pub type EasingFn = fn(f64) -> f64;

/// Linear easing (no acceleration)
pub fn ease_linear(t: f64) -> f64 {
    t.clamp(0.0, 1.0)
}

/// Ease-in (starts slow, accelerates)
pub fn ease_in(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t
}

/// Ease-out (starts fast, decelerates)
pub fn ease_out(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t)
}

/// Ease-in-out (smooth start and end)
pub fn ease_in_out(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
    }
}

/// Animation target property
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimationProperty {
    /// Window opacity (0.0 to 1.0)
    Opacity,
    /// Window scale (1.0 = normal)
    Scale,
    /// X position offset
    PositionX,
    /// Y position offset
    PositionY,
    /// Border color blend (0.0 = start color, 1.0 = end color)
    BorderColor,
}

/// An animation that interpolates a property over time
#[derive(Debug, Clone)]
pub struct Animation {
    /// Property being animated
    pub property: AnimationProperty,
    /// Start value
    pub from: f64,
    /// End value
    pub to: f64,
    /// Start time (milliseconds)
    pub start_time: f64,
    /// Duration (milliseconds)
    pub duration: f64,
    /// Easing function
    pub easing: EasingFn,
}

impl Animation {
    /// Create a new animation
    pub fn new(property: AnimationProperty, from: f64, to: f64, duration: f64) -> Self {
        Self {
            property,
            from,
            to,
            start_time: 0.0, // Set when animation starts
            duration,
            easing: ease_out,
        }
    }

    /// Set the easing function
    pub fn with_easing(mut self, easing: EasingFn) -> Self {
        self.easing = easing;
        self
    }

    /// Set the start time
    pub fn with_start_time(mut self, start_time: f64) -> Self {
        self.start_time = start_time;
        self
    }

    /// Get the current value at a given time
    pub fn value_at(&self, current_time: f64) -> f64 {
        let elapsed = current_time - self.start_time;
        if elapsed <= 0.0 {
            return self.from;
        }
        if elapsed >= self.duration {
            return self.to;
        }

        let progress = elapsed / self.duration;
        let eased = (self.easing)(progress);
        self.from + (self.to - self.from) * eased
    }

    /// Check if the animation is complete
    pub fn is_complete(&self, current_time: f64) -> bool {
        current_time >= self.start_time + self.duration
    }
}

/// Animation state for a window
#[derive(Debug, Clone, Default)]
pub struct WindowAnimationState {
    /// Active animations
    pub animations: Vec<Animation>,
    /// Current opacity (1.0 = fully visible)
    pub opacity: f64,
    /// Current scale (1.0 = normal size)
    pub scale: f64,
    /// Position offset X
    pub offset_x: f64,
    /// Position offset Y
    pub offset_y: f64,
}

impl WindowAnimationState {
    /// Create a new animation state
    pub fn new() -> Self {
        Self {
            animations: Vec::new(),
            opacity: 1.0,
            scale: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }

    /// Add an animation
    pub fn add_animation(&mut self, animation: Animation) {
        self.animations.push(animation);
    }

    /// Update all animations at the current time
    /// Returns true if any animations are still running
    pub fn update(&mut self, current_time: f64) -> bool {
        // Apply animations
        for anim in &self.animations {
            let value = anim.value_at(current_time);
            match anim.property {
                AnimationProperty::Opacity => self.opacity = value,
                AnimationProperty::Scale => self.scale = value,
                AnimationProperty::PositionX => self.offset_x = value,
                AnimationProperty::PositionY => self.offset_y = value,
                AnimationProperty::BorderColor => {} // Handled separately
            }
        }

        // Remove completed animations
        self.animations.retain(|a| !a.is_complete(current_time));

        !self.animations.is_empty()
    }

    /// Create a window open animation
    pub fn window_open(start_time: f64) -> Self {
        let mut state = Self::new();
        state.opacity = 0.0;
        state.scale = 0.95;
        state.add_animation(
            Animation::new(AnimationProperty::Opacity, 0.0, 1.0, 200.0)
                .with_start_time(start_time)
                .with_easing(ease_out),
        );
        state.add_animation(
            Animation::new(AnimationProperty::Scale, 0.95, 1.0, 200.0)
                .with_start_time(start_time)
                .with_easing(ease_out),
        );
        state
    }

    /// Create a window close animation
    pub fn window_close(start_time: f64) -> Self {
        let mut state = Self::new();
        state.add_animation(
            Animation::new(AnimationProperty::Opacity, 1.0, 0.0, 150.0)
                .with_start_time(start_time)
                .with_easing(ease_in),
        );
        state.add_animation(
            Animation::new(AnimationProperty::Scale, 1.0, 0.9, 150.0)
                .with_start_time(start_time)
                .with_easing(ease_in),
        );
        state
    }

    /// Create a focus animation
    pub fn focus_change(start_time: f64) -> Self {
        let mut state = Self::new();
        state.add_animation(
            Animation::new(AnimationProperty::BorderColor, 0.0, 1.0, 150.0)
                .with_start_time(start_time)
                .with_easing(ease_out),
        );
        state
    }
}

// ============================================================================
// Window Decoration Constants
// ============================================================================

/// Size of window decoration buttons (close, maximize, minimize)
pub const DECORATION_BUTTON_SIZE: f64 = 16.0;

/// Padding around decoration buttons
pub const DECORATION_BUTTON_PADDING: f64 = 4.0;

/// Button type for window decorations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationButton {
    Close,
    Maximize,
    Minimize,
}

/// Get the rectangle for a decoration button in a window
///
/// Buttons are positioned in the title bar: [Minimize] [Maximize] [Close]
pub fn decoration_button_rect(window_rect: &Rect, button: DecorationButton) -> Rect {
    let titlebar_height = Window::TITLEBAR_HEIGHT;
    let button_y = (titlebar_height - DECORATION_BUTTON_SIZE) / 2.0;

    let button_x = match button {
        DecorationButton::Close => {
            window_rect.x + window_rect.width - DECORATION_BUTTON_PADDING - DECORATION_BUTTON_SIZE
        }
        DecorationButton::Maximize => {
            window_rect.x + window_rect.width
                - DECORATION_BUTTON_PADDING * 2.0
                - DECORATION_BUTTON_SIZE * 2.0
        }
        DecorationButton::Minimize => {
            window_rect.x + window_rect.width
                - DECORATION_BUTTON_PADDING * 3.0
                - DECORATION_BUTTON_SIZE * 3.0
        }
    };

    Rect::new(
        button_x,
        window_rect.y + button_y,
        DECORATION_BUTTON_SIZE,
        DECORATION_BUTTON_SIZE,
    )
}

/// Colors for decoration buttons
pub struct DecorationColors {
    /// Close button background
    pub close_bg: Color,
    /// Close button hover
    pub close_hover: Color,
    /// Maximize button background
    pub maximize_bg: Color,
    /// Maximize button hover
    pub maximize_hover: Color,
    /// Minimize button background
    pub minimize_bg: Color,
    /// Minimize button hover
    pub minimize_hover: Color,
}

impl Default for DecorationColors {
    fn default() -> Self {
        Self {
            close_bg: Color::from_hex("#ff5555").unwrap_or(Color::RED),
            close_hover: Color::from_hex("#ff6666").unwrap_or(Color::RED),
            maximize_bg: Color::from_hex("#55ff55").unwrap_or(Color::GREEN),
            maximize_hover: Color::from_hex("#66ff66").unwrap_or(Color::GREEN),
            minimize_bg: Color::from_hex("#ffff55").unwrap_or(Color::WHITE),
            minimize_hover: Color::from_hex("#ffff66").unwrap_or(Color::WHITE),
        }
    }
}

/// The main compositor - manages windows and rendering
pub struct Compositor {
    /// All windows managed by the compositor
    windows: Vec<Window>,
    /// Map from window ID to index in windows vec
    window_map: HashMap<WindowId, usize>,
    /// Next window ID to assign
    next_window_id: u64,
    /// The tiling layout
    layout: TilingLayout,
    /// WebGPU surface (only on wasm32)
    #[cfg(target_arch = "wasm32")]
    surface: Option<Surface>,
    /// Currently focused window index
    focused: Option<usize>,
    /// Visual theme
    theme: Theme,
    /// Dirty flag - needs redraw
    dirty: bool,
}

impl Compositor {
    /// Create a new compositor
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
            window_map: HashMap::new(),
            next_window_id: 1,
            layout: TilingLayout::new(Rect::new(0.0, 0.0, 800.0, 600.0)),
            #[cfg(target_arch = "wasm32")]
            surface: None,
            focused: None,
            theme: Theme::default(),
            dirty: true,
        }
    }

    /// Create a new window
    pub fn create_window(&mut self, title: &str, owner: TaskId) -> WindowId {
        let id = WindowId(self.next_window_id);
        self.next_window_id += 1;

        let window = Window::new(id, title.to_string(), owner);
        let index = self.windows.len();
        self.windows.push(window);
        self.window_map.insert(id, index);

        // Add to layout
        self.layout.add_window(id);
        self.update_window_rects();

        // Focus the new window
        self.focused = Some(index);
        self.dirty = true;

        id
    }

    /// Close a window by ID
    pub fn close_window(&mut self, id: WindowId) -> bool {
        if let Some(&index) = self.window_map.get(&id) {
            // Remove from layout
            self.layout.remove_window(id);

            // Remove from windows vec
            self.windows.remove(index);
            self.window_map.remove(&id);

            // Update indices in window_map
            self.window_map.clear();
            for (i, window) in self.windows.iter().enumerate() {
                self.window_map.insert(window.id, i);
            }

            // Update focus
            if let Some(focused_idx) = self.focused {
                if focused_idx == index {
                    // Focused window was closed
                    self.focused = if self.windows.is_empty() {
                        None
                    } else {
                        Some(self.windows.len().saturating_sub(1))
                    };
                } else if focused_idx > index {
                    self.focused = Some(focused_idx - 1);
                }
            }

            self.update_window_rects();
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Get a window by ID
    pub fn get_window(&self, id: WindowId) -> Option<&Window> {
        self.window_map.get(&id).map(|&idx| &self.windows[idx])
    }

    /// Get a mutable window by ID
    pub fn get_window_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        if let Some(&idx) = self.window_map.get(&id) {
            Some(&mut self.windows[idx])
        } else {
            None
        }
    }

    /// Get the focused window
    pub fn focused_window(&self) -> Option<&Window> {
        self.focused.map(|idx| &self.windows[idx])
    }

    /// Get the focused window mutably
    pub fn focused_window_mut(&mut self) -> Option<&mut Window> {
        if let Some(idx) = self.focused {
            Some(&mut self.windows[idx])
        } else {
            None
        }
    }

    /// Get the focused window ID
    pub fn focused_window_id(&self) -> Option<WindowId> {
        self.focused.map(|idx| self.windows[idx].id)
    }

    /// Focus a window by ID
    pub fn focus_window(&mut self, id: WindowId) -> bool {
        if let Some(&idx) = self.window_map.get(&id) {
            self.focused = Some(idx);
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Focus next window (for keyboard navigation)
    pub fn focus_next(&mut self) {
        if self.windows.is_empty() {
            return;
        }
        self.focused = Some(match self.focused {
            Some(idx) => (idx + 1) % self.windows.len(),
            None => 0,
        });
        self.dirty = true;
    }

    /// Focus previous window
    pub fn focus_prev(&mut self) {
        if self.windows.is_empty() {
            return;
        }
        self.focused = Some(match self.focused {
            Some(idx) => {
                if idx == 0 {
                    self.windows.len() - 1
                } else {
                    idx - 1
                }
            }
            None => 0,
        });
        self.dirty = true;
    }

    /// Handle a mouse click at (x, y)
    pub fn handle_click(&mut self, x: f64, y: f64, _button: i16) {
        // Find which window was clicked
        for (i, window) in self.windows.iter().enumerate() {
            if window.rect.contains(x, y) {
                self.focused = Some(i);
                self.dirty = true;
                break;
            }
        }
    }

    /// Handle window resize
    pub fn resize(&mut self, width: u32, height: u32) {
        self.layout
            .set_bounds(Rect::new(0.0, 0.0, width as f64, height as f64));
        self.update_window_rects();

        #[cfg(target_arch = "wasm32")]
        if let Some(surface) = &mut self.surface {
            surface.resize(width, height);
        }

        self.dirty = true;
    }

    /// Update window rectangles from the layout
    fn update_window_rects(&mut self) {
        let rects = self.layout.calculate_rects();
        for (id, rect) in rects {
            if let Some(&idx) = self.window_map.get(&id) {
                self.windows[idx].rect = rect;
            }
        }
    }

    /// Get the number of windows
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    /// Get all window IDs
    pub fn window_ids(&self) -> Vec<WindowId> {
        self.windows.iter().map(|w| w.id).collect()
    }

    /// Check if compositor needs redraw
    pub fn is_dirty(&self) -> bool {
        self.dirty || self.windows.iter().any(|w| w.dirty)
    }

    /// Mark as clean after render
    pub fn mark_clean(&mut self) {
        self.dirty = false;
        for window in &mut self.windows {
            window.dirty = false;
        }
    }

    /// Get the theme
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Set the theme
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.dirty = true;
    }

    /// Get an iterator over all windows
    pub fn windows(&self) -> impl Iterator<Item = &Window> {
        self.windows.iter()
    }

    /// Get the layout
    pub fn layout(&self) -> &TilingLayout {
        &self.layout
    }

    /// Get mutable layout
    pub fn layout_mut(&mut self) -> &mut TilingLayout {
        &mut self.layout
    }
}

impl Default for Compositor {
    fn default() -> Self {
        Self::new()
    }
}

// WASM-specific implementations
#[cfg(target_arch = "wasm32")]
impl Compositor {
    /// Initialize the compositor with a canvas element
    pub async fn init(&mut self) -> Result<(), String> {
        let surface = Surface::from_canvas_id("canvas").await?;
        self.surface = Some(surface);
        Ok(())
    }

    /// Initialize with a specific canvas ID
    pub async fn init_with_canvas(&mut self, canvas_id: &str) -> Result<(), String> {
        let surface = Surface::from_canvas_id(canvas_id).await?;
        self.surface = Some(surface);
        Ok(())
    }

    /// Check if the compositor is initialized
    pub fn is_initialized(&self) -> bool {
        self.surface.is_some()
    }

    /// Get the surface
    pub fn surface(&self) -> Option<&Surface> {
        self.surface.as_ref()
    }

    /// Get the surface mutably
    pub fn surface_mut(&mut self) -> Option<&mut Surface> {
        self.surface.as_mut()
    }

    /// Render all windows using WebGPU
    pub fn render(&mut self) {
        if !self.is_dirty() {
            return;
        }

        if let Some(surface) = &mut self.surface {
            // Clear the surface
            surface.clear();

            // Draw each window
            for (i, window) in self.windows.iter().enumerate() {
                if !window.flags.visible {
                    continue;
                }

                let is_focused = self.focused == Some(i);
                let rect = window.rect;

                // Determine border color based on focus
                let border_color = if is_focused {
                    self.theme.focus_border
                } else {
                    self.theme.unfocus_border
                };

                // Draw window with border
                surface.draw_rect_with_border(
                    rect,
                    self.theme.window_bg,
                    border_color,
                    self.theme.border_width,
                );

                // Draw title bar
                if window.flags.decorated {
                    let titlebar = window.titlebar_rect();
                    surface.draw_rect(titlebar, self.theme.titlebar_bg);
                }
            }

            // Submit all queued rectangles to GPU
            surface.render(self.theme.background);
        }

        self.mark_clean();
    }
}

// Global compositor instance
thread_local! {
    /// The global compositor instance
    pub static COMPOSITOR: RefCell<Compositor> = RefCell::new(Compositor::new());
}

/// Render the compositor (call from requestAnimationFrame)
#[cfg(target_arch = "wasm32")]
pub fn render() {
    COMPOSITOR.with(|c| c.borrow_mut().render());
}

/// Handle a click event
pub fn handle_click(x: f64, y: f64, button: i16) {
    COMPOSITOR.with(|c| c.borrow_mut().handle_click(x, y, button));
}

/// Handle resize event
pub fn handle_resize(width: u32, height: u32) {
    COMPOSITOR.with(|c| c.borrow_mut().resize(width, height));
}

/// Create a new window
pub fn create_window(title: &str, owner: TaskId) -> WindowId {
    COMPOSITOR.with(|c| c.borrow_mut().create_window(title, owner))
}

/// Close a window
pub fn close_window(id: WindowId) -> bool {
    COMPOSITOR.with(|c| c.borrow_mut().close_window(id))
}

/// Focus a window
pub fn focus_window(id: WindowId) -> bool {
    COMPOSITOR.with(|c| c.borrow_mut().focus_window(id))
}

/// Get focused window ID
pub fn focused_window_id() -> Option<WindowId> {
    COMPOSITOR.with(|c| c.borrow().focused_window_id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_window() {
        let mut comp = Compositor::new();
        let id = comp.create_window("Test Window", TaskId(1));

        assert_eq!(comp.window_count(), 1);
        assert!(comp.get_window(id).is_some());
        assert_eq!(comp.get_window(id).unwrap().title, "Test Window");
    }

    #[test]
    fn test_close_window() {
        let mut comp = Compositor::new();
        let id = comp.create_window("Test", TaskId(1));

        assert!(comp.close_window(id));
        assert_eq!(comp.window_count(), 0);
        assert!(comp.get_window(id).is_none());
    }

    #[test]
    fn test_focus_management() {
        let mut comp = Compositor::new();
        let id1 = comp.create_window("Window 1", TaskId(1));
        let id2 = comp.create_window("Window 2", TaskId(2));

        // Most recently created window is focused
        assert_eq!(comp.focused_window_id(), Some(id2));

        // Focus first window
        comp.focus_window(id1);
        assert_eq!(comp.focused_window_id(), Some(id1));

        // Focus next cycles
        comp.focus_next();
        assert_eq!(comp.focused_window_id(), Some(id2));
        comp.focus_next();
        assert_eq!(comp.focused_window_id(), Some(id1));
    }

    #[test]
    fn test_click_focus() {
        let mut comp = Compositor::new();
        comp.resize(800, 600);

        let id1 = comp.create_window("Window 1", TaskId(1));
        let _id2 = comp.create_window("Window 2", TaskId(2));

        // Get the rect of window 1
        let rect = comp.get_window(id1).unwrap().rect;

        // Click in window 1
        comp.handle_click(rect.x + 10.0, rect.y + 10.0, 0);
        assert_eq!(comp.focused_window_id(), Some(id1));
    }

    #[test]
    fn test_multiple_windows_layout() {
        let mut comp = Compositor::new();
        comp.resize(800, 600);

        let id1 = comp.create_window("W1", TaskId(1));
        let id2 = comp.create_window("W2", TaskId(2));
        let id3 = comp.create_window("W3", TaskId(3));

        // All windows should have non-zero rects
        assert!(comp.get_window(id1).unwrap().rect.width > 0.0);
        assert!(comp.get_window(id2).unwrap().rect.width > 0.0);
        assert!(comp.get_window(id3).unwrap().rect.width > 0.0);
    }

    // ========================================================================
    // Theme Tests
    // ========================================================================

    #[test]
    fn test_theme_presets() {
        let dark = Theme::dark();
        let light = Theme::light();

        // Dark theme should have dark background
        assert!(dark.background.r < 0.5);
        // Light theme should have light background
        assert!(light.background.r > 0.5);
    }

    #[test]
    fn test_theme_by_name() {
        assert!(Theme::by_name("dark").is_some());
        assert!(Theme::by_name("light").is_some());
        assert!(Theme::by_name("nord").is_some());
        assert!(Theme::by_name("monokai").is_some());
        assert!(Theme::by_name("high-contrast").is_some());
        assert!(Theme::by_name("nonexistent").is_none());
    }

    #[test]
    fn test_available_themes() {
        let themes = Theme::available_themes();
        assert!(themes.contains(&"dark"));
        assert!(themes.contains(&"light"));
    }

    #[test]
    fn test_set_theme() {
        let mut comp = Compositor::new();
        let light = Theme::light();
        comp.set_theme(light.clone());

        assert!(comp.theme().background.r > 0.5);
        assert!(comp.is_dirty());
    }

    // ========================================================================
    // Animation Tests
    // ========================================================================

    #[test]
    fn test_easing_functions() {
        // Linear: start and end match
        assert_eq!(ease_linear(0.0), 0.0);
        assert_eq!(ease_linear(1.0), 1.0);

        // Ease out: faster at start
        assert!(ease_out(0.5) > 0.5);

        // Ease in: slower at start
        assert!(ease_in(0.5) < 0.5);

        // Ease in-out: symmetric
        let mid = ease_in_out(0.5);
        assert!((mid - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_animation_value_at() {
        let anim = Animation::new(AnimationProperty::Opacity, 0.0, 1.0, 100.0)
            .with_start_time(0.0)
            .with_easing(ease_linear);

        assert_eq!(anim.value_at(0.0), 0.0);
        assert_eq!(anim.value_at(50.0), 0.5);
        assert_eq!(anim.value_at(100.0), 1.0);
        assert_eq!(anim.value_at(150.0), 1.0); // Past end
    }

    #[test]
    fn test_animation_is_complete() {
        let anim = Animation::new(AnimationProperty::Scale, 1.0, 2.0, 100.0).with_start_time(0.0);

        assert!(!anim.is_complete(50.0));
        assert!(anim.is_complete(100.0));
        assert!(anim.is_complete(150.0));
    }

    #[test]
    fn test_window_animation_state() {
        let mut state = WindowAnimationState::new();
        assert_eq!(state.opacity, 1.0);
        assert_eq!(state.scale, 1.0);

        // Add opacity animation
        state.add_animation(
            Animation::new(AnimationProperty::Opacity, 0.0, 1.0, 100.0)
                .with_start_time(0.0)
                .with_easing(ease_linear),
        );

        // Update at halfway
        let running = state.update(50.0);
        assert!(running);
        assert!((state.opacity - 0.5).abs() < 0.01);

        // Update at end
        let running = state.update(100.0);
        assert!(!running); // Animation complete
        assert!((state.opacity - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_window_open_animation() {
        let state = WindowAnimationState::window_open(0.0);
        assert!(state.opacity < 1.0); // Should start invisible
        assert!(state.scale < 1.0); // Should start smaller
        assert_eq!(state.animations.len(), 2); // Opacity and scale
    }

    #[test]
    fn test_window_close_animation() {
        let state = WindowAnimationState::window_close(0.0);
        assert_eq!(state.animations.len(), 2); // Opacity and scale
    }

    // ========================================================================
    // Window Decoration Tests
    // ========================================================================

    #[test]
    fn test_decoration_button_positions() {
        let window_rect = Rect::new(100.0, 100.0, 400.0, 300.0);

        let close_rect = decoration_button_rect(&window_rect, DecorationButton::Close);
        let max_rect = decoration_button_rect(&window_rect, DecorationButton::Maximize);
        let min_rect = decoration_button_rect(&window_rect, DecorationButton::Minimize);

        // Close should be rightmost
        assert!(close_rect.x > max_rect.x);
        // Maximize should be in the middle
        assert!(max_rect.x > min_rect.x);
        // All should be in the titlebar area (top of window)
        assert!(close_rect.y < window_rect.y + 30.0);

        // All should have the same size
        assert_eq!(close_rect.width, DECORATION_BUTTON_SIZE);
        assert_eq!(max_rect.width, DECORATION_BUTTON_SIZE);
        assert_eq!(min_rect.width, DECORATION_BUTTON_SIZE);
    }

    #[test]
    fn test_decoration_colors() {
        let colors = DecorationColors::default();

        // Close should be reddish
        assert!(colors.close_bg.r > colors.close_bg.g);
        // Maximize should be greenish
        assert!(colors.maximize_bg.g > colors.maximize_bg.r);
    }
}
