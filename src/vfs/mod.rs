//! Virtual File System
//!
//! A minimal VFS that provides a unified interface over different backends.
//! Start with in-memory, add OPFS persistence later.
//!
//! Design: trait-based abstraction, keeping it simple.

pub mod layered;
pub mod memory;
pub mod persist;

pub use layered::LayeredFs;
pub use memory::{FsSnapshot, MemoryFs};
pub use persist::Persistence;

use std::io;

/// A file handle
pub type FileHandle = usize;

/// File open modes
#[derive(Debug, Clone, Copy)]
pub struct OpenOptions {
    pub read: bool,
    pub write: bool,
    pub create: bool,
    pub truncate: bool,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            read: true,
            write: false,
            create: false,
            truncate: false,
        }
    }
}

impl OpenOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read(mut self, read: bool) -> Self {
        self.read = read;
        self
    }

    pub fn write(mut self, write: bool) -> Self {
        self.write = write;
        self
    }

    pub fn create(mut self, create: bool) -> Self {
        self.create = create;
        self
    }

    pub fn truncate(mut self, truncate: bool) -> Self {
        self.truncate = truncate;
        self
    }
}

/// File metadata
#[derive(Debug, Clone)]
pub struct Metadata {
    pub size: u64,
    pub is_dir: bool,
    pub is_file: bool,
    pub is_symlink: bool,
    pub symlink_target: Option<String>,
    /// Owner user ID
    pub uid: u32,
    /// Owner group ID
    pub gid: u32,
    /// Unix permission mode (rwxrwxrwx)
    pub mode: u16,
    /// Access time (last read) in milliseconds since epoch
    pub atime: f64,
    /// Modification time (last write to content) in milliseconds since epoch
    pub mtime: f64,
    /// Change time (last metadata change) in milliseconds since epoch
    pub ctime: f64,
    /// Number of hard links to this inode
    pub nlink: u32,
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            size: 0,
            is_dir: false,
            is_file: true,
            is_symlink: false,
            symlink_target: None,
            uid: 1000, // Default to regular user
            gid: 1000,
            mode: 0o644, // rw-r--r--
            atime: 0.0,
            mtime: 0.0,
            ctime: 0.0,
            nlink: 1, // New files have 1 link
        }
    }
}

/// Directory entry
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
}

/// The FileSystem trait - implement this for different backends
pub trait FileSystem {
    /// Open a file, returning a handle
    fn open(&mut self, path: &str, options: OpenOptions) -> io::Result<FileHandle>;

    /// Close a file handle
    fn close(&mut self, handle: FileHandle) -> io::Result<()>;

    /// Read from a file
    fn read(&mut self, handle: FileHandle, buf: &mut [u8]) -> io::Result<usize>;

    /// Write to a file
    fn write(&mut self, handle: FileHandle, buf: &[u8]) -> io::Result<usize>;

    /// Seek within a file
    fn seek(&mut self, handle: FileHandle, pos: io::SeekFrom) -> io::Result<u64>;

    /// Get file metadata
    fn metadata(&self, path: &str) -> io::Result<Metadata>;

    /// Create a directory
    fn create_dir(&mut self, path: &str) -> io::Result<()>;

    /// Read directory contents
    fn read_dir(&self, path: &str) -> io::Result<Vec<DirEntry>>;

    /// Remove a file
    fn remove_file(&mut self, path: &str) -> io::Result<()>;

    /// Remove a directory
    fn remove_dir(&mut self, path: &str) -> io::Result<()>;

    /// Rename/move a file or directory
    fn rename(&mut self, from: &str, to: &str) -> io::Result<()>;

    /// Copy a file to a new location
    fn copy_file(&mut self, from: &str, to: &str) -> io::Result<u64>;

    /// Check if path exists
    fn exists(&self, path: &str) -> bool;

    /// Create a symbolic link
    fn symlink(&mut self, target: &str, link_path: &str) -> io::Result<()>;

    /// Read the target of a symbolic link
    fn read_link(&self, path: &str) -> io::Result<String>;

    /// Create a hard link
    ///
    /// Creates a new directory entry that points to the same inode as the source.
    /// The link count is incremented. Both paths must be on the same filesystem.
    fn link(&mut self, source: &str, dest: &str) -> io::Result<()>;

    /// Change file mode (permissions)
    fn chmod(&mut self, path: &str, mode: u16) -> io::Result<()>;

    /// Change file owner
    fn chown(&mut self, path: &str, uid: Option<u32>, gid: Option<u32>) -> io::Result<()>;

    /// Get metadata for an open file handle (fstat)
    ///
    /// This is used for atomic permission checking - get metadata for the
    /// actual file that was opened, not a path that could have changed.
    fn fstat(&self, handle: FileHandle) -> io::Result<Metadata>;

    /// Get the resolved path for an open file handle
    ///
    /// Returns the canonical path after symlink resolution.
    fn handle_path(&self, handle: FileHandle) -> io::Result<String>;

    /// Set the filesystem clock for timestamp updates
    ///
    /// Called by the kernel before operations to ensure timestamps are accurate.
    fn set_clock(&mut self, now: f64);

    /// Update access and modification times (utimes/touch)
    ///
    /// If atime or mtime is None, the current clock time is used.
    fn utimes(&mut self, path: &str, atime: Option<f64>, mtime: Option<f64>) -> io::Result<()>;
}

/// Convenience wrapper for reading entire file to string
pub fn read_to_string<F: FileSystem>(fs: &mut F, path: &str) -> io::Result<String> {
    let handle = fs.open(path, OpenOptions::new().read(true))?;
    let meta = fs.metadata(path)?;
    let mut buf = vec![0u8; meta.size as usize];
    fs.read(handle, &mut buf)?;
    fs.close(handle)?;
    String::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Convenience wrapper for writing string to file
pub fn write_string<F: FileSystem>(fs: &mut F, path: &str, content: &str) -> io::Result<()> {
    let handle = fs.open(
        path,
        OpenOptions::new().write(true).create(true).truncate(true),
    )?;
    fs.write(handle, content.as_bytes())?;
    fs.close(handle)?;
    Ok(())
}
