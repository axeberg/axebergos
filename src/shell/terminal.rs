//! Terminal - Text-based interface for the shell
//!
//! Provides:
//! - Scrolling text buffer for output
//! - Command line input with editing
//! - Connection to shell executor
//! - Keyboard event handling
//! - Text selection with clipboard support

use crate::shell::Executor;
use std::collections::VecDeque;

/// A position in the terminal grid (row, column)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermPos {
    /// Line index (0 = first line in buffer)
    pub line: usize,
    /// Column index (0 = first character)
    pub col: usize,
}

impl TermPos {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }

    /// Returns true if self comes before other in reading order
    pub fn before(&self, other: &TermPos) -> bool {
        self.line < other.line || (self.line == other.line && self.col < other.col)
    }
}

impl PartialOrd for TermPos {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TermPos {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.line.cmp(&other.line) {
            std::cmp::Ordering::Equal => self.col.cmp(&other.col),
            ord => ord,
        }
    }
}

/// Selection state for text highlighting
#[derive(Debug, Clone)]
pub struct Selection {
    /// Where the selection started (anchor point)
    pub anchor: TermPos,
    /// Current end of selection (moves with mouse)
    pub end: TermPos,
    /// Is selection currently active (mouse held down)?
    pub active: bool,
}

impl Selection {
    pub fn new(pos: TermPos) -> Self {
        Self {
            anchor: pos,
            end: pos,
            active: true,
        }
    }

    /// Get the selection range in reading order (start, end)
    pub fn range(&self) -> (TermPos, TermPos) {
        if self.anchor.before(&self.end) {
            (self.anchor, self.end)
        } else {
            (self.end, self.anchor)
        }
    }

    /// Check if a position is within the selection
    pub fn contains(&self, pos: TermPos) -> bool {
        let (start, end) = self.range();
        pos >= start && pos <= end
    }

    /// Check if a specific line/col is selected
    pub fn is_selected(&self, line: usize, col: usize) -> bool {
        let (start, end) = self.range();

        if line < start.line || line > end.line {
            return false;
        }

        if start.line == end.line {
            // Single line selection
            col >= start.col && col < end.col
        } else if line == start.line {
            // First line of multi-line selection
            col >= start.col
        } else if line == end.line {
            // Last line of multi-line selection
            col < end.col
        } else {
            // Middle line - fully selected
            true
        }
    }

    /// Check if selection is empty (start == end)
    pub fn is_empty(&self) -> bool {
        self.anchor == self.end
    }
}

/// Maximum lines to keep in scrollback buffer.
/// When exceeded, oldest lines are discarded from the top (FIFO).
/// This prevents unbounded memory growth during long sessions.
const MAX_LINES: usize = 1000;

/// Maximum command history entries.
/// When exceeded, oldest commands are discarded from the bottom.
/// History is stored newest-first, so we pop from the back.
const MAX_HISTORY: usize = 100;

/// A line in the terminal
#[derive(Debug, Clone)]
pub struct TerminalLine {
    pub text: String,
    pub is_input: bool,  // Was this a user input line?
}

impl TerminalLine {
    pub fn output(text: impl Into<String>) -> Self {
        Self { text: text.into(), is_input: false }
    }

    pub fn input(text: impl Into<String>) -> Self {
        Self { text: text.into(), is_input: true }
    }
}

/// Terminal state
pub struct Terminal {
    /// Output buffer (scrollback)
    lines: VecDeque<TerminalLine>,

    /// Current input line
    input: String,

    /// Cursor position in input
    cursor: usize,

    /// Command history
    history: VecDeque<String>,

    /// Position in history (for up/down navigation)
    history_pos: Option<usize>,

    /// Saved input when navigating history
    saved_input: String,

    /// Shell executor
    executor: Executor,

    /// Prompt string
    prompt: String,

    /// Number of visible rows (set by renderer)
    visible_rows: usize,

    /// Scroll offset (0 = bottom)
    scroll_offset: usize,

    /// Is the terminal active/focused?
    active: bool,

    /// Current text selection (if any)
    selection: Option<Selection>,
}

