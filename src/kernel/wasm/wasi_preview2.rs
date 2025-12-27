//! WASI Preview2 Implementation
//!
//! WASI Preview2 (aka WASI 0.2) is the Component Model-based version of WASI.
//! It introduces typed interfaces, resources, and a more structured approach
//! to system APIs.
//!
//! # Core Interfaces
//!
//! - `wasi:io/streams`: Input/output streams with async support
//! - `wasi:io/poll`: Pollable resources for async I/O
//! - `wasi:clocks/wall-clock`: Wall clock time
//! - `wasi:clocks/monotonic-clock`: Monotonic time for measurements
//! - `wasi:random`: Secure random number generation
//! - `wasi:filesystem`: File system access
//! - `wasi:cli`: Command-line interface (args, env, exit)
//!
//! # Resource Handles
//!
//! WASI Preview2 uses resource handles instead of raw file descriptors.
//! Resources are opaque handles with associated methods.

use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// ============================================================================
// Type Definitions (based on WIT types)
// ============================================================================

/// A unique identifier for a stream resource
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StreamId(pub u32);

/// A unique identifier for a pollable resource
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PollableId(pub u32);

/// A unique identifier for a descriptor (file/directory) resource
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DescriptorId(pub u32);

/// Wall clock timestamp
#[derive(Debug, Clone, Copy, Default)]
pub struct Datetime {
    /// Seconds since Unix epoch
    pub seconds: u64,
    /// Nanoseconds within the second
    pub nanoseconds: u32,
}

impl Datetime {
    pub fn now() -> Self {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => Self {
                seconds: d.as_secs(),
                nanoseconds: d.subsec_nanos(),
            },
            Err(_) => Self::default(),
        }
    }
}

/// Duration type for monotonic clock
#[derive(Debug, Clone, Copy, Default)]
pub struct MonotonicDuration(pub u64); // nanoseconds

/// Instant type for monotonic clock
#[derive(Debug, Clone, Copy)]
pub struct MonotonicInstant(pub u64); // nanoseconds since some arbitrary point

/// Error codes for I/O operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamError {
    /// Stream has reached end of file
    Closed,
    /// An I/O error occurred
    LastOperationFailed,
}

/// Error codes for filesystem operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesystemError {
    /// Access denied
    Access,
    /// Already exists
    Exist,
    /// No such file or directory
    NoEntry,
    /// Not a directory
    NotDirectory,
    /// Is a directory
    IsDirectory,
    /// Invalid argument
    Invalid,
    /// I/O error
    Io,
    /// Not permitted
    NotPermitted,
    /// Read-only file system
    ReadOnly,
    /// Directory not empty
    NotEmpty,
}

impl FilesystemError {
    pub fn code(&self) -> i32 {
        match self {
            FilesystemError::Access => -3,
            FilesystemError::Exist => -4,
            FilesystemError::NoEntry => -2,
            FilesystemError::NotDirectory => -5,
            FilesystemError::IsDirectory => -6,
            FilesystemError::Invalid => -7,
            FilesystemError::Io => -9,
            FilesystemError::NotPermitted => -3,
            FilesystemError::ReadOnly => -8,
            FilesystemError::NotEmpty => -11,
        }
    }
}

/// File type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptorType {
    /// Unknown file type
    Unknown,
    /// Block device
    BlockDevice,
    /// Character device
    CharacterDevice,
    /// Directory
    Directory,
    /// FIFO/pipe
    Fifo,
    /// Symbolic link
    SymbolicLink,
    /// Regular file
    RegularFile,
    /// Socket
    Socket,
}

/// File/directory metadata
#[derive(Debug, Clone, Default)]
pub struct DescriptorStat {
    /// Type of the file
    pub type_: DescriptorType,
    /// Number of hard links
    pub link_count: u64,
    /// Size in bytes
    pub size: u64,
    /// Last access time
    pub access_time: Option<Datetime>,
    /// Last modification time
    pub modification_time: Option<Datetime>,
    /// Status change time
    pub status_change_time: Option<Datetime>,
}

impl Default for DescriptorType {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Directory entry
#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    /// Name of the entry
    pub name: String,
    /// Type of the entry
    pub type_: DescriptorType,
}

/// Open flags for opening files (WASI Preview2)
#[derive(Debug, Clone, Copy, Default)]
pub struct WasiOpenFlags {
    /// Create file if it doesn't exist
    pub create: bool,
    /// Fail if file exists (with create)
    pub exclusive: bool,
    /// Truncate file to zero length
    pub truncate: bool,
    /// Open in append mode
    pub append: bool,
}

