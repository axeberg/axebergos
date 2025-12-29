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
    /// Access time (last read) in milliseconds since epoch
    #[serde(default)]
    atime: f64,
    /// Modification time (last write to content) in milliseconds since epoch
    #[serde(default)]
    mtime: f64,
    /// Change time (last metadata change) in milliseconds since epoch
    #[serde(default)]
    ctime: f64,
}

impl Default for NodeMeta {
    fn default() -> Self {
        Self {
            uid: 1000,
            gid: 1000,
            mode: 0o644,
            atime: 0.0,
            mtime: 0.0,
            ctime: 0.0,
        }
    }
}

impl NodeMeta {
    fn with_time(uid: u32, gid: u32, mode: u16, now: f64) -> Self {
        Self {
            uid,
            gid,
            mode,
            atime: now,
            mtime: now,
            ctime: now,
        }
    }

    fn file_default(now: f64) -> Self {
        Self::with_time(1000, 1000, 0o644, now)
    }

    fn dir_default() -> Self {
        Self {
            uid: 1000,
            gid: 1000,
            mode: 0o755,
            atime: 0.0,
            mtime: 0.0,
            ctime: 0.0,
        }
    }

    fn dir_default_with_time(now: f64) -> Self {
        Self::with_time(1000, 1000, 0o755, now)
    }

    fn root_dir() -> Self {
        Self {
            uid: 0,
            gid: 0,
            mode: 0o755,
            atime: 0.0,
            mtime: 0.0,
            ctime: 0.0,
        }
    }

    fn symlink_default(now: f64) -> Self {
        Self::with_time(1000, 1000, 0o777, now)
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
    /// Current clock time (set by kernel before operations)
    clock: f64,
}

impl MemoryFs {
    pub fn new() -> Self {
        let mut fs = Self {
            nodes: HashMap::new(),
            meta: HashMap::new(),
            handles: Slab::new(),
            clock: 0.0,
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
    /// Maximum symlink recursion depth (POSIX standard: SYMLOOP_MAX = 40)
    pub const MAX_SYMLINK_DEPTH: usize = 40;

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
                format!(
                    "path too long: {} bytes (max {})",
                    path.len(),
                    Self::MAX_PATH_LEN
                ),
            ));
        }