impl Terminal {
    pub fn new() -> Self {
        #[cfg(all(target_arch = "wasm32", not(test)))]
        crate::console_log!("[terminal] Creating terminal...");

        let mut term = Self {
            lines: VecDeque::with_capacity(MAX_LINES),
            input: String::new(),
            cursor: 0,
            history: VecDeque::with_capacity(MAX_HISTORY),
            history_pos: None,
            saved_input: String::new(),
            executor: Executor::new(),
            prompt: "$ ".to_string(),
            visible_rows: 24,
            scroll_offset: 0,
            active: true,
            selection: None,
        };

        #[cfg(all(target_arch = "wasm32", not(test)))]
        crate::console_log!("[terminal] Terminal created, cwd: {}", term.executor.state.cwd.display());

        // Welcome message
        term.print("Welcome to axeberg!");
        term.print("Type 'help' for available commands.");
        term.print("");

        term
    }

    /// Print a line to the terminal
    pub fn print(&mut self, text: &str) {
        // Handle multiple lines
        for line in text.lines() {
            self.lines.push_back(TerminalLine::output(line));
        }
        // Also add if the text was empty (prints blank line)
        if text.is_empty() {
            self.lines.push_back(TerminalLine::output(""));
        }

        // Trim old lines
        while self.lines.len() > MAX_LINES {
            self.lines.pop_front();
        }

        // Reset scroll to bottom
        self.scroll_offset = 0;
    }

    /// Print error output
    pub fn print_error(&mut self, text: &str) {
        // For now, errors are just printed. Could add color later.
        self.print(text);
    }