/// Descriptor flags (access mode) for WASI Preview2
#[derive(Debug, Clone, Copy, Default)]
pub struct WasiDescriptorFlags {
    /// Read access
    pub read: bool,
    /// Write access
    pub write: bool,
    /// Data sync on write
    pub sync: bool,
}

// ============================================================================
// wasi:io/streams Interface
// ============================================================================

/// An input stream for reading bytes
pub struct InputStream {
    id: StreamId,
    /// Data buffer for the stream
    buffer: Vec<u8>,
    /// Current read position
    position: usize,
    /// Whether the stream is closed
    closed: bool,
}

impl InputStream {
    pub fn new(id: StreamId) -> Self {
        Self {
            id,
            buffer: Vec::new(),
            position: 0,
            closed: false,
        }
    }

    pub fn with_data(id: StreamId, data: Vec<u8>) -> Self {
        Self {
            id,
            buffer: data,
            position: 0,
            closed: false,
        }
    }

    pub fn id(&self) -> StreamId {
        self.id
    }

    /// Read up to `len` bytes
    pub fn read(&mut self, len: usize) -> Result<Vec<u8>, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let remaining = self.buffer.len().saturating_sub(self.position);
        if remaining == 0 {
            return Err(StreamError::Closed);
        }

        let to_read = len.min(remaining);
        let data = self.buffer[self.position..self.position + to_read].to_vec();
        self.position += to_read;
        Ok(data)
    }

    /// Block until data is available
    pub fn blocking_read(&mut self, len: usize) -> Result<Vec<u8>, StreamError> {
        self.read(len)
    }

    /// Skip bytes
    pub fn skip(&mut self, len: usize) -> Result<usize, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let remaining = self.buffer.len().saturating_sub(self.position);
        let to_skip = len.min(remaining);
        self.position += to_skip;
        Ok(to_skip)
    }

    /// Get a pollable for this stream
    pub fn subscribe(&self) -> PollableId {
        PollableId(self.id.0)
    }
}

/// An output stream for writing bytes
pub struct OutputStream {
    id: StreamId,
    /// Data buffer for the stream
    buffer: Vec<u8>,
    /// Whether the stream is closed
    closed: bool,
}

impl OutputStream {
    pub fn new(id: StreamId) -> Self {
        Self {
            id,
            buffer: Vec::new(),
            closed: false,
        }
    }

    pub fn id(&self) -> StreamId {
        self.id
    }

    /// Check how many bytes can be written
    pub fn check_write(&self) -> Result<usize, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }
        // Unlimited buffering for now
        Ok(usize::MAX)
    }

    /// Write bytes to the stream
    pub fn write(&mut self, data: &[u8]) -> Result<usize, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }
        self.buffer.extend_from_slice(data);
        Ok(data.len())
    }

    /// Flush the stream
    pub fn flush(&mut self) -> Result<(), StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }
        Ok(())
    }

    /// Block until write completes
    pub fn blocking_write_and_flush(&mut self, data: &[u8]) -> Result<usize, StreamError> {
        let written = self.write(data)?;
        self.flush()?;
        Ok(written)
    }

    /// Get a pollable for this stream
    pub fn subscribe(&self) -> PollableId {
        PollableId(self.id.0)
    }

    /// Take the written data
    pub fn take_buffer(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.buffer)
    }

    /// Get the written data
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }
}

// ============================================================================
// wasi:io/poll Interface
// ============================================================================

/// A pollable resource that can be waited on
pub struct Pollable {
    id: PollableId,
    ready: bool,
}

impl Pollable {
    pub fn new(id: PollableId) -> Self {
        Self { id, ready: false }
    }

    pub fn id(&self) -> PollableId {
        self.id
    }

    /// Check if ready without blocking
    pub fn ready(&self) -> bool {
        self.ready
    }

    /// Block until ready
    pub fn block(&mut self) {
        self.ready = true;
    }
}

/// Poll multiple pollables
pub fn poll(pollables: &[PollableId]) -> Vec<u32> {
    // For now, all pollables are immediately ready
    (0..pollables.len() as u32).collect()
}

// ============================================================================
// wasi:clocks/wall-clock Interface
// ============================================================================

/// Get the current wall clock time
pub fn wall_clock_now() -> Datetime {
    Datetime::now()
}

/// Get the resolution of the wall clock
pub fn wall_clock_resolution() -> Datetime {
    Datetime {
        seconds: 0,
        nanoseconds: 1_000_000, // 1ms resolution
    }
}

