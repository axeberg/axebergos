//! In-memory filesystem implementation
//!
//! Simple, fast, ephemeral. Good for development and as a cache layer.
//! Supports serialization for persistence to OPFS.

use super::{DirEntry, FileHandle, FileSystem, Metadata, OpenOptions};
use serde::{Deserialize, Serialize};
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
#[derive(Clone, Serialize, Deserialize)]
enum Node {
    File(Vec<u8>),
    Directory,
    Symlink(String),
}

/// Permission and ownership metadata for a file
#[derive(Clone, Serialize, Deserialize)]
struct NodeMeta {
    /// Owner user ID
    uid: u32,
    /// Owner group ID
    gid: u32,
    /// Unix permission mode (rwxrwxrwx)
    mode: u16,
}

impl Default for NodeMeta {
    fn default() -> Self {
        Self {
            uid: 1000,
            gid: 1000,
            mode: 0o644,
        }
    }
}

impl NodeMeta {
    fn dir_default() -> Self {
        Self {
            uid: 1000,
            gid: 1000,
            mode: 0o755,
        }
    }

    fn root_dir() -> Self {
        Self {
            uid: 0,
            gid: 0,
            mode: 0o755,
        }
    }
}

/// Serializable snapshot of the filesystem
#[derive(Serialize, Deserialize)]
pub struct FsSnapshot {
    /// All files and directories
    nodes: HashMap<String, Node>,
    /// Permission metadata for each path
    #[serde(default)]
    meta: HashMap<String, NodeMeta>,
    /// Format version for future compatibility
    version: u32,
}

/// In-memory filesystem
pub struct MemoryFs {
    /// All files and directories, keyed by path
    nodes: HashMap<String, Node>,
    /// Permission metadata for each path
    meta: HashMap<String, NodeMeta>,
    /// Open file handles
    handles: Slab<OpenFile>,
}

impl MemoryFs {
    pub fn new() -> Self {
        let mut fs = Self {
            nodes: HashMap::new(),
            meta: HashMap::new(),
            handles: Slab::new(),
        };
        // Root directory always exists
        fs.nodes.insert("/".to_string(), Node::Directory);
        fs.meta.insert("/".to_string(), NodeMeta::root_dir());
        fs
    }

    /// Maximum path length (similar to Linux PATH_MAX)
    const MAX_PATH_LEN: usize = 4096;
    /// Maximum length for a single path component (filename)
    const MAX_NAME_LEN: usize = 255;

    /// Validate a path for security and correctness
    pub fn validate_path(path: &str) -> io::Result<()> {
        // Check for null bytes (security issue)
        if path.contains('\0') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path contains null byte",
            ));
        }

        // Check total path length
        if path.len() > Self::MAX_PATH_LEN {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("path too long: {} bytes (max {})", path.len(), Self::MAX_PATH_LEN),
            ));
        }

        // Check individual component lengths
        for component in path.split('/') {
            if component.len() > Self::MAX_NAME_LEN {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("path component too long: {} bytes (max {})", component.len(), Self::MAX_NAME_LEN),
                ));
            }
        }

        Ok(())
    }

    /// Normalize a path (ensure leading slash, no trailing slash except root, resolve . and ..)
    fn normalize_path(path: &str) -> String {
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{}", path)
        };

        // Resolve . and .. components
        let mut result: Vec<&str> = Vec::new();
        for component in path.split('/') {
            match component {
                "" | "." => {} // skip empty and current dir
                ".." => {
                    result.pop(); // go up one level
                }
                name => result.push(name),
            }
        }

        if result.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", result.join("/"))
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

/// Snapshot version - increment when format changes
const SNAPSHOT_VERSION: u32 = 2;

impl MemoryFs {
    /// Create a snapshot of the filesystem for persistence
    pub fn snapshot(&self) -> FsSnapshot {
        FsSnapshot {
            nodes: self.nodes.clone(),
            meta: self.meta.clone(),
            version: SNAPSHOT_VERSION,
        }
    }

