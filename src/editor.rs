//! Text editor for axeberg
//!
//! A minimal terminal text editor inspired by kibi/kilo.
//! Uses xterm.js for terminal I/O and VFS for file operations.
//!
//! Keybindings:
//! - Ctrl+S: Save
//! - Ctrl+Q: Quit (press twice if unsaved changes)
//! - Ctrl+F: Find
//! - Ctrl+G: Go to line
//! - Ctrl+D: Duplicate line
//! - Ctrl+K: Delete line
//! - Arrows: Move cursor
//! - Ctrl+Arrows: Move by word
//! - Home/End: Start/end of line
//! - Page Up/Down: Scroll

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::kernel::syscall;

// Global editor state
thread_local! {
    static EDITOR: RefCell<Option<Editor>> = RefCell::new(None);
    static EDITOR_ACTIVE: RefCell<bool> = RefCell::new(false);
}

/// Check if editor is currently active
pub fn is_active() -> bool {
    EDITOR_ACTIVE.with(|a| *a.borrow())
}

/// Start the editor with a file
pub fn start(filename: Option<&str>) -> Result<(), String> {
    let mut editor = Editor::new();

    // Set screen size (will be updated on first render)
    editor.set_screen_size(80, 24);

    if let Some(path) = filename {
        editor.load(path)?;
    }

    EDITOR.with(|e| {
        *e.borrow_mut() = Some(editor);
    });

    EDITOR_ACTIVE.with(|a| {
        *a.borrow_mut() = true;
    });

    // Trigger initial render
    refresh();

    Ok(())
}

/// Stop the editor and return to shell
pub fn stop() {
    EDITOR_ACTIVE.with(|a| {
        *a.borrow_mut() = false;
    });
    EDITOR.with(|e| {
        *e.borrow_mut() = None;
    });
}

/// Refresh the editor display
pub fn refresh() {
    EDITOR.with(|e| {
        if let Some(ref mut editor) = *e.borrow_mut() {
            let output = editor.render();
            crate::terminal::write(&output);
        }
    });
}

/// Process a key event in the editor
/// Returns true if editor should exit
pub fn process_key(key: Key) -> bool {
    EDITOR.with(|e| {
        if let Some(ref mut editor) = *e.borrow_mut() {
            let should_quit = editor.process_key(key);
            if should_quit {
                return true;
            }
            // Refresh display after key
            let output = editor.render();
            crate::terminal::write(&output);
            false
        } else {
            true // No editor, exit
        }
    })
}

/// Update editor screen size
pub fn set_screen_size(cols: usize, rows: usize) {
    EDITOR.with(|e| {
        if let Some(ref mut editor) = *e.borrow_mut() {
            editor.set_screen_size(cols, rows);
        }
    });
}

/// Parse a key from xterm.js key event
pub fn parse_key(key: &str, key_code: u32, ctrl: bool, alt: bool, shift: bool) -> Option<Key> {
    // Handle control characters
    if ctrl && !alt {
        return match key_code {
            // Ctrl+letter
            65..=90 => {
                let ch = (key_code as u8 - 65 + b'a') as char;
                Some(Key::Ctrl(ch))
            }
            // Ctrl+Arrow
            37 => Some(Key::CtrlArrow(Arrow::Left)),
            38 => Some(Key::CtrlArrow(Arrow::Up)),
            39 => Some(Key::CtrlArrow(Arrow::Right)),
            40 => Some(Key::CtrlArrow(Arrow::Down)),
            _ => None,
        };
    }

    // Handle special keys
    match key_code {
        8 => Some(Key::Backspace),
        9 => Some(Key::Tab),
        13 => Some(Key::Enter),
        27 => Some(Key::Escape),
        33 => Some(Key::PageUp),
        34 => Some(Key::PageDown),
        35 => Some(Key::End),
        36 => Some(Key::Home),
        37 => Some(Key::Arrow(Arrow::Left)),
        38 => Some(Key::Arrow(Arrow::Up)),
        39 => Some(Key::Arrow(Arrow::Right)),
        40 => Some(Key::Arrow(Arrow::Down)),
        46 => Some(Key::Delete),
        _ => {
            // Regular character
            if key.len() == 1 && !ctrl && !alt {
                let ch = key.chars().next()?;
                if ch.is_ascii_graphic() || ch == ' ' {
                    return Some(Key::Char(ch));
                }
            }
            None
        }
    }
}