// ============================================================================
// wasi:clocks/monotonic-clock Interface
// ============================================================================

/// Monotonic clock state (per-instance)
pub struct MonotonicClock {
    start: Instant,
}

impl MonotonicClock {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    /// Get the current monotonic time in nanoseconds
    pub fn now(&self) -> MonotonicInstant {
        let elapsed = self.start.elapsed();
        MonotonicInstant(elapsed.as_nanos() as u64)
    }

    /// Get the resolution of the monotonic clock
    pub fn resolution(&self) -> MonotonicDuration {
        MonotonicDuration(1_000_000) // 1ms resolution
    }

    /// Subscribe to a timer
    pub fn subscribe_instant(&self, when: MonotonicInstant) -> PollableId {
        // Generate a unique pollable ID
        PollableId(when.0 as u32)
    }

    /// Subscribe to a duration from now
    pub fn subscribe_duration(&self, duration: MonotonicDuration) -> PollableId {
        let now = self.now();
        let when = MonotonicInstant(now.0 + duration.0);
        self.subscribe_instant(when)
    }
}

impl Default for MonotonicClock {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// wasi:random Interface
// ============================================================================

/// Get random bytes
pub fn get_random_bytes(len: usize) -> Vec<u8> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Simple PRNG for demonstration (not cryptographically secure)
    // In production, use a proper RNG
    let mut result = Vec::with_capacity(len);
    let mut hasher = DefaultHasher::new();

    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    seed.hash(&mut hasher);

    for i in 0..len {
        (i as u64).hash(&mut hasher);
        result.push(hasher.finish() as u8);
    }

    result
}

/// Get a random u64
pub fn get_random_u64() -> u64 {
    let bytes = get_random_bytes(8);
    u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ])
}

/// Get insecure random bytes (for non-security purposes)
pub fn get_insecure_random_bytes(len: usize) -> Vec<u8> {
    get_random_bytes(len)
}

/// Get an insecure random u64
pub fn get_insecure_random_u64() -> u64 {
    get_random_u64()
}

// ============================================================================
// wasi:cli Interface
// ============================================================================

/// CLI state for a WASI command
pub struct CliState {
    /// Command-line arguments
    args: Vec<String>,
    /// Environment variables
    env: HashMap<String, String>,
    /// Exit code (if exited)
    exit_code: Option<i32>,
    /// Stdin stream
    stdin: InputStream,
    /// Stdout stream
    stdout: OutputStream,
    /// Stderr stream
    stderr: OutputStream,
}

impl CliState {
    pub fn new() -> Self {
        Self {
            args: Vec::new(),
            env: HashMap::new(),
            exit_code: None,
            stdin: InputStream::new(StreamId(0)),
            stdout: OutputStream::new(StreamId(1)),
            stderr: OutputStream::new(StreamId(2)),
        }
    }

    /// Set command-line arguments
    pub fn set_args(&mut self, args: Vec<String>) {
        self.args = args;
    }

    /// Get command-line arguments
    pub fn get_args(&self) -> &[String] {
        &self.args
    }

    /// Set environment variable
    pub fn set_env(&mut self, key: &str, value: &str) {
        self.env.insert(key.to_string(), value.to_string());
    }

    /// Get environment variable
    pub fn get_env(&self, key: &str) -> Option<&str> {
        self.env.get(key).map(|s| s.as_str())
    }

    /// Get all environment variables
    pub fn get_all_env(&self) -> impl Iterator<Item = (&str, &str)> {
        self.env.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Exit with code
    pub fn exit(&mut self, code: i32) {
        self.exit_code = Some(code);
    }

    /// Check if exited
    pub fn has_exited(&self) -> bool {
        self.exit_code.is_some()
    }

    /// Get exit code
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    /// Get stdin stream
    pub fn stdin(&mut self) -> &mut InputStream {
        &mut self.stdin
    }

    /// Get stdout stream
    pub fn stdout(&mut self) -> &mut OutputStream {
        &mut self.stdout
    }

    /// Get stderr stream
    pub fn stderr(&mut self) -> &mut OutputStream {
        &mut self.stderr
    }

    /// Set stdin data
    pub fn set_stdin_data(&mut self, data: Vec<u8>) {
        self.stdin = InputStream::with_data(StreamId(0), data);
    }

    /// Take stdout data
    pub fn take_stdout(&mut self) -> Vec<u8> {
        self.stdout.take_buffer()
    }

    /// Take stderr data
    pub fn take_stderr(&mut self) -> Vec<u8> {
        self.stderr.take_buffer()
    }
}

impl Default for CliState {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// wasi:filesystem Interface
// ============================================================================

use crate::kernel::syscall as ksyscall;

/// Filesystem descriptor (file or directory)
pub struct Descriptor {
    id: DescriptorId,
    path: String,
    flags: WasiDescriptorFlags,
    position: u64,
}

impl Descriptor {
    pub fn new(id: DescriptorId, path: &str, flags: WasiDescriptorFlags) -> Self {
        Self {
            id,
            path: path.to_string(),
            flags,
            position: 0,
        }
    }

