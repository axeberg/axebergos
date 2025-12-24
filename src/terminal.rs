//! Terminal using xterm.js
//!
//! Direct wasm_bindgen bindings to xterm.js loaded via script tag.
//! This avoids the bundler requirement of xterm-js-rs.
//!
//! Implements readline-like line editing:
//! - Ctrl+A/E: start/end of line
//! - Ctrl+K: kill to end of line
//! - Ctrl+U: kill to start of line
//! - Ctrl+W: delete word backward
//! - Ctrl+Y: yank (paste from kill ring)
//! - Alt+B/F: word backward/forward
//! - Alt+D: delete word forward
//! - Ctrl+R: reverse history search
//! - Tab: file/command completion

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use crate::kernel::syscall;
use crate::shell;

// Direct bindings to xterm.js globals (loaded via script tag)
#[wasm_bindgen]
extern "C" {
    /// The xterm.js Terminal class (global `Terminal`)
    #[wasm_bindgen(js_name = Terminal)]
    type XTerm;

    #[wasm_bindgen(constructor, js_class = "Terminal")]
    fn new(options: &JsValue) -> XTerm;

    #[wasm_bindgen(method)]
    fn open(this: &XTerm, element: &web_sys::HtmlElement);

    #[wasm_bindgen(method)]
    fn write(this: &XTerm, data: &str);

    #[wasm_bindgen(method)]
    fn writeln(this: &XTerm, data: &str);

    #[wasm_bindgen(method)]
    fn clear(this: &XTerm);

    #[wasm_bindgen(method)]
    fn focus(this: &XTerm);

    #[wasm_bindgen(method, js_name = loadAddon)]
    fn load_addon(this: &XTerm, addon: &JsValue);

    #[wasm_bindgen(method, js_name = onKey)]
    fn on_key(this: &XTerm, callback: &js_sys::Function);

    #[wasm_bindgen(method, js_name = onData)]
    fn on_data(this: &XTerm, callback: &js_sys::Function);

    #[wasm_bindgen(method, getter)]
    fn cols(this: &XTerm) -> u32;

    #[wasm_bindgen(method, getter)]
    fn rows(this: &XTerm) -> u32;

    /// The xterm-addon-fit FitAddon class (global `FitAddon`)
    #[wasm_bindgen(js_name = FitAddon)]
    type XTermFitAddon;

    #[wasm_bindgen(constructor, js_class = "FitAddon")]
    fn new_fit() -> XTermFitAddon;

    #[wasm_bindgen(method)]
    fn fit(this: &XTermFitAddon);
}

thread_local! {
    static TERMINAL: RefCell<Option<Rc<XTerm>>> = RefCell::new(None);
    static FIT_ADDON: RefCell<Option<Rc<XTermFitAddon>>> = RefCell::new(None);
    static INPUT_BUFFER: RefCell<String> = RefCell::new(String::new());
    static CURSOR_POS: RefCell<usize> = RefCell::new(0);
    // Command history
    static HISTORY: RefCell<Vec<String>> = RefCell::new(Vec::new());
    static HISTORY_POS: RefCell<usize> = RefCell::new(0);
    // Buffer to restore when navigating past end of history
    static SAVED_BUFFER: RefCell<String> = RefCell::new(String::new());
    // Kill ring for Ctrl+K, Ctrl+W, Ctrl+Y
    static KILL_RING: RefCell<String> = RefCell::new(String::new());
    // Reverse search state
    static SEARCH_MODE: RefCell<bool> = RefCell::new(false);
    static SEARCH_QUERY: RefCell<String> = RefCell::new(String::new());
    static SEARCH_RESULT_IDX: RefCell<Option<usize>> = RefCell::new(None);
}

const PROMPT: &str = "$ ";
const SEARCH_PROMPT: &str = "(reverse-i-search)`";

