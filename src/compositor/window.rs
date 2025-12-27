//! Window types for the compositor
//!
//! A window represents a rectangular region on screen that belongs to a task.

use super::geometry::Rect;
use crate::kernel::TaskId;

/// Unique identifier for a window
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub u64);

impl WindowId {
    /// Create a new window ID
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw ID value
    pub const fn raw(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for WindowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Window({})", self.0)
    }
}

/// Window state flags
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WindowFlags {
    /// Window is visible
    pub visible: bool,
    /// Window can receive focus
    pub focusable: bool,
    /// Window has decorations (title bar, border)
    pub decorated: bool,
    /// Window is maximized
    pub maximized: bool,
    /// Window is minimized
    pub minimized: bool,
}

impl WindowFlags {
    /// Default flags for a normal window
    pub fn normal() -> Self {
        Self {
            visible: true,
            focusable: true,
            decorated: true,
            maximized: false,
            minimized: false,
        }
    }

    /// Flags for an undecorated window (like a popup)
    pub fn undecorated() -> Self {
        Self {
            visible: true,
            focusable: true,
            decorated: false,
            maximized: false,
            minimized: false,
        }
    }
}

/// A window in the compositor
#[derive(Debug, Clone)]
pub struct Window {
    /// Unique window identifier
    pub id: WindowId,
    /// Window title (shown in title bar)
    pub title: String,
    /// Owning task
    pub owner: TaskId,
    /// Position and size
    pub rect: Rect,
    /// Window flags
    pub flags: WindowFlags,
    /// Text content to display
    pub content: Vec<String>,
    /// Scroll offset for content
    pub scroll_offset: usize,
    /// Needs redraw
    pub dirty: bool,
}

impl Window {
    /// Create a new window
    pub fn new(id: WindowId, title: String, owner: TaskId) -> Self {
        Self {
            id,
            title,
            owner,
            rect: Rect::default(),
            flags: WindowFlags::normal(),
            content: Vec::new(),
            scroll_offset: 0,
            dirty: true,
        }
    }

    /// Create a window with custom flags
    pub fn with_flags(id: WindowId, title: String, owner: TaskId, flags: WindowFlags) -> Self {
        Self {
            id,
            title,
            owner,
            rect: Rect::default(),
            flags,
            content: Vec::new(),
            scroll_offset: 0,
            dirty: true,
        }
    }

    /// Title bar height in pixels
    pub const TITLEBAR_HEIGHT: f64 = 24.0;

    /// Border width in pixels
    pub const BORDER_WIDTH: f64 = 2.0;

    /// Get the content area (inside decorations)
    pub fn content_rect(&self) -> Rect {
        if self.flags.decorated {
            Rect::new(
                self.rect.x + Self::BORDER_WIDTH,
                self.rect.y + Self::TITLEBAR_HEIGHT,
                (self.rect.width - 2.0 * Self::BORDER_WIDTH).max(0.0),
                (self.rect.height - Self::TITLEBAR_HEIGHT - Self::BORDER_WIDTH).max(0.0),
            )
        } else {
            self.rect
        }
    }

    /// Get the title bar area
    pub fn titlebar_rect(&self) -> Rect {
        if self.flags.decorated {
            Rect::new(self.rect.x, self.rect.y, self.rect.width, Self::TITLEBAR_HEIGHT)
        } else {
            Rect::new(0.0, 0.0, 0.0, 0.0)
        }
    }

    /// Check if a point is in the title bar
    pub fn is_in_titlebar(&self, x: f64, y: f64) -> bool {
        self.flags.decorated && self.titlebar_rect().contains(x, y)
    }

    /// Check if a point is in the content area
    pub fn is_in_content(&self, x: f64, y: f64) -> bool {
        self.content_rect().contains(x, y)
    }

    /// Append a line of content
    pub fn append_line(&mut self, line: String) {
        self.content.push(line);
        self.dirty = true;
    }

    /// Clear all content
    pub fn clear_content(&mut self) {
        self.content.clear();
        self.scroll_offset = 0;
        self.dirty = true;
    }

    /// Set the title
    pub fn set_title(&mut self, title: String) {
        self.title = title;
        self.dirty = true;
    }