    pub fn id(&self) -> DescriptorId {
        self.id
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    /// Read directory entries
    pub fn read_directory(&self) -> Result<Vec<DirectoryEntry>, FilesystemError> {
        match ksyscall::readdir(&self.path) {
            Ok(entries) => Ok(entries
                .into_iter()
                .map(|name| {
                    // Determine type by checking if it's a directory
                    let full_path = if self.path == "/" {
                        format!("/{}", name)
                    } else {
                        format!("{}/{}", self.path, name)
                    };
                    let type_ = match ksyscall::metadata(&full_path) {
                        Ok(meta) => {
                            if meta.is_dir {
                                DescriptorType::Directory
                            } else {
                                DescriptorType::RegularFile
                            }
                        }
                        Err(_) => DescriptorType::Unknown,
                    };
                    DirectoryEntry { name, type_ }
                })
                .collect()),
            Err(_) => Err(FilesystemError::NoEntry),
        }
    }

    /// Get metadata
    pub fn stat(&self) -> Result<DescriptorStat, FilesystemError> {
        match ksyscall::metadata(&self.path) {
            Ok(meta) => Ok(DescriptorStat {
                type_: if meta.is_dir {
                    DescriptorType::Directory
                } else {
                    DescriptorType::RegularFile
                },
                link_count: 1,
                size: meta.size,
                access_time: None,
                modification_time: None,
                status_change_time: None,
            }),
            Err(_) => Err(FilesystemError::NoEntry),
        }
    }

    /// Get metadata for a child path
    pub fn stat_at(&self, path: &str) -> Result<DescriptorStat, FilesystemError> {
        let full_path = if self.path == "/" {
            format!("/{}", path)
        } else {
            format!("{}/{}", self.path, path)
        };

        match ksyscall::metadata(&full_path) {
            Ok(meta) => Ok(DescriptorStat {
                type_: if meta.is_dir {
                    DescriptorType::Directory
                } else {
                    DescriptorType::RegularFile
                },
                link_count: 1,
                size: meta.size,
                access_time: None,
                modification_time: None,
                status_change_time: None,
            }),
            Err(_) => Err(FilesystemError::NoEntry),
        }
    }

    /// Read bytes from a file
    pub fn read(&mut self, len: usize) -> Result<(Vec<u8>, bool), FilesystemError> {
        if !self.flags.read {
            return Err(FilesystemError::Access);
        }

        match ksyscall::read_file(&self.path) {
            Ok(content) => {
                let bytes = content.as_bytes();
                let pos = self.position as usize;

                if pos >= bytes.len() {
                    return Ok((Vec::new(), true)); // EOF
                }

                let remaining = &bytes[pos..];
                let to_read = len.min(remaining.len());
                let data = remaining[..to_read].to_vec();
                self.position += to_read as u64;

                let eof = self.position as usize >= bytes.len();
                Ok((data, eof))
            }
            Err(_) => Err(FilesystemError::Io),
        }
    }

    /// Write bytes to a file
    pub fn write(&mut self, data: &[u8]) -> Result<usize, FilesystemError> {
        if !self.flags.write {
            return Err(FilesystemError::Access);
        }

        let content = String::from_utf8_lossy(data);
        match ksyscall::write_file(&self.path, &content) {
            Ok(()) => {
                self.position += data.len() as u64;
                Ok(data.len())
            }
            Err(_) => Err(FilesystemError::Io),
        }
    }

    /// Seek to a position
    pub fn seek(&mut self, position: u64) -> Result<u64, FilesystemError> {
        self.position = position;
        Ok(self.position)
    }

    /// Get the current position
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Create a directory
    pub fn create_directory_at(&self, path: &str) -> Result<(), FilesystemError> {
        let full_path = if self.path == "/" {
            format!("/{}", path)
        } else {
            format!("{}/{}", self.path, path)
        };

        match ksyscall::mkdir(&full_path) {
            Ok(()) => Ok(()),
            Err(_) => Err(FilesystemError::Io),
        }
    }

    /// Remove a directory
    pub fn remove_directory_at(&self, path: &str) -> Result<(), FilesystemError> {
        let full_path = if self.path == "/" {
            format!("/{}", path)
        } else {
            format!("{}/{}", self.path, path)
        };

        match ksyscall::rmdir(&full_path) {
            Ok(()) => Ok(()),
            Err(_) => Err(FilesystemError::NotEmpty),
        }
    }

    /// Unlink a file
    pub fn unlink_file_at(&self, path: &str) -> Result<(), FilesystemError> {
        let full_path = if self.path == "/" {
            format!("/{}", path)
        } else {
            format!("{}/{}", self.path, path)
        };

        match ksyscall::unlink(&full_path) {
            Ok(()) => Ok(()),
            Err(_) => Err(FilesystemError::Io),
        }
    }

    /// Rename
    pub fn rename_at(
        &self,
        old_path: &str,
        new_dir: &Descriptor,
        new_path: &str,
    ) -> Result<(), FilesystemError> {
        let full_old = if self.path == "/" {
            format!("/{}", old_path)
        } else {
            format!("{}/{}", self.path, old_path)
        };

        let full_new = if new_dir.path == "/" {
            format!("/{}", new_path)
        } else {
            format!("{}/{}", new_dir.path, new_path)
        };

        match ksyscall::rename(&full_old, &full_new) {
            Ok(()) => Ok(()),
            Err(_) => Err(FilesystemError::Io),
        }
    }
}

/// Filesystem manager for WASI Preview2
pub struct FilesystemState {
    next_id: u32,
    descriptors: HashMap<DescriptorId, Descriptor>,
    /// Preopened directories (like WASI Preview1 preopens)
    preopens: Vec<(DescriptorId, String)>,
}

impl FilesystemState {
    pub fn new() -> Self {
        let mut state = Self {
            next_id: 3, // 0, 1, 2 are reserved for stdin, stdout, stderr
            descriptors: HashMap::new(),
            preopens: Vec::new(),
        };

        // Add root as a preopen
        let root_id = state.open_preopen("/");
        state.preopens.push((root_id, "/".to_string()));

        state
    }

