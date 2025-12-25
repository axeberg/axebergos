//! Browser Platform Implementation
//!
//! Provides platform support for running in web browsers via:
//! - Canvas2D for terminal rendering
//! - OPFS for persistence
//! - DOM events for input
//! - requestAnimationFrame for timing

use super::{KeyEvent, Platform, PlatformError, PlatformResult, TermSize};
use std::cell::RefCell;
use std::collections::VecDeque;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

/// Browser platform state
pub struct WebPlatform {
    /// Pending key events
    key_queue: RefCell<VecDeque<KeyEvent>>,
    /// Terminal dimensions (in characters)
    term_size: TermSize,
    /// Pending output text
    output_buffer: RefCell<String>,
}

impl WebPlatform {
    pub fn new() -> Self {
        Self {
            key_queue: RefCell::new(VecDeque::new()),
            term_size: TermSize { cols: 80, rows: 24 },
            output_buffer: RefCell::new(String::new()),
        }
    }

    /// Push a key event (called from JS event handlers)
    pub fn push_key(&self, event: KeyEvent) {
        self.key_queue.borrow_mut().push_back(event);
    }

    /// Update terminal size
    pub fn set_term_size(&mut self, cols: u32, rows: u32) {
        self.term_size = TermSize { cols, rows };
    }

    /// Get pending output and clear buffer
    pub fn take_output(&self) -> String {
        self.output_buffer.borrow_mut().split_off(0)
    }
}

impl Default for WebPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl Platform for WebPlatform {
    fn write(&mut self, text: &str) {
        self.output_buffer.borrow_mut().push_str(text);
    }

    fn clear(&mut self) {
        self.output_buffer.borrow_mut().push_str("\x1b[2J\x1b[H");
    }

    fn term_size(&self) -> TermSize {
        self.term_size
    }

    fn poll_key(&mut self) -> Option<KeyEvent> {
        self.key_queue.borrow_mut().pop_front()
    }

    fn now_ms(&self) -> f64 {
        web_sys::window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0)
    }

    fn save_state(&mut self, data: &[u8]) -> PlatformResult<()> {
        // OPFS save is async, but we need sync interface
        // Queue it via spawn_local
        let data = data.to_vec();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = save_to_opfs(&data).await {
                web_sys::console::error_1(&format!("Save failed: {}", e).into());
            }
        });
        Ok(())
    }

    fn load_state(&mut self) -> PlatformResult<Option<Vec<u8>>> {
        // OPFS load is async - this is called during init
        // For sync interface, we return None and load separately
        Ok(None)
    }
}

/// Save data to OPFS
async fn save_to_opfs(data: &[u8]) -> Result<(), String> {
    let root = get_opfs_root().await?;

    let file_opts = web_sys::FileSystemGetFileOptions::new();
    file_opts.set_create(true);

    let file_handle: web_sys::FileSystemFileHandle =
        JsFuture::from(root.get_file_handle_with_options("axeberg_state.json", &file_opts))
            .await
            .map_err(|e| format!("Failed to get file handle: {:?}", e))?
            .dyn_into()
            .map_err(|_| "Failed to cast to FileSystemFileHandle")?;

    let writable: web_sys::FileSystemWritableFileStream =
        JsFuture::from(file_handle.create_writable())
            .await
            .map_err(|e| format!("Failed to create writable: {:?}", e))?
            .dyn_into()
            .map_err(|_| "Failed to cast to FileSystemWritableFileStream")?;

    let uint8_array = js_sys::Uint8Array::from(data);
    let write_promise = writable
        .write_with_buffer_source(&uint8_array)
        .map_err(|e| format!("Failed to get write promise: {:?}", e))?;
    JsFuture::from(write_promise)
        .await
        .map_err(|e| format!("Failed to write: {:?}", e))?;

    JsFuture::from(writable.close())
        .await
        .map_err(|e| format!("Failed to close: {:?}", e))?;

    Ok(())
}

/// Load data from OPFS
pub async fn load_from_opfs() -> Result<Option<Vec<u8>>, String> {
    let root = match get_opfs_root().await {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };

    let file_opts = web_sys::FileSystemGetFileOptions::new();
    file_opts.set_create(false);

    let file_handle: web_sys::FileSystemFileHandle =
        match JsFuture::from(root.get_file_handle_with_options("axeberg_state.json", &file_opts))
            .await
        {
            Ok(h) => h
                .dyn_into()
                .map_err(|_| "Failed to cast to FileSystemFileHandle")?,
            Err(_) => return Ok(None),
        };

    let file: web_sys::File = JsFuture::from(file_handle.get_file())
        .await
        .map_err(|e| format!("Failed to get file: {:?}", e))?
        .dyn_into()
        .map_err(|_| "Failed to cast to File")?;

    let array_buffer = JsFuture::from(file.array_buffer())
        .await
        .map_err(|e| format!("Failed to read file: {:?}", e))?;

    let uint8_array = js_sys::Uint8Array::new(&array_buffer);
    Ok(Some(uint8_array.to_vec()))
}

/// Get OPFS root directory handle
async fn get_opfs_root() -> Result<web_sys::FileSystemDirectoryHandle, String> {
    let window = web_sys::window().ok_or_else(|| "No window object".to_string())?;
    let navigator = window.navigator();
    let storage = navigator.storage();

    JsFuture::from(storage.get_directory())
        .await
        .map_err(|e| format!("Failed to get OPFS root: {:?}", e))?
        .dyn_into()
        .map_err(|_| "Failed to cast to FileSystemDirectoryHandle".to_string())
}
