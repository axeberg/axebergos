//! Platform Abstraction Layer
//!
//! This module defines traits that abstract platform-specific functionality,
//! allowing axeberg to run on multiple platforms:
//!
//! - Browser (via wasm-bindgen, web-sys)
//! - WASI CLI (via wasmtime, wasmer)
//! - Bare metal (future, via UEFI)
//!
//! The kernel and shell are platform-agnostic. Only the Platform implementation
//! knows about the host environment.

#[cfg(target_arch = "wasm32")]
#[cfg(target_os = "unknown")] // Browser WASM (no WASI)
pub mod web;

#[cfg(target_arch = "wasm32")]
#[cfg(target_os = "wasi")]
pub mod wasi;

/// Result type for platform operations
pub type PlatformResult<T> = Result<T, PlatformError>;

/// Platform-specific errors
#[derive(Debug, Clone)]
pub enum PlatformError {
    /// I/O error
    Io(String),
    /// Feature not supported on this platform
    NotSupported(String),
    /// Initialization failed
    InitFailed(String),
}

impl std::fmt::Display for PlatformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlatformError::Io(s) => write!(f, "I/O error: {}", s),
            PlatformError::NotSupported(s) => write!(f, "Not supported: {}", s),
            PlatformError::InitFailed(s) => write!(f, "Init failed: {}", s),
        }
    }
}

impl std::error::Error for PlatformError {}

/// Key event from input
#[derive(Debug, Clone)]
pub struct KeyEvent {
    /// The key value (e.g., "a", "Enter", "Backspace")
    pub key: String,
    /// The key code (e.g., "KeyA", "Enter", "Backspace")
    pub code: String,
    /// Modifier keys
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

/// Terminal dimensions
#[derive(Debug, Clone, Copy)]
pub struct TermSize {
    pub cols: u32,
    pub rows: u32,
}

/// Platform abstraction trait
///
/// Each platform implements this trait to provide:
/// - Terminal I/O (display text, read input)
/// - Timing (for delays, scheduling)
/// - Persistence (save/load state)
pub trait Platform {
    // ===== Terminal Output =====

    /// Write text to the terminal
    fn write(&mut self, text: &str);

    /// Clear the terminal screen
    fn clear(&mut self);

    /// Get terminal dimensions
    fn term_size(&self) -> TermSize;

    // ===== Input =====

    /// Poll for a key event (non-blocking)
    fn poll_key(&mut self) -> Option<KeyEvent>;

    // ===== Timing =====

    /// Get current time in milliseconds since some epoch
    fn now_ms(&self) -> f64;

    // ===== Persistence =====

    /// Save state to persistent storage
    fn save_state(&mut self, data: &[u8]) -> PlatformResult<()>;

    /// Load state from persistent storage
    fn load_state(&mut self) -> PlatformResult<Option<Vec<u8>>>;

    // ===== Lifecycle =====

    /// Called each frame/tick of the main loop
    fn tick(&mut self) {}

    /// Check if the platform wants to exit
    fn should_exit(&self) -> bool {
        false
    }
}

/// Trait for platforms that need async initialization
pub trait AsyncPlatform: Platform {
    /// Initialize the platform asynchronously
    fn init(&mut self) -> impl std::future::Future<Output = PlatformResult<()>>;
}
