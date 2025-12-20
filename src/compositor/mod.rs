//! The compositor - tiling window manager
//!
//! Design:
//! - Canvas2D rendering (WebGPU upgrade later)
//! - Tiling layout (no overlapping windows)
//! - Each window is owned by a task
//! - Compositor runs as a Critical priority task

pub mod layout;
pub mod surface;
pub mod window;

pub use layout::{Layout, LayoutNode, Split};
pub use surface::Surface;
pub use window::{Window, WindowId};

use crate::console_log;
use crate::kernel::{events, TaskId};
use crate::shell::Terminal;
use std::cell::RefCell;
use std::collections::HashMap;

/// Color in linear RGBA (0.0 - 1.0)
#[derive(Debug, Clone, Copy)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    // Some nice defaults (Tokyo Night inspired)
    pub const BACKGROUND: Self = Self::rgb(0.1, 0.1, 0.15);
    pub const SURFACE: Self = Self::rgb(0.15, 0.15, 0.2);
    pub const BORDER: Self = Self::rgb(0.3, 0.3, 0.4);
    pub const ACCENT: Self = Self::rgb(0.48, 0.63, 0.97);
    pub const TEXT: Self = Self::rgb(0.78, 0.8, 0.85);
}

/// Rectangle in screen coordinates
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }

    /// Split horizontally at ratio (0.0 - 1.0)
    pub fn split_horizontal(&self, ratio: f32) -> (Rect, Rect) {
        let split_x = self.x + self.width * ratio;
        (
            Rect::new(self.x, self.y, self.width * ratio, self.height),
            Rect::new(split_x, self.y, self.width * (1.0 - ratio), self.height),
        )
    }

    /// Split vertically at ratio (0.0 - 1.0)
    pub fn split_vertical(&self, ratio: f32) -> (Rect, Rect) {
        let split_y = self.y + self.height * ratio;
        (
            Rect::new(self.x, self.y, self.width, self.height * ratio),
            Rect::new(self.x, split_y, self.width, self.height * (1.0 - ratio)),
        )
    }

    /// Inset by padding on all sides
    pub fn inset(&self, padding: f32) -> Rect {
        Rect::new(
            self.x + padding,
            self.y + padding,
            (self.width - padding * 2.0).max(0.0),
            (self.height - padding * 2.0).max(0.0),
        )
    }
}

/// The compositor state
pub struct Compositor {
    /// Rendering surface (WebGPU or Canvas2D fallback)
    surface: Option<Surface>,

    /// All windows
    windows: HashMap<WindowId, Window>,

    /// Terminal state for terminal windows
    terminals: HashMap<WindowId, Terminal>,

    /// Next window ID
    next_window_id: u64,

    /// The layout tree
    layout: Layout,

    /// Screen dimensions
    width: u32,
    height: u32,

    /// Currently focused window
    focused: Option<WindowId>,

    /// Is compositor initialized?
    initialized: bool,
}

impl Compositor {
    pub fn new() -> Self {
        Self {
            surface: None,
            windows: HashMap::new(),
            terminals: HashMap::new(),
            next_window_id: 0,
            layout: Layout::new(),
            width: 800,
            height: 600,
            focused: None,
            initialized: false,
        }
    }

    /// Initialize the compositor (creates WebGPU surface)
    pub async fn init(&mut self) -> Result<(), String> {
        if self.initialized {
            return Ok(());
        }

        console_log!("[compositor] Initializing...");

        // Get window dimensions
        if let Some(window) = web_sys::window() {
            self.width = window
                .inner_width()
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(800.0) as u32;
            self.height = window
                .inner_height()
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(600.0) as u32;
        }

        // Create rendering surface
        match Surface::new(self.width, self.height).await {
            Ok(surface) => {
                self.surface = Some(surface);
                console_log!(
                    "[compositor] Surface created ({}x{})",
                    self.width,
                    self.height
                );
            }
            Err(e) => {
                console_log!("[compositor] Failed to create surface: {}", e);
                return Err(e);
            }
        }

        self.initialized = true;
        Ok(())
    }