    /// Open a preopened directory
    fn open_preopen(&mut self, path: &str) -> DescriptorId {
        let id = DescriptorId(self.next_id);
        self.next_id += 1;

        let desc = Descriptor::new(
            id,
            path,
            WasiDescriptorFlags {
                read: true,
                write: true,
                sync: false,
            },
        );
        self.descriptors.insert(id, desc);
        id
    }

    /// Get preopened directories
    pub fn get_preopens(&self) -> &[(DescriptorId, String)] {
        &self.preopens
    }

    /// Open a file relative to a directory
    pub fn open_at(
        &mut self,
        dir: DescriptorId,
        path: &str,
        flags: WasiOpenFlags,
        desc_flags: WasiDescriptorFlags,
    ) -> Result<DescriptorId, FilesystemError> {
        let dir_desc = self
            .descriptors
            .get(&dir)
            .ok_or(FilesystemError::NoEntry)?;

        let full_path = if dir_desc.path == "/" {
            format!("/{}", path)
        } else {
            format!("{}/{}", dir_desc.path, path)
        };

        // Check if file exists or needs to be created
        let exists = ksyscall::metadata(&full_path).is_ok();

        if !exists && !flags.create {
            return Err(FilesystemError::NoEntry);
        }

        if exists && flags.exclusive && flags.create {
            return Err(FilesystemError::Exist);
        }

        if flags.create && !exists {
            // Create the file
            if let Err(_) = ksyscall::write_file(&full_path, "") {
                return Err(FilesystemError::Io);
            }
        }

        if flags.truncate {
            // Truncate the file
            if let Err(_) = ksyscall::write_file(&full_path, "") {
                return Err(FilesystemError::Io);
            }
        }

        let id = DescriptorId(self.next_id);
        self.next_id += 1;

        let desc = Descriptor::new(id, &full_path, desc_flags);
        self.descriptors.insert(id, desc);

        Ok(id)
    }

    /// Get a descriptor
    pub fn get(&self, id: DescriptorId) -> Option<&Descriptor> {
        self.descriptors.get(&id)
    }

    /// Get a mutable descriptor
    pub fn get_mut(&mut self, id: DescriptorId) -> Option<&mut Descriptor> {
        self.descriptors.get_mut(&id)
    }

