//! Virtual File System
//!
//! A minimal VFS that provides a unified interface over different backends.
//! Start with in-memory, add OPFS persistence later.
//!
//! Design: trait-based abstraction, keeping it simple.

pub mod memory;
pub mod persist;

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

    /// Change file mode (permissions)
    fn chmod(&mut self, path: &str, mode: u16) -> io::Result<()>;

    /// Change file owner
    fn chown(&mut self, path: &str, uid: Option<u32>, gid: Option<u32>) -> io::Result<()>;
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
