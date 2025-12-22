//! Terminal using xterm.js
//!
//! Direct wasm_bindgen bindings to xterm.js loaded via script tag.
//! This avoids the bundler requirement of xterm-js-rs.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

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
}

const PROMPT: &str = "$ ";

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

    // Theme
    let theme = js_sys::Object::new();
    js_sys::Reflect::set(&theme, &"foreground".into(), &"#c0caf5".into())?;
    js_sys::Reflect::set(&theme, &"background".into(), &"#1a1b26".into())?;
    js_sys::Reflect::set(&theme, &"cursor".into(), &"#7aa2f7".into())?;
    js_sys::Reflect::set(&theme, &"cursorAccent".into(), &"#1a1b26".into())?;
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

    // Set up keyboard handler
    setup_keyboard_handler(term_rc.clone());

    // Set up resize handler
    setup_resize_handler(fit_rc);

    // Focus terminal
    term_rc.focus();

    Ok(())
}

fn write_prompt(term: &XTerm) {
    term.write(PROMPT);
}

fn setup_keyboard_handler(term: Rc<XTerm>) {
    let term_for_closure = term.clone();

    let callback = Closure::wrap(Box::new(move |event: JsValue| {
        // event is { key: string, domEvent: KeyboardEvent }
        let dom_event: web_sys::KeyboardEvent = js_sys::Reflect::get(&event, &"domEvent".into())
            .unwrap()
            .unchecked_into();
        let key: String = js_sys::Reflect::get(&event, &"key".into())
            .unwrap()
            .as_string()
            .unwrap_or_default();

        let key_code = dom_event.key_code();
        let ctrl = dom_event.ctrl_key();

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

fn setup_resize_handler(fit_addon: Rc<XTermFitAddon>) {
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
