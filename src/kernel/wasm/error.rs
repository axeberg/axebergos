//! Error types for the WASM loader
//!
//! Provides detailed error information for debugging and user feedback.

use std::fmt;

/// Result type for WASM loader operations
pub type WasmResult<T> = Result<T, WasmError>;

/// Errors that can occur during WASM module loading and execution
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasmError {
    /// Module binary is malformed or invalid
    InvalidModule {
        reason: String,
    },

    /// Required export is missing
    MissingExport {
        name: &'static str,
    },

    /// Export has wrong type
    WrongExportType {
        name: &'static str,
        expected: &'static str,
        got: String,
    },

    /// Memory access out of bounds
    MemoryAccessOutOfBounds {
        address: u32,
        size: u32,
        memory_size: u32,
    },

    /// Invalid file descriptor
    InvalidFd {
        fd: i32,
    },

    /// Syscall error (wraps ABI error code)
    Syscall {
        name: &'static str,
        code: i32,
    },

    /// Command exited with non-zero code
    NonZeroExit {
        code: i32,
    },

    /// Command was aborted (e.g., via trap)
    Aborted {
        reason: String,
    },

    /// Module instantiation failed
    InstantiationFailed {
        reason: String,
    },

    /// Command not found in filesystem
    CommandNotFound {
        name: String,
    },

    /// I/O error reading module
    IoError {
        message: String,
    },

    /// Memory allocation failed
    OutOfMemory {
        requested: u32,
        available: u32,
    },

    /// Maximum open files exceeded
    TooManyOpenFiles {
        max: usize,
    },
}

impl fmt::Display for WasmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidModule { reason } => {
                write!(f, "invalid WASM module: {}", reason)
            }
            Self::MissingExport { name } => {
                write!(f, "missing required export: '{}'", name)
            }
            Self::WrongExportType {
                name,
                expected,
                got,
            } => {
                write!(
                    f,
                    "export '{}' has wrong type: expected {}, got {}",
                    name, expected, got
                )
            }
            Self::MemoryAccessOutOfBounds {
                address,
                size,
                memory_size,
            } => {
                write!(
                    f,
                    "memory access out of bounds: address {} + size {} > memory size {}",
                    address, size, memory_size
                )
            }
            Self::InvalidFd { fd } => {
                write!(f, "invalid file descriptor: {}", fd)
            }
            Self::Syscall { name, code } => {
                write!(f, "syscall '{}' failed with code {}", name, code)
            }
            Self::NonZeroExit { code } => {
                write!(f, "command exited with code {}", code)
            }
            Self::Aborted { reason } => {
                write!(f, "command aborted: {}", reason)
            }
            Self::InstantiationFailed { reason } => {
                write!(f, "module instantiation failed: {}", reason)
            }
            Self::CommandNotFound { name } => {
                write!(f, "command not found: {}", name)
            }
            Self::IoError { message } => {
                write!(f, "I/O error: {}", message)
            }
            Self::OutOfMemory {
                requested,
                available,
            } => {
                write!(
                    f,
                    "out of memory: requested {} bytes, only {} available",
                    requested, available
                )
            }
            Self::TooManyOpenFiles { max } => {
                write!(f, "too many open files (max {})", max)
            }
        }
    }
}

impl std::error::Error for WasmError {}

/// Command execution result
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// Exit code (0 = success)
    pub exit_code: i32,
    /// Stdout output
    pub stdout: Vec<u8>,
    /// Stderr output
    pub stderr: Vec<u8>,
}

impl CommandResult {
    /// Create a successful result
    pub fn success() -> Self {
        Self {
            exit_code: 0,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    /// Create a result with given exit code
    pub fn with_code(code: i32) -> Self {
        Self {
            exit_code: code,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    /// Check if command succeeded
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }

    /// Get stdout as string (lossy UTF-8)
    pub fn stdout_str(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    /// Get stderr as string (lossy UTF-8)
    pub fn stderr_str(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = WasmError::CommandNotFound {
            name: "foo".to_string(),
        };
        assert_eq!(err.to_string(), "command not found: foo");

        let err = WasmError::MissingExport { name: "main" };
        assert_eq!(err.to_string(), "missing required export: 'main'");

        let err = WasmError::MemoryAccessOutOfBounds {
            address: 1000,
            size: 100,
            memory_size: 1024,
        };
        assert!(err.to_string().contains("1000"));
        assert!(err.to_string().contains("100"));
        assert!(err.to_string().contains("1024"));
    }

    #[test]
    fn test_command_result() {
        let result = CommandResult::success();
        assert!(result.is_success());
        assert_eq!(result.exit_code, 0);

        let result = CommandResult::with_code(1);
        assert!(!result.is_success());
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn test_command_result_output() {
        let mut result = CommandResult::success();
        result.stdout = b"hello world\n".to_vec();
        assert_eq!(result.stdout_str(), "hello world\n");
    }
}
