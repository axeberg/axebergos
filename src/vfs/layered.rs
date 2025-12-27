//! Layered (Union) filesystem implementation
//!
//! Provides a union mount with a read-only base layer and a writable overlay.
//! Files are read from the upper layer first, falling back to the lower layer.
//! All writes go to the upper layer (copy-on-write semantics).
//! Deletions are tracked via whiteout markers in the upper layer.

use super::{DirEntry, FileHandle, FileSystem, MemoryFs, Metadata, OpenOptions};
use std::collections::HashSet;
use std::io::{self, SeekFrom};

/// Whiteout prefix for marking deleted files
/// A whiteout is a special marker that indicates a file was deleted in the upper layer,
/// hiding it from the lower layer.
const WHITEOUT_PREFIX: &str = ".wh.";

/// Opaque directory marker
/// When a directory is marked opaque, its contents from the lower layer are completely hidden.
const OPAQUE_MARKER: &str = ".wh..wh..opq";

/// A handle tracking which layer owns a file operation
#[derive(Debug, Clone)]
struct LayerHandle {
    /// The underlying handle in the respective layer
    inner_handle: FileHandle,
    /// Which layer this handle belongs to
    layer: Layer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layer {
    Upper,
    Lower,
}

/// Layered filesystem with read-only base and writable overlay
///
/// This implements union mount semantics:
/// - Reads check the upper layer first, falling back to lower
/// - Writes always go to the upper layer (copy-on-write)
/// - Deletions create whiteout markers in the upper layer
/// - Directory listings merge entries from both layers
pub struct LayeredFs {
    /// The upper (writable) layer - typically a MemoryFs
    upper: MemoryFs,
    /// The lower (read-only) layer
    lower: MemoryFs,
    /// Maps our handles to layer-specific handles
    handles: slab::Slab<LayerHandle>,
}

impl LayeredFs {
    /// Create a new layered filesystem
    ///
    /// # Arguments
    /// * `lower` - The read-only base layer
    /// * `upper` - The writable overlay layer (can be empty MemoryFs)
    pub fn new(lower: MemoryFs, upper: MemoryFs) -> Self {
        Self {
            upper,
            lower,
            handles: slab::Slab::new(),
        }
    }

    /// Create a new layered filesystem with an empty upper layer
    pub fn with_base(lower: MemoryFs) -> Self {
        Self::new(lower, MemoryFs::new())
    }

    /// Get a reference to the upper layer
    pub fn upper(&self) -> &MemoryFs {
        &self.upper
    }

    /// Get a mutable reference to the upper layer
    pub fn upper_mut(&mut self) -> &mut MemoryFs {
        &mut self.upper
    }

    /// Get a reference to the lower layer
    pub fn lower(&self) -> &MemoryFs {
        &self.lower
    }

    /// Check if a path has a whiteout marker (is deleted)
    fn is_whiteout(&self, path: &str) -> bool {
        let whiteout_path = Self::whiteout_path(path);
        self.upper.exists(&whiteout_path)
    }

    /// Get the whiteout marker path for a given path
    fn whiteout_path(path: &str) -> String {
        let path = Self::normalize_path(path);
        if let Some(idx) = path.rfind('/') {
            if idx == 0 {
                format!("/{}{}", WHITEOUT_PREFIX, &path[1..])
            } else {
                format!("{}/{}{}", &path[..idx], WHITEOUT_PREFIX, &path[idx + 1..])
            }
        } else {
            format!("{}{}", WHITEOUT_PREFIX, path)
        }
    }

    /// Extract the original filename from a whiteout path
    fn from_whiteout_name(name: &str) -> Option<&str> {
        name.strip_prefix(WHITEOUT_PREFIX)
    }

    /// Check if a name is a whiteout marker
    fn is_whiteout_name(name: &str) -> bool {
        name.starts_with(WHITEOUT_PREFIX)
    }

    /// Check if a directory is marked opaque (hides all lower layer contents)
    fn is_opaque(&self, dir_path: &str) -> bool {
        let path = Self::normalize_path(dir_path);
        let opaque_path = if path == "/" {
            format!("/{}", OPAQUE_MARKER)
        } else {
            format!("{}/{}", path, OPAQUE_MARKER)
        };
        self.upper.exists(&opaque_path)
    }

    /// Normalize a path (ensure leading slash, resolve . and ..)
    fn normalize_path(path: &str) -> String {
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{}", path)
        };

