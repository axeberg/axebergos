//! Terminal using xterm.js
//!
//! This replaces the custom wgpu compositor with xterm.js for proper
//! terminal emulation including fonts, colors, scrollback, and selection.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use xterm_js_rs::addons::fit::FitAddon;
use xterm_js_rs::{OnKeyEvent, Terminal, TerminalOptions, Theme};

use crate::shell;

thread_local! {
    static TERMINAL: RefCell<Option<Rc<Terminal>>> = RefCell::new(None);
    static INPUT_BUFFER: RefCell<String> = RefCell::new(String::new());
    static CURSOR_POS: RefCell<usize> = RefCell::new(0);
}

const PROMPT: &str = "$ ";

/// Initialize the xterm.js terminal
pub fn init() -> Result<(), JsValue> {
    let terminal = Terminal::new(
        TerminalOptions::new()
            .with_cursor_blink(true)
            .with_cursor_width(2)
            .with_font_size(14)
            .with_font_family("'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace")
            .with_draw_bold_text_in_bright_colors(true)
            .with_right_click_selects_word(true)
            .with_theme(
                Theme::new()
                    .with_foreground("#c0caf5")      // Tokyo Night text
                    .with_background("#1a1b26")      // Tokyo Night bg
                    .with_cursor("#7aa2f7")          // Tokyo Night blue
                    .with_cursor_accent("#1a1b26")
            ),
    );

    // Create container div
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;

    let container = document.create_element("div")?;
    container.set_id("terminal");

    // Style the container to fill the screen
    let style = container.dyn_ref::<web_sys::HtmlElement>()
        .ok_or("not html element")?
        .style();
    style.set_property("position", "fixed")?;
    style.set_property("top", "0")?;
    style.set_property("left", "0")?;
    style.set_property("width", "100%")?;
    style.set_property("height", "100%")?;
    style.set_property("background", "#1a1b26")?;

    document.body().ok_or("no body")?.append_child(&container)?;

    // Open terminal in container
    terminal.open(container.dyn_into()?);

    // Add fit addon to auto-resize
    let fit_addon = FitAddon::new();
    terminal.load_addon(fit_addon.clone().dyn_into()?);
    fit_addon.fit();

    // Welcome message
    terminal.writeln("axeberg v0.1.0");
    terminal.writeln("Type 'help' for available commands.");
    terminal.writeln("");
    write_prompt(&terminal);

    // Store terminal globally
    let term_rc = Rc::new(terminal);
    TERMINAL.with(|t| {
        *t.borrow_mut() = Some(term_rc.clone());
    });

    // Set up keyboard handler
    setup_keyboard_handler(term_rc.clone());

    // Set up resize handler
    setup_resize_handler(fit_addon);

    Ok(())
}

fn write_prompt(term: &Terminal) {
    term.write(PROMPT);
}

fn setup_keyboard_handler(term: Rc<Terminal>) {
    // Clone term for use inside the closure
    let term_for_closure = term.clone();

    let callback = Closure::wrap(Box::new(move |e: OnKeyEvent| {
        let event = e.dom_event();
        let key = event.key();
        let key_code = event.key_code();
        let ctrl = event.ctrl_key();

        INPUT_BUFFER.with(|buf| {
            CURSOR_POS.with(|pos| {
                let mut buffer = buf.borrow_mut();
                let mut cursor = pos.borrow_mut();

                match key_code {
                    // Enter
                    13 => {
                        term_for_closure.writeln("");
                        if !buffer.is_empty() {
                            let input = buffer.clone();
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
                    // Backspace
                    8 => {
                        if *cursor > 0 {
                            buffer.remove(*cursor - 1);
                            *cursor -= 1;
                            // Redraw line
                            term_for_closure.write("\x1b[2K\r"); // Clear line
                            term_for_closure.write(PROMPT);
                            term_for_closure.write(&buffer);
                            // Move cursor back if needed
                            let move_back = buffer.len() - *cursor;
                            if move_back > 0 {
                                term_for_closure.write(&format!("\x1b[{}D", move_back));
                            }
                        }
                    }
                    // Left arrow
                    37 => {
                        if *cursor > 0 {
                            term_for_closure.write("\x1b[D");
                            *cursor -= 1;
                        }
                    }
                    // Right arrow
                    39 => {
                        if *cursor < buffer.len() {
                            term_for_closure.write("\x1b[C");
                            *cursor += 1;
                        }
                    }
                    // Ctrl+C
                    67 if ctrl => {
                        term_for_closure.writeln("^C");
                        buffer.clear();
                        *cursor = 0;
                        write_prompt(&term_for_closure);
                    }
                    // Ctrl+L - clear screen
                    76 if ctrl => {
                        term_for_closure.clear();
                        write_prompt(&term_for_closure);
                        term_for_closure.write(&buffer);
                    }
                    // Ctrl+U - clear line
                    85 if ctrl => {
                        buffer.clear();
                        *cursor = 0;
                        term_for_closure.write("\x1b[2K\r");
                        write_prompt(&term_for_closure);
                    }
                    // Ctrl+A - start of line
                    65 if ctrl => {
                        if *cursor > 0 {
                            term_for_closure.write(&format!("\x1b[{}D", *cursor));
                            *cursor = 0;
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
                    // Regular printable character
                    _ => {
                        if key.len() == 1 && !ctrl {
                            let ch = key.chars().next().unwrap();
                            if ch.is_ascii_graphic() || ch == ' ' {
                                buffer.insert(*cursor, ch);
                                *cursor += 1;
                                // Redraw from cursor
                                term_for_closure.write(&buffer[*cursor - 1..]);
                                let move_back = buffer.len() - *cursor;
                                if move_back > 0 {
                                    term_for_closure.write(&format!("\x1b[{}D", move_back));
                                }
                            }
                        }
                    }
                }
            });
        });
    }) as Box<dyn FnMut(_)>);

    term.on_key(callback.as_ref().unchecked_ref());
    callback.forget();
}

fn setup_resize_handler(fit_addon: FitAddon) {
    let callback = Closure::wrap(Box::new(move || {
        fit_addon.fit();
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