    /// Create a new window
    pub fn create_window(&mut self, title: &str, owner: TaskId) -> WindowId {
        let id = WindowId(self.next_window_id);
        self.next_window_id += 1;

        let window = Window::new(id, title.to_string(), owner);
        self.windows.insert(id, window);

        // Add to layout
        self.layout.add_window(id);
        self.recalculate_layout();

        // Focus the new window
        self.focused = Some(id);

        console_log!("[compositor] Created window {} '{}'", id.0, title);
        id
    }

    /// Create a new terminal window
    pub fn create_terminal_window(&mut self, title: &str, owner: TaskId) -> WindowId {
        let id = self.create_window(title, owner);
        self.terminals.insert(id, Terminal::new());
        console_log!("[compositor] Window {} is a terminal", id.0);
        id
    }

    /// Close a window
    pub fn close_window(&mut self, id: WindowId) {
        if self.windows.remove(&id).is_some() {
            self.terminals.remove(&id);
            self.layout.remove_window(id);
            self.recalculate_layout();

            if self.focused == Some(id) {
                self.focused = self.windows.keys().next().copied();
            }

            console_log!("[compositor] Closed window {}", id.0);
        }
    }

    /// Handle keyboard input - forwards to focused terminal if applicable
    pub fn handle_key(&mut self, key: &str, code: &str, ctrl: bool, alt: bool) -> bool {
        if let Some(focused_id) = self.focused {
            if let Some(terminal) = self.terminals.get_mut(&focused_id) {
                return terminal.handle_key(key, code, ctrl, alt);
            }
        }
        false
    }

    /// Handle resize
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == self.width && height == self.height {
            return;
        }

        self.width = width;
        self.height = height;

        if let Some(ref mut surface) = self.surface {
            surface.resize(width, height);
        }

        self.recalculate_layout();
        console_log!("[compositor] Resized to {}x{}", width, height);
    }

    /// Recalculate window positions based on layout
    fn recalculate_layout(&mut self) {
        let screen = Rect::new(0.0, 0.0, self.width as f32, self.height as f32);
        let rects = self.layout.calculate(screen);

        for (id, rect) in rects {
            if let Some(window) = self.windows.get_mut(&id) {
                window.bounds = rect;
            }
        }
    }

    /// Get window at screen position
    pub fn window_at(&self, x: f32, y: f32) -> Option<WindowId> {
        for (id, window) in &self.windows {
            if window.bounds.contains(x, y) {
                return Some(*id);
            }
        }
        None
    }

    /// Handle mouse click
    pub fn handle_click(&mut self, x: f64, y: f64, _button: events::MouseButton) {
        if let Some(id) = self.window_at(x as f32, y as f32) {
            self.focused = Some(id);
        }
    }

    /// Render a frame
    pub fn render(&mut self) {
        let Some(ref mut surface) = self.surface else {
            return;
        };

        // Begin frame
        let Some(frame) = surface.begin_frame() else {
            return;
        };

        // Clear to background
        surface.clear(&frame, Color::BACKGROUND);

        // Draw each window
        for (id, window) in &self.windows {
            let is_focused = self.focused == Some(*id);
            render_window(surface, &frame, window, is_focused);

            // Render terminal content if this window has a terminal
            if let Some(terminal) = self.terminals.get(id) {
                render_terminal(surface, &frame, window, terminal, is_focused);
            }
        }

        // End frame
        surface.end_frame(frame);
    }

    /// Get current dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Is initialized?
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get focused window
    pub fn focused_window(&self) -> Option<WindowId> {
        self.focused
    }
}

impl Default for Compositor {
    fn default() -> Self {
        Self::new()
    }
}

/// Render a single window (standalone to avoid borrow issues)
fn render_window(surface: &mut Surface, frame: &surface::Frame, window: &Window, focused: bool) {
    let border_color = if focused {
        Color::ACCENT
    } else {
        Color::BORDER
    };

    // Draw border
    surface.draw_rect(frame, window.bounds, border_color);

    // Draw content area (inset by border)
    let content = window.bounds.inset(2.0);
    surface.draw_rect(frame, content, Color::SURFACE);

    // Draw title bar
    let title_bar = Rect::new(content.x, content.y, content.width, 24.0);
    let title_color = if focused {
        Color::rgba(0.2, 0.2, 0.3, 1.0)
    } else {
        Color::rgba(0.15, 0.15, 0.2, 1.0)
    };
    surface.draw_rect(frame, title_bar, title_color);

    // Draw window title
    surface.draw_text(
        frame,
        &window.title,
        title_bar.x + 8.0,
        title_bar.y + 16.0,
        Color::TEXT,
        14.0,
    );
}

