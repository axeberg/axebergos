//! axeberg - A personal mini-OS in Rust, compiled to WASM
//!
//! Design principles (inspired by Radiant + Oxide):
//! - Tractable: bounded complexity, comprehensible by one human
//! - Performance is fundamental: no perceptible delay
//! - Build-time task specification: no dynamic chaos
//! - True ownership: you can read and modify everything

use wasm_bindgen::prelude::*;

pub mod kernel;
pub mod vfs;

mod boot;

/// Initialize panic hook for better error messages in browser console
fn init_panic_hook() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

/// Boot the system. This is the WASM entry point.
#[wasm_bindgen(start)]
pub fn main() {
    init_panic_hook();
    boot::boot();
}

/// Console logging helper
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

/// Log to browser console
#[macro_export]
macro_rules! console_log {
    ($($t:tt)*) => {
        $crate::log(&format!($($t)*))
    };
}
