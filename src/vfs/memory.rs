//! In-memory filesystem implementation
//!
//! Simple, fast, ephemeral. Good for development and as a cache layer.
//! Data lives only as long as the page is open.

use super::{DirEntry, FileHandle, FileSystem, Metadata, OpenOptions};
use slab::Slab;
use std::collections::HashMap;
use std::io::{self, SeekFrom};

/// A file's contents and position
struct OpenFile {
    path: String,
    position: u64,
    readable: bool,
    writable: bool,
}

/// A stored file or directory
#[derive(Clone)]
enum Node {
    File(Vec<u8>),
    Directory,
}

/// In-memory filesystem
pub struct MemoryFs {
    /// All files and directories, keyed by path
    nodes: HashMap<String, Node>,
    /// Open file handles
    handles: Slab<OpenFile>,
}

impl MemoryFs {
    pub fn new() -> Self {
        let mut fs = Self {
            nodes: HashMap::new(),
            handles: Slab::new(),
        };
        // Root directory always exists
        fs.nodes.insert("/".to_string(), Node::Directory);
        fs
    }

    /// Normalize a path (ensure leading slash, no trailing slash except root)
    fn normalize_path(path: &str) -> String {
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{}", path)
        };

        if path.len() > 1 && path.ends_with('/') {
            path[..path.len() - 1].to_string()
        } else {
            path
        }
    }

    /// Get parent directory of a path
    fn parent_path(path: &str) -> Option<String> {
        let path = Self::normalize_path(path);
        if path == "/" {
            return None;
        }
        let idx = path.rfind('/')?;
        if idx == 0 {
            Some("/".to_string())
        } else {
            Some(path[..idx].to_string())
        }
    }

    /// Ensure parent directories exist
    fn ensure_parent(&mut self, path: &str) -> io::Result<()> {
        if let Some(parent) = Self::parent_path(path) {
            if !self.nodes.contains_key(&parent) {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Parent directory not found: {}", parent),
                ));
            }
        }
        Ok(())
    }
}

impl Default for MemoryFs {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystem for MemoryFs {
    fn open(&mut self, path: &str, options: OpenOptions) -> io::Result<FileHandle> {
        let path = Self::normalize_path(path);

        // Check if file exists
        let exists = self.nodes.contains_key(&path);

        if !exists && !options.create {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("File not found: {}", path),
            ));
        }

        if !exists {
            // Create new file
            self.ensure_parent(&path)?;
            self.nodes.insert(path.clone(), Node::File(Vec::new()));
        } else if options.truncate {
            // Truncate existing file
            if let Some(Node::File(data)) = self.nodes.get_mut(&path) {
                data.clear();
            }
        }