/// Terminal rendering constants
const TERM_FONT_SIZE: f32 = 14.0;
const TERM_LINE_HEIGHT: f32 = 18.0;
const TERM_PADDING: f32 = 8.0;

/// Render terminal content inside a window
fn render_terminal(
    surface: &mut Surface,
    frame: &surface::Frame,
    window: &Window,
    terminal: &Terminal,
    focused: bool,
) {
    let content = window.content_rect();

    // Calculate visible rows
    let visible_rows = ((content.height - TERM_PADDING * 2.0) / TERM_LINE_HEIGHT) as usize;

    // Update terminal's visible rows (for scroll calculation)
    // Note: terminal is immutable here, so we set this elsewhere

    // Draw scrollback lines
    let mut y = content.y + TERM_PADDING + TERM_LINE_HEIGHT;
    for line in terminal.visible_lines().take(visible_rows.saturating_sub(1)) {
        let color = if line.is_input {
            Color::rgb(0.6, 0.8, 0.6) // Greenish for user input
        } else {
            Color::TEXT
        };
        surface.draw_text(frame, &line.text, content.x + TERM_PADDING, y, color, TERM_FONT_SIZE);
        y += TERM_LINE_HEIGHT;
    }

    // Draw input line at bottom
    let (prompt, input, cursor_pos) = terminal.input_line();
    let input_y = content.y + content.height - TERM_PADDING - 4.0;

    // Draw prompt
    surface.draw_text(
        frame,
        prompt,
        content.x + TERM_PADDING,
        input_y,
        Color::rgb(0.5, 0.7, 0.9), // Blueish prompt
        TERM_FONT_SIZE,
    );

    // Draw input text
    let prompt_width = prompt.len() as f32 * 8.4; // Approximate char width
    surface.draw_text(
        frame,
        input,
        content.x + TERM_PADDING + prompt_width,
        input_y,
        Color::TEXT,
        TERM_FONT_SIZE,
    );

    // Draw cursor (blinking effect could be added via frame count)
    if focused {
        let cursor_x = content.x + TERM_PADDING + prompt_width + (cursor_pos as f32 * 8.4);
        let cursor_rect = Rect::new(cursor_x, input_y - TERM_LINE_HEIGHT + 4.0, 2.0, TERM_LINE_HEIGHT);
        surface.draw_rect(frame, cursor_rect, Color::ACCENT);
    }
}

// Global compositor instance
thread_local! {
    pub static COMPOSITOR: RefCell<Compositor> = RefCell::new(Compositor::new());
}

/// Initialize the global compositor
pub async fn init() -> Result<(), String> {
    // We need to use a different approach for async init in thread_local
    // For now, we'll handle this in the compositor task
    Ok(())
}

/// Create a window on the global compositor
pub fn create_window(title: &str, owner: TaskId) -> WindowId {
    COMPOSITOR.with(|c| c.borrow_mut().create_window(title, owner))
}

/// Close a window
pub fn close_window(id: WindowId) {
    COMPOSITOR.with(|c| c.borrow_mut().close_window(id))
}

/// Render a frame
pub fn render() {
    COMPOSITOR.with(|c| c.borrow_mut().render())
}

/// Handle resize event
pub fn resize(width: u32, height: u32) {
    COMPOSITOR.with(|c| c.borrow_mut().resize(width, height))
}

/// Handle click event
pub fn handle_click(x: f64, y: f64, button: events::MouseButton) {
    COMPOSITOR.with(|c| c.borrow_mut().handle_click(x, y, button))
}

/// Create a terminal window on the global compositor
pub fn create_terminal_window(title: &str, owner: TaskId) -> WindowId {
    COMPOSITOR.with(|c| c.borrow_mut().create_terminal_window(title, owner))
}

/// Handle keyboard event - forwards to focused terminal
pub fn handle_key(key: &str, code: &str, ctrl: bool, alt: bool) -> bool {
    COMPOSITOR.with(|c| c.borrow_mut().handle_key(key, code, ctrl, alt))
}
