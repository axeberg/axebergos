//! WASM Command Runtime Environment
//!
//! Provides the syscall implementations that WASM commands can call.
//! This is the bridge between WASM modules and the axeberg kernel.

use super::abi::{OpenFlags, StatBuf, SyscallError, fd};
use super::loader::FdTable;
use crate::kernel::syscall as ksyscall;
use std::collections::HashMap;

/// Runtime environment for executing WASM commands
///
/// Each command execution gets a fresh Runtime instance, providing:
/// - Standard streams (stdin, stdout, stderr)
/// - File descriptor table
/// - Environment variables
/// - Current working directory
pub struct Runtime {
    /// File descriptor table
    fd_table: FdTable,

    /// Captured stdout
    stdout: Vec<u8>,

    /// Captured stderr
    stderr: Vec<u8>,

    /// Stdin data (to be read) - public for executor access
    pub stdin: Vec<u8>,

    /// Current read position in stdin
    stdin_pos: usize,

    /// Environment variables
    env: HashMap<String, String>,

    /// Current working directory
    cwd: String,

    /// Exit code (if exited)
    exit_code: Option<i32>,
}

impl Runtime {
    /// Create a new runtime with default settings
    pub fn new() -> Self {
        Self {
            fd_table: FdTable::new(),
            stdout: Vec::new(),
            stderr: Vec::new(),
            stdin: Vec::new(),
            stdin_pos: 0,
            env: HashMap::new(),
            cwd: "/".to_string(),
            exit_code: None,
        }
    }

    /// Create a runtime with stdin data
    pub fn with_stdin(stdin: Vec<u8>) -> Self {
        Self {
            stdin,
            ..Self::new()
        }
    }

    /// Create a runtime with a specific working directory
    pub fn with_cwd(cwd: &str) -> Self {
        Self {
            cwd: cwd.to_string(),
            ..Self::new()
        }
    }

    /// Get current working directory
    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// Set current working directory
    pub fn set_cwd(&mut self, cwd: &str) {
        self.cwd = cwd.to_string();
    }

    /// Get environment variable
    pub fn get_env(&self, name: &str) -> Option<String> {
        self.env.get(name).cloned()
    }

    /// Set environment variable
    pub fn set_env(&mut self, name: &str, value: &str) {
        self.env.insert(name.to_string(), value.to_string());
    }

    /// Check if command has exited
    pub fn has_exited(&self) -> bool {
        self.exit_code.is_some()
    }

    /// Get exit code (if exited)
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    /// Exit the command
    pub fn exit(&mut self, code: i32) {
        self.exit_code = Some(code);
    }

    /// Write to stdout
    pub fn write_stdout(&mut self, data: &[u8]) {
        self.stdout.extend_from_slice(data);
    }

    /// Write to stderr
    pub fn write_stderr(&mut self, data: &[u8]) {
        self.stderr.extend_from_slice(data);
    }

    /// Read from stdin
    pub fn read_stdin(&mut self, buf: &mut [u8]) -> usize {
        let remaining = self.stdin.len() - self.stdin_pos;
        let to_read = std::cmp::min(remaining, buf.len());

        if to_read > 0 {
            buf[..to_read].copy_from_slice(&self.stdin[self.stdin_pos..self.stdin_pos + to_read]);
            self.stdin_pos += to_read;
        }

        to_read
    }

