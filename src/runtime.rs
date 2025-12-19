//! Browser runtime integration
//!
//! This module bridges the kernel to the browser's event loop:
//! - requestAnimationFrame drives the tick loop
//! - DOM events are captured and pushed to the event queue
//!
//! The goal: make the browser disappear. You're running an OS.

use crate::console_log;
use crate::kernel::{self, events};
use std::cell::RefCell;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// State for the animation frame loop
struct RuntimeState {
    /// Callback for requestAnimationFrame (stored to prevent GC)
    frame_closure: Option<Closure<dyn FnMut(f64)>>,
    /// Is the runtime running?
    running: bool,
    /// Frame count
    frame_count: u64,
}

thread_local! {
    static STATE: RefCell<RuntimeState> = RefCell::new(RuntimeState {
        frame_closure: None,
        running: false,
        frame_count: 0,
    });
}

/// Start the runtime loop
pub fn start() {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.running {
            console_log!("[runtime] Already running");
            return;
        }
        state.running = true;
    });

    console_log!("[runtime] Starting frame loop...");

    // Set up event listeners
    setup_event_listeners();

    // Kick off the frame loop
    request_animation_frame();
}

/// Stop the runtime loop
pub fn stop() {
    STATE.with(|state| {
        state.borrow_mut().running = false;
    });
    console_log!("[runtime] Stopped");
}

/// Request the next animation frame
fn request_animation_frame() {
    let window = match web_sys::window() {
        Some(w) => w,
        None => {
            console_log!("[runtime] No window object");
            return;
        }
    };

    // Create a closure that will be called on each frame
    let closure = Closure::wrap(Box::new(move |timestamp: f64| {
        frame_tick(timestamp);
    }) as Box<dyn FnMut(f64)>);

    // Store the closure to prevent it from being dropped
    STATE.with(|state| {
        state.borrow_mut().frame_closure = Some(closure);
    });

    // Request the frame
    STATE.with(|state| {
        let state = state.borrow();
        if let Some(ref closure) = state.frame_closure {
            let _ = window.request_animation_frame(closure.as_ref().unchecked_ref());
        }
    });
}

/// Called every frame by requestAnimationFrame
fn frame_tick(timestamp: f64) {
    let should_continue = STATE.with(|state| {
        let mut state = state.borrow_mut();
        if !state.running {
            return false;
        }
        state.frame_count += 1;
        true
    });

    if !should_continue {
        return;
    }

    // Push a frame event
    events::push_system(events::SystemEvent::Frame { timestamp });

    // Tick the kernel
    kernel::tick();

    // Schedule next frame
    request_animation_frame();
}

/// Set up event listeners for input
fn setup_event_listeners() {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return,
    };

    let document = match window.document() {
        Some(d) => d,
        None => return,
    };

    // Mouse move
    {
        let closure = Closure::wrap(Box::new(|event: web_sys::MouseEvent| {
            events::push_input(events::InputEvent::MouseMove {
                x: event.client_x() as f64,
                y: event.client_y() as f64,
            });
        }) as Box<dyn FnMut(_)>);

        let _ = document.add_event_listener_with_callback(
            "mousemove",
            closure.as_ref().unchecked_ref(),
        );
        closure.forget(); // Leak intentionally - lives for page lifetime
    }

    // Mouse down
    {
        let closure = Closure::wrap(Box::new(|event: web_sys::MouseEvent| {
            events::push_input(events::InputEvent::MouseDown {
                x: event.client_x() as f64,
                y: event.client_y() as f64,
                button: mouse_button(event.button()),
            });
        }) as Box<dyn FnMut(_)>);

        let _ = document.add_event_listener_with_callback(
            "mousedown",
            closure.as_ref().unchecked_ref(),
        );
        closure.forget();
    }

    // Mouse up
    {
        let closure = Closure::wrap(Box::new(|event: web_sys::MouseEvent| {
            events::push_input(events::InputEvent::MouseUp {
                x: event.client_x() as f64,
                y: event.client_y() as f64,
                button: mouse_button(event.button()),
            });
        }) as Box<dyn FnMut(_)>);

        let _ = document.add_event_listener_with_callback(
            "mouseup",
            closure.as_ref().unchecked_ref(),
        );
        closure.forget();
    }

    // Key down
    {
        let closure = Closure::wrap(Box::new(|event: web_sys::KeyboardEvent| {
            events::push_input(events::InputEvent::KeyDown {
                key: event.key(),
                code: event.code(),
                modifiers: key_modifiers(&event),
            });
        }) as Box<dyn FnMut(_)>);

        let _ = document.add_event_listener_with_callback(
            "keydown",
            closure.as_ref().unchecked_ref(),
        );
        closure.forget();
    }

    // Key up
    {
        let closure = Closure::wrap(Box::new(|event: web_sys::KeyboardEvent| {
            events::push_input(events::InputEvent::KeyUp {
                key: event.key(),
                code: event.code(),
                modifiers: key_modifiers(&event),
            });
        }) as Box<dyn FnMut(_)>);

        let _ = document.add_event_listener_with_callback(
            "keyup",
            closure.as_ref().unchecked_ref(),
        );
        closure.forget();
    }

    // Window resize
    {
        let closure = Closure::wrap(Box::new(move |_event: web_sys::Event| {
            if let Some(window) = web_sys::window() {
                let width = window.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
                let height = window.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
                events::push_input(events::InputEvent::Resize { width, height });
            }
        }) as Box<dyn FnMut(_)>);

        let _ = window.add_event_listener_with_callback(
            "resize",
            closure.as_ref().unchecked_ref(),
        );
        closure.forget();
    }

    console_log!("[runtime] Event listeners installed");
}

/// Convert JS mouse button to our type
fn mouse_button(button: i16) -> events::MouseButton {
    match button {
        0 => events::MouseButton::Left,
        1 => events::MouseButton::Middle,
        2 => events::MouseButton::Right,
        n => events::MouseButton::Other(n as u16),
    }
}

/// Extract modifiers from keyboard event
fn key_modifiers(event: &web_sys::KeyboardEvent) -> events::Modifiers {
    events::Modifiers {
        shift: event.shift_key(),
        ctrl: event.ctrl_key(),
        alt: event.alt_key(),
        meta: event.meta_key(),
    }
}

/// Get current frame count
pub fn frame_count() -> u64 {
    STATE.with(|state| state.borrow().frame_count)
}