    /// Restore filesystem from a snapshot
    pub fn restore(snapshot: FsSnapshot) -> io::Result<Self> {
        // Accept version 1 (no meta) or version 2 (with meta)
        if snapshot.version != SNAPSHOT_VERSION && snapshot.version != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Snapshot version mismatch: expected {} or 1, got {}",
                    SNAPSHOT_VERSION, snapshot.version
                ),
            ));
        }

        // If version 1, meta will be empty (serde default)
        // Generate default meta for all nodes
        let meta = if snapshot.meta.is_empty() {
            let mut meta = HashMap::new();
            for (path, node) in &snapshot.nodes {
                let node_meta = match node {
                    Node::Directory => {
                        if path == "/" {
                            NodeMeta::root_dir()
                        } else {
                            NodeMeta::dir_default()
                        }
                    }
                    _ => NodeMeta::default(),
                };
                meta.insert(path.clone(), node_meta);
            }
            meta
        } else {
            snapshot.meta
        };

        Ok(Self {
            nodes: snapshot.nodes,
            meta,
            handles: Slab::new(),
        })
    }

    /// Serialize to JSON bytes
    pub fn to_json(&self) -> io::Result<Vec<u8>> {
        serde_json::to_vec(&self.snapshot())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Deserialize from JSON bytes
    pub fn from_json(data: &[u8]) -> io::Result<Self> {
        let snapshot: FsSnapshot = serde_json::from_slice(data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Self::restore(snapshot)
    }
}

impl FileSystem for MemoryFs {
    fn open(&mut self, path: &str, options: OpenOptions) -> io::Result<FileHandle> {
        // Validate path before processing
        Self::validate_path(path)?;
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
            self.meta.insert(path.clone(), NodeMeta::default());
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
                    size.checked_add(n as u64).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidInput, "seek position overflow")
                    })?
                } else {
                    size.saturating_sub((-n) as u64)
                }
            }
            SeekFrom::Current(n) => {
                if n >= 0 {
                    current.checked_add(n as u64).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidInput, "seek position overflow")
                    })?
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

        let meta = self.meta.get(&path).cloned().unwrap_or_default();

        match self.nodes.get(&path) {
            Some(Node::File(data)) => Ok(Metadata {
                size: data.len() as u64,
                is_dir: false,
                is_file: true,
                is_symlink: false,
                symlink_target: None,
                uid: meta.uid,
                gid: meta.gid,
                mode: meta.mode,
            }),
            Some(Node::Directory) => Ok(Metadata {
                size: 0,
                is_dir: true,
                is_file: false,
                is_symlink: false,
                symlink_target: None,
                uid: meta.uid,
                gid: meta.gid,
                mode: meta.mode,
            }),
            Some(Node::Symlink(target)) => Ok(Metadata {
                size: target.len() as u64,
                is_dir: false,
                is_file: false,
                is_symlink: true,
                symlink_target: Some(target.clone()),
                uid: meta.uid,
                gid: meta.gid,
                mode: meta.mode,
            }),
            None => Err(io::Error::new(io::ErrorKind::NotFound, "Path not found")),
        }
    }

    fn create_dir(&mut self, path: &str) -> io::Result<()> {
        Self::validate_path(path)?;
        let path = Self::normalize_path(path);

        if self.nodes.contains_key(&path) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Path already exists",
            ));
        }

        self.ensure_parent(&path)?;
        self.nodes.insert(path.clone(), Node::Directory);
        self.meta.insert(path, NodeMeta::dir_default());
        Ok(())
    }

    fn read_dir(&self, path: &str) -> io::Result<Vec<DirEntry>> {
        let path = Self::normalize_path(path);

        match self.nodes.get(&path) {
            Some(Node::Directory) => {}
            Some(Node::File(_)) | Some(Node::Symlink(_)) => {
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
                    is_symlink: matches!(node, Node::Symlink(_)),
                })
            })
            .collect();

        Ok(entries)
    }

    fn remove_file(&mut self, path: &str) -> io::Result<()> {
        Self::validate_path(path)?;
        let path = Self::normalize_path(path);

        match self.nodes.get(&path) {
            Some(Node::File(_)) | Some(Node::Symlink(_)) => {
                self.nodes.remove(&path);
                self.meta.remove(&path);
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
        Self::validate_path(path)?;
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
                self.meta.remove(&path);
                Ok(())
            }
            Some(Node::File(_)) | Some(Node::Symlink(_)) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Not a directory",
            )),
            None => Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Directory not found",
            )),
        }
    }

    fn rename(&mut self, from: &str, to: &str) -> io::Result<()> {
        let from = Self::normalize_path(from);
        let to = Self::normalize_path(to);

        if from == "/" {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Cannot rename root directory",
            ));
        }

        if !self.nodes.contains_key(&from) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Source not found"));
        }

        // Check destination parent exists
        self.ensure_parent(&to)?;

        // Check if destination already exists
        if self.nodes.contains_key(&to) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Destination already exists",
            ));
        }

        // For directories, we need to rename all children too
        let is_dir = matches!(self.nodes.get(&from), Some(Node::Directory));

        if is_dir {
            // Collect all paths that need renaming
            let from_prefix = format!("{}/", from);
            let to_prefix = format!("{}/", to);
            let children: Vec<(String, Node, Option<NodeMeta>)> = self
                .nodes
                .iter()
                .filter(|(p, _)| p.starts_with(&from_prefix))
                .map(|(p, n)| {
                    let new_path = format!("{}{}", to_prefix, &p[from_prefix.len()..]);
                    let meta = self.meta.get(p).cloned();
                    (new_path, n.clone(), meta)
                })
                .collect();

            // Remove old paths
            self.nodes.retain(|p, _| !p.starts_with(&from_prefix));
            self.meta.retain(|p, _| !p.starts_with(&from_prefix));

            // Insert new paths
            for (path, node, meta) in children {
                self.nodes.insert(path.clone(), node);
                if let Some(m) = meta {
                    self.meta.insert(path, m);
                }
            }
        }

        // Move the node itself
        if let Some(node) = self.nodes.remove(&from) {
            self.nodes.insert(to.clone(), node);
        }
        if let Some(meta) = self.meta.remove(&from) {
            self.meta.insert(to, meta);
        }

        Ok(())
    }

    fn copy_file(&mut self, from: &str, to: &str) -> io::Result<u64> {
        let from = Self::normalize_path(from);
        let to = Self::normalize_path(to);

        // Get source data (for symlinks, copy the link itself)
        let node_to_copy = match self.nodes.get(&from) {
            Some(Node::File(data)) => Node::File(data.clone()),
            Some(Node::Symlink(target)) => Node::Symlink(target.clone()),
            Some(Node::Directory) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Cannot copy directory with copy_file",
                ))
            }
            None => return Err(io::Error::new(io::ErrorKind::NotFound, "Source not found")),
        };

        let size = match &node_to_copy {
            Node::File(data) => data.len() as u64,
            Node::Symlink(target) => target.len() as u64,
            Node::Directory => 0,
        };

        // Ensure parent exists
        self.ensure_parent(&to)?;

        // Copy metadata (but set new owner to current user would require context)
        let meta = self.meta.get(&from).cloned().unwrap_or_default();

        // Insert copy at destination
        self.nodes.insert(to.clone(), node_to_copy);
        self.meta.insert(to, meta);

        Ok(size)
    }

    fn exists(&self, path: &str) -> bool {
        let path = Self::normalize_path(path);
        self.nodes.contains_key(&path)
    }

    fn symlink(&mut self, target: &str, link_path: &str) -> io::Result<()> {
        let link_path = Self::normalize_path(link_path);

        // Check if link path already exists
        if self.nodes.contains_key(&link_path) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Path already exists",
            ));
        }

        // Ensure parent directory exists
        self.ensure_parent(&link_path)?;

        // Create the symlink (target is stored as-is, can be relative or absolute)
        self.nodes.insert(link_path.clone(), Node::Symlink(target.to_string()));
        // Symlinks have mode 0o777 by convention (permissions are on target)
        self.meta.insert(link_path, NodeMeta {
            uid: 1000,
            gid: 1000,
            mode: 0o777,
        });
        Ok(())
    }

    fn read_link(&self, path: &str) -> io::Result<String> {
        let path = Self::normalize_path(path);

        match self.nodes.get(&path) {
            Some(Node::Symlink(target)) => Ok(target.clone()),
            Some(_) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Not a symbolic link",
            )),
            None => Err(io::Error::new(io::ErrorKind::NotFound, "Path not found")),
        }
    }

    fn chmod(&mut self, path: &str, mode: u16) -> io::Result<()> {
        let path = Self::normalize_path(path);

        if !self.nodes.contains_key(&path) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Path not found"));
        }

        if let Some(meta) = self.meta.get_mut(&path) {
            meta.mode = mode & 0o7777; // Mask to valid permission bits
        } else {
            // Create default meta with the new mode
            self.meta.insert(path, NodeMeta {
                uid: 1000,
                gid: 1000,
                mode: mode & 0o7777,
            });
        }

        Ok(())
    }

    fn chown(&mut self, path: &str, uid: Option<u32>, gid: Option<u32>) -> io::Result<()> {
        let path = Self::normalize_path(path);

        if !self.nodes.contains_key(&path) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Path not found"));
        }

        let meta = self.meta.entry(path).or_insert_with(NodeMeta::default);

        if let Some(new_uid) = uid {
            meta.uid = new_uid;
        }
        if let Some(new_gid) = gid {
            meta.gid = new_gid;
        }

        Ok(())
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

    #[test]
    fn test_root_exists() {
        let fs = MemoryFs::new();
        assert!(fs.exists("/"));
        assert!(fs.metadata("/").unwrap().is_dir);
    }

    #[test]
    fn test_create_directory() {
        let mut fs = MemoryFs::new();

        fs.create_dir("/home").unwrap();
        assert!(fs.exists("/home"));
        assert!(fs.metadata("/home").unwrap().is_dir);
    }

    #[test]
    fn test_nested_directories() {
        let mut fs = MemoryFs::new();

        fs.create_dir("/home").unwrap();
        fs.create_dir("/home/user").unwrap();
        fs.create_dir("/home/user/docs").unwrap();

        assert!(fs.exists("/home/user/docs"));
    }

    #[test]
    fn test_create_dir_without_parent_fails() {
        let mut fs = MemoryFs::new();

        let result = fs.create_dir("/home/user");
        assert!(result.is_err());
    }

    #[test]
    fn test_file_not_found() {
        let mut fs = MemoryFs::new();

        let result = fs.open("/nonexistent.txt", OpenOptions::new().read(true));
        assert!(result.is_err());
    }

    #[test]
    fn test_create_file() {
        let mut fs = MemoryFs::new();

        let handle = fs
            .open("/test.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.close(handle).unwrap();

        assert!(fs.exists("/test.txt"));
        assert!(fs.metadata("/test.txt").unwrap().is_file);
    }

    #[test]
    fn test_truncate() {
        let mut fs = MemoryFs::new();

        // Write initial content
        let handle = fs
            .open("/test.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.write(handle, b"hello world").unwrap();
        fs.close(handle).unwrap();

        // Truncate and write new content
        let handle = fs
            .open(
                "/test.txt",
                OpenOptions::new().write(true).truncate(true),
            )
            .unwrap();
        fs.write(handle, b"hi").unwrap();
        fs.close(handle).unwrap();

        // Verify
        assert_eq!(fs.metadata("/test.txt").unwrap().size, 2);
    }

    #[test]
    fn test_seek() {
        let mut fs = MemoryFs::new();

        let handle = fs
            .open("/test.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.write(handle, b"hello world").unwrap();
        fs.close(handle).unwrap();

        let handle = fs
            .open("/test.txt", OpenOptions::new().read(true))
            .unwrap();

        // Seek to position 6
        fs.seek(handle, SeekFrom::Start(6)).unwrap();

        let mut buf = [0u8; 5];
        fs.read(handle, &mut buf).unwrap();
        assert_eq!(&buf, b"world");

        fs.close(handle).unwrap();
    }

    #[test]
    fn test_read_dir() {
        let mut fs = MemoryFs::new();

        fs.create_dir("/home").unwrap();
        let _ = fs
            .open("/home/file1.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        let _ = fs
            .open("/home/file2.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.create_dir("/home/subdir").unwrap();

        let entries = fs.read_dir("/home").unwrap();
        assert_eq!(entries.len(), 3);

        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"file1.txt"));
        assert!(names.contains(&"file2.txt"));
        assert!(names.contains(&"subdir"));
    }

    #[test]
    fn test_remove_file() {
        let mut fs = MemoryFs::new();

        let handle = fs
            .open("/test.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.close(handle).unwrap();

        assert!(fs.exists("/test.txt"));
        fs.remove_file("/test.txt").unwrap();
        assert!(!fs.exists("/test.txt"));
    }

    #[test]
    fn test_remove_empty_dir() {
        let mut fs = MemoryFs::new();

        fs.create_dir("/empty").unwrap();
        fs.remove_dir("/empty").unwrap();
        assert!(!fs.exists("/empty"));
    }

    #[test]
    fn test_remove_nonempty_dir_fails() {
        let mut fs = MemoryFs::new();

        fs.create_dir("/dir").unwrap();
        let handle = fs
            .open("/dir/file.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.close(handle).unwrap();

        let result = fs.remove_dir("/dir");
        assert!(result.is_err());
    }

    #[test]
    fn test_cannot_remove_root() {
        let mut fs = MemoryFs::new();

        let result = fs.remove_dir("/");
        assert!(result.is_err());
    }

    #[test]
    fn test_path_normalization() {
        let mut fs = MemoryFs::new();

        fs.create_dir("/home").unwrap();

        // Both should refer to same directory
        assert!(fs.exists("/home"));
        assert!(fs.exists("/home/"));
        assert!(fs.exists("home"));
    }

    #[test]
    fn test_file_metadata_size() {
        let mut fs = MemoryFs::new();

        let handle = fs
            .open("/test.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.write(handle, b"12345").unwrap();
        fs.close(handle).unwrap();

        let meta = fs.metadata("/test.txt").unwrap();
        assert_eq!(meta.size, 5);
        assert!(meta.is_file);
        assert!(!meta.is_dir);
    }

    #[test]
    fn test_rename_file() {
        let mut fs = MemoryFs::new();

        let handle = fs
            .open("/old.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.write(handle, b"content").unwrap();
        fs.close(handle).unwrap();

        fs.rename("/old.txt", "/new.txt").unwrap();

        assert!(!fs.exists("/old.txt"));
        assert!(fs.exists("/new.txt"));

        // Content should be preserved
        let handle = fs.open("/new.txt", OpenOptions::new().read(true)).unwrap();
        let mut buf = [0u8; 7];
        fs.read(handle, &mut buf).unwrap();
        assert_eq!(&buf, b"content");
    }

    #[test]
    fn test_rename_directory() {
        let mut fs = MemoryFs::new();

        fs.create_dir("/olddir").unwrap();
        let handle = fs
            .open("/olddir/file.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.write(handle, b"test").unwrap();
        fs.close(handle).unwrap();

        fs.rename("/olddir", "/newdir").unwrap();

        assert!(!fs.exists("/olddir"));
        assert!(!fs.exists("/olddir/file.txt"));
        assert!(fs.exists("/newdir"));
        assert!(fs.exists("/newdir/file.txt"));
    }

    #[test]
    fn test_rename_to_existing_fails() {
        let mut fs = MemoryFs::new();

        let handle = fs
            .open("/a.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.close(handle).unwrap();

        let handle = fs
            .open("/b.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.close(handle).unwrap();

        let result = fs.rename("/a.txt", "/b.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_rename_nonexistent_fails() {
        let mut fs = MemoryFs::new();

        let result = fs.rename("/nonexistent", "/new");
        assert!(result.is_err());
    }

    #[test]
    fn test_rename_root_fails() {
        let mut fs = MemoryFs::new();

        let result = fs.rename("/", "/newroot");
        assert!(result.is_err());
    }

    #[test]
    fn test_copy_file() {
        let mut fs = MemoryFs::new();

        let handle = fs
            .open("/source.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.write(handle, b"hello world").unwrap();
        fs.close(handle).unwrap();

        let size = fs.copy_file("/source.txt", "/dest.txt").unwrap();
        assert_eq!(size, 11);

        // Both files should exist
        assert!(fs.exists("/source.txt"));
        assert!(fs.exists("/dest.txt"));

        // Content should match
        let handle = fs.open("/dest.txt", OpenOptions::new().read(true)).unwrap();
        let mut buf = [0u8; 11];
        fs.read(handle, &mut buf).unwrap();
        assert_eq!(&buf, b"hello world");
    }

    #[test]
    fn test_copy_directory_fails() {
        let mut fs = MemoryFs::new();

        fs.create_dir("/mydir").unwrap();

        let result = fs.copy_file("/mydir", "/copydir");
        assert!(result.is_err());
    }

    #[test]
    fn test_copy_nonexistent_fails() {
        let mut fs = MemoryFs::new();

        let result = fs.copy_file("/nonexistent", "/dest");
        assert!(result.is_err());
    }

    #[test]
    fn test_move_file_across_dirs() {
        let mut fs = MemoryFs::new();

        fs.create_dir("/dir1").unwrap();
        fs.create_dir("/dir2").unwrap();

        let handle = fs
            .open("/dir1/file.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.write(handle, b"moving").unwrap();
        fs.close(handle).unwrap();

        fs.rename("/dir1/file.txt", "/dir2/file.txt").unwrap();

        assert!(!fs.exists("/dir1/file.txt"));
        assert!(fs.exists("/dir2/file.txt"));
    }

    // ============ Symlinks ============

    #[test]
    fn test_symlink_create() {
        let mut fs = MemoryFs::new();

        // Create a target file
        let handle = fs
            .open("/target.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.write(handle, b"content").unwrap();
        fs.close(handle).unwrap();

        // Create symlink
        fs.symlink("/target.txt", "/link.txt").unwrap();

        // Symlink should exist
        assert!(fs.exists("/link.txt"));
    }

    #[test]
    fn test_symlink_read_link() {
        let mut fs = MemoryFs::new();

        fs.symlink("/some/target", "/mylink").unwrap();

        let target = fs.read_link("/mylink").unwrap();
        assert_eq!(target, "/some/target");
    }

    #[test]
    fn test_symlink_metadata() {
        let mut fs = MemoryFs::new();

        fs.symlink("/target", "/link").unwrap();

        let meta = fs.metadata("/link").unwrap();
        assert!(meta.is_symlink);
        assert!(!meta.is_file);
        assert!(!meta.is_dir);
        assert_eq!(meta.symlink_target, Some("/target".to_string()));
    }

    #[test]
    fn test_symlink_in_read_dir() {
        let mut fs = MemoryFs::new();

        fs.create_dir("/home").unwrap();
        fs.symlink("/elsewhere", "/home/mylink").unwrap();

        let entries = fs.read_dir("/home").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "mylink");
        assert!(entries[0].is_symlink);
        assert!(!entries[0].is_dir);
    }

    #[test]
    fn test_symlink_remove() {
        let mut fs = MemoryFs::new();

        fs.symlink("/target", "/link").unwrap();
        assert!(fs.exists("/link"));

        fs.remove_file("/link").unwrap();
        assert!(!fs.exists("/link"));
    }

    #[test]
    fn test_symlink_already_exists() {
        let mut fs = MemoryFs::new();

        fs.symlink("/target1", "/link").unwrap();

        let result = fs.symlink("/target2", "/link");
        assert!(result.is_err());
    }

    #[test]
    fn test_read_link_on_file_fails() {
        let mut fs = MemoryFs::new();

        let handle = fs
            .open("/file.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.close(handle).unwrap();

        let result = fs.read_link("/file.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_symlink_copy() {
        let mut fs = MemoryFs::new();

        fs.symlink("/original/target", "/link1").unwrap();

        // Copy symlink should copy the link itself, not follow it
        fs.copy_file("/link1", "/link2").unwrap();

        let target = fs.read_link("/link2").unwrap();
        assert_eq!(target, "/original/target");
    }
}