        let mut result: Vec<&str> = Vec::new();
        for component in path.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    result.pop();
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

    /// Get the parent path
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

    /// Determine which layer a path exists in (considering whiteouts)
    fn find_layer(&self, path: &str) -> Option<Layer> {
        let path = Self::normalize_path(path);

        // Check if whited out
        if self.is_whiteout(&path) {
            return None;
        }

        // Check upper layer first
        if self.upper.exists(&path) {
            return Some(Layer::Upper);
        }

        // Check lower layer
        if self.lower.exists(&path) {
            return Some(Layer::Lower);
        }

        None
    }

    /// Copy a file from lower layer to upper layer for modification
    fn copy_up(&mut self, path: &str) -> io::Result<()> {
        let path = Self::normalize_path(path);

        // Already in upper layer?
        if self.upper.exists(&path) {
            return Ok(());
        }

        // Must exist in lower layer
        if !self.lower.exists(&path) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "File not found"));
        }

        // Ensure parent directories exist in upper layer
        if let Some(parent) = Self::parent_path(&path) {
            self.ensure_upper_path(&parent)?;
        }

        // Get metadata from lower
        let meta = self.lower.metadata(&path)?;

        if meta.is_dir {
            // Create directory in upper
            if !self.upper.exists(&path) {
                self.upper.create_dir(&path)?;
            }
        } else if meta.is_symlink {
            // Copy symlink
            let target = self.lower.read_link(&path)?;
            self.upper.symlink(&target, &path)?;
        } else {
            // Copy file contents
            let handle = self.lower.open(&path, OpenOptions::new().read(true))?;
            let mut data = vec![0u8; meta.size as usize];
            self.lower.read(handle, &mut data)?;
            self.lower.close(handle)?;

            // Create file in upper
            let handle = self
                .upper
                .open(&path, OpenOptions::new().write(true).create(true))?;
            self.upper.write(handle, &data)?;
            self.upper.close(handle)?;
        }

        // Copy permissions
        self.upper.chmod(&path, meta.mode)?;
        self.upper.chown(&path, Some(meta.uid), Some(meta.gid))?;

        Ok(())
    }

    /// Ensure all parent directories exist in upper layer
    fn ensure_upper_path(&mut self, path: &str) -> io::Result<()> {
        let path = Self::normalize_path(path);
        if path == "/" {
            return Ok(());
        }

        if self.upper.exists(&path) {
            return Ok(());
        }

        // Recursively ensure parent
        if let Some(parent) = Self::parent_path(&path) {
            self.ensure_upper_path(&parent)?;
        }

        // Create this directory in upper if it exists in lower
        if self
            .lower
            .metadata(&path)
            .map(|m| m.is_dir)
            .unwrap_or(false)
        {
            self.upper.create_dir(&path)?;
        } else if !self.upper.exists(&path) {
            // Parent doesn't exist in either layer
            self.upper.create_dir(&path)?;
        }

        Ok(())
    }

    /// Create a whiteout marker for a path
    fn create_whiteout(&mut self, path: &str) -> io::Result<()> {
        let whiteout_path = Self::whiteout_path(path);

        // Ensure parent directory exists in upper
        if let Some(parent) = Self::parent_path(&whiteout_path) {
            self.ensure_upper_path(&parent)?;
        }

        // Create empty whiteout file
        let handle = self
            .upper
            .open(&whiteout_path, OpenOptions::new().write(true).create(true))?;
        self.upper.close(handle)?;

        Ok(())
    }

    /// Remove a whiteout marker if it exists
    fn remove_whiteout(&mut self, path: &str) -> io::Result<()> {
        let whiteout_path = Self::whiteout_path(path);
        if self.upper.exists(&whiteout_path) {
            self.upper.remove_file(&whiteout_path)?;
        }
        Ok(())
    }
}