/// Handle pasted text in editor
pub fn handle_paste(text: &str) {
    EDITOR.with(|e| {
        if let Some(ref mut editor) = *e.borrow_mut() {
            for ch in text.chars() {
                if ch == '\n' || ch == '\r' {
                    editor.insert_newline();
                } else if ch.is_ascii_graphic() || ch == ' ' || ch == '\t' {
                    editor.insert_char(ch);
                }
            }
            let output = editor.render();
            crate::terminal::write(&output);
        }
    });
}

// ANSI escape sequences
const CLEAR_SCREEN: &str = "\x1b[2J";
const CURSOR_HOME: &str = "\x1b[H";
const CLEAR_LINE: &str = "\x1b[K";
const CURSOR_HIDE: &str = "\x1b[?25l";
const CURSOR_SHOW: &str = "\x1b[?25h";
const INVERT_COLORS: &str = "\x1b[7m";
const RESET_COLORS: &str = "\x1b[m";

/// Arrow key directions
#[derive(Clone, Copy, PartialEq)]
pub enum Arrow {
    Left,
    Right,
    Up,
    Down,
}

/// Key input events
#[derive(Clone, PartialEq)]
pub enum Key {
    Arrow(Arrow),
    CtrlArrow(Arrow),
    Char(char),
    Ctrl(char),
    Enter,
    Backspace,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    Escape,
    Tab,
}

/// A row of text in the document
#[derive(Clone)]
pub struct Row {
    /// Raw characters
    chars: String,
    /// Rendered string (tabs expanded)
    render: String,
}

impl Row {
    pub fn new(chars: String) -> Self {
        let mut row = Self {
            chars,
            render: String::new(),
        };
        row.update_render();
        row
    }

    pub fn empty() -> Self {
        Self::new(String::new())
    }

    /// Update the rendered string (expand tabs)
    fn update_render(&mut self) {
        self.render.clear();
        let mut col = 0;
        for ch in self.chars.chars() {
            if ch == '\t' {
                // Tab stops every 4 columns
                let spaces = 4 - (col % 4);
                for _ in 0..spaces {
                    self.render.push(' ');
                }
                col += spaces;
            } else {
                self.render.push(ch);
                col += 1;
            }
        }
    }

    /// Get the raw character count
    pub fn len(&self) -> usize {
        self.chars.chars().count()
    }

    /// Get the rendered width
    pub fn render_len(&self) -> usize {
        self.render.chars().count()
    }

    /// Convert cursor x position to render x position
    pub fn cx_to_rx(&self, cx: usize) -> usize {
        let mut rx = 0;
        for (i, ch) in self.chars.chars().enumerate() {
            if i >= cx {
                break;
            }
            if ch == '\t' {
                rx += 4 - (rx % 4);
            } else {
                rx += 1;
            }
        }
        rx
    }

    /// Insert a character at position
    pub fn insert_char(&mut self, at: usize, ch: char) {
        let byte_pos = self.char_to_byte_pos(at);
        self.chars.insert(byte_pos, ch);
        self.update_render();
    }

    /// Delete a character at position
    pub fn delete_char(&mut self, at: usize) {
        if at < self.len() {
            let byte_pos = self.char_to_byte_pos(at);
            let next_pos = self.char_to_byte_pos(at + 1);
            self.chars.drain(byte_pos..next_pos);
            self.update_render();
        }
    }

    /// Append a string
    pub fn append(&mut self, s: &str) {
        self.chars.push_str(s);
        self.update_render();
    }

    /// Split row at position, returning the right part
    pub fn split(&mut self, at: usize) -> String {
        let byte_pos = self.char_to_byte_pos(at);
        let right = self.chars[byte_pos..].to_string();
        self.chars.truncate(byte_pos);
        self.update_render();
        right
    }