        // Check individual component lengths
        for component in path.split('/') {
            if component.len() > Self::MAX_NAME_LEN {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "path component too long: {} bytes (max {})",
                        component.len(),
                        Self::MAX_NAME_LEN
                    ),
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
        if let Some(parent) = Self::parent_path(path)
            && !self.nodes.contains_key(&parent)
        {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Parent directory not found: {}", parent),
            ));
        }
        Ok(())
    }

    /// Resolve a path, following symlinks with loop detection
    ///
    /// This function resolves symlinks up to MAX_SYMLINK_DEPTH levels deep.
    /// If the depth is exceeded (possible symlink loop), returns an error.
    ///
    /// # Arguments
    /// * `path` - The path to resolve
    ///
    /// # Returns
    /// * `Ok(String)` - The resolved path (may be the same if no symlinks)
    /// * `Err` - If path not found or symlink loop detected
    ///
    /// # Example
    /// ```ignore
    /// // If /link -> /target and /target is a file:
    /// fs.resolve_symlinks("/link") // Returns Ok("/target")
    ///
    /// // If /a -> /b and /b -> /a (loop):
    /// fs.resolve_symlinks("/a") // Returns Err(TooManyLinks)
    /// ```
    pub fn resolve_symlinks(&self, path: &str) -> io::Result<String> {
        self.resolve_symlinks_internal(path, 0)
    }

    /// Internal symlink resolution with depth tracking
    fn resolve_symlinks_internal(&self, path: &str, depth: usize) -> io::Result<String> {
        if depth > Self::MAX_SYMLINK_DEPTH {
            return Err(io::Error::other(
                "too many levels of symbolic links (possible loop)",
            ));
        }

        let normalized = Self::normalize_path(path);

        match self.nodes.get(&normalized) {
            Some(Node::Symlink(target)) => {
                // Resolve the symlink target
                let resolved_target = if target.starts_with('/') {
                    // Absolute symlink
                    target.clone()
                } else {
                    // Relative symlink - resolve relative to symlink's parent
                    match Self::parent_path(&normalized) {
                        Some(parent) => format!("{}/{}", parent, target),
                        None => format!("/{}", target),
                    }
                };

                // Recursively resolve in case target is also a symlink
                self.resolve_symlinks_internal(&resolved_target, depth + 1)
            }
            Some(_) => {
                // Not a symlink, return the normalized path
                Ok(normalized)
            }
            None => {
                // Path doesn't exist - could be a path with symlinks in parent
                // Try to resolve parent directories that might be symlinks
                self.resolve_path_components(&normalized, depth)
            }
        }
    }

    /// Resolve a path component by component, following symlinks
    fn resolve_path_components(&self, path: &str, depth: usize) -> io::Result<String> {
        if depth > Self::MAX_SYMLINK_DEPTH {
            return Err(io::Error::other(
                "too many levels of symbolic links (possible loop)",
            ));
        }

        let normalized = Self::normalize_path(path);
        if normalized == "/" {
            return Ok("/".to_string());
        }

        // Split into components and resolve each
        let components: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
        let mut resolved = String::new();

        for component in components {
            let current_path = format!("{}/{}", resolved, component);

            match self.nodes.get(&current_path) {
                Some(Node::Symlink(target)) => {
                    // Resolve the symlink
                    let resolved_target = if target.starts_with('/') {
                        target.clone()
                    } else if resolved.is_empty() {
                        format!("/{}", target)
                    } else {
                        format!("{}/{}", resolved, target)
                    };

                    // Recursively resolve the symlink target
                    resolved = self.resolve_symlinks_internal(&resolved_target, depth + 1)?;
                }
                Some(_) => {
                    // Regular file or directory
                    resolved = current_path;
                }
                None => {
                    // Path component doesn't exist
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("path not found: {}", current_path),
                    ));
                }
            }
        }

        if resolved.is_empty() {
            Ok("/".to_string())
        } else {
            Ok(resolved)
        }
    }

    /// Check if a path is a symlink (without following it)
    pub fn is_symlink(&self, path: &str) -> bool {
        let normalized = Self::normalize_path(path);
        matches!(self.nodes.get(&normalized), Some(Node::Symlink(_)))
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
            clock: 0.0,
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
            // Create new file with current timestamp
            self.ensure_parent(&path)?;
            self.nodes.insert(path.clone(), Node::File(Vec::new()));
            self.meta
                .insert(path.clone(), NodeMeta::file_default(self.clock));
        } else if options.truncate {
            // Truncate existing file and update mtime/ctime
            if let Some(Node::File(data)) = self.nodes.get_mut(&path) {
                data.clear();
            }
            // Update modification time
            if let Some(meta) = self.meta.get_mut(&path) {
                meta.mtime = self.clock;
                meta.ctime = self.clock;
            }
        }

        // Verify it's a file, not directory
        match self.nodes.get(&path) {
            Some(Node::Directory) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Cannot open directory as file",
                ));
            }
            None => return Err(io::Error::new(io::ErrorKind::NotFound, "File not found")),
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
        let file = self
            .handles
            .get_mut(handle)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid file handle"))?;

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

        // Update access time (atime) on read
        if to_read > 0
            && let Some(meta) = self.meta.get_mut(&path)
        {
            meta.atime = self.clock;
        }

        Ok(to_read)
    }

    fn write(&mut self, handle: FileHandle, buf: &[u8]) -> io::Result<usize> {
        let file = self
            .handles
            .get_mut(handle)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid file handle"))?;

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

        // Update modification time (mtime) and change time (ctime) on write
        if !buf.is_empty()
            && let Some(meta) = self.meta.get_mut(&path)
        {
            meta.mtime = self.clock;
            meta.ctime = self.clock;
        }

        Ok(buf.len())
    }

    fn seek(&mut self, handle: FileHandle, pos: SeekFrom) -> io::Result<u64> {
        let file = self
            .handles
            .get_mut(handle)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid file handle"))?;

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
                atime: meta.atime,
                mtime: meta.mtime,
                ctime: meta.ctime,
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
                atime: meta.atime,
                mtime: meta.mtime,
                ctime: meta.ctime,
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
                atime: meta.atime,
                mtime: meta.mtime,
                ctime: meta.ctime,
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
        self.meta
            .insert(path, NodeMeta::dir_default_with_time(self.clock));
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
                ));
            }
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "Directory not found",
                ));
            }
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
                ));
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
        self.nodes
            .insert(link_path.clone(), Node::Symlink(target.to_string()));
        // Symlinks have mode 0o777 by convention (permissions are on target)
        self.meta
            .insert(link_path, NodeMeta::symlink_default(self.clock));
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
            meta.ctime = self.clock; // Update change time on metadata change
        } else {
            // Create default meta with the new mode and current timestamp
            let mut node_meta = NodeMeta::file_default(self.clock);
            node_meta.mode = mode & 0o7777;
            self.meta.insert(path, node_meta);
        }

        Ok(())
    }

    fn chown(&mut self, path: &str, uid: Option<u32>, gid: Option<u32>) -> io::Result<()> {
        let path = Self::normalize_path(path);

        if !self.nodes.contains_key(&path) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Path not found"));
        }

        let clock = self.clock;
        let meta = self.meta.entry(path).or_default();

        if let Some(new_uid) = uid {
            meta.uid = new_uid;
        }
        if let Some(new_gid) = gid {
            meta.gid = new_gid;
        }
        // Update change time on metadata change
        meta.ctime = clock;

        Ok(())
    }

    fn fstat(&self, handle: FileHandle) -> io::Result<Metadata> {
        let file = self
            .handles
            .get(handle)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid file handle"))?;

        // Get metadata for the path stored in the handle
        // This is the resolved path after symlink resolution during open
        self.metadata(&file.path)
    }

    fn handle_path(&self, handle: FileHandle) -> io::Result<String> {
        let file = self
            .handles
            .get(handle)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid file handle"))?;

        Ok(file.path.clone())
    }

    fn set_clock(&mut self, now: f64) {
        self.clock = now;
    }

    fn utimes(&mut self, path: &str, atime: Option<f64>, mtime: Option<f64>) -> io::Result<()> {
        let path = Self::normalize_path(path);

        if !self.nodes.contains_key(&path) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Path not found"));
        }

        if let Some(meta) = self.meta.get_mut(&path) {
            meta.atime = atime.unwrap_or(self.clock);
            meta.mtime = mtime.unwrap_or(self.clock);
            meta.ctime = self.clock; // ctime always updated when changing times
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
            .open("/test.txt", OpenOptions::new().write(true).truncate(true))
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

        let handle = fs.open("/test.txt", OpenOptions::new().read(true)).unwrap();

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
            .open(
                "/home/file1.txt",
                OpenOptions::new().write(true).create(true),
            )
            .unwrap();
        let _ = fs
            .open(
                "/home/file2.txt",
                OpenOptions::new().write(true).create(true),
            )
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
            .open(
                "/olddir/file.txt",
                OpenOptions::new().write(true).create(true),
            )
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
            .open(
                "/dir1/file.txt",
                OpenOptions::new().write(true).create(true),
            )
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

    // ============ Symlink Loop Detection ============

    /// Helper to create a file in tests
    fn create_test_file(fs: &mut MemoryFs, path: &str) {
        let handle = fs
            .open(path, OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.close(handle).unwrap();
    }

    #[test]
    fn test_resolve_symlink_simple() {
        let mut fs = MemoryFs::new();

        // Create a file and a symlink to it
        create_test_file(&mut fs, "/target.txt");
        fs.symlink("/target.txt", "/link.txt").unwrap();

        // Resolve the symlink
        let resolved = fs.resolve_symlinks("/link.txt").unwrap();
        assert_eq!(resolved, "/target.txt");
    }

    #[test]
    fn test_resolve_symlink_chain() {
        let mut fs = MemoryFs::new();

        // Create a chain: link1 -> link2 -> link3 -> file
        create_test_file(&mut fs, "/file.txt");
        fs.symlink("/file.txt", "/link3").unwrap();
        fs.symlink("/link3", "/link2").unwrap();
        fs.symlink("/link2", "/link1").unwrap();

        // Resolve the chain
        let resolved = fs.resolve_symlinks("/link1").unwrap();
        assert_eq!(resolved, "/file.txt");
    }

    #[test]
    fn test_resolve_symlink_loop_direct() {
        let mut fs = MemoryFs::new();

        // Create a direct loop: a -> b -> a
        fs.symlink("/b", "/a").unwrap();
        fs.symlink("/a", "/b").unwrap();

        // Should fail with loop detection
        let result = fs.resolve_symlinks("/a");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("too many levels of symbolic links")
        );
    }

    #[test]
    fn test_resolve_symlink_loop_self() {
        let mut fs = MemoryFs::new();

        // Create a self-referencing symlink: a -> a
        fs.symlink("/a", "/a").unwrap();

        // Should fail with loop detection
        let result = fs.resolve_symlinks("/a");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_symlink_loop_long_chain() {
        let mut fs = MemoryFs::new();

        // Create a long chain that eventually loops back
        // link0 -> link1 -> link2 -> ... -> link50 -> link0
        for i in 0..50 {
            let target = format!("/link{}", (i + 1) % 51);
            let link = format!("/link{}", i);
            fs.symlink(&target, &link).unwrap();
        }
        fs.symlink("/link0", "/link50").unwrap();

        // Should fail with loop detection
        let result = fs.resolve_symlinks("/link0");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_symlink_relative() {
        let mut fs = MemoryFs::new();

        // Create directory structure with relative symlink
        fs.create_dir("/dir").unwrap();
        create_test_file(&mut fs, "/dir/target.txt");
        fs.symlink("target.txt", "/dir/link.txt").unwrap(); // relative symlink

        // Resolve the relative symlink
        let resolved = fs.resolve_symlinks("/dir/link.txt").unwrap();
        assert_eq!(resolved, "/dir/target.txt");
    }

    #[test]
    fn test_is_symlink() {
        let mut fs = MemoryFs::new();

        create_test_file(&mut fs, "/file.txt");
        fs.create_dir("/dir").unwrap();
        fs.symlink("/file.txt", "/link.txt").unwrap();

        assert!(!fs.is_symlink("/file.txt"));
        assert!(!fs.is_symlink("/dir"));
        assert!(fs.is_symlink("/link.txt"));
        assert!(!fs.is_symlink("/nonexistent"));
    }

    #[test]
    fn test_max_symlink_depth_constant() {
        // Verify the constant is set to POSIX standard
        assert_eq!(MemoryFs::MAX_SYMLINK_DEPTH, 40);
    }

    // ============ fstat (TOCTOU-safe metadata) ============

    #[test]
    fn test_fstat_basic() {
        let mut fs = MemoryFs::new();

        // Create a file with some content
        let handle = fs
            .open("/test.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.write(handle, b"hello world").unwrap();
        fs.close(handle).unwrap();

        // Set custom permissions
        fs.chmod("/test.txt", 0o755).unwrap();
        fs.chown("/test.txt", Some(1000), Some(1001)).unwrap();

        // Open and check fstat
        let handle = fs.open("/test.txt", OpenOptions::new().read(true)).unwrap();
        let meta = fs.fstat(handle).unwrap();

        assert_eq!(meta.size, 11); // "hello world".len()
        assert!(meta.is_file);
        assert!(!meta.is_dir);
        assert!(!meta.is_symlink);
        assert_eq!(meta.mode, 0o755);
        assert_eq!(meta.uid, 1000);
        assert_eq!(meta.gid, 1001);

        fs.close(handle).unwrap();
    }

    #[test]
    fn test_fstat_invalid_handle() {
        let fs = MemoryFs::new();

        // Try to fstat with invalid handle
        let result = fs.fstat(999);
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_path() {
        let mut fs = MemoryFs::new();

        // Create and open a file
        create_test_file(&mut fs, "/myfile.txt");
        let handle = fs
            .open("/myfile.txt", OpenOptions::new().read(true))
            .unwrap();

        // Get path from handle
        let path = fs.handle_path(handle).unwrap();
        assert_eq!(path, "/myfile.txt");

        fs.close(handle).unwrap();
    }

    // ============ Timestamps ============

    #[test]
    fn test_timestamp_on_file_create() {
        let mut fs = MemoryFs::new();

        // Set a specific clock time
        fs.set_clock(1000.0);

        // Create a file
        let handle = fs
            .open("/test.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.close(handle).unwrap();

        // Check timestamps are set
        let meta = fs.metadata("/test.txt").unwrap();
        assert_eq!(meta.atime, 1000.0);
        assert_eq!(meta.mtime, 1000.0);
        assert_eq!(meta.ctime, 1000.0);
    }

    #[test]
    fn test_timestamp_on_read() {
        let mut fs = MemoryFs::new();

        // Create file at time 1000
        fs.set_clock(1000.0);
        let handle = fs
            .open("/test.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.write(handle, b"hello").unwrap();
        fs.close(handle).unwrap();

        // Read file at time 2000
        fs.set_clock(2000.0);
        let handle = fs.open("/test.txt", OpenOptions::new().read(true)).unwrap();
        let mut buf = [0u8; 5];
        fs.read(handle, &mut buf).unwrap();
        fs.close(handle).unwrap();

        // atime should be updated, mtime/ctime unchanged
        let meta = fs.metadata("/test.txt").unwrap();
        assert_eq!(meta.atime, 2000.0);
        assert_eq!(meta.mtime, 1000.0);
        assert_eq!(meta.ctime, 1000.0);
    }

    #[test]
    fn test_timestamp_on_write() {
        let mut fs = MemoryFs::new();

        // Create file at time 1000
        fs.set_clock(1000.0);
        let handle = fs
            .open("/test.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.close(handle).unwrap();

        // Write to file at time 2000
        fs.set_clock(2000.0);
        let handle = fs
            .open("/test.txt", OpenOptions::new().write(true))
            .unwrap();
        fs.write(handle, b"hello").unwrap();
        fs.close(handle).unwrap();

        // mtime and ctime should be updated
        let meta = fs.metadata("/test.txt").unwrap();
        assert_eq!(meta.atime, 1000.0); // unchanged
        assert_eq!(meta.mtime, 2000.0);
        assert_eq!(meta.ctime, 2000.0);
    }

    #[test]
    fn test_timestamp_on_chmod() {
        let mut fs = MemoryFs::new();

        // Create file at time 1000
        fs.set_clock(1000.0);
        create_test_file(&mut fs, "/test.txt");

        // chmod at time 2000
        fs.set_clock(2000.0);
        fs.chmod("/test.txt", 0o755).unwrap();

        // Only ctime should be updated
        let meta = fs.metadata("/test.txt").unwrap();
        assert_eq!(meta.atime, 1000.0);
        assert_eq!(meta.mtime, 1000.0);
        assert_eq!(meta.ctime, 2000.0);
    }

    #[test]
    fn test_timestamp_on_chown() {
        let mut fs = MemoryFs::new();

        // Create file at time 1000
        fs.set_clock(1000.0);
        create_test_file(&mut fs, "/test.txt");

        // chown at time 2000
        fs.set_clock(2000.0);
        fs.chown("/test.txt", Some(0), Some(0)).unwrap();

        // Only ctime should be updated
        let meta = fs.metadata("/test.txt").unwrap();
        assert_eq!(meta.atime, 1000.0);
        assert_eq!(meta.mtime, 1000.0);
        assert_eq!(meta.ctime, 2000.0);
    }

    #[test]
    fn test_timestamp_on_mkdir() {
        let mut fs = MemoryFs::new();

        // Create directory at time 1000
        fs.set_clock(1000.0);
        fs.create_dir("/mydir").unwrap();

        // Check timestamps
        let meta = fs.metadata("/mydir").unwrap();
        assert_eq!(meta.atime, 1000.0);
        assert_eq!(meta.mtime, 1000.0);
        assert_eq!(meta.ctime, 1000.0);
    }

    #[test]
    fn test_timestamp_on_symlink() {
        let mut fs = MemoryFs::new();

        // Create symlink at time 1000
        fs.set_clock(1000.0);
        fs.symlink("/target", "/link").unwrap();

        // Check timestamps
        let meta = fs.metadata("/link").unwrap();
        assert_eq!(meta.atime, 1000.0);
        assert_eq!(meta.mtime, 1000.0);
        assert_eq!(meta.ctime, 1000.0);
    }

    #[test]
    fn test_utimes() {
        let mut fs = MemoryFs::new();

        // Create file at time 1000
        fs.set_clock(1000.0);
        create_test_file(&mut fs, "/test.txt");

        // Set specific times
        fs.set_clock(3000.0);
        fs.utimes("/test.txt", Some(2000.0), Some(2500.0)).unwrap();

        let meta = fs.metadata("/test.txt").unwrap();
        assert_eq!(meta.atime, 2000.0);
        assert_eq!(meta.mtime, 2500.0);
        assert_eq!(meta.ctime, 3000.0); // ctime always updated
    }

    #[test]
    fn test_utimes_with_none() {
        let mut fs = MemoryFs::new();

        // Create file at time 1000
        fs.set_clock(1000.0);
        create_test_file(&mut fs, "/test.txt");

        // Set times to current clock (None = use current time)
        fs.set_clock(2000.0);
        fs.utimes("/test.txt", None, None).unwrap();

        let meta = fs.metadata("/test.txt").unwrap();
        assert_eq!(meta.atime, 2000.0);
        assert_eq!(meta.mtime, 2000.0);
        assert_eq!(meta.ctime, 2000.0);
    }

    #[test]
    fn test_truncate_updates_times() {
        let mut fs = MemoryFs::new();

        // Create file with content at time 1000
        fs.set_clock(1000.0);
        let handle = fs
            .open("/test.txt", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.write(handle, b"hello world").unwrap();
        fs.close(handle).unwrap();

        // Truncate at time 2000
        fs.set_clock(2000.0);
        let handle = fs
            .open("/test.txt", OpenOptions::new().write(true).truncate(true))
            .unwrap();
        fs.close(handle).unwrap();

        // mtime and ctime should be updated
        let meta = fs.metadata("/test.txt").unwrap();
        assert_eq!(meta.atime, 1000.0);
        assert_eq!(meta.mtime, 2000.0);
        assert_eq!(meta.ctime, 2000.0);
    }
}