impl FileSystem for LayeredFs {
    fn open(&mut self, path: &str, options: OpenOptions) -> io::Result<FileHandle> {
        let path = Self::normalize_path(path);

        // Check for whiteout - if whited out, the file was deleted
        let was_whiteout = self.is_whiteout(&path);
        if was_whiteout {
            if options.create {
                // Remove whiteout and create new file (ignore lower layer)
                self.remove_whiteout(&path)?;
                // Ensure parent exists
                if let Some(parent) = Self::parent_path(&path) {
                    self.ensure_upper_path(&parent)?;
                }
                // Create new file in upper layer directly (don't copy from lower)
                let inner = self.upper.open(&path, options)?;
                let handle = self.handles.insert(LayerHandle {
                    inner_handle: inner,
                    layer: Layer::Upper,
                });
                return Ok(handle);
            } else {
                return Err(io::Error::new(io::ErrorKind::NotFound, "File not found"));
            }
        }

        // Determine layer and handle writes
        let layer = self.find_layer(&path);

        if options.write || options.create || options.truncate {
            // Need write access - must be in upper layer
            match layer {
                Some(Layer::Upper) => {
                    // Already in upper, open directly
                    let inner = self.upper.open(&path, options)?;
                    let handle = self.handles.insert(LayerHandle {
                        inner_handle: inner,
                        layer: Layer::Upper,
                    });
                    Ok(handle)
                }
                Some(Layer::Lower) => {
                    // Copy up first, then open
                    self.copy_up(&path)?;
                    let inner = self.upper.open(&path, options)?;
                    let handle = self.handles.insert(LayerHandle {
                        inner_handle: inner,
                        layer: Layer::Upper,
                    });
                    Ok(handle)
                }
                None => {
                    if options.create {
                        // Ensure parent exists
                        if let Some(parent) = Self::parent_path(&path) {
                            self.ensure_upper_path(&parent)?;
                        }
                        // Create new file in upper
                        let inner = self.upper.open(&path, options)?;
                        let handle = self.handles.insert(LayerHandle {
                            inner_handle: inner,
                            layer: Layer::Upper,
                        });
                        Ok(handle)
                    } else {
                        Err(io::Error::new(io::ErrorKind::NotFound, "File not found"))
                    }
                }
            }
        } else {
            // Read-only access
            match layer {
                Some(Layer::Upper) => {
                    let inner = self.upper.open(&path, options)?;
                    let handle = self.handles.insert(LayerHandle {
                        inner_handle: inner,
                        layer: Layer::Upper,
                    });
                    Ok(handle)
                }
                Some(Layer::Lower) => {
                    let inner = self.lower.open(&path, options)?;
                    let handle = self.handles.insert(LayerHandle {
                        inner_handle: inner,
                        layer: Layer::Lower,
                    });
                    Ok(handle)
                }
                None => Err(io::Error::new(io::ErrorKind::NotFound, "File not found")),
            }
        }
    }

    fn close(&mut self, handle: FileHandle) -> io::Result<()> {
        let layer_handle = self
            .handles
            .try_remove(handle)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid file handle"))?;

        match layer_handle.layer {
            Layer::Upper => self.upper.close(layer_handle.inner_handle),
            Layer::Lower => self.lower.close(layer_handle.inner_handle),
        }
    }

    fn read(&mut self, handle: FileHandle, buf: &mut [u8]) -> io::Result<usize> {
        let layer_handle = self
            .handles
            .get(handle)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid file handle"))?;

        match layer_handle.layer {
            Layer::Upper => self.upper.read(layer_handle.inner_handle, buf),
            Layer::Lower => self.lower.read(layer_handle.inner_handle, buf),
        }
    }

