//! Browser runtime integration
//!
//! This module bridges the kernel to the browser's event loop:
//! - requestAnimationFrame drives the tick loop
//! - DOM events are captured and pushed to the event queue
//! - beforeunload saves state to OPFS
//!
//! The goal: make the browser disappear. You're running an OS.

use crate::compositor;
use crate::console_log;
use crate::kernel::{self, events, syscall};
use crate::vfs::Persistence;
use std::cell::RefCell;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Auto-save interval in milliseconds (30 seconds)
const AUTO_SAVE_INTERVAL_MS: f64 = 30_000.0;

/// State for the animation frame loop
struct RuntimeState {
    /// Callback for requestAnimationFrame (stored to prevent GC)
    frame_closure: Option<Closure<dyn FnMut(f64)>>,
    /// Is the runtime running?
    running: bool,
    /// Frame count
    frame_count: u64,
    /// Last auto-save timestamp
    last_save_time: f64,
    /// Is save in progress?
    save_in_progress: bool,
}

thread_local! {
    static STATE: RefCell<RuntimeState> = RefCell::new(RuntimeState {
        frame_closure: None,
        running: false,
        frame_count: 0,
        last_save_time: 0.0,
        save_in_progress: false,
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
    let (should_continue, should_save) = STATE.with(|state| {
        let mut state = state.borrow_mut();
        if !state.running {
            return (false, false);
        }
        state.frame_count += 1;

        // Check if it's time for auto-save
        let should_save = !state.save_in_progress
            && (timestamp - state.last_save_time) >= AUTO_SAVE_INTERVAL_MS;

        if should_save {
            state.save_in_progress = true;
            state.last_save_time = timestamp;
        }

        (true, should_save)
    });

    if !should_continue {
        return;
    }

    // Trigger auto-save if needed
    if should_save {
        wasm_bindgen_futures::spawn_local(async {
            match save_state().await {
                Ok(()) => {
                    // Mark save as complete
                    STATE.with(|state| {
                        state.borrow_mut().save_in_progress = false;
                    });
                }
                Err(e) => {
                    console_log!("[runtime] Auto-save failed: {}", e);
                    STATE.with(|state| {
                        state.borrow_mut().save_in_progress = false;
                    });
                }
            }
        });
    }

    // Push a frame event
    events::push_system(events::SystemEvent::Frame { timestamp });

    // Process input events for compositor
    process_compositor_events();

    // Tick the kernel
    kernel::tick();

    // Render the compositor
    compositor::render();

    // Schedule next frame
    request_animation_frame();
}

/// Forward relevant events to the compositor
fn process_compositor_events() {
    // Drain events and forward to compositor
    for event in events::drain_events() {
        match event {
            events::Event::Input(events::InputEvent::Resize { width, height }) => {
                compositor::resize(width, height);
            }
            events::Event::Input(events::InputEvent::KeyDown {
                key,
                code,
                modifiers,
            }) => {
                // Forward to compositor (terminal windows handle keyboard input)
                compositor::handle_key(&key, &code, modifiers.ctrl, modifiers.alt);
            }
            _ => {}
        }
    }
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

    // Key down
    {
        let closure = Closure::wrap(Box::new(|event: web_sys::KeyboardEvent| {
            // Ignore auto-repeat events
            if event.repeat() {
                return;
            }
            // Prevent default for most keys to avoid browser shortcuts
            event.prevent_default();
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

    // Before unload - save state to OPFS
    {
        let closure = Closure::wrap(Box::new(move |_event: web_sys::BeforeUnloadEvent| {
            // Sync VFS to OPFS
            wasm_bindgen_futures::spawn_local(async {
                if let Err(e) = save_state().await {
                    console_log!("[runtime] Failed to save state: {}", e);
                }
            });
        }) as Box<dyn FnMut(_)>);

        let _ = window.add_event_listener_with_callback(
            "beforeunload",
            closure.as_ref().unchecked_ref(),
        );
        closure.forget();
    }

    console_log!("[runtime] Event listeners installed");
}

/// Save VFS state to OPFS
async fn save_state() -> Result<(), String> {
    // Get VFS snapshot
    let data = syscall::vfs_snapshot().map_err(|e| e.to_string())?;

    // Restore into a MemoryFs to pass to Persistence::save
    let fs = crate::vfs::MemoryFs::from_json(&data).map_err(|e| e.to_string())?;

    // Save to OPFS
    Persistence::save(&fs).await?;

    console_log!("[runtime] State saved to OPFS");
    Ok(())
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
