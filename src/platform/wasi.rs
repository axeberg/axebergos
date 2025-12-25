//! WASI Platform Implementation
//!
//! Provides platform support for running in WASI runtimes (wasmtime, wasmer):
//! - stdin/stdout for terminal I/O
//! - Real filesystem for persistence (via --dir mapping)
//! - WASI clocks for timing

use super::{KeyEvent, Platform, PlatformError, PlatformResult, TermSize};
use std::io::{self, BufRead, Write};

/// State file path (relative to mapped directory)
const STATE_FILE: &str = ".axeberg/state.json";

/// WASI platform state
pub struct WasiPlatform {
    /// Terminal dimensions
    term_size: TermSize,
    /// Stdin reader (line-buffered for now)
    stdin_buffer: String,
    /// Should we exit?
    exit_requested: bool,
}

impl WasiPlatform {
    pub fn new() -> Self {
        // Try to get terminal size from environment or use defaults
        let cols = std::env::var("COLUMNS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(80);
        let rows = std::env::var("LINES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(24);

        Self {
            term_size: TermSize { cols, rows },
            stdin_buffer: String::new(),
            exit_requested: false,
        }
    }

    /// Request exit
    pub fn request_exit(&mut self) {
        self.exit_requested = true;
    }
}

impl Default for WasiPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl Platform for WasiPlatform {
    fn write(&mut self, text: &str) {
        // Write directly to stdout
        let _ = io::stdout().write_all(text.as_bytes());
        let _ = io::stdout().flush();
    }

    fn clear(&mut self) {
        // ANSI escape sequence to clear screen
        self.write("\x1b[2J\x1b[H");
    }

    fn term_size(&self) -> TermSize {
        self.term_size
    }

    fn poll_key(&mut self) -> Option<KeyEvent> {
        // WASI doesn't have non-blocking stdin yet
        // For now, we do line-buffered input
        // This is a limitation - true character-by-character input
        // would require raw terminal mode which WASI doesn't support well

        // Check if we have buffered input
        if !self.stdin_buffer.is_empty() {
            let c = self.stdin_buffer.remove(0);
            return Some(KeyEvent {
                key: c.to_string(),
                code: format!("Key{}", c.to_ascii_uppercase()),
                ctrl: false,
                alt: false,
                shift: c.is_ascii_uppercase(),
                meta: false,
            });
        }

        // Try to read a line (this blocks in WASI)
        // In a real implementation, we'd need async or non-blocking I/O
        None
    }

    fn now_ms(&self) -> f64 {
        // Use WASI clocks
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as f64)
            .unwrap_or(0.0)
    }

    fn save_state(&mut self, data: &[u8]) -> PlatformResult<()> {
        // Ensure directory exists
        let state_dir = std::path::Path::new(STATE_FILE).parent().unwrap();
        if !state_dir.exists() {
            std::fs::create_dir_all(state_dir)
                .map_err(|e| PlatformError::Io(format!("Failed to create dir: {}", e)))?;
        }

        // Write state file
        std::fs::write(STATE_FILE, data)
            .map_err(|e| PlatformError::Io(format!("Failed to write state: {}", e)))?;

        Ok(())
    }

    fn load_state(&mut self) -> PlatformResult<Option<Vec<u8>>> {
        match std::fs::read(STATE_FILE) {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(PlatformError::Io(format!("Failed to read state: {}", e))),
        }
    }

    fn should_exit(&self) -> bool {
        self.exit_requested
    }
}

/// Run the WASI main loop
///
/// This is a simple REPL since WASI doesn't have good async/non-blocking I/O
pub fn run_repl<F>(mut process_line: F) -> !
where
    F: FnMut(&str) -> bool, // Returns true to continue, false to exit
{
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        // Print prompt
        let _ = write!(stdout, "$ ");
        let _ = stdout.flush();

        // Read line
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {
                let line = line.trim();
                if !process_line(line) {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    std::process::exit(0);
}