    /// Handle a key press
    pub fn handle_key(&mut self, key: &str, code: &str, ctrl: bool, _alt: bool) -> bool {
        // Handle Ctrl combinations
        if ctrl {
            match key {
                "c" => {
                    // Ctrl+C - cancel current input
                    self.print(&format!("{}{}^C", self.prompt, self.input));
                    self.input.clear();
                    self.cursor = 0;
                    self.history_pos = None;
                    return true;
                }
                "l" => {
                    // Ctrl+L - clear screen
                    self.lines.clear();
                    return true;
                }
                "a" => {
                    // Ctrl+A - beginning of line
                    self.cursor = 0;
                    return true;
                }
                "e" => {
                    // Ctrl+E - end of line
                    self.cursor = self.input.len();
                    return true;
                }
                "u" => {
                    // Ctrl+U - delete to beginning
                    self.input.drain(..self.cursor);
                    self.cursor = 0;
                    return true;
                }
                "k" => {
                    // Ctrl+K - delete to end
                    self.input.truncate(self.cursor);
                    return true;
                }
                "w" => {
                    // Ctrl+W - delete previous word
                    self.delete_word_back();
                    return true;
                }
                _ => {}
            }
        }

        // Handle special keys by code
        match code {
            "Enter" | "NumpadEnter" => {
                self.submit();
                return true;
            }
            "Backspace" => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.input.remove(self.cursor);
                }
                return true;
            }
            "Delete" => {
                if self.cursor < self.input.len() {
                    self.input.remove(self.cursor);
                }
                return true;
            }
            "ArrowLeft" => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                return true;
            }
            "ArrowRight" => {
                if self.cursor < self.input.len() {
                    self.cursor += 1;
                }
                return true;
            }
            "ArrowUp" => {
                self.history_prev();
                return true;
            }
            "ArrowDown" => {
                self.history_next();
                return true;
            }
            "Home" => {
                self.cursor = 0;
                return true;
            }
            "End" => {
                self.cursor = self.input.len();
                return true;
            }
            "PageUp" => {
                self.scroll_up(self.visible_rows.saturating_sub(1));
                return true;
            }
            "PageDown" => {
                self.scroll_down(self.visible_rows.saturating_sub(1));
                return true;
            }
            "Tab" => {
                self.tab_complete();
                return true;
            }
            _ => {}
        }

        // Handle printable characters
        if key.len() == 1 {
            let ch = key.chars().next().unwrap();
            if !ch.is_control() {
                self.input.insert(self.cursor, ch);
                self.cursor += 1;
                self.history_pos = None;
                return true;
            }
        }

        false
    }

    /// Submit the current input line
    fn submit(&mut self) {
        let input = std::mem::take(&mut self.input);
        self.cursor = 0;
        self.history_pos = None;

        // Echo the input
        self.lines.push_back(TerminalLine::input(format!("{}{}", self.prompt, input)));

        // Add to history if non-empty
        if !input.trim().is_empty() {
            // Remove duplicate if at front
            if self.history.front() == Some(&input) {
                self.history.pop_front();
            }
            self.history.push_front(input.clone());
            while self.history.len() > MAX_HISTORY {
                self.history.pop_back();
            }
        }

        // Execute the command
        let result = self.executor.execute_line(&input);

        // Handle output
        if !result.output.is_empty() {
            self.print(&result.output);
        }
        if !result.error.is_empty() {
            self.print_error(&result.error);
        }

        // Update prompt with cwd
        self.update_prompt();

        // Handle exit
        if result.should_exit {
            self.print(&format!("exit {}", result.code));
            // In a real system, we'd exit. For now, just print.
        }
    }

    /// Update the prompt based on current directory
    fn update_prompt(&mut self) {
        let cwd = self.executor.state.cwd.display().to_string();
        // Shorten home directory
        let home = self.executor.state.get_env("HOME").unwrap_or("/home");
        let display = if let Some(rest) = cwd.strip_prefix(home) {
            format!("~{}", rest)
        } else {
            cwd
        };
        self.prompt = format!("{} $ ", display);
    }

    /// Navigate to previous history entry
    fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }

        match self.history_pos {
            None => {
                self.saved_input = self.input.clone();
                self.history_pos = Some(0);
                self.input = self.history[0].clone();
            }
            Some(pos) if pos + 1 < self.history.len() => {
                self.history_pos = Some(pos + 1);
                self.input = self.history[pos + 1].clone();
            }
            _ => {}
        }
        self.cursor = self.input.len();
    }

    /// Navigate to next history entry
    fn history_next(&mut self) {
        match self.history_pos {
            Some(0) => {
                self.history_pos = None;
                self.input = std::mem::take(&mut self.saved_input);
            }
            Some(pos) => {
                self.history_pos = Some(pos - 1);
                self.input = self.history[pos - 1].clone();
            }
            None => {}
        }
        self.cursor = self.input.len();
    }

    /// Delete word backwards
    fn delete_word_back(&mut self) {
        if self.cursor == 0 {
            return;
        }

        // Skip trailing whitespace
        let mut end = self.cursor;
        while end > 0 && self.input.chars().nth(end - 1) == Some(' ') {
            end -= 1;
        }

        // Find start of word
        let mut start = end;
        while start > 0 && self.input.chars().nth(start - 1) != Some(' ') {
            start -= 1;
        }

        self.input.drain(start..self.cursor);
        self.cursor = start;
    }

    /// Scroll up by n lines
    pub fn scroll_up(&mut self, n: usize) {
        let max_scroll = self.lines.len().saturating_sub(self.visible_rows);
        self.scroll_offset = (self.scroll_offset + n).min(max_scroll);
    }

    /// Scroll down by n lines
    pub fn scroll_down(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }

    /// Get visible lines for rendering
    pub fn visible_lines(&self) -> impl Iterator<Item = &TerminalLine> {
        let total = self.lines.len();
        let start = total.saturating_sub(self.visible_rows + self.scroll_offset);
        let end = total.saturating_sub(self.scroll_offset);
        self.lines.range(start..end)
    }

    /// Get the current input line with cursor
    pub fn input_line(&self) -> (&str, &str, usize) {
        (&self.prompt, &self.input, self.cursor)
    }

    /// Set number of visible rows
    pub fn set_visible_rows(&mut self, rows: usize) {
        self.visible_rows = rows.max(1);
    }

    /// Is terminal active/focused?
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Set active state
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    /// Get line count
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    // ==================== Selection Methods ====================

    /// Start a selection at the given position (mouse down)
    pub fn start_selection(&mut self, line: usize, col: usize) {
        // Convert visible line index to absolute line index
        let abs_line = self.visible_line_to_absolute(line);
        self.selection = Some(Selection::new(TermPos::new(abs_line, col)));
    }

    /// Update selection end point (mouse move while held)
    pub fn update_selection(&mut self, line: usize, col: usize) {
        // Calculate absolute line first to avoid borrow issues
        let abs_line = self.visible_line_to_absolute(line);

        if let Some(ref mut sel) = self.selection
            && sel.active {
                sel.end = TermPos::new(abs_line, col);
            }
    }

    /// Finish selection (mouse up)
    pub fn finish_selection(&mut self) {
        if let Some(ref mut sel) = self.selection {
            sel.active = false;
            // Clear selection if it's empty
            if sel.is_empty() {
                self.selection = None;
            }
        }
    }

    /// Clear any existing selection
    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Get the current selection (if any)
    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    /// Check if a character at (visible_line, col) is selected
    pub fn is_char_selected(&self, visible_line: usize, col: usize) -> bool {
        if let Some(ref sel) = self.selection {
            let abs_line = self.visible_line_to_absolute(visible_line);
            sel.is_selected(abs_line, col)
        } else {
            false
        }
    }

    /// Convert visible line index to absolute line index in the buffer
    fn visible_line_to_absolute(&self, visible_line: usize) -> usize {
        let total = self.lines.len();
        let start = total.saturating_sub(self.visible_rows + self.scroll_offset);
        start + visible_line
    }

    /// Get the selected text as a string
    pub fn get_selected_text(&self) -> Option<String> {
        let sel = self.selection.as_ref()?;
        if sel.is_empty() {
            return None;
        }

        let (start, end) = sel.range();
        let mut result = String::new();

        for (abs_idx, line) in self.lines.iter().enumerate() {
            if abs_idx < start.line || abs_idx > end.line {
                continue;
            }

            let text = &line.text;
            let line_start = if abs_idx == start.line { start.col } else { 0 };
            let line_end = if abs_idx == end.line {
                end.col.min(text.len())
            } else {
                text.len()
            };

            if line_start < text.len() {
                // Get the substring using character indices
                let chars: Vec<char> = text.chars().collect();
                let substr: String = chars
                    .get(line_start..line_end.min(chars.len()))
                    .map(|c| c.iter().collect())
                    .unwrap_or_default();
                result.push_str(&substr);
            }

            // Add newline between lines (but not after the last line)
            if abs_idx < end.line {
                result.push('\n');
            }
        }

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Get text of a specific line by absolute index
    pub fn get_line_text(&self, abs_line: usize) -> Option<&str> {
        self.lines.get(abs_line).map(|l| l.text.as_str())
    }

    /// Get visible lines with their absolute indices for rendering
    pub fn visible_lines_indexed(&self) -> impl Iterator<Item = (usize, &TerminalLine)> {
        let total = self.lines.len();
        let start = total.saturating_sub(self.visible_rows + self.scroll_offset);
        let end = total.saturating_sub(self.scroll_offset);
        self.lines.range(start..end).enumerate().map(move |(i, line)| (start + i, line))
    }

    /// Tab completion for commands and files
    fn tab_complete(&mut self) {
        // Clone data we need before any mutable operations
        let input = self.input[..self.cursor].to_string();
        let words: Vec<&str> = input.split_whitespace().collect();

        if words.is_empty() {
            return;
        }

        // Get the word being completed (last word or partial word)
        let (prefix, word_start) = if input.ends_with(' ') {
            // Starting a new word
            (String::new(), self.cursor)
        } else {
            // Completing partial word
            let last = words.last().unwrap_or(&"").to_string();
            let start = input.rfind(&last).unwrap_or(self.cursor);
            (last, start)
        };

        // Determine what to complete: command (first word) or file (subsequent)
        let completions = if words.len() == 1 && !input.ends_with(' ') {
            // Complete command
            self.complete_command(&prefix)
        } else {
            // Complete file path
            self.complete_file(&prefix)
        };

        if completions.is_empty() {
            return;
        }

        let prefix_len = prefix.len();
        let current_input = self.input.clone();
        let current_cursor = self.cursor;

        if completions.len() == 1 {
            // Single match - insert it
            let completion = &completions[0];
            self.input = format!(
                "{}{}{}",
                &current_input[..word_start],
                completion,
                &current_input[current_cursor..]
            );
            self.cursor = word_start + completion.len();
        } else {
            // Multiple matches - show them and find common prefix
            let display = format!("\n{}", completions.join("  "));
            let prompt_line = format!("\n{}{}", self.prompt, current_input);
            self.print(&display);
            self.print(&prompt_line);

            // Insert common prefix
            let common = Self::common_prefix(&completions);
            if common.len() > prefix_len {
                self.input = format!(
                    "{}{}{}",
                    &current_input[..word_start],
                    common,
                    &current_input[current_cursor..]
                );
                self.cursor = word_start + common.len();
            }
        }
    }

    /// Get command completions
    fn complete_command(&self, prefix: &str) -> Vec<String> {
        let mut completions = Vec::new();

        // Built-in commands
        let builtins = [
            "cd", "pwd", "exit", "echo", "export", "unset", "env", "true", "false",
            "help", "alias", "unalias",
        ];
        for cmd in builtins {
            if cmd.starts_with(prefix) {
                completions.push(cmd.to_string());
            }
        }

        // Programs from registry
        for prog in self.executor.registry.list() {
            if prog.starts_with(prefix) && !completions.contains(&prog.to_string()) {
                completions.push(prog.to_string());
            }
        }

        completions.sort();
        completions
    }

    /// Get file completions
    fn complete_file(&self, prefix: &str) -> Vec<String> {
        use crate::kernel::syscall;

        let (dir, file_prefix) = if prefix.contains('/') {
            let last_slash = prefix.rfind('/').unwrap();
            let dir = if last_slash == 0 { "/" } else { &prefix[..last_slash] };
            (dir.to_string(), &prefix[last_slash + 1..])
        } else {
            (".".to_string(), prefix)
        };

        let mut completions = Vec::new();
        if let Ok(entries) = syscall::readdir(&dir) {
            for entry in entries {
                if entry.starts_with(file_prefix) {
                    let path = if dir == "." {
                        entry.clone()
                    } else if dir == "/" {
                        format!("/{}", entry)
                    } else {
                        format!("{}/{}", dir, entry)
                    };

                    // Add trailing slash for directories
                    if let Ok(meta) = syscall::metadata(&path) {
                        if meta.is_dir {
                            completions.push(format!("{}/", path));
                        } else {
                            completions.push(path);
                        }
                    } else {
                        completions.push(path);
                    }
                }
            }
        }

        completions.sort();
        completions
    }

    /// Find common prefix of strings
    fn common_prefix(strings: &[String]) -> String {
        if strings.is_empty() {
            return String::new();
        }
        if strings.len() == 1 {
            return strings[0].clone();
        }

        let first = &strings[0];
        let mut len = first.len();

        for s in &strings[1..] {
            len = first
                .chars()
                .zip(s.chars())
                .take_while(|(a, b)| a == b)
                .count()
                .min(len);
        }

        first[..len].to_string()
    }
}