    fn write(&mut self, handle: FileHandle, buf: &[u8]) -> io::Result<usize> {
        let layer_handle = self
            .handles
            .get(handle)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid file handle"))?;

        if layer_handle.layer == Layer::Lower {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Cannot write to lower layer",
            ));
        }

        self.upper.write(layer_handle.inner_handle, buf)
    }

    fn seek(&mut self, handle: FileHandle, pos: SeekFrom) -> io::Result<u64> {
        let layer_handle = self
            .handles
            .get(handle)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid file handle"))?;

        match layer_handle.layer {
            Layer::Upper => self.upper.seek(layer_handle.inner_handle, pos),
            Layer::Lower => self.lower.seek(layer_handle.inner_handle, pos),
        }
    }

    fn metadata(&self, path: &str) -> io::Result<Metadata> {
        let path = Self::normalize_path(path);

        // Check for whiteout
        if self.is_whiteout(&path) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Path not found"));
        }

        // Check upper first, then lower
        if self.upper.exists(&path) {
            self.upper.metadata(&path)
        } else if self.lower.exists(&path) {
            self.lower.metadata(&path)
        } else {
            Err(io::Error::new(io::ErrorKind::NotFound, "Path not found"))
        }
    }

    fn create_dir(&mut self, path: &str) -> io::Result<()> {
        let path = Self::normalize_path(path);

        // Check if whiteout exists (path was deleted)
        let was_whiteout = self.is_whiteout(&path);
        if was_whiteout {
            // Remove whiteout and create fresh directory
            self.remove_whiteout(&path)?;
            // Ensure parent exists in upper
            if let Some(parent) = Self::parent_path(&path) {
                self.ensure_upper_path(&parent)?;
            }
            // Create directory in upper layer (ignore lower layer)
            return self.upper.create_dir(&path);
        }

        // Check if already exists (in either layer)
        if self.exists(&path) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Path already exists",
            ));
        }

        // Ensure parent exists in upper
        if let Some(parent) = Self::parent_path(&path) {
            self.ensure_upper_path(&parent)?;
        }

        self.upper.create_dir(&path)
    }

    fn read_dir(&self, path: &str) -> io::Result<Vec<DirEntry>> {
        let path = Self::normalize_path(path);

        // Check for whiteout on the directory itself
        if self.is_whiteout(&path) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Directory not found",
            ));
        }

        // Verify directory exists
        if !self.exists(&path) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Directory not found",
            ));
        }

        let meta = self.metadata(&path)?;
        if !meta.is_dir {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Not a directory",
            ));
        }

        // Collect whiteouts in this directory
        let mut whiteouts: HashSet<String> = HashSet::new();
        if let Ok(upper_entries) = self.upper.read_dir(&path) {
            for entry in upper_entries {
                if let Some(original) = Self::from_whiteout_name(&entry.name) {
                    whiteouts.insert(original.to_string());
                }
            }
        }

        // Check if directory is opaque
        let is_opaque = self.is_opaque(&path);

        // Collect entries from both layers
        let mut entries: Vec<DirEntry> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // Upper layer entries first (they take precedence)
        if let Ok(upper_entries) = self.upper.read_dir(&path) {
            for entry in upper_entries {
                // Skip whiteout markers and opaque markers
                if Self::is_whiteout_name(&entry.name) || entry.name == OPAQUE_MARKER {
                    continue;
                }
                seen.insert(entry.name.clone());
                entries.push(entry);
            }
        }

        // Lower layer entries (if not opaque, skip whiteouts and duplicates)
        if !is_opaque && let Ok(lower_entries) = self.lower.read_dir(&path) {
            for entry in lower_entries {
                // Skip if already in upper or whited out
                if seen.contains(&entry.name) || whiteouts.contains(&entry.name) {
                    continue;
                }
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    fn remove_file(&mut self, path: &str) -> io::Result<()> {
        let path = Self::normalize_path(path);

        // Check if file exists
        let layer = self.find_layer(&path);
        if layer.is_none() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "File not found"));
        }

        // Verify it's a file or symlink
        let meta = self.metadata(&path)?;
        if meta.is_dir {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Cannot remove directory with remove_file",
            ));
        }

        // Remove from upper if present
        if self.upper.exists(&path) {
            self.upper.remove_file(&path)?;
        }

        // If exists in lower, create whiteout
        if self.lower.exists(&path) {
            self.create_whiteout(&path)?;
        }

        Ok(())
    }

    fn remove_dir(&mut self, path: &str) -> io::Result<()> {
        let path = Self::normalize_path(path);

        if path == "/" {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Cannot remove root directory",
            ));
        }

        // Check if directory exists
        let layer = self.find_layer(&path);
        if layer.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Directory not found",
            ));
        }

        // Verify it's a directory
        let meta = self.metadata(&path)?;
        if !meta.is_dir {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Not a directory",
            ));
        }

        // Check if empty (merged view)
        let entries = self.read_dir(&path)?;
        if !entries.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Directory not empty",
            ));
        }

        // Remove from upper if present
        if self.upper.exists(&path) {
            self.upper.remove_dir(&path)?;
        }

        // If exists in lower, create whiteout
        if self.lower.exists(&path) {
            self.create_whiteout(&path)?;
        }

        Ok(())
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

        // Check source exists
        if self.find_layer(&from).is_none() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Source not found"));
        }

        // Check destination doesn't exist
        if self.exists(&to) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Destination already exists",
            ));
        }

        // Copy up source if needed
        self.copy_up(&from)?;

        // Ensure destination parent exists
        if let Some(parent) = Self::parent_path(&to) {
            self.ensure_upper_path(&parent)?;
        }

        // Remove any whiteout at destination
        self.remove_whiteout(&to)?;

        // Rename in upper layer
        self.upper.rename(&from, &to)?;

        // Create whiteout for source if it exists in lower
        if self.lower.exists(&from) {
            self.create_whiteout(&from)?;
        }

        Ok(())
    }

    fn copy_file(&mut self, from: &str, to: &str) -> io::Result<u64> {
        let from = Self::normalize_path(from);
        let to = Self::normalize_path(to);

        // Check source exists
        let layer = self
            .find_layer(&from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Source not found"))?;

        // Get source metadata
        let meta = self.metadata(&from)?;
        if meta.is_dir {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Cannot copy directory with copy_file",
            ));
        }

        // Ensure destination parent exists
        if let Some(parent) = Self::parent_path(&to) {
            self.ensure_upper_path(&parent)?;
        }

        // Remove any whiteout at destination
        self.remove_whiteout(&to)?;

        // Copy to upper layer
        match layer {
            Layer::Upper => self.upper.copy_file(&from, &to),
            Layer::Lower => {
                // Read from lower, write to upper
                if meta.is_symlink {
                    let target = self.lower.read_link(&from)?;
                    self.upper.symlink(&target, &to)?;
                    Ok(target.len() as u64)
                } else {
                    let handle = self.lower.open(&from, OpenOptions::new().read(true))?;
                    let mut data = vec![0u8; meta.size as usize];
                    self.lower.read(handle, &mut data)?;
                    self.lower.close(handle)?;

                    let handle = self
                        .upper
                        .open(&to, OpenOptions::new().write(true).create(true))?;
                    self.upper.write(handle, &data)?;
                    self.upper.close(handle)?;

                    Ok(meta.size)
                }
            }
        }
    }

    fn exists(&self, path: &str) -> bool {
        let path = Self::normalize_path(path);

        // Check for whiteout
        if self.is_whiteout(&path) {
            return false;
        }

        self.upper.exists(&path) || self.lower.exists(&path)
    }

    fn symlink(&mut self, target: &str, link_path: &str) -> io::Result<()> {
        let link_path = Self::normalize_path(link_path);

        // Remove whiteout if exists
        self.remove_whiteout(&link_path)?;

        // Check if already exists
        if self.exists(&link_path) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Path already exists",
            ));
        }

        // Ensure parent exists in upper
        if let Some(parent) = Self::parent_path(&link_path) {
            self.ensure_upper_path(&parent)?;
        }

        self.upper.symlink(target, &link_path)
    }

    fn read_link(&self, path: &str) -> io::Result<String> {
        let path = Self::normalize_path(path);

        // Check for whiteout
        if self.is_whiteout(&path) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Path not found"));
        }

        // Check upper first, then lower
        if self.upper.exists(&path) {
            self.upper.read_link(&path)
        } else if self.lower.exists(&path) {
            self.lower.read_link(&path)
        } else {
            Err(io::Error::new(io::ErrorKind::NotFound, "Path not found"))
        }
    }

    fn chmod(&mut self, path: &str, mode: u16) -> io::Result<()> {
        let path = Self::normalize_path(path);

        // Check if whited out
        if self.is_whiteout(&path) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Path not found"));
        }

        // Need to copy up before modifying
        if !self.upper.exists(&path) && self.lower.exists(&path) {
            self.copy_up(&path)?;
        }

        if self.upper.exists(&path) {
            self.upper.chmod(&path, mode)
        } else {
            Err(io::Error::new(io::ErrorKind::NotFound, "Path not found"))
        }
    }

    fn chown(&mut self, path: &str, uid: Option<u32>, gid: Option<u32>) -> io::Result<()> {
        let path = Self::normalize_path(path);

        // Check if whited out
        if self.is_whiteout(&path) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Path not found"));
        }

        // Need to copy up before modifying
        if !self.upper.exists(&path) && self.lower.exists(&path) {
            self.copy_up(&path)?;
        }

        if self.upper.exists(&path) {
            self.upper.chown(&path, uid, gid)
        } else {
            Err(io::Error::new(io::ErrorKind::NotFound, "Path not found"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lower() -> MemoryFs {
        let mut fs = MemoryFs::new();
        fs.create_dir("/etc").unwrap();
        fs.create_dir("/usr").unwrap();
        fs.create_dir("/usr/bin").unwrap();

        let handle = fs
            .open("/etc/passwd", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.write(handle, b"root:x:0:0:root:/root:/bin/bash")
            .unwrap();
        fs.close(handle).unwrap();

        let handle = fs
            .open("/etc/hosts", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.write(handle, b"127.0.0.1 localhost").unwrap();
        fs.close(handle).unwrap();

        let handle = fs
            .open("/usr/bin/ls", OpenOptions::new().write(true).create(true))
            .unwrap();
        fs.write(handle, b"#!/bin/sh\necho ls").unwrap();
        fs.close(handle).unwrap();

        fs
    }

    #[test]
    fn test_read_from_lower() {
        let lower = setup_lower();
        let mut layered = LayeredFs::with_base(lower);

        // Should be able to read from lower layer
        let handle = layered
            .open("/etc/passwd", OpenOptions::new().read(true))
            .unwrap();
        let mut buf = vec![0u8; 100];
        let n = layered.read(handle, &mut buf).unwrap();
        layered.close(handle).unwrap();

        assert!(n > 0);
        assert!(String::from_utf8_lossy(&buf[..n]).contains("root"));
    }

    #[test]
    fn test_read_dir_from_lower() {
        let lower = setup_lower();
        let layered = LayeredFs::with_base(lower);

        let entries = layered.read_dir("/etc").unwrap();
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();

        assert!(names.contains(&"passwd"));
        assert!(names.contains(&"hosts"));
    }

    #[test]
    fn test_write_creates_in_upper() {
        let lower = setup_lower();
        let mut layered = LayeredFs::with_base(lower);

        // Create new file
        let handle = layered
            .open("/etc/newfile", OpenOptions::new().write(true).create(true))
            .unwrap();
        layered.write(handle, b"new content").unwrap();
        layered.close(handle).unwrap();

        // Should exist in upper layer
        assert!(layered.upper().exists("/etc/newfile"));
        // Should not exist in lower layer
        assert!(!layered.lower().exists("/etc/newfile"));
    }

    #[test]
    fn test_copy_on_write() {
        let lower = setup_lower();
        let mut layered = LayeredFs::with_base(lower);

        // Modify file from lower
        let handle = layered
            .open("/etc/passwd", OpenOptions::new().write(true).truncate(true))
            .unwrap();
        layered.write(handle, b"modified content").unwrap();
        layered.close(handle).unwrap();

        // Upper should have the file now
        assert!(layered.upper().exists("/etc/passwd"));

        // Read back should show modified content
        let handle = layered
            .open("/etc/passwd", OpenOptions::new().read(true))
            .unwrap();
        let mut buf = vec![0u8; 100];
        let n = layered.read(handle, &mut buf).unwrap();
        layered.close(handle).unwrap();

        assert_eq!(&buf[..n], b"modified content");

        // Lower layer should be unchanged
        let lower_handle = layered
            .lower
            .open("/etc/passwd", OpenOptions::new().read(true))
            .unwrap();
        let mut lower_buf = vec![0u8; 100];
        let lower_n = layered.lower.read(lower_handle, &mut lower_buf).unwrap();
        layered.lower.close(lower_handle).unwrap();
        assert!(String::from_utf8_lossy(&lower_buf[..lower_n]).contains("root"));
    }

    #[test]
    fn test_upper_shadows_lower() {
        let lower = setup_lower();
        let mut upper = MemoryFs::new();

        // Create same path in upper with different content
        upper.create_dir("/etc").unwrap();
        let handle = upper
            .open("/etc/passwd", OpenOptions::new().write(true).create(true))
            .unwrap();
        upper.write(handle, b"shadow content").unwrap();
        upper.close(handle).unwrap();

        let mut layered = LayeredFs::new(lower, upper);

        // Read should return upper layer content
        let handle = layered
            .open("/etc/passwd", OpenOptions::new().read(true))
            .unwrap();
        let mut buf = vec![0u8; 100];
        let n = layered.read(handle, &mut buf).unwrap();
        layered.close(handle).unwrap();

        assert_eq!(&buf[..n], b"shadow content");
    }

    #[test]
    fn test_whiteout_hides_lower() {
        let lower = setup_lower();
        let mut layered = LayeredFs::with_base(lower);

        // File exists initially
        assert!(layered.exists("/etc/passwd"));

        // Delete it
        layered.remove_file("/etc/passwd").unwrap();

        // Should not exist anymore
        assert!(!layered.exists("/etc/passwd"));

        // Whiteout should exist in upper
        assert!(!layered.upper().exists("/.wh.etc/passwd"));
        // Check the correct whiteout path
        assert!(layered.upper().exists("/etc/.wh.passwd"));
    }

    #[test]
    fn test_merged_read_dir() {
        let lower = setup_lower();
        let mut upper = MemoryFs::new();

        // Add a file to upper in /etc
        upper.create_dir("/etc").unwrap();
        let handle = upper
            .open("/etc/shadow", OpenOptions::new().write(true).create(true))
            .unwrap();
        upper.write(handle, b"shadow file").unwrap();
        upper.close(handle).unwrap();

        let layered = LayeredFs::new(lower, upper);

        // Should see files from both layers
        let entries = layered.read_dir("/etc").unwrap();
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();

        assert!(names.contains(&"passwd")); // from lower
        assert!(names.contains(&"hosts")); // from lower
        assert!(names.contains(&"shadow")); // from upper
    }

    #[test]
    fn test_merged_read_dir_with_whiteout() {
        let lower = setup_lower();
        let mut layered = LayeredFs::with_base(lower);

        // Delete a file
        layered.remove_file("/etc/passwd").unwrap();

        // Should not appear in listing
        let entries = layered.read_dir("/etc").unwrap();
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();

        assert!(!names.contains(&"passwd"));
        assert!(names.contains(&"hosts"));
        // Whiteout marker should not appear
        assert!(!names.contains(&".wh.passwd"));
    }

    #[test]
    fn test_create_after_delete() {
        let lower = setup_lower();
        let mut layered = LayeredFs::with_base(lower);

        // Delete file
        layered.remove_file("/etc/passwd").unwrap();
        assert!(!layered.exists("/etc/passwd"));

        // Create new file at same path
        let handle = layered
            .open("/etc/passwd", OpenOptions::new().write(true).create(true))
            .unwrap();
        layered.write(handle, b"new passwd").unwrap();
        layered.close(handle).unwrap();

        // Should exist with new content
        assert!(layered.exists("/etc/passwd"));

        let handle = layered
            .open("/etc/passwd", OpenOptions::new().read(true))
            .unwrap();
        let mut buf = vec![0u8; 100];
        let n = layered.read(handle, &mut buf).unwrap();
        layered.close(handle).unwrap();

        assert_eq!(&buf[..n], b"new passwd");
    }

    #[test]
    fn test_rename_within_layers() {
        let lower = setup_lower();
        let mut layered = LayeredFs::with_base(lower);

        // Rename file from lower
        layered.rename("/etc/passwd", "/etc/passwd.bak").unwrap();

        assert!(!layered.exists("/etc/passwd"));
        assert!(layered.exists("/etc/passwd.bak"));

        // Old location should be whited out
        assert!(layered.upper().exists("/etc/.wh.passwd"));
        // New location should be in upper
        assert!(layered.upper().exists("/etc/passwd.bak"));
    }

    #[test]
    fn test_copy_file_lower_to_upper() {
        let lower = setup_lower();
        let mut layered = LayeredFs::with_base(lower);

        let size = layered
            .copy_file("/etc/passwd", "/etc/passwd.copy")
            .unwrap();
        assert!(size > 0);

        // Both should exist
        assert!(layered.exists("/etc/passwd"));
        assert!(layered.exists("/etc/passwd.copy"));

        // Copy should be in upper
        assert!(layered.upper().exists("/etc/passwd.copy"));
    }

    #[test]
    fn test_metadata_from_layers() {
        let lower = setup_lower();
        let mut layered = LayeredFs::with_base(lower);

        // Get metadata from lower
        let meta = layered.metadata("/etc/passwd").unwrap();
        assert!(meta.is_file);
        assert!(meta.size > 0);

        // Create file in upper
        let handle = layered
            .open("/etc/new", OpenOptions::new().write(true).create(true))
            .unwrap();
        layered.write(handle, b"12345").unwrap();
        layered.close(handle).unwrap();

        let meta = layered.metadata("/etc/new").unwrap();
        assert!(meta.is_file);
        assert_eq!(meta.size, 5);
    }

    #[test]
    fn test_remove_dir_empty() {
        let lower = setup_lower();
        let mut layered = LayeredFs::with_base(lower);

        // Create empty dir in upper
        layered.create_dir("/empty").unwrap();

        // Remove it
        layered.remove_dir("/empty").unwrap();
        assert!(!layered.exists("/empty"));
    }

    #[test]
    fn test_remove_dir_from_lower() {
        let mut lower = MemoryFs::new();
        lower.create_dir("/emptydir").unwrap();
        let mut layered = LayeredFs::with_base(lower);

        // Remove directory that only exists in lower
        layered.remove_dir("/emptydir").unwrap();

        // Should not exist
        assert!(!layered.exists("/emptydir"));

        // Whiteout should exist
        assert!(layered.upper().exists("/.wh.emptydir"));
    }

    #[test]
    fn test_symlink_in_layered() {
        let lower = setup_lower();
        let mut layered = LayeredFs::with_base(lower);

        // Create symlink in upper
        layered.symlink("/etc/passwd", "/etc/passwd.link").unwrap();

        assert!(layered.exists("/etc/passwd.link"));
        let target = layered.read_link("/etc/passwd.link").unwrap();
        assert_eq!(target, "/etc/passwd");

        // Should be in upper
        assert!(layered.upper().exists("/etc/passwd.link"));
    }

    #[test]
    fn test_symlink_from_lower() {
        let mut lower = MemoryFs::new();
        lower.create_dir("/etc").unwrap();
        lower.symlink("/target", "/etc/mylink").unwrap();

        let layered = LayeredFs::with_base(lower);

        assert!(layered.exists("/etc/mylink"));
        let meta = layered.metadata("/etc/mylink").unwrap();
        assert!(meta.is_symlink);

        let target = layered.read_link("/etc/mylink").unwrap();
        assert_eq!(target, "/target");
    }

    #[test]
    fn test_chmod_triggers_copy_up() {
        let lower = setup_lower();
        let mut layered = LayeredFs::with_base(lower);

        // chmod on lower layer file
        layered.chmod("/etc/passwd", 0o600).unwrap();

        // Should be copied up
        assert!(layered.upper().exists("/etc/passwd"));

        let meta = layered.metadata("/etc/passwd").unwrap();
        assert_eq!(meta.mode, 0o600);
    }

    #[test]
    fn test_chown_triggers_copy_up() {
        let lower = setup_lower();
        let mut layered = LayeredFs::with_base(lower);

        // chown on lower layer file
        layered.chown("/etc/passwd", Some(0), Some(0)).unwrap();

        // Should be copied up
        assert!(layered.upper().exists("/etc/passwd"));

        let meta = layered.metadata("/etc/passwd").unwrap();
        assert_eq!(meta.uid, 0);
        assert_eq!(meta.gid, 0);
    }

    #[test]
    fn test_create_dir_removes_whiteout() {
        let mut lower = MemoryFs::new();
        lower.create_dir("/mydir").unwrap();

        let mut layered = LayeredFs::with_base(lower);

        // Remove dir (creates whiteout)
        layered.remove_dir("/mydir").unwrap();
        assert!(!layered.exists("/mydir"));

        // Create dir again (should remove whiteout)
        layered.create_dir("/mydir").unwrap();
        assert!(layered.exists("/mydir"));

        // Whiteout should be gone
        assert!(!layered.upper().exists("/.wh.mydir"));
    }

    #[test]
    fn test_nested_path_copy_up() {
        let lower = setup_lower();
        let mut layered = LayeredFs::with_base(lower);

        // Modify nested file
        let handle = layered
            .open("/usr/bin/ls", OpenOptions::new().write(true).truncate(true))
            .unwrap();
        layered.write(handle, b"modified ls").unwrap();
        layered.close(handle).unwrap();

        // Parent directories should be created in upper
        assert!(layered.upper().exists("/usr"));
        assert!(layered.upper().exists("/usr/bin"));
        assert!(layered.upper().exists("/usr/bin/ls"));
    }

    #[test]
    fn test_not_found_errors() {
        let lower = setup_lower();
        let mut layered = LayeredFs::with_base(lower);

        // Non-existent file
        assert!(layered.metadata("/nonexistent").is_err());
        assert!(
            layered
                .open("/nonexistent", OpenOptions::new().read(true))
                .is_err()
        );

        // Whited out file
        layered.remove_file("/etc/passwd").unwrap();
        assert!(layered.metadata("/etc/passwd").is_err());
        assert!(
            layered
                .open("/etc/passwd", OpenOptions::new().read(true))
                .is_err()
        );
    }
}
