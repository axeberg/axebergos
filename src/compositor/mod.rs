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
mod surface;
mod window;

pub use geometry::{Color, Point, Rect};
pub use layout::{LayoutNode, SplitDirection, TilingLayout};
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
                self.queue_window_draw(surface, window, is_focused);
            }

            // Submit all queued rectangles to GPU
            surface.render(self.theme.background);
        }

        self.mark_clean();
    }

    /// Queue draw commands for a window
    fn queue_window_draw(&self, surface: &mut Surface, window: &Window, is_focused: bool) {
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

        // Note: Text rendering requires a separate text rendering system
        // For now, we only render the window frames. Text can be added later
        // using a glyph atlas and texture sampling in the shader.
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
}