impl Default for Terminal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_new() {
        let term = Terminal::new();
        assert!(term.line_count() > 0); // Welcome message
        assert!(term.input.is_empty());
    }

    #[test]
    fn test_terminal_print() {
        let mut term = Terminal::new();
        let initial = term.line_count();
        term.print("hello");
        assert_eq!(term.line_count(), initial + 1);
    }

    #[test]
    fn test_terminal_multiline_print() {
        let mut term = Terminal::new();
        let initial = term.line_count();
        term.print("line1\nline2\nline3");
        assert_eq!(term.line_count(), initial + 3);
    }

    #[test]
    fn test_terminal_input() {
        let mut term = Terminal::new();
        term.handle_key("h", "KeyH", false, false);
        term.handle_key("i", "KeyI", false, false);
        assert_eq!(term.input, "hi");
        assert_eq!(term.cursor, 2);
    }

    #[test]
    fn test_terminal_backspace() {
        let mut term = Terminal::new();
        term.input = "hello".to_string();
        term.cursor = 5;
        term.handle_key("Backspace", "Backspace", false, false);
        assert_eq!(term.input, "hell");
        assert_eq!(term.cursor, 4);
    }

    #[test]
    fn test_terminal_arrow_keys() {
        let mut term = Terminal::new();
        term.input = "hello".to_string();
        term.cursor = 5;

        term.handle_key("ArrowLeft", "ArrowLeft", false, false);
        assert_eq!(term.cursor, 4);

        term.handle_key("ArrowRight", "ArrowRight", false, false);
        assert_eq!(term.cursor, 5);
    }

    #[test]
    fn test_terminal_ctrl_a_e() {
        let mut term = Terminal::new();
        term.input = "hello world".to_string();
        term.cursor = 5;

        term.handle_key("a", "KeyA", true, false);
        assert_eq!(term.cursor, 0);

        term.handle_key("e", "KeyE", true, false);
        assert_eq!(term.cursor, 11);
    }

    #[test]
    fn test_terminal_ctrl_c() {
        let mut term = Terminal::new();
        term.input = "some input".to_string();
        term.cursor = 5;

        term.handle_key("c", "KeyC", true, false);
        assert!(term.input.is_empty());
        assert_eq!(term.cursor, 0);
    }

    #[test]
    fn test_terminal_ctrl_u() {
        let mut term = Terminal::new();
        term.input = "hello world".to_string();
        term.cursor = 6;

        term.handle_key("u", "KeyU", true, false);
        assert_eq!(term.input, "world");
        assert_eq!(term.cursor, 0);
    }

    #[test]
    fn test_terminal_ctrl_k() {
        let mut term = Terminal::new();
        term.input = "hello world".to_string();
        term.cursor = 5;

        term.handle_key("k", "KeyK", true, false);
        assert_eq!(term.input, "hello");
    }

    #[test]
    fn test_terminal_history() {
        let mut term = Terminal::new();

        // Execute some commands
        term.input = "echo one".to_string();
        term.cursor = term.input.len();
        term.handle_key("Enter", "Enter", false, false);

        term.input = "echo two".to_string();
        term.cursor = term.input.len();
        term.handle_key("Enter", "Enter", false, false);

        // Navigate history
        term.handle_key("ArrowUp", "ArrowUp", false, false);
        assert_eq!(term.input, "echo two");

        term.handle_key("ArrowUp", "ArrowUp", false, false);
        assert_eq!(term.input, "echo one");

        term.handle_key("ArrowDown", "ArrowDown", false, false);
        assert_eq!(term.input, "echo two");
    }

    #[test]
    fn test_terminal_scroll() {
        let mut term = Terminal::new();
        term.set_visible_rows(5);

        // Add many lines
        for i in 0..20 {
            term.print(&format!("line {}", i));
        }

        assert_eq!(term.scroll_offset, 0);

        term.scroll_up(3);
        assert_eq!(term.scroll_offset, 3);

        term.scroll_down(1);
        assert_eq!(term.scroll_offset, 2);
    }

    #[test]
    fn test_terminal_execute_echo() {
        let mut term = Terminal::new();
        term.input = "echo hello world".to_string();
        term.cursor = term.input.len();
        term.handle_key("Enter", "Enter", false, false);

        // Check output contains "hello world"
        let has_output = term.lines.iter().any(|l| l.text.contains("hello world"));
        assert!(has_output);
    }

    #[test]
    fn test_terminal_execute_pwd() {
        let mut term = Terminal::new();
        term.input = "pwd".to_string();
        term.cursor = term.input.len();
        term.handle_key("Enter", "Enter", false, false);

        // Check output contains a path
        let has_path = term.lines.iter().any(|l| l.text.contains("/home"));
        assert!(has_path);
    }

    #[test]
    fn test_terminal_max_lines_trimming() {
        let mut term = Terminal::new();

        // Add more than MAX_LINES
        for i in 0..(MAX_LINES + 100) {
            term.print(&format!("line {}", i));
        }

        // Should be capped at MAX_LINES
        assert_eq!(term.line_count(), MAX_LINES);

        // Oldest lines should be gone, newest should remain
        // The last line we printed was "line {MAX_LINES + 99}"
        let last_line = term.lines.back().unwrap();
        assert!(last_line.text.contains(&format!("{}", MAX_LINES + 99)));

        // The first line should NOT be one of the initial welcome messages
        // or the very first lines we printed (they were trimmed)
        let first_line = term.lines.front().unwrap();
        // First line should be around line 100 (since we added 1100 lines to ~3 initial)
        assert!(first_line.text.starts_with("line "));
    }

    #[test]
    fn test_terminal_max_history_trimming() {
        let mut term = Terminal::new();

        // Execute more than MAX_HISTORY commands
        for i in 0..(MAX_HISTORY + 50) {
            term.input = format!("echo cmd{}", i);
            term.cursor = term.input.len();
            term.handle_key("Enter", "Enter", false, false);
        }

        // History should be capped at MAX_HISTORY
        assert_eq!(term.history.len(), MAX_HISTORY);

        // Most recent command should be at the front
        assert_eq!(term.history.front().unwrap(), &format!("echo cmd{}", MAX_HISTORY + 49));

        // Oldest commands should be gone - the back should NOT be cmd0
        // It should be cmd50 (we kept the last 100 of 150 commands)
        assert_eq!(term.history.back().unwrap(), &format!("echo cmd{}", 50));
    }

    #[test]
    fn test_terminal_history_no_duplicates_at_front() {
        let mut term = Terminal::new();

        // Execute same command twice
        term.input = "echo test".to_string();
        term.cursor = term.input.len();
        term.handle_key("Enter", "Enter", false, false);

        term.input = "echo test".to_string();
        term.cursor = term.input.len();
        term.handle_key("Enter", "Enter", false, false);

        // Should only have one entry (deduped at front)
        let count = term.history.iter().filter(|h| *h == "echo test").count();
        assert_eq!(count, 1);
    }
}