        // Verify it's a file, not directory
        match self.nodes.get(&path) {
            Some(Node::Directory) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Cannot open directory as file",
                ))
            }
            None => {
                return Err(io::Error::new(io::ErrorKind::NotFound, "File not found"))
            }
            _ => {}
        }

        // Create handle
        let handle = self.handles.insert(OpenFile {
            path,
            position: 0,
            readable: options.read,
            writable: options.write,
        });

        Ok(handle)
    }

    fn close(&mut self, handle: FileHandle) -> io::Result<()> {
        if self.handles.contains(handle) {
            self.handles.remove(handle);
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid file handle",
            ))
        }
    }

    fn read(&mut self, handle: FileHandle, buf: &mut [u8]) -> io::Result<usize> {
        let file = self.handles.get_mut(handle).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Invalid file handle")
        })?;

        if !file.readable {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "File not opened for reading",
            ));
        }

        let path = file.path.clone();
        let position = file.position as usize;

        let data = match self.nodes.get(&path) {
            Some(Node::File(data)) => data,
            _ => return Err(io::Error::new(io::ErrorKind::NotFound, "File not found")),
        };

        let available = data.len().saturating_sub(position);
        let to_read = buf.len().min(available);

        buf[..to_read].copy_from_slice(&data[position..position + to_read]);

        // Update position
        if let Some(file) = self.handles.get_mut(handle) {
            file.position += to_read as u64;
        }

        Ok(to_read)
    }

    fn write(&mut self, handle: FileHandle, buf: &[u8]) -> io::Result<usize> {
        let file = self.handles.get_mut(handle).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Invalid file handle")
        })?;

        if !file.writable {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "File not opened for writing",
            ));
        }

        let path = file.path.clone();
        let position = file.position as usize;

        let data = match self.nodes.get_mut(&path) {
            Some(Node::File(data)) => data,
            _ => return Err(io::Error::new(io::ErrorKind::NotFound, "File not found")),
        };

        // Extend file if necessary
        if position + buf.len() > data.len() {
            data.resize(position + buf.len(), 0);
        }

        data[position..position + buf.len()].copy_from_slice(buf);

        // Update position
        if let Some(file) = self.handles.get_mut(handle) {
            file.position += buf.len() as u64;
        }

        Ok(buf.len())
    }

    fn seek(&mut self, handle: FileHandle, pos: SeekFrom) -> io::Result<u64> {
        let file = self.handles.get_mut(handle).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Invalid file handle")
        })?;

        let path = file.path.clone();
        let current = file.position;

        let size = match self.nodes.get(&path) {
            Some(Node::File(data)) => data.len() as u64,
            _ => return Err(io::Error::new(io::ErrorKind::NotFound, "File not found")),
        };

        let new_pos = match pos {
            SeekFrom::Start(n) => n,
            SeekFrom::End(n) => {
                if n >= 0 {
                    size + n as u64
                } else {
                    size.saturating_sub((-n) as u64)
                }
            }
            SeekFrom::Current(n) => {
                if n >= 0 {
                    current + n as u64
                } else {
                    current.saturating_sub((-n) as u64)
                }
            }
        };

        if let Some(file) = self.handles.get_mut(handle) {
            file.position = new_pos;
        }

        Ok(new_pos)
    }

    fn metadata(&self, path: &str) -> io::Result<Metadata> {
        let path = Self::normalize_path(path);

        match self.nodes.get(&path) {
            Some(Node::File(data)) => Ok(Metadata {
                size: data.len() as u64,
                is_dir: false,
                is_file: true,
            }),
            Some(Node::Directory) => Ok(Metadata {
                size: 0,
                is_dir: true,
                is_file: false,
            }),
            None => Err(io::Error::new(io::ErrorKind::NotFound, "Path not found")),
        }
    }

    fn create_dir(&mut self, path: &str) -> io::Result<()> {
        let path = Self::normalize_path(path);

        if self.nodes.contains_key(&path) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Path already exists",
            ));
        }

        self.ensure_parent(&path)?;
        self.nodes.insert(path, Node::Directory);
        Ok(())
    }

    fn read_dir(&self, path: &str) -> io::Result<Vec<DirEntry>> {
        let path = Self::normalize_path(path);

        match self.nodes.get(&path) {
            Some(Node::Directory) => {}
            Some(Node::File(_)) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Not a directory",
                ))
            }
            None => return Err(io::Error::new(io::ErrorKind::NotFound, "Directory not found")),
        }

        let prefix = if path == "/" {
            "/".to_string()
        } else {
            format!("{}/", path)
        };

        let entries: Vec<DirEntry> = self
            .nodes
            .iter()
            .filter_map(|(p, node)| {
                if p == &path {
                    return None; // Skip self
                }

                // Check if this is a direct child
                if !p.starts_with(&prefix) {
                    return None;
                }

                let relative = &p[prefix.len()..];
                if relative.contains('/') {
                    return None; // Not a direct child
                }

                Some(DirEntry {
                    name: relative.to_string(),
                    is_dir: matches!(node, Node::Directory),
                })
            })
            .collect();

        Ok(entries)
    }

    fn remove_file(&mut self, path: &str) -> io::Result<()> {
        let path = Self::normalize_path(path);

        match self.nodes.get(&path) {
            Some(Node::File(_)) => {
                self.nodes.remove(&path);
                Ok(())
            }
            Some(Node::Directory) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Cannot remove directory with remove_file",
            )),
            None => Err(io::Error::new(io::ErrorKind::NotFound, "File not found")),
        }
    }

    fn remove_dir(&mut self, path: &str) -> io::Result<()> {
        let path = Self::normalize_path(path);

        if path == "/" {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Cannot remove root directory",
            ));
        }

        match self.nodes.get(&path) {
            Some(Node::Directory) => {
                // Check if empty
                let prefix = format!("{}/", path);
                let has_children = self.nodes.keys().any(|p| p.starts_with(&prefix));
                if has_children {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Directory not empty",
                    ));
                }
                self.nodes.remove(&path);
                Ok(())
            }
            Some(Node::File(_)) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Not a directory",
            )),
            None => Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Directory not found",
            )),
        }
    }

    fn exists(&self, path: &str) -> bool {
        let path = Self::normalize_path(path);
        self.nodes.contains_key(&path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_file_ops() {
        let mut fs = MemoryFs::new();

        // Create and write
        let handle = fs
            .open("/test.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.write(handle, b"hello world").unwrap();
        fs.close(handle).unwrap();

        // Read back
        let handle = fs.open("/test.txt", OpenOptions::new().read(true)).unwrap();
        let mut buf = [0u8; 11];
        let n = fs.read(handle, &mut buf).unwrap();
        assert_eq!(n, 11);
        assert_eq!(&buf, b"hello world");
        fs.close(handle).unwrap();
    }
}