    /// Get slice of rendered string for display
    pub fn render_slice(&self, start: usize, len: usize) -> &str {
        let bytes = self.render.as_bytes();
        let mut byte_start = 0;
        let mut byte_end = bytes.len();
        let mut col = 0;

        for (i, ch) in self.render.char_indices() {
            if col == start {
                byte_start = i;
            }
            col += 1;
            if col == start + len {
                byte_end = i + ch.len_utf8();
                break;
            }
        }

        if col < start {
            return "";
        }

        &self.render[byte_start..byte_end.min(self.render.len())]
    }

    /// Convert character position to byte position
    fn char_to_byte_pos(&self, char_pos: usize) -> usize {
        self.chars
            .char_indices()
            .nth(char_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.chars.len())
    }
}

/// Editor prompt mode
#[derive(Clone, PartialEq)]
pub enum PromptMode {
    None,
    Save(String),
    Find(String),
    GoTo(String),
}

/// Editor state
pub struct Editor {
    /// Document rows
    rows: Vec<Row>,
    /// Cursor x position (in characters)
    cx: usize,
    /// Cursor y position (row index)
    cy: usize,
    /// Horizontal scroll offset
    col_offset: usize,
    /// Vertical scroll offset
    row_offset: usize,
    /// Screen width in columns
    screen_cols: usize,
    /// Screen height in rows (minus status bars)
    screen_rows: usize,
    /// Current filename
    filename: Option<String>,
    /// Dirty flag (unsaved changes)
    dirty: bool,
    /// Status message
    status_msg: String,
    /// Quit confirmation counter
    quit_times: u8,
    /// Current prompt mode
    prompt_mode: PromptMode,
    /// Copied row for paste
    copied_row: Option<String>,
    /// Search direction (true = forward)
    search_forward: bool,
    /// Last search match position
    last_match: Option<(usize, usize)>,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            rows: vec![Row::empty()],
            cx: 0,
            cy: 0,
            col_offset: 0,
            row_offset: 0,
            screen_cols: 80,
            screen_rows: 24,
            filename: None,
            dirty: false,
            status_msg: String::from("Ctrl+S = save | Ctrl+Q = quit | Ctrl+F = find"),
            quit_times: 2,
            prompt_mode: PromptMode::None,
            copied_row: None,
            search_forward: true,
            last_match: None,
        }
    }

    /// Load a file into the editor
    pub fn load(&mut self, path: &str) -> Result<(), String> {
        match syscall::read_file(path) {
            Ok(content) => {
                self.rows.clear();
                for line in content.lines() {
                    self.rows.push(Row::new(line.to_string()));
                }
                if self.rows.is_empty() {
                    self.rows.push(Row::empty());
                }
                self.filename = Some(path.to_string());
                self.dirty = false;
                self.cx = 0;
                self.cy = 0;
                self.col_offset = 0;
                self.row_offset = 0;
                self.status_msg = format!("Loaded: {}", path);
                Ok(())
            }
            Err(_) => {
                // New file
                self.filename = Some(path.to_string());
                self.rows = vec![Row::empty()];
                self.dirty = false;
                self.status_msg = format!("New file: {}", path);
                Ok(())
            }
        }
    }

    /// Save the document
    pub fn save(&mut self) -> Result<(), String> {
        if let Some(ref path) = self.filename {
            let content: String = self
                .rows
                .iter()
                .map(|r| r.chars.as_str())
                .collect::<Vec<_>>()
                .join("\n");

            syscall::write_file(path, &content).map_err(|e| format!("{:?}", e))?;

            self.dirty = false;
            self.status_msg = format!("Saved: {} ({} bytes)", path, content.len());
            Ok(())
        } else {
            Err("No filename".to_string())
        }
    }

    /// Set screen dimensions
    pub fn set_screen_size(&mut self, cols: usize, rows: usize) {
        self.screen_cols = cols;
        self.screen_rows = rows.saturating_sub(2); // Reserve 2 lines for status
    }

    /// Get current row
    fn current_row(&self) -> Option<&Row> {
        self.rows.get(self.cy)
    }

    /// Get current row mutably
    fn current_row_mut(&mut self) -> Option<&mut Row> {
        self.rows.get_mut(self.cy)
    }

    /// Move cursor in direction
    pub fn move_cursor(&mut self, arrow: Arrow) {
        match arrow {
            Arrow::Left => {
                if self.cx > 0 {
                    self.cx -= 1;
                } else if self.cy > 0 {
                    self.cy -= 1;
                    self.cx = self.current_row().map(|r| r.len()).unwrap_or(0);
                }
            }
            Arrow::Right => {
                let row_len = self.current_row().map(|r| r.len()).unwrap_or(0);
                if self.cx < row_len {
                    self.cx += 1;
                } else if self.cy < self.rows.len() - 1 {
                    self.cy += 1;
                    self.cx = 0;
                }
            }
            Arrow::Up => {
                if self.cy > 0 {
                    self.cy -= 1;
                }
            }
            Arrow::Down => {
                if self.cy < self.rows.len() - 1 {
                    self.cy += 1;
                }
            }
        }
        // Snap cursor to end of line if past it
        let row_len = self.current_row().map(|r| r.len()).unwrap_or(0);
        if self.cx > row_len {
            self.cx = row_len;
        }
    }

    /// Move cursor by word
    pub fn move_cursor_word(&mut self, arrow: Arrow) {
        match arrow {
            Arrow::Left => {
                // Move to start of previous word
                if self.cx == 0 && self.cy > 0 {
                    self.cy -= 1;
                    self.cx = self.current_row().map(|r| r.len()).unwrap_or(0);
                } else if let Some(row) = self.current_row() {
                    let chars: Vec<char> = row.chars.chars().collect();
                    let mut pos = self.cx;
                    // Skip whitespace
                    while pos > 0 && chars.get(pos - 1).map(|c| c.is_whitespace()).unwrap_or(false)
                    {
                        pos -= 1;
                    }
                    // Skip word
                    while pos > 0 && chars.get(pos - 1).map(|c| !c.is_whitespace()).unwrap_or(false)
                    {
                        pos -= 1;
                    }
                    self.cx = pos;
                }
            }
            Arrow::Right => {
                // Move to start of next word
                if let Some(row) = self.current_row() {
                    let chars: Vec<char> = row.chars.chars().collect();
                    let len = chars.len();
                    let mut pos = self.cx;
                    // Skip current word
                    while pos < len && !chars[pos].is_whitespace() {
                        pos += 1;
                    }
                    // Skip whitespace
                    while pos < len && chars[pos].is_whitespace() {
                        pos += 1;
                    }
                    if pos >= len && self.cy < self.rows.len() - 1 {
                        self.cy += 1;
                        self.cx = 0;
                    } else {
                        self.cx = pos;
                    }
                }
            }
            Arrow::Up | Arrow::Down => {
                // Move by paragraph (find empty line)
                let direction: i32 = if arrow == Arrow::Up { -1 } else { 1 };
                let mut y = self.cy as i32 + direction;
                while y >= 0 && y < self.rows.len() as i32 {
                    if self.rows[y as usize].len() == 0 {
                        break;
                    }
                    y += direction;
                }
                self.cy = (y.max(0) as usize).min(self.rows.len() - 1);
                let row_len = self.current_row().map(|r| r.len()).unwrap_or(0);
                if self.cx > row_len {
                    self.cx = row_len;
                }
            }
        }
    }

    /// Insert a character at cursor
    pub fn insert_char(&mut self, ch: char) {
        if self.cy == self.rows.len() {
            self.rows.push(Row::empty());
        }
        if let Some(row) = self.current_row_mut() {
            row.insert_char(self.cx, ch);
            self.cx += 1;
            self.dirty = true;
        }
    }

    /// Insert a new line
    pub fn insert_newline(&mut self) {
        if self.cy >= self.rows.len() {
            self.rows.push(Row::empty());
        } else if self.cx == 0 {
            self.rows.insert(self.cy, Row::empty());
        } else {
            let right = self.rows[self.cy].split(self.cx);
            self.rows.insert(self.cy + 1, Row::new(right));
        }
        self.cy += 1;
        self.cx = 0;
        self.dirty = true;
    }

    /// Delete character (backspace)
    pub fn delete_char(&mut self) {
        if self.cy >= self.rows.len() {
            return;
        }
        if self.cx == 0 {
            if self.cy > 0 {
                let current = self.rows.remove(self.cy);
                self.cy -= 1;
                self.cx = self.rows[self.cy].len();
                self.rows[self.cy].append(&current.chars);
                self.dirty = true;
            }
        } else {
            self.rows[self.cy].delete_char(self.cx - 1);
            self.cx -= 1;
            self.dirty = true;
        }
    }

    /// Delete character forward
    pub fn delete_char_forward(&mut self) {
        if self.cy >= self.rows.len() {
            return;
        }
        let row_len = self.rows[self.cy].len();
        if self.cx < row_len {
            self.rows[self.cy].delete_char(self.cx);
            self.dirty = true;
        } else if self.cy < self.rows.len() - 1 {
            // Merge with next row
            let next = self.rows.remove(self.cy + 1);
            self.rows[self.cy].append(&next.chars);
            self.dirty = true;
        }
    }

    /// Delete current line
    pub fn delete_line(&mut self) {
        if self.rows.len() > 1 {
            self.rows.remove(self.cy);
            if self.cy >= self.rows.len() {
                self.cy = self.rows.len() - 1;
            }
            self.dirty = true;
        } else {
            self.rows[0] = Row::empty();
            self.cx = 0;
            self.dirty = true;
        }
        let row_len = self.current_row().map(|r| r.len()).unwrap_or(0);
        if self.cx > row_len {
            self.cx = row_len;
        }
    }

    /// Duplicate current line
    pub fn duplicate_line(&mut self) {
        if self.cy < self.rows.len() {
            let copy = self.rows[self.cy].clone();
            self.rows.insert(self.cy + 1, copy);
            self.cy += 1;
            self.dirty = true;
        }
    }

    /// Copy current line
    pub fn copy_line(&mut self) {
        if let Some(row) = self.current_row() {
            self.copied_row = Some(row.chars.clone());
            self.status_msg = String::from("Line copied");
        }
    }

    /// Paste copied line
    pub fn paste_line(&mut self) {
        if let Some(ref text) = self.copied_row.clone() {
            self.rows.insert(self.cy + 1, Row::new(text.clone()));
            self.cy += 1;
            self.dirty = true;
        }
    }

    /// Update scroll offsets based on cursor position
    fn scroll(&mut self) {
        // Vertical scrolling
        if self.cy < self.row_offset {
            self.row_offset = self.cy;
        }
        if self.cy >= self.row_offset + self.screen_rows {
            self.row_offset = self.cy - self.screen_rows + 1;
        }

        // Horizontal scrolling
        let rx = self
            .current_row()
            .map(|r| r.cx_to_rx(self.cx))
            .unwrap_or(0);
        if rx < self.col_offset {
            self.col_offset = rx;
        }
        if rx >= self.col_offset + self.screen_cols {
            self.col_offset = rx - self.screen_cols + 1;
        }
    }

    /// Render the screen to a string buffer
    pub fn render(&mut self) -> String {
        self.scroll();

        let mut buf = String::with_capacity(self.screen_cols * (self.screen_rows + 2) * 2);

        buf.push_str(CURSOR_HIDE);
        buf.push_str(CURSOR_HOME);

        // Draw rows
        for y in 0..self.screen_rows {
            let file_row = y + self.row_offset;
            if file_row < self.rows.len() {
                let row = &self.rows[file_row];
                let len = row.render_len().saturating_sub(self.col_offset);
                let display_len = len.min(self.screen_cols);
                buf.push_str(row.render_slice(self.col_offset, display_len));
            } else {
                buf.push('~');
            }
            buf.push_str(CLEAR_LINE);
            buf.push_str("\r\n");
        }

        // Draw status bar
        self.draw_status_bar(&mut buf);

        // Draw message bar
        self.draw_message_bar(&mut buf);

        // Position cursor
        let cursor_y = self.cy - self.row_offset + 1;
        let rx = self
            .current_row()
            .map(|r| r.cx_to_rx(self.cx))
            .unwrap_or(0);
        let cursor_x = rx - self.col_offset + 1;
        buf.push_str(&format!("\x1b[{};{}H", cursor_y, cursor_x));

        buf.push_str(CURSOR_SHOW);

        buf
    }

    /// Draw the status bar
    fn draw_status_bar(&self, buf: &mut String) {
        buf.push_str(INVERT_COLORS);

        let filename = self
            .filename
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("[No Name]");
        let modified = if self.dirty { "(modified)" } else { "" };
        let left = format!("{} {} ", filename, modified);
        let right = format!(" {}/{} ", self.cy + 1, self.rows.len());

        let width = self.screen_cols;
        let left_len = left.chars().count().min(width);
        let right_len = right.chars().count();

        buf.push_str(&left[..left.char_indices().nth(left_len).map(|(i, _)| i).unwrap_or(left.len())]);

        let padding = width.saturating_sub(left_len + right_len);
        for _ in 0..padding {
            buf.push(' ');
        }

        if left_len + padding + right_len <= width {
            buf.push_str(&right);
        }

        buf.push_str(RESET_COLORS);
        buf.push_str("\r\n");
    }

    /// Draw the message/prompt bar
    fn draw_message_bar(&self, buf: &mut String) {
        buf.push_str(CLEAR_LINE);

        let msg = match &self.prompt_mode {
            PromptMode::None => self.status_msg.clone(),
            PromptMode::Save(input) => format!("Save as: {}", input),
            PromptMode::Find(query) => format!("Find: {} (ESC to cancel)", query),
            PromptMode::GoTo(input) => format!("Go to line: {}", input),
        };

        let len = msg.chars().count().min(self.screen_cols);
        buf.push_str(
            &msg[..msg
                .char_indices()
                .nth(len)
                .map(|(i, _)| i)
                .unwrap_or(msg.len())],
        );
    }

    /// Find text in document
    fn find(&mut self, query: &str, forward: bool) {
        if query.is_empty() {
            return;
        }

        let start_row = self.last_match.map(|(r, _)| r).unwrap_or(self.cy);
        let start_col = self.last_match.map(|(_, c)| c + 1).unwrap_or(self.cx);

        let rows_len = self.rows.len();
        let mut found = false;

        for i in 0..rows_len {
            let row_idx = if forward {
                (start_row + i) % rows_len
            } else {
                (start_row + rows_len - i) % rows_len
            };

            let row = &self.rows[row_idx];
            let search_start = if i == 0 && forward {
                start_col.min(row.chars.len())
            } else if i == 0 && !forward {
                start_col.saturating_sub(query.len() + 1)
            } else {
                0
            };

            if let Some(col) = if forward {
                row.chars[search_start..].find(query).map(|p| {
                    // Convert byte position to char position
                    row.chars[..search_start + p].chars().count()
                })
            } else {
                row.chars[..search_start].rfind(query).map(|p| {
                    row.chars[..p].chars().count()
                })
            } {
                self.cy = row_idx;
                self.cx = col;
                self.last_match = Some((row_idx, col));
                self.status_msg = format!("Found at line {}", row_idx + 1);
                found = true;
                break;
            }
        }

        if !found {
            self.status_msg = format!("'{}' not found", query);
            self.last_match = None;
        }
    }

    /// Go to line number
    fn goto_line(&mut self, line_str: &str) {
        if let Ok(line) = line_str.parse::<usize>() {
            if line > 0 && line <= self.rows.len() {
                self.cy = line - 1;
                self.cx = 0;
                self.status_msg = format!("Jumped to line {}", line);
            } else {
                self.status_msg = format!("Invalid line: {}", line);
            }
        }
    }

    /// Process a key press, returns true if should quit
    pub fn process_key(&mut self, key: Key) -> bool {
        match &self.prompt_mode {
            PromptMode::None => self.process_key_normal(key),
            PromptMode::Save(_) | PromptMode::Find(_) | PromptMode::GoTo(_) => {
                self.process_key_prompt(key)
            }
        }
    }

    /// Process key in normal mode
    fn process_key_normal(&mut self, key: Key) -> bool {
        match key {
            Key::Ctrl('q') => {
                if self.dirty && self.quit_times > 0 {
                    self.status_msg = format!(
                        "Unsaved changes! Press Ctrl+Q {} more time(s) to quit",
                        self.quit_times
                    );
                    self.quit_times -= 1;
                    return false;
                }
                return true;
            }
            Key::Ctrl('s') => {
                if self.filename.is_none() {
                    self.prompt_mode = PromptMode::Save(String::new());
                } else if let Err(e) = self.save() {
                    self.status_msg = format!("Save failed: {}", e);
                }
            }
            Key::Ctrl('f') => {
                self.prompt_mode = PromptMode::Find(String::new());
                self.last_match = None;
            }
            Key::Ctrl('g') => {
                self.prompt_mode = PromptMode::GoTo(String::new());
            }
            Key::Ctrl('k') => {
                self.delete_line();
            }
            Key::Ctrl('d') => {
                self.duplicate_line();
            }
            Key::Ctrl('c') => {
                self.copy_line();
            }
            Key::Ctrl('v') => {
                self.paste_line();
            }
            Key::Arrow(dir) => {
                self.move_cursor(dir);
            }
            Key::CtrlArrow(dir) => {
                self.move_cursor_word(dir);
            }
            Key::Home => {
                self.cx = 0;
            }
            Key::End => {
                self.cx = self.current_row().map(|r| r.len()).unwrap_or(0);
            }
            Key::PageUp => {
                self.cy = self.row_offset;
                for _ in 0..self.screen_rows {
                    self.move_cursor(Arrow::Up);
                }
            }
            Key::PageDown => {
                self.cy = (self.row_offset + self.screen_rows - 1).min(self.rows.len() - 1);
                for _ in 0..self.screen_rows {
                    self.move_cursor(Arrow::Down);
                }
            }
            Key::Enter => {
                self.insert_newline();
            }
            Key::Backspace => {
                self.delete_char();
            }
            Key::Delete => {
                self.delete_char_forward();
            }
            Key::Tab => {
                self.insert_char('\t');
            }
            Key::Char(ch) => {
                self.insert_char(ch);
            }
            Key::Escape => {}
            _ => {}
        }

        // Reset quit counter on any other key
        if !matches!(key, Key::Ctrl('q')) {
            self.quit_times = 2;
        }

        false
    }

    /// Process key in prompt mode
    fn process_key_prompt(&mut self, key: Key) -> bool {
        match key {
            Key::Escape => {
                self.prompt_mode = PromptMode::None;
                self.status_msg = String::from("Cancelled");
            }
            Key::Enter => {
                match &self.prompt_mode {
                    PromptMode::Save(input) => {
                        let path = input.clone();
                        self.filename = Some(path.clone());
                        self.prompt_mode = PromptMode::None;
                        if let Err(e) = self.save() {
                            self.status_msg = format!("Save failed: {}", e);
                        }
                    }
                    PromptMode::Find(query) => {
                        let q = query.clone();
                        self.prompt_mode = PromptMode::None;
                        self.find(&q, true);
                    }
                    PromptMode::GoTo(input) => {
                        let line = input.clone();
                        self.prompt_mode = PromptMode::None;
                        self.goto_line(&line);
                    }
                    PromptMode::None => {}
                }
            }
            Key::Backspace => match &mut self.prompt_mode {
                PromptMode::Save(input)
                | PromptMode::Find(input)
                | PromptMode::GoTo(input) => {
                    input.pop();
                }
                PromptMode::None => {}
            },
            Key::Char(ch) => match &mut self.prompt_mode {
                PromptMode::Save(input)
                | PromptMode::Find(input)
                | PromptMode::GoTo(input) => {
                    input.push(ch);
                    // Live search for Find mode
                    if matches!(self.prompt_mode, PromptMode::Find(_)) {
                        if let PromptMode::Find(query) = &self.prompt_mode {
                            let q = query.clone();
                            self.find(&q, true);
                        }
                    }
                }
                PromptMode::None => {}
            },
            Key::Arrow(Arrow::Up) | Key::Arrow(Arrow::Left) => {
                if let PromptMode::Find(query) = &self.prompt_mode {
                    let q = query.clone();
                    self.find(&q, false);
                }
            }
            Key::Arrow(Arrow::Down) | Key::Arrow(Arrow::Right) => {
                if let PromptMode::Find(query) = &self.prompt_mode {
                    let q = query.clone();
                    self.find(&q, true);
                }
            }
            _ => {}
        }
        false
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_row_new() {
        let row = Row::new("hello".to_string());
        assert_eq!(row.len(), 5);
        assert_eq!(row.render, "hello");
    }

    #[test]
    fn test_row_tabs() {
        let row = Row::new("\thello".to_string());
        assert_eq!(row.len(), 6);
        assert_eq!(row.render, "    hello");
    }

    #[test]
    fn test_row_insert() {
        let mut row = Row::new("helo".to_string());
        row.insert_char(3, 'l');
        assert_eq!(row.chars, "hello");
    }

    #[test]
    fn test_row_delete() {
        let mut row = Row::new("helllo".to_string());
        row.delete_char(3);
        assert_eq!(row.chars, "hello");
    }

    #[test]
    fn test_row_split() {
        let mut row = Row::new("hello world".to_string());
        let right = row.split(6);
        assert_eq!(row.chars, "hello ");
        assert_eq!(right, "world");
    }

    #[test]
    fn test_editor_new() {
        let editor = Editor::new();
        assert_eq!(editor.rows.len(), 1);
        assert_eq!(editor.cx, 0);
        assert_eq!(editor.cy, 0);
        assert!(!editor.dirty);
    }

    #[test]
    fn test_editor_insert_char() {
        let mut editor = Editor::new();
        editor.insert_char('h');
        editor.insert_char('i');
        assert_eq!(editor.rows[0].chars, "hi");
        assert_eq!(editor.cx, 2);
        assert!(editor.dirty);
    }

    #[test]
    fn test_editor_insert_newline() {
        let mut editor = Editor::new();
        editor.insert_char('a');
        editor.insert_char('b');
        editor.insert_newline();
        editor.insert_char('c');
        assert_eq!(editor.rows.len(), 2);
        assert_eq!(editor.rows[0].chars, "ab");
        assert_eq!(editor.rows[1].chars, "c");
    }

    #[test]
    fn test_editor_delete_char() {
        let mut editor = Editor::new();
        editor.insert_char('a');
        editor.insert_char('b');
        editor.insert_char('c');
        editor.delete_char();
        assert_eq!(editor.rows[0].chars, "ab");
        assert_eq!(editor.cx, 2);
    }

    #[test]
    fn test_editor_cursor_movement() {
        let mut editor = Editor::new();
        editor.rows = vec![
            Row::new("line one".to_string()),
            Row::new("line two".to_string()),
        ];

        editor.move_cursor(Arrow::Down);
        assert_eq!(editor.cy, 1);

        editor.move_cursor(Arrow::Right);
        assert_eq!(editor.cx, 1);

        editor.move_cursor(Arrow::Up);
        assert_eq!(editor.cy, 0);
        assert_eq!(editor.cx, 1);
    }

    #[test]
    fn test_editor_delete_line() {
        let mut editor = Editor::new();
        editor.rows = vec![
            Row::new("line one".to_string()),
            Row::new("line two".to_string()),
            Row::new("line three".to_string()),
        ];
        editor.cy = 1;
        editor.delete_line();
        assert_eq!(editor.rows.len(), 2);
        assert_eq!(editor.rows[1].chars, "line three");
    }

    #[test]
    fn test_editor_duplicate_line() {
        let mut editor = Editor::new();
        editor.rows = vec![Row::new("test".to_string())];
        editor.duplicate_line();
        assert_eq!(editor.rows.len(), 2);
        assert_eq!(editor.rows[0].chars, "test");
        assert_eq!(editor.rows[1].chars, "test");
        assert_eq!(editor.cy, 1);
    }
}