    /// Scroll up by n lines
    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
        self.dirty = true;
    }

    /// Scroll down by n lines
    pub fn scroll_down(&mut self, lines: usize) {
        let max_scroll = self.content.len().saturating_sub(1);
        self.scroll_offset = (self.scroll_offset + lines).min(max_scroll);
        self.dirty = true;
    }

    /// Get visible content lines based on window height
    pub fn visible_content(&self) -> impl Iterator<Item = &String> {
        let line_height = 18.0; // pixels per line
        let content_height = self.content_rect().height;
        let visible_lines = (content_height / line_height) as usize;

        self.content
            .iter()
            .skip(self.scroll_offset)
            .take(visible_lines.max(1))
    }

    /// Show the window
    pub fn show(&mut self) {
        self.flags.visible = true;
        self.dirty = true;
    }

    /// Hide the window
    pub fn hide(&mut self) {
        self.flags.visible = false;
        self.dirty = true;
    }

    /// Maximize the window
    pub fn maximize(&mut self) {
        self.flags.maximized = true;
        self.flags.minimized = false;
        self.dirty = true;
    }

    /// Minimize the window
    pub fn minimize(&mut self) {
        self.flags.minimized = true;
        self.dirty = true;
    }

    /// Restore the window from maximized/minimized state
    pub fn restore(&mut self) {
        self.flags.maximized = false;
        self.flags.minimized = false;
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_creation() {
        let window = Window::new(WindowId(1), "Test".to_string(), TaskId(1));
        assert_eq!(window.id.0, 1);
        assert_eq!(window.title, "Test");
        assert!(window.flags.visible);
        assert!(window.flags.decorated);
    }

    #[test]
    fn test_content_rect() {
        let mut window = Window::new(WindowId(1), "Test".to_string(), TaskId(1));
        window.rect = Rect::new(0.0, 0.0, 200.0, 150.0);

        let content = window.content_rect();
        assert_eq!(content.x, Window::BORDER_WIDTH);
        assert_eq!(content.y, Window::TITLEBAR_HEIGHT);
        assert_eq!(content.width, 200.0 - 2.0 * Window::BORDER_WIDTH);
    }

    #[test]
    fn test_titlebar_detection() {
        let mut window = Window::new(WindowId(1), "Test".to_string(), TaskId(1));
        window.rect = Rect::new(10.0, 10.0, 200.0, 150.0);

        // In title bar
        assert!(window.is_in_titlebar(50.0, 20.0));
        // In content
        assert!(!window.is_in_titlebar(50.0, 50.0));
        // Outside window
        assert!(!window.is_in_titlebar(5.0, 5.0));
    }

    #[test]
    fn test_window_content() {
        let mut window = Window::new(WindowId(1), "Test".to_string(), TaskId(1));
        window.append_line("Line 1".to_string());
        window.append_line("Line 2".to_string());

        assert_eq!(window.content.len(), 2);
        assert!(window.dirty);

        window.clear_content();
        assert!(window.content.is_empty());
    }

    #[test]
    fn test_scrolling() {
        let mut window = Window::new(WindowId(1), "Test".to_string(), TaskId(1));
        for i in 0..100 {
            window.append_line(format!("Line {}", i));
        }

        window.scroll_down(10);
        assert_eq!(window.scroll_offset, 10);

        window.scroll_up(5);
        assert_eq!(window.scroll_offset, 5);

        window.scroll_up(100);
        assert_eq!(window.scroll_offset, 0);
    }

    #[test]
    fn test_undecorated_window() {
        let mut window = Window::with_flags(
            WindowId(1),
            "Popup".to_string(),
            TaskId(1),
            WindowFlags::undecorated(),
        );
        window.rect = Rect::new(0.0, 0.0, 100.0, 100.0);

        // Undecorated window has no title bar
        assert!(!window.flags.decorated);
        assert_eq!(window.titlebar_rect().area(), 0.0);

        // Content rect is the full window
        let content = window.content_rect();
        assert_eq!(content.x, 0.0);
        assert_eq!(content.y, 0.0);
        assert_eq!(content.width, 100.0);
        assert_eq!(content.height, 100.0);
    }
}