    /// Close a descriptor
    pub fn close(&mut self, id: DescriptorId) -> Result<(), FilesystemError> {
        self.descriptors
            .remove(&id)
            .map(|_| ())
            .ok_or(FilesystemError::NoEntry)
    }
}

impl Default for FilesystemState {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// WASI Preview2 Runtime (combines all interfaces)
// ============================================================================

/// Complete WASI Preview2 runtime state
pub struct WasiPreview2 {
    /// CLI state (args, env, exit, streams)
    pub cli: CliState,
    /// Filesystem state
    pub filesystem: FilesystemState,
    /// Monotonic clock
    pub monotonic_clock: MonotonicClock,
}

impl WasiPreview2 {
    pub fn new() -> Self {
        Self {
            cli: CliState::new(),
            filesystem: FilesystemState::new(),
            monotonic_clock: MonotonicClock::new(),
        }
    }

    /// Create with command-line arguments
    pub fn with_args(args: Vec<String>) -> Self {
        let mut state = Self::new();
        state.cli.set_args(args);
        state
    }

    /// Check if the command has exited
    pub fn has_exited(&self) -> bool {
        self.cli.has_exited()
    }

    /// Get the exit code
    pub fn exit_code(&self) -> Option<i32> {
        self.cli.exit_code()
    }
}

impl Default for WasiPreview2 {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_datetime_now() {
        let dt = Datetime::now();
        // Should be after 2020
        assert!(dt.seconds > 1577836800);
    }

    #[test]
    fn test_input_stream() {
        let mut stream = InputStream::with_data(StreamId(1), b"hello world".to_vec());

        let data = stream.read(5).unwrap();
        assert_eq!(data, b"hello");

        let data = stream.read(10).unwrap();
        assert_eq!(data, b" world");

        // EOF
        assert!(matches!(stream.read(1), Err(StreamError::Closed)));
    }

    #[test]
    fn test_output_stream() {
        let mut stream = OutputStream::new(StreamId(1));

        stream.write(b"hello ").unwrap();
        stream.write(b"world").unwrap();

        assert_eq!(stream.buffer(), b"hello world");
        assert_eq!(stream.take_buffer(), b"hello world");
        assert!(stream.buffer().is_empty());
    }

    #[test]
    fn test_cli_state() {
        let mut cli = CliState::new();

        cli.set_args(vec!["cmd".to_string(), "arg1".to_string()]);
        assert_eq!(cli.get_args(), &["cmd", "arg1"]);

        cli.set_env("PATH", "/bin");
        assert_eq!(cli.get_env("PATH"), Some("/bin"));
        assert_eq!(cli.get_env("MISSING"), None);

        assert!(!cli.has_exited());
        cli.exit(42);
        assert!(cli.has_exited());
        assert_eq!(cli.exit_code(), Some(42));
    }

    #[test]
    fn test_monotonic_clock() {
        let clock = MonotonicClock::new();

        let t1 = clock.now();
        // Small delay
        for _ in 0..1000 {
            std::hint::black_box(42);
        }
        let t2 = clock.now();

        assert!(t2.0 >= t1.0);
    }

    #[test]
    fn test_random_bytes() {
        let bytes1 = get_random_bytes(16);
        let bytes2 = get_random_bytes(16);

        assert_eq!(bytes1.len(), 16);
        assert_eq!(bytes2.len(), 16);
        // Not a strong test, but bytes should usually differ
    }

    #[test]
    fn test_wasi_preview2_state() {
        let wasi = WasiPreview2::with_args(vec!["test".to_string()]);

        assert_eq!(wasi.cli.get_args(), &["test"]);
        assert!(!wasi.has_exited());

        // Check preopens
        let preopens = wasi.filesystem.get_preopens();
        assert!(!preopens.is_empty());
        assert_eq!(preopens[0].1, "/");
    }

    #[test]
    fn test_filesystem_error_codes() {
        assert_eq!(FilesystemError::NoEntry.code(), -2);
        assert_eq!(FilesystemError::Access.code(), -3);
        assert_eq!(FilesystemError::Exist.code(), -4);
    }

    #[test]
    fn test_pollable() {
        let mut p = Pollable::new(PollableId(1));
        assert!(!p.ready());
        p.block();
        assert!(p.ready());
    }

    #[test]
    fn test_poll_multiple() {
        let pollables = vec![PollableId(1), PollableId(2), PollableId(3)];
        let ready = poll(&pollables);
        assert_eq!(ready, vec![0, 1, 2]);
    }
}