/// Initialize the xterm.js terminal
pub fn init() -> Result<(), JsValue> {
    // Create terminal options
    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &"cursorBlink".into(), &true.into())?;
    js_sys::Reflect::set(&options, &"cursorWidth".into(), &2.into())?;
    js_sys::Reflect::set(&options, &"fontSize".into(), &14.into())?;
    js_sys::Reflect::set(
        &options,
        &"fontFamily".into(),
        &"'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace".into(),
    )?;
    js_sys::Reflect::set(&options, &"drawBoldTextInBrightColors".into(), &true.into())?;
    js_sys::Reflect::set(&options, &"rightClickSelectsWord".into(), &true.into())?;

    // Theme - Tokyo Night
    let theme = js_sys::Object::new();
    js_sys::Reflect::set(&theme, &"foreground".into(), &"#c0caf5".into())?;
    js_sys::Reflect::set(&theme, &"background".into(), &"#1a1b26".into())?;
    js_sys::Reflect::set(&theme, &"cursor".into(), &"#7aa2f7".into())?;
    js_sys::Reflect::set(&theme, &"cursorAccent".into(), &"#1a1b26".into())?;
    js_sys::Reflect::set(&theme, &"selectionBackground".into(), &"#33467c".into())?;
    // ANSI colors
    js_sys::Reflect::set(&theme, &"black".into(), &"#15161e".into())?;
    js_sys::Reflect::set(&theme, &"red".into(), &"#f7768e".into())?;
    js_sys::Reflect::set(&theme, &"green".into(), &"#9ece6a".into())?;
    js_sys::Reflect::set(&theme, &"yellow".into(), &"#e0af68".into())?;
    js_sys::Reflect::set(&theme, &"blue".into(), &"#7aa2f7".into())?;
    js_sys::Reflect::set(&theme, &"magenta".into(), &"#bb9af7".into())?;
    js_sys::Reflect::set(&theme, &"cyan".into(), &"#7dcfff".into())?;
    js_sys::Reflect::set(&theme, &"white".into(), &"#a9b1d6".into())?;
    js_sys::Reflect::set(&theme, &"brightBlack".into(), &"#414868".into())?;
    js_sys::Reflect::set(&theme, &"brightRed".into(), &"#f7768e".into())?;
    js_sys::Reflect::set(&theme, &"brightGreen".into(), &"#9ece6a".into())?;
    js_sys::Reflect::set(&theme, &"brightYellow".into(), &"#e0af68".into())?;
    js_sys::Reflect::set(&theme, &"brightBlue".into(), &"#7aa2f7".into())?;
    js_sys::Reflect::set(&theme, &"brightMagenta".into(), &"#bb9af7".into())?;
    js_sys::Reflect::set(&theme, &"brightCyan".into(), &"#7dcfff".into())?;
    js_sys::Reflect::set(&theme, &"brightWhite".into(), &"#c0caf5".into())?;
    js_sys::Reflect::set(&options, &"theme".into(), &theme)?;

    // Create terminal
    let terminal = XTerm::new(&options.into());

    // Create container div
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;

    let container = document.create_element("div")?;
    container.set_id("terminal");

    // Style the container to fill the screen
    let html_container: web_sys::HtmlElement = container.dyn_into()?;
    let style = html_container.style();
    style.set_property("position", "fixed")?;
    style.set_property("top", "0")?;
    style.set_property("left", "0")?;
    style.set_property("width", "100%")?;
    style.set_property("height", "100%")?;
    style.set_property("background", "#1a1b26")?;

    document
        .body()
        .ok_or("no body")?
        .append_child(&html_container)?;

    // Open terminal in container
    terminal.open(&html_container);

    // Add fit addon to auto-resize
    let fit_addon = XTermFitAddon::new_fit();
    terminal.load_addon(&fit_addon.unchecked_ref());
    fit_addon.fit();

    // Load history from filesystem
    load_history();

    // Welcome message
    terminal.writeln("axeberg v0.1.0");
    terminal.writeln("Type 'help' for available commands.");
    terminal.writeln("");
    write_prompt(&terminal);

    // Store terminal and fit addon globally
    let term_rc = Rc::new(terminal);
    let fit_rc = Rc::new(fit_addon);

    TERMINAL.with(|t| {
        *t.borrow_mut() = Some(term_rc.clone());
    });
    FIT_ADDON.with(|f| {
        *f.borrow_mut() = Some(fit_rc.clone());
    });

    // Set up keyboard handler (for special keys like Ctrl+, arrows)
    setup_keyboard_handler(term_rc.clone());

    // Set up data handler (for text input including paste)
    setup_data_handler(term_rc.clone());

    // Set up resize handler
    setup_resize_handler(fit_rc);

    // Focus terminal
    term_rc.focus();

    Ok(())
}

fn write_prompt(term: &XTerm) {
    term.write(PROMPT);
}

/// Replace the current input line with new text
fn replace_line(term: &XTerm, buffer: &mut String, cursor: &mut usize, new_text: &str) {
    term.write("\x1b[2K\r"); // Clear line, move to start
    term.write(PROMPT);
    term.write(new_text);
    *buffer = new_text.to_string();
    *cursor = buffer.len();
}