    /// Get captured stdout
    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }

    /// Get captured stderr
    pub fn stderr(&self) -> &[u8] {
        &self.stderr
    }

    /// Take ownership of stdout (clears internal buffer)
    pub fn take_stdout(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.stdout)
    }

    /// Take ownership of stderr (clears internal buffer)
    pub fn take_stderr(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.stderr)
    }

    // =========================================================================
    // Syscall implementations
    // =========================================================================

    /// Write syscall: write(fd, buf, len) -> bytes_written
    pub fn sys_write(&mut self, fd_num: i32, data: &[u8]) -> i32 {
        match fd_num {
            fd if fd == fd::STDOUT => {
                self.write_stdout(data);
                data.len() as i32
            }
            fd if fd == fd::STDERR => {
                self.write_stderr(data);
                data.len() as i32
            }
            fd if fd == fd::STDIN => SyscallError::InvalidArgument.code(),
            fd => {
                if !self.fd_table.is_valid(fd) {
                    return SyscallError::BadFd.code();
                }

                // Get file path and write via VFS
                if let Some(path) = self.fd_table.get_path(fd) {
                    // For simplicity, we append to file (proper seek support would need more state)
                    let content = String::from_utf8_lossy(data);
                    match ksyscall::write_file(&path, &content) {
                        Ok(()) => data.len() as i32,
                        Err(_) => SyscallError::Generic.code(),
                    }
                } else {
                    SyscallError::BadFd.code()
                }
            }
        }
    }

    /// Read syscall: read(fd, buf, len) -> bytes_read
    pub fn sys_read(&mut self, fd_num: i32, buf: &mut [u8]) -> i32 {
        match fd_num {
            fd if fd == fd::STDIN => self.read_stdin(buf) as i32,
            fd if fd == fd::STDOUT || fd == fd::STDERR => SyscallError::InvalidArgument.code(),
            fd => {
                if !self.fd_table.is_valid(fd) {
                    return SyscallError::BadFd.code();
                }

                // Get file path and read via VFS
                if let Some(path) = self.fd_table.get_path(fd) {
                    match ksyscall::read_file(&path) {
                        Ok(content) => {
                            let bytes = content.as_bytes();
                            let pos = self.fd_table.get_position(fd).unwrap_or(0) as usize;

                            if pos >= bytes.len() {
                                return 0; // EOF
                            }

                            let remaining = &bytes[pos..];
                            let to_read = std::cmp::min(remaining.len(), buf.len());
                            buf[..to_read].copy_from_slice(&remaining[..to_read]);

                            // Advance position
                            self.fd_table.advance_position(fd, to_read as u64);

                            to_read as i32
                        }
                        Err(_) => SyscallError::NotFound.code(),
                    }
                } else {
                    SyscallError::BadFd.code()
                }
            }
        }
    }

    /// Open syscall: open(path, flags) -> fd
    pub fn sys_open(&mut self, path: &str, flags: OpenFlags) -> i32 {
        match self.fd_table.allocate(path, flags) {
            Ok(fd) => fd,
            Err(_) => SyscallError::Generic.code(),
        }
    }

    /// Close syscall: close(fd) -> 0 or error
    pub fn sys_close(&mut self, fd: i32) -> i32 {
        match self.fd_table.close(fd) {
            Ok(()) => 0,
            Err(_) => SyscallError::BadFd.code(),
        }
    }

    /// Exit syscall: exit(code) -> never returns
    pub fn sys_exit(&mut self, code: i32) {
        self.exit(code);
    }

    /// Getenv syscall: getenv(name) -> value or None
    pub fn sys_getenv(&self, name: &str) -> Option<&str> {
        self.env.get(name).map(|s| s.as_str())
    }

    /// Getcwd syscall: getcwd() -> path
    pub fn sys_getcwd(&self) -> &str {
        &self.cwd
    }

    /// Stat syscall
    pub fn sys_stat(&self, path: &str) -> Result<StatBuf, SyscallError> {
        match ksyscall::metadata(path) {
            Ok(meta) => Ok(StatBuf {
                size: meta.size as u32,
                is_dir: if meta.is_dir { 1 } else { 0 },
                modified_time: 0, // VFS doesn't track times yet
                created_time: 0,
                reserved: 0,
            }),
            Err(_) => Err(SyscallError::NotFound),
        }
    }

    /// Mkdir syscall: mkdir(path) -> 0 or error
    pub fn sys_mkdir(&self, path: &str) -> i32 {
        // Resolve path relative to cwd if needed
        let full_path = self.resolve_path(path);
        match ksyscall::mkdir(&full_path) {
            Ok(()) => 0,
            Err(_) => SyscallError::Generic.code(),
        }
    }

    /// Readdir syscall: readdir(path) -> entries or error
    ///
    /// Returns entries as null-terminated strings concatenated together.
    pub fn sys_readdir(&self, path: &str) -> Result<Vec<String>, SyscallError> {
        let full_path = self.resolve_path(path);
        match ksyscall::readdir(&full_path) {
            Ok(entries) => Ok(entries),
            Err(_) => Err(SyscallError::NotFound),
        }
    }

    /// Rmdir syscall: rmdir(path) -> 0 or error
    pub fn sys_rmdir(&self, path: &str) -> i32 {
        let full_path = self.resolve_path(path);
        match ksyscall::rmdir(&full_path) {
            Ok(()) => 0,
            Err(_) => SyscallError::Generic.code(),
        }
    }

    /// Unlink syscall: unlink(path) -> 0 or error
    pub fn sys_unlink(&self, path: &str) -> i32 {
        let full_path = self.resolve_path(path);
        match ksyscall::unlink(&full_path) {
            Ok(()) => 0,
            Err(_) => SyscallError::Generic.code(),
        }
    }

    /// Rename syscall: rename(from, to) -> 0 or error
    pub fn sys_rename(&self, from: &str, to: &str) -> i32 {
        let from_path = self.resolve_path(from);
        let to_path = self.resolve_path(to);
        match ksyscall::rename(&from_path, &to_path) {
            Ok(()) => 0,
            Err(_) => SyscallError::Generic.code(),
        }
    }

    /// Seek syscall: seek(fd, offset, whence) -> position or error
    ///
    /// whence: 0 = SET, 1 = CUR, 2 = END
    pub fn sys_seek(&mut self, fd_num: i32, offset: i64, whence: i32) -> i64 {
        if !self.fd_table.is_valid(fd_num) {
            return SyscallError::BadFd.code() as i64;
        }

        // Get current position and file size
        let current_pos = self.fd_table.get_position(fd_num).unwrap_or(0);
        let file_size = if let Some(path) = self.fd_table.get_path(fd_num) {
            match ksyscall::metadata(&path) {
                Ok(meta) => meta.size,
                Err(_) => return SyscallError::Generic.code() as i64,
            }
        } else {
            return SyscallError::BadFd.code() as i64;
        };

        // Calculate new position based on whence
        let new_pos = match whence {
            0 => offset,                      // SEEK_SET
            1 => current_pos as i64 + offset, // SEEK_CUR
            2 => file_size as i64 + offset,   // SEEK_END
            _ => return SyscallError::InvalidArgument.code() as i64,
        };

        if new_pos < 0 {
            return SyscallError::InvalidArgument.code() as i64;
        }

        // Update position
        self.fd_table.set_position(fd_num, new_pos as u64);
        new_pos
    }

    /// Dup syscall: dup(fd) -> new_fd or error
    pub fn sys_dup(&mut self, fd_num: i32) -> i32 {
        if !self.fd_table.is_valid(fd_num) {
            return SyscallError::BadFd.code();
        }

        if let Some(path) = self.fd_table.get_path(fd_num) {
            match self.fd_table.allocate(&path, OpenFlags::READ) {
                Ok(new_fd) => new_fd,
                Err(_) => SyscallError::Generic.code(),
            }
        } else {
            SyscallError::BadFd.code()
        }
    }

    /// Copy a file: copy(from, to) -> bytes_copied or error
    pub fn sys_copy(&self, from: &str, to: &str) -> i64 {
        let from_path = self.resolve_path(from);
        let to_path = self.resolve_path(to);

        // Read source file
        let content = match ksyscall::read_file(&from_path) {
            Ok(c) => c,
            Err(_) => return SyscallError::NotFound.code() as i64,
        };

        // Write to destination
        match ksyscall::write_file(&to_path, &content) {
            Ok(()) => content.len() as i64,
            Err(_) => SyscallError::Generic.code() as i64,
        }
    }

    /// Resolve a path relative to cwd
    fn resolve_path(&self, path: &str) -> String {
        if path.starts_with('/') {
            path.to_string()
        } else if self.cwd == "/" {
            format!("/{}", path)
        } else {
            format!("{}/{}", self.cwd, path)
        }
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for configuring a Runtime
pub struct RuntimeBuilder {
    stdin: Vec<u8>,
    env: HashMap<String, String>,
    cwd: String,
}

impl RuntimeBuilder {
    pub fn new() -> Self {
        Self {
            stdin: Vec::new(),
            env: HashMap::new(),
            cwd: "/".to_string(),
        }
    }

    /// Set stdin data
    pub fn stdin(mut self, data: Vec<u8>) -> Self {
        self.stdin = data;
        self
    }

    /// Set an environment variable
    pub fn env(mut self, name: &str, value: &str) -> Self {
        self.env.insert(name.to_string(), value.to_string());
        self
    }

    /// Set working directory
    pub fn cwd(mut self, path: &str) -> Self {
        self.cwd = path.to_string();
        self
    }

    /// Build the runtime
    pub fn build(self) -> Runtime {
        let mut runtime = Runtime::new();
        runtime.stdin = self.stdin;
        runtime.env = self.env;
        runtime.cwd = self.cwd;
        runtime
    }
}

impl Default for RuntimeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_builder() {
        let runtime = RuntimeBuilder::new()
            .stdin(b"hello".to_vec())
            .env("HOME", "/home/user")
            .cwd("/tmp")
            .build();

        assert_eq!(runtime.cwd(), "/tmp");
        assert_eq!(runtime.get_env("HOME"), Some("/home/user".to_string()));
    }

    #[test]
    fn test_sys_write_stdout() {
        let mut runtime = Runtime::new();
        let result = runtime.sys_write(fd::STDOUT, b"hello");
        assert_eq!(result, 5);
        assert_eq!(runtime.stdout(), b"hello");
    }

    #[test]
    fn test_sys_write_stderr() {
        let mut runtime = Runtime::new();
        let result = runtime.sys_write(fd::STDERR, b"error");
        assert_eq!(result, 5);
        assert_eq!(runtime.stderr(), b"error");
    }

    #[test]
    fn test_sys_read_stdin() {
        let mut runtime = Runtime::with_stdin(b"hello world".to_vec());
        let mut buf = [0u8; 5];
        let result = runtime.sys_read(fd::STDIN, &mut buf);
        assert_eq!(result, 5);
        assert_eq!(&buf, b"hello");
    }

    #[test]
    fn test_sys_open_close() {
        let mut runtime = Runtime::new();
        let fd = runtime.sys_open("/tmp/file.txt", OpenFlags::READ);
        assert!(fd >= 3);

        let result = runtime.sys_close(fd);
        assert_eq!(result, 0);

        // Closing again should fail
        let result = runtime.sys_close(fd);
        assert!(result < 0);
    }

    #[test]
    fn test_sys_exit() {
        let mut runtime = Runtime::new();
        assert!(!runtime.has_exited());

        runtime.sys_exit(42);

        assert!(runtime.has_exited());
        assert_eq!(runtime.exit_code(), Some(42));
    }
}
