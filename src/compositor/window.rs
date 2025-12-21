//! Window abstraction
//!
//! A window is a rectangular region of the screen owned by a task.
//! Windows don't overlap (tiling WM) and are managed by the compositor.

use super::Rect;
use crate::kernel::TaskId;

/// Unique window identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub u64);

impl std::fmt::Display for WindowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Window({})", self.0)
    }
}

/// A window in the compositor
pub struct Window {
    /// Unique identifier
    pub id: WindowId,

    /// Window title
    pub title: String,

    /// Task that owns this window
    pub owner: TaskId,

    /// Current bounds (set by layout)
    pub bounds: Rect,

    /// Is window visible?
    pub visible: bool,

    /// Needs redraw?
    pub dirty: bool,
}

impl Window {
    pub fn new(id: WindowId, title: String, owner: TaskId) -> Self {
        Self {
            id,
            title,
            owner,
            bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
            visible: true,
            dirty: true,
        }
    }

    /// Clone the window data needed for rendering
    pub fn clone_for_render(&self) -> Self {
        Self {
            id: self.id,
            title: self.title.clone(),
            owner: self.owner,
            bounds: self.bounds,
            visible: self.visible,
            dirty: self.dirty,
        }
    }

    /// Mark window as needing redraw
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// Clear dirty flag
    pub fn validate(&mut self) {
        self.dirty = false;
    }

    /// Get content area (bounds minus decorations)
    pub fn content_rect(&self) -> Rect {
        // 2px border + 24px title bar
        Rect::new(
            self.bounds.x + 2.0,
            self.bounds.y + 2.0 + 24.0,
            (self.bounds.width - 4.0).max(0.0),
            (self.bounds.height - 4.0 - 24.0).max(0.0),
        )
    }

    /// Get title bar rect
    pub fn title_rect(&self) -> Rect {
        Rect::new(
            self.bounds.x + 2.0,
            self.bounds.y + 2.0,
            (self.bounds.width - 4.0).max(0.0),
            24.0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_creation() {
        let window = Window::new(WindowId(0), "Test".to_string(), TaskId(1));
        assert_eq!(window.id, WindowId(0));
        assert_eq!(window.title, "Test");
        assert_eq!(window.owner, TaskId(1));
        assert!(window.visible);
        assert!(window.dirty);
    }

    #[test]
    fn test_content_rect() {
        let mut window = Window::new(WindowId(0), "Test".to_string(), TaskId(1));
        window.bounds = Rect::new(0.0, 0.0, 200.0, 150.0);

        let content = window.content_rect();
        assert_eq!(content.x, 2.0);
        assert_eq!(content.y, 26.0); // 2 + 24 title bar
        assert_eq!(content.width, 196.0);
        assert_eq!(content.height, 122.0); // 150 - 4 - 24 = 122
    }

    #[test]
    fn test_invalidate() {
        let mut window = Window::new(WindowId(0), "Test".to_string(), TaskId(1));
        window.validate();
        assert!(!window.dirty);

        window.invalidate();
        assert!(window.dirty);
    }
}