/// Redraw the current line (used after buffer modifications)
fn redraw_line(term: &XTerm, buffer: &str, cursor: usize) {
    term.write("\x1b[2K\r");
    term.write(PROMPT);
    term.write(buffer);
    let move_back = buffer.len() - cursor;
    if move_back > 0 {
        term.write(&format!("\x1b[{}D", move_back));
    }
}

/// Find word boundary going backward from position
fn word_start(buffer: &str, pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    let bytes = buffer.as_bytes();
    let mut i = pos - 1;
    // Skip whitespace
    while i > 0 && bytes[i].is_ascii_whitespace() {
        i -= 1;
    }
    // Skip word chars
    while i > 0 && !bytes[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    i
}

/// Find word boundary going forward from position
fn word_end(buffer: &str, pos: usize) -> usize {
    let len = buffer.len();
    if pos >= len {
        return len;
    }
    let bytes = buffer.as_bytes();
    let mut i = pos;
    // Skip current word chars
    while i < len && !bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    // Skip whitespace
    while i < len && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// Perform tab completion
fn complete(buffer: &str, cursor: usize) -> Option<(String, usize)> {
    // Find the word being completed
    let before_cursor = &buffer[..cursor];
    let word_start = before_cursor.rfind(|c: char| c.is_whitespace()).map(|i| i + 1).unwrap_or(0);
    let prefix = &before_cursor[word_start..];

    if prefix.is_empty() {
        return None;
    }

    // Check if this looks like a path
    if prefix.contains('/') || word_start > 0 {
        // File completion
        complete_path(prefix, buffer, cursor, word_start)
    } else {
        // Command completion (first word)
        complete_command(prefix, buffer, cursor, word_start)
    }
}

fn complete_path(prefix: &str, buffer: &str, cursor: usize, word_start: usize) -> Option<(String, usize)> {
    let (dir, file_prefix) = if let Some(last_slash) = prefix.rfind('/') {
        let dir = if last_slash == 0 { "/" } else { &prefix[..last_slash] };
        (dir.to_string(), &prefix[last_slash + 1..])
    } else {
        (".".to_string(), prefix)
    };

    // List directory contents
    let entries = syscall::readdir(&dir).ok()?;
    let matches: Vec<_> = entries
        .iter()
        .filter(|e| e.starts_with(file_prefix))
        .collect();

    if matches.is_empty() {
        return None;
    }

    if matches.len() == 1 {
        // Single match - complete it
        let completion = matches[0];
        let full_path = if dir == "." {
            completion.to_string()
        } else if dir == "/" {
            format!("/{}", completion)
        } else {
            format!("{}/{}", dir, completion)
        };

        // Check if it's a directory and add /
        let stat_path = if full_path.starts_with('/') {
            full_path.clone()
        } else {
            format!("{}/{}", syscall::getcwd().unwrap_or_default().display(), full_path)
        };
        let is_dir = syscall::stat(&stat_path).map(|s| s.is_dir).unwrap_or(false);
        let suffix = if is_dir { "/" } else { " " };

        let new_buffer = format!(
            "{}{}{}{}",
            &buffer[..word_start],
            full_path,
            suffix,
            &buffer[cursor..]
        );
        let new_cursor = word_start + full_path.len() + suffix.len();
        Some((new_buffer, new_cursor))
    } else {
        // Multiple matches - find common prefix
        let common = common_prefix(&matches);
        if common.len() > file_prefix.len() {
            let full_path = if dir == "." {
                common.clone()
            } else if dir == "/" {
                format!("/{}", common)
            } else {
                format!("{}/{}", dir, common)
            };
            let new_buffer = format!(
                "{}{}{}",
                &buffer[..word_start],
                full_path,
                &buffer[cursor..]
            );
            let new_cursor = word_start + full_path.len();
            Some((new_buffer, new_cursor))
        } else {
            None
        }
    }
}

fn complete_command(prefix: &str, buffer: &str, cursor: usize, word_start: usize) -> Option<(String, usize)> {
    // Built-in commands
    let builtins = [
        "cd", "pwd", "exit", "echo", "export", "unset", "env", "true", "false", "help",
        "ls", "cat", "mkdir", "touch", "rm", "cp", "mv", "grep", "head", "tail",
        "sort", "uniq", "wc", "tee", "clear", "history", "edit", "tree", "ln", "readlink",
    ];

    let matches: Vec<_> = builtins.iter().filter(|c| c.starts_with(prefix)).collect();

    if matches.is_empty() {
        return None;
    }

    if matches.len() == 1 {
        let new_buffer = format!(
            "{}{} {}",
            &buffer[..word_start],
            matches[0],
            &buffer[cursor..]
        );
        let new_cursor = word_start + matches[0].len() + 1;
        Some((new_buffer, new_cursor))
    } else {
        let common = common_prefix_str(&matches);
        if common.len() > prefix.len() {
            let new_buffer = format!(
                "{}{}{}",
                &buffer[..word_start],
                common,
                &buffer[cursor..]
            );
            let new_cursor = word_start + common.len();
            Some((new_buffer, new_cursor))
        } else {
            None
        }
    }
}

fn common_prefix(strings: &[&String]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    let first = strings[0].as_str();
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

fn common_prefix_str(strings: &[&&str]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    let first = *strings[0];
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

/// Search history backward for query
fn search_history(query: &str, start_idx: Option<usize>) -> Option<(usize, String)> {
    HISTORY.with(|h| {
        let history = h.borrow();
        let start = start_idx.unwrap_or(history.len());
        for i in (0..start).rev() {
            if history[i].contains(query) {
                return Some((i, history[i].clone()));
            }
        }
        None
    })
}

/// Display search prompt
fn show_search_prompt(term: &XTerm, query: &str, result: Option<&str>) {
    term.write("\x1b[2K\r");
    term.write(SEARCH_PROMPT);
    term.write(query);
    term.write("': ");
    if let Some(cmd) = result {
        term.write(cmd);
    }
}

/// Exit search mode and restore normal prompt
fn exit_search_mode(term: &XTerm, buffer: &mut String, cursor: &mut usize, accept: bool) {
    SEARCH_MODE.with(|m| *m.borrow_mut() = false);

    if accept {
        SEARCH_RESULT_IDX.with(|idx| {
            if let Some(i) = *idx.borrow() {
                HISTORY.with(|h| {
                    let history = h.borrow();
                    if i < history.len() {
                        *buffer = history[i].clone();
                        *cursor = buffer.len();
                    }
                });
            }
        });
    }

    SEARCH_QUERY.with(|q| q.borrow_mut().clear());
    SEARCH_RESULT_IDX.with(|idx| *idx.borrow_mut() = None);

    redraw_line(term, buffer, *cursor);
}

/// Load history from filesystem
fn load_history() {
    if let Ok(content) = syscall::read_file("/home/user/.shell_history") {
        HISTORY.with(|h| {
            let mut history = h.borrow_mut();
            for line in content.lines() {
                if !line.is_empty() {
                    history.push(line.to_string());
                }
            }
            HISTORY_POS.with(|p| {
                *p.borrow_mut() = history.len();
            });
        });
    }
}

/// Save history to filesystem
fn save_history() {
    HISTORY.with(|h| {
        let history = h.borrow();
        // Keep last 1000 entries
        let start = history.len().saturating_sub(1000);
        let content: String = history[start..]
            .iter()
            .map(|s| format!("{}\n", s))
            .collect();
        let _ = syscall::write_file("/home/user/.shell_history", &content);
    });
}

fn setup_keyboard_handler(term: Rc<XTerm>) {
    let term_for_closure = term.clone();

    let callback = Closure::wrap(Box::new(move |event: JsValue| {
        let dom_event: web_sys::KeyboardEvent = js_sys::Reflect::get(&event, &"domEvent".into())
            .unwrap()
            .unchecked_into();
        let key: String = js_sys::Reflect::get(&event, &"key".into())
            .unwrap()
            .as_string()
            .unwrap_or_default();

        let key_code = dom_event.key_code();
        let ctrl = dom_event.ctrl_key();
        let alt = dom_event.alt_key();
        let shift = dom_event.shift_key();

        // Check if editor is active - route special keys to editor
        // Regular characters are handled by on_data via handle_paste
        if crate::editor::is_active() {
            if let Some(editor_key) = crate::editor::parse_key(&key, key_code, ctrl, alt, shift) {
                // Skip regular characters - on_data handles those
                if matches!(editor_key, crate::editor::Key::Char(_)) {
                    return;
                }
                let should_quit = crate::editor::process_key(editor_key);
                if should_quit {
                    crate::editor::stop();
                    // Clear screen and show prompt
                    term_for_closure.clear();
                    term_for_closure.write("\x1b[H");
                    term_for_closure.writeln("Editor closed.");
                    write_prompt(&term_for_closure);
                }
            }
            return;
        }

        // Check if in search mode
        let in_search = SEARCH_MODE.with(|m| *m.borrow());

        INPUT_BUFFER.with(|buf| {
            CURSOR_POS.with(|pos| {
                let mut buffer = buf.borrow_mut();
                let mut cursor = pos.borrow_mut();

                if in_search {
                    // Search mode key handling
                    match key_code {
                        // Escape or Ctrl+G - cancel search
                        27 | 71 if key_code == 27 || ctrl => {
                            exit_search_mode(&term_for_closure, &mut buffer, &mut cursor, false);
                        }
                        // Enter - accept search result
                        13 => {
                            exit_search_mode(&term_for_closure, &mut buffer, &mut cursor, true);
                        }
                        // Ctrl+R - search again (previous match)
                        82 if ctrl => {
                            SEARCH_QUERY.with(|q| {
                                let query = q.borrow().clone();
                                if !query.is_empty() {
                                    let start = SEARCH_RESULT_IDX.with(|idx| *idx.borrow());
                                    if let Some((i, cmd)) = search_history(&query, start) {
                                        SEARCH_RESULT_IDX.with(|idx| *idx.borrow_mut() = Some(i));
                                        show_search_prompt(&term_for_closure, &query, Some(&cmd));
                                    }
                                }
                            });
                        }
                        // Backspace
                        8 => {
                            SEARCH_QUERY.with(|q| {
                                let mut query = q.borrow_mut();
                                if !query.is_empty() {
                                    query.pop();
                                    let query_str = query.clone();
                                    drop(query);
                                    if query_str.is_empty() {
                                        SEARCH_RESULT_IDX.with(|idx| *idx.borrow_mut() = None);
                                        show_search_prompt(&term_for_closure, "", None);
                                    } else if let Some((i, cmd)) = search_history(&query_str, None) {
                                        SEARCH_RESULT_IDX.with(|idx| *idx.borrow_mut() = Some(i));
                                        show_search_prompt(&term_for_closure, &query_str, Some(&cmd));
                                    } else {
                                        show_search_prompt(&term_for_closure, &query_str, None);
                                    }
                                }
                            });
                        }
                        // Printable character - add to search
                        _ => {
                            if key.len() == 1 && !ctrl && !alt {
                                let ch = key.chars().next().unwrap();
                                if ch.is_ascii_graphic() || ch == ' ' {
                                    SEARCH_QUERY.with(|q| {
                                        let mut query = q.borrow_mut();
                                        query.push(ch);
                                        let query_str = query.clone();
                                        drop(query);
                                        if let Some((i, cmd)) = search_history(&query_str, None) {
                                            SEARCH_RESULT_IDX.with(|idx| *idx.borrow_mut() = Some(i));
                                            show_search_prompt(&term_for_closure, &query_str, Some(&cmd));
                                        } else {
                                            show_search_prompt(&term_for_closure, &query_str, None);
                                        }
                                    });
                                }
                            }
                        }
                    }
                    return;
                }

                // Normal mode key handling
                match key_code {
                    // Enter
                    13 => {
                        term_for_closure.writeln("");
                        if !buffer.is_empty() {
                            let input = buffer.clone();

                            // Add to history (avoid duplicates of last command)
                            HISTORY.with(|h| {
                                let mut history = h.borrow_mut();
                                if history.last() != Some(&input) {
                                    history.push(input.clone());
                                }
                                HISTORY_POS.with(|p| {
                                    *p.borrow_mut() = history.len();
                                });
                            });
                            SAVED_BUFFER.with(|s| s.borrow_mut().clear());

                            // Save history periodically
                            HISTORY.with(|h| {
                                if h.borrow().len() % 10 == 0 {
                                    save_history();
                                }
                            });

                            buffer.clear();
                            *cursor = 0;

                            // Execute command through shell
                            let output = shell::execute_command(&input);
                            for line in output.lines() {
                                term_for_closure.writeln(line);
                            }
                        }
                        write_prompt(&term_for_closure);
                    }
                    // Tab - completion
                    9 => {
                        if let Some((new_buffer, new_cursor)) = complete(&buffer, *cursor) {
                            *buffer = new_buffer;
                            *cursor = new_cursor;
                            redraw_line(&term_for_closure, &buffer, *cursor);
                        }
                    }
                    // Backspace
                    8 => {
                        if *cursor > 0 {
                            buffer.remove(*cursor - 1);
                            *cursor -= 1;
                            redraw_line(&term_for_closure, &buffer, *cursor);
                        }
                    }
                    // Delete
                    46 => {
                        if *cursor < buffer.len() {
                            buffer.remove(*cursor);
                            redraw_line(&term_for_closure, &buffer, *cursor);
                        }
                    }
                    // Home
                    36 => {
                        if *cursor > 0 {
                            term_for_closure.write(&format!("\x1b[{}D", *cursor));
                            *cursor = 0;
                        }
                    }
                    // End
                    35 => {
                        let move_right = buffer.len() - *cursor;
                        if move_right > 0 {
                            term_for_closure.write(&format!("\x1b[{}C", move_right));
                            *cursor = buffer.len();
                        }
                    }
                    // Left arrow
                    37 => {
                        if alt {
                            // Alt+Left = word backward
                            let new_pos = word_start(&buffer, *cursor);
                            if new_pos < *cursor {
                                term_for_closure.write(&format!("\x1b[{}D", *cursor - new_pos));
                                *cursor = new_pos;
                            }
                        } else if *cursor > 0 {
                            term_for_closure.write("\x1b[D");
                            *cursor -= 1;
                        }
                    }
                    // Right arrow
                    39 => {
                        if alt {
                            // Alt+Right = word forward
                            let new_pos = word_end(&buffer, *cursor);
                            if new_pos > *cursor {
                                term_for_closure.write(&format!("\x1b[{}C", new_pos - *cursor));
                                *cursor = new_pos;
                            }
                        } else if *cursor < buffer.len() {
                            term_for_closure.write("\x1b[C");
                            *cursor += 1;
                        }
                    }
                    // Up arrow - previous history
                    38 => {
                        HISTORY.with(|h| {
                            HISTORY_POS.with(|p| {
                                SAVED_BUFFER.with(|s| {
                                    let history = h.borrow();
                                    let mut hist_pos = p.borrow_mut();

                                    if history.is_empty() {
                                        return;
                                    }

                                    if *hist_pos == history.len() {
                                        *s.borrow_mut() = buffer.clone();
                                    }

                                    if *hist_pos > 0 {
                                        *hist_pos -= 1;
                                        let cmd = &history[*hist_pos];
                                        replace_line(&term_for_closure, &mut buffer, &mut cursor, cmd);
                                    }
                                });
                            });
                        });
                    }
                    // Down arrow - next history
                    40 => {
                        HISTORY.with(|h| {
                            HISTORY_POS.with(|p| {
                                SAVED_BUFFER.with(|s| {
                                    let history = h.borrow();
                                    let mut hist_pos = p.borrow_mut();

                                    if *hist_pos < history.len() {
                                        *hist_pos += 1;
                                        if *hist_pos == history.len() {
                                            let saved = s.borrow().clone();
                                            replace_line(&term_for_closure, &mut buffer, &mut cursor, &saved);
                                        } else {
                                            let cmd = &history[*hist_pos];
                                            replace_line(&term_for_closure, &mut buffer, &mut cursor, cmd);
                                        }
                                    }
                                });
                            });
                        });
                    }
                    // Ctrl+A - start of line
                    65 if ctrl => {
                        if *cursor > 0 {
                            term_for_closure.write(&format!("\x1b[{}D", *cursor));
                            *cursor = 0;
                        }
                    }
                    // Ctrl+B - back one char (same as left arrow)
                    66 if ctrl => {
                        if *cursor > 0 {
                            term_for_closure.write("\x1b[D");
                            *cursor -= 1;
                        }
                    }
                    // Ctrl+C - cancel
                    67 if ctrl => {
                        term_for_closure.writeln("^C");
                        buffer.clear();
                        *cursor = 0;
                        write_prompt(&term_for_closure);
                    }
                    // Ctrl+D - EOF (exit if empty)
                    68 if ctrl => {
                        if buffer.is_empty() {
                            term_for_closure.writeln("exit");
                            // Could trigger exit here, but we're in browser
                        } else {
                            // Delete char at cursor (like Delete key)
                            if *cursor < buffer.len() {
                                buffer.remove(*cursor);
                                redraw_line(&term_for_closure, &buffer, *cursor);
                            }
                        }
                    }
                    // Ctrl+E - end of line
                    69 if ctrl => {
                        let move_right = buffer.len() - *cursor;
                        if move_right > 0 {
                            term_for_closure.write(&format!("\x1b[{}C", move_right));
                            *cursor = buffer.len();
                        }
                    }
                    // Ctrl+F - forward one char (same as right arrow)
                    70 if ctrl => {
                        if *cursor < buffer.len() {
                            term_for_closure.write("\x1b[C");
                            *cursor += 1;
                        }
                    }
                    // Ctrl+K - kill to end of line
                    75 if ctrl => {
                        if *cursor < buffer.len() {
                            let killed = buffer[*cursor..].to_string();
                            KILL_RING.with(|k| *k.borrow_mut() = killed);
                            buffer.truncate(*cursor);
                            redraw_line(&term_for_closure, &buffer, *cursor);
                        }
                    }
                    // Ctrl+L - clear screen
                    76 if ctrl => {
                        term_for_closure.clear();
                        // Move cursor to home position after clear
                        term_for_closure.write("\x1b[H");
                        write_prompt(&term_for_closure);
                        term_for_closure.write(&buffer);
                        let move_back = buffer.len() - *cursor;
                        if move_back > 0 {
                            term_for_closure.write(&format!("\x1b[{}D", move_back));
                        }
                    }
                    // Ctrl+N - next history (same as down arrow)
                    78 if ctrl => {
                        HISTORY.with(|h| {
                            HISTORY_POS.with(|p| {
                                SAVED_BUFFER.with(|s| {
                                    let history = h.borrow();
                                    let mut hist_pos = p.borrow_mut();

                                    if *hist_pos < history.len() {
                                        *hist_pos += 1;
                                        if *hist_pos == history.len() {
                                            let saved = s.borrow().clone();
                                            replace_line(&term_for_closure, &mut buffer, &mut cursor, &saved);
                                        } else {
                                            let cmd = &history[*hist_pos];
                                            replace_line(&term_for_closure, &mut buffer, &mut cursor, cmd);
                                        }
                                    }
                                });
                            });
                        });
                    }
                    // Ctrl+P - previous history (same as up arrow)
                    80 if ctrl => {
                        HISTORY.with(|h| {
                            HISTORY_POS.with(|p| {
                                SAVED_BUFFER.with(|s| {
                                    let history = h.borrow();
                                    let mut hist_pos = p.borrow_mut();

                                    if history.is_empty() {
                                        return;
                                    }

                                    if *hist_pos == history.len() {
                                        *s.borrow_mut() = buffer.clone();
                                    }

                                    if *hist_pos > 0 {
                                        *hist_pos -= 1;
                                        let cmd = &history[*hist_pos];
                                        replace_line(&term_for_closure, &mut buffer, &mut cursor, cmd);
                                    }
                                });
                            });
                        });
                    }
                    // Ctrl+R - reverse search
                    82 if ctrl => {
                        SEARCH_MODE.with(|m| *m.borrow_mut() = true);
                        SEARCH_QUERY.with(|q| q.borrow_mut().clear());
                        SEARCH_RESULT_IDX.with(|idx| *idx.borrow_mut() = None);
                        show_search_prompt(&term_for_closure, "", None);
                    }
                    // Ctrl+T - transpose characters
                    84 if ctrl => {
                        if *cursor > 0 && buffer.len() >= 2 {
                            let swap_pos = if *cursor == buffer.len() {
                                *cursor - 1
                            } else {
                                *cursor
                            };
                            if swap_pos > 0 {
                                let chars: Vec<char> = buffer.chars().collect();
                                let mut new_chars = chars.clone();
                                new_chars.swap(swap_pos - 1, swap_pos);
                                *buffer = new_chars.into_iter().collect();
                                if *cursor < buffer.len() {
                                    *cursor += 1;
                                }
                                redraw_line(&term_for_closure, &buffer, *cursor);
                            }
                        }
                    }
                    // Ctrl+U - kill to start of line
                    85 if ctrl => {
                        if *cursor > 0 {
                            let killed = buffer[..*cursor].to_string();
                            KILL_RING.with(|k| *k.borrow_mut() = killed);
                            buffer.drain(..*cursor);
                            *cursor = 0;
                            redraw_line(&term_for_closure, &buffer, *cursor);
                        }
                    }
                    // Ctrl+W - delete word backward
                    87 if ctrl => {
                        if *cursor > 0 {
                            let new_pos = word_start(&buffer, *cursor);
                            let killed = buffer[new_pos..*cursor].to_string();
                            KILL_RING.with(|k| *k.borrow_mut() = killed);
                            buffer.drain(new_pos..*cursor);
                            *cursor = new_pos;
                            redraw_line(&term_for_closure, &buffer, *cursor);
                        }
                    }
                    // Ctrl+Y - yank (paste from kill ring)
                    89 if ctrl => {
                        KILL_RING.with(|k| {
                            let text = k.borrow().clone();
                            if !text.is_empty() {
                                buffer.insert_str(*cursor, &text);
                                *cursor += text.len();
                                redraw_line(&term_for_closure, &buffer, *cursor);
                            }
                        });
                    }
                    // Alt+B - word backward
                    66 if alt => {
                        let new_pos = word_start(&buffer, *cursor);
                        if new_pos < *cursor {
                            term_for_closure.write(&format!("\x1b[{}D", *cursor - new_pos));
                            *cursor = new_pos;
                        }
                    }
                    // Alt+D - delete word forward
                    68 if alt => {
                        let end_pos = word_end(&buffer, *cursor);
                        if end_pos > *cursor {
                            let killed = buffer[*cursor..end_pos].to_string();
                            KILL_RING.with(|k| *k.borrow_mut() = killed);
                            buffer.drain(*cursor..end_pos);
                            redraw_line(&term_for_closure, &buffer, *cursor);
                        }
                    }
                    // Alt+F - word forward
                    70 if alt => {
                        let new_pos = word_end(&buffer, *cursor);
                        if new_pos > *cursor {
                            term_for_closure.write(&format!("\x1b[{}C", new_pos - *cursor));
                            *cursor = new_pos;
                        }
                    }
                    // Regular printable characters are handled by onData handler
                    // This allows proper paste support and handles all keyboard layouts
                    _ => {}
                }
            });
        });
    }) as Box<dyn FnMut(_)>);

    term.on_key(callback.as_ref().unchecked_ref());
    callback.forget();
}

/// Handle text data input (typed characters and paste)
fn setup_data_handler(term: Rc<XTerm>) {
    let term_for_closure = term.clone();

    let callback = Closure::wrap(Box::new(move |data: String| {
        // Skip control characters (handled by onKey)
        // onData receives the raw character/string
        if data.is_empty() {
            return;
        }

        // Check for control characters that should be handled by onKey
        let first_byte = data.as_bytes()[0];
        if first_byte < 32 && first_byte != 9 {
            // Control character (except tab which we handle in onKey)
            return;
        }

        // Check if editor is active - route to editor
        if crate::editor::is_active() {
            crate::editor::handle_paste(&data);
            return;
        }

        // Check if in search mode
        let in_search = SEARCH_MODE.with(|m| *m.borrow());
        if in_search {
            // Let onKey handle search mode
            return;
        }

        INPUT_BUFFER.with(|buf| {
            CURSOR_POS.with(|pos| {
                let mut buffer = buf.borrow_mut();
                let mut cursor = pos.borrow_mut();

                // Filter to only printable characters
                let printable: String = data
                    .chars()
                    .filter(|c| c.is_ascii_graphic() || *c == ' ')
                    .collect();

                if printable.is_empty() {
                    return;
                }

                // Insert at cursor position
                buffer.insert_str(*cursor, &printable);
                *cursor += printable.len();

                if printable.len() == 1 {
                    // Single character: efficient update without full redraw
                    // Write from inserted position to end of buffer
                    term_for_closure.write(&buffer[*cursor - 1..]);
                    // Move cursor back to correct position
                    let move_back = buffer.len() - *cursor;
                    if move_back > 0 {
                        term_for_closure.write(&format!("\x1b[{}D", move_back));
                    }
                } else {
                    // Multi-character paste: full redraw
                    redraw_line(&term_for_closure, &buffer, *cursor);
                }
            });
        });
    }) as Box<dyn FnMut(_)>);

    term.on_data(callback.as_ref().unchecked_ref());
    callback.forget();
}

fn setup_resize_handler(fit_addon: Rc<XTermFitAddon>) {
    let callback = Closure::wrap(Box::new(move || {
        fit_addon.fit();
        // Update editor size if active
        if crate::editor::is_active() {
            let (cols, rows) = get_size();
            crate::editor::set_screen_size(cols, rows);
            crate::editor::refresh();
        }
    }) as Box<dyn FnMut()>);

    if let Some(window) = web_sys::window() {
        let _ = window.add_event_listener_with_callback(
            "resize",
            callback.as_ref().unchecked_ref(),
        );
    }
    callback.forget();
}

/// Write a line to the terminal
pub fn writeln(text: &str) {
    TERMINAL.with(|t| {
        if let Some(term) = t.borrow().as_ref() {
            term.writeln(text);
        }
    });
}

/// Write text to the terminal (no newline)
pub fn write(text: &str) {
    TERMINAL.with(|t| {
        if let Some(term) = t.borrow().as_ref() {
            term.write(text);
        }
    });
}

/// Clear the terminal
pub fn clear() {
    TERMINAL.with(|t| {
        if let Some(term) = t.borrow().as_ref() {
            term.clear();
        }
    });
}

/// Get terminal dimensions (cols, rows)
pub fn get_size() -> (usize, usize) {
    TERMINAL.with(|t| {
        if let Some(term) = t.borrow().as_ref() {
            (term.cols() as usize, term.rows() as usize)
        } else {
            (80, 24) // fallback
        }
    })
}

/// Get command history
pub fn get_history() -> Vec<String> {
    HISTORY.with(|h| h.borrow().clone())
}
