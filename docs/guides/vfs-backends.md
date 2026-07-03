# Implementing VFS Backends

Guide to creating custom filesystem implementations.

## FileSystem Trait

All filesystems implement this trait (verbatim from `src/vfs/mod.rs`, which
is authoritative):

```rust
pub trait FileSystem {
    // File operations
    fn open(&mut self, path: &str, options: OpenOptions) -> io::Result<FileHandle>;
    fn close(&mut self, handle: FileHandle) -> io::Result<()>;
    fn read(&mut self, handle: FileHandle, buf: &mut [u8]) -> io::Result<usize>;
    fn write(&mut self, handle: FileHandle, buf: &[u8]) -> io::Result<usize>;
    fn seek(&mut self, handle: FileHandle, pos: io::SeekFrom) -> io::Result<u64>;

    // Metadata
    fn metadata(&self, path: &str) -> io::Result<Metadata>;
    fn fstat(&self, handle: FileHandle) -> io::Result<Metadata>;
    fn handle_path(&self, handle: FileHandle) -> io::Result<String>;
    fn exists(&self, path: &str) -> bool;

    // Directory operations
    fn create_dir(&mut self, path: &str) -> io::Result<()>;
    fn read_dir(&self, path: &str) -> io::Result<Vec<DirEntry>>;
    fn remove_file(&mut self, path: &str) -> io::Result<()>;
    fn remove_dir(&mut self, path: &str) -> io::Result<()>;

    // Move / copy
    fn rename(&mut self, from: &str, to: &str) -> io::Result<()>;
    fn copy_file(&mut self, from: &str, to: &str) -> io::Result<u64>;

    // Links
    fn symlink(&mut self, target: &str, link_path: &str) -> io::Result<()>;
    fn read_link(&self, path: &str) -> io::Result<String>;
    fn link(&mut self, source: &str, dest: &str) -> io::Result<()>;

    // Permissions / ownership
    fn chmod(&mut self, path: &str, mode: u16) -> io::Result<()>;
    fn chown(&mut self, path: &str, uid: Option<u32>, gid: Option<u32>) -> io::Result<()>;

    // Timestamps
    fn set_clock(&mut self, now: f64);
    fn utimes(&mut self, path: &str, atime: Option<f64>, mtime: Option<f64>) -> io::Result<()>;
}
```

Note: there is no `truncate(handle, len)` method — truncation happens via
`OpenOptions::truncate` at open time.

## Minimal Implementation

```rust
use axeberg::vfs::{FileSystem, FileHandle, OpenOptions, Metadata, DirEntry};
use std::io::{self, SeekFrom};
use std::collections::HashMap;

pub struct SimpleFs {
    files: HashMap<String, Vec<u8>>,
    handles: slab::Slab<OpenHandle>,
}

struct OpenHandle {
    path: String,
    position: usize,
    writable: bool,
}

impl SimpleFs {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            handles: slab::Slab::new(),
        }
    }
}

impl FileSystem for SimpleFs {
    fn open(&mut self, path: &str, opts: OpenOptions) -> io::Result<FileHandle> {
        let path = normalize_path(path);

        // Create if needed
        if opts.create && !self.files.contains_key(&path) {
            self.files.insert(path.clone(), Vec::new());
        }

        // Check exists
        if !self.files.contains_key(&path) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "file not found"));
        }

        // Truncate if requested
        if opts.truncate {
            self.files.insert(path.clone(), Vec::new());
        }

        let handle = self.handles.insert(OpenHandle {
            path,
            position: 0,
            writable: opts.write,
        });

        Ok(FileHandle(handle))
    }

    fn close(&mut self, handle: FileHandle) -> io::Result<()> {
        self.handles.try_remove(handle.0)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "bad handle"))?;
        Ok(())
    }

    fn read(&mut self, handle: FileHandle, buf: &mut [u8]) -> io::Result<usize> {
        let h = self.handles.get_mut(handle.0)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "bad handle"))?;

        let data = self.files.get(&h.path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "file gone"))?;

        let available = data.len().saturating_sub(h.position);
        let to_read = buf.len().min(available);

        buf[..to_read].copy_from_slice(&data[h.position..h.position + to_read]);
        h.position += to_read;

        Ok(to_read)
    }

    fn write(&mut self, handle: FileHandle, buf: &[u8]) -> io::Result<usize> {
        let h = self.handles.get_mut(handle.0)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "bad handle"))?;

        if !h.writable {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "not writable"));
        }

        let path = h.path.clone();
        let pos = h.position;

        let data = self.files.get_mut(&path)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "file gone"))?;

        // Extend if needed
        if pos + buf.len() > data.len() {
            data.resize(pos + buf.len(), 0);
        }

        data[pos..pos + buf.len()].copy_from_slice(buf);

        self.handles.get_mut(handle.0).unwrap().position += buf.len();

        Ok(buf.len())
    }

    // ... implement remaining methods
}
```

## Metadata Structure

```rust
pub struct Metadata {
    pub size: u64,
    pub is_dir: bool,
    pub is_file: bool,
    pub is_symlink: bool,
    pub symlink_target: Option<String>,
    pub uid: u32,
    pub gid: u32,
    pub mode: u16,          // Unix permissions (0o755, etc.)
    pub atime: f64,         // Access time (ms since epoch)
    pub mtime: f64,         // Modification time
    pub ctime: f64,         // Change time
    pub nlink: u32,         // Hard link count
}
```

## OpenOptions

```rust
pub struct OpenOptions {
    pub read: bool,
    pub write: bool,
    pub create: bool,
    pub truncate: bool,
}

impl OpenOptions {
    pub fn new() -> Self;                       // defaults: read = true, others false
    pub fn read(self, read: bool) -> Self;      // builder methods chain
    pub fn write(self, write: bool) -> Self;
    pub fn create(self, create: bool) -> Self;
    pub fn truncate(self, truncate: bool) -> Self;
}
```

Construct options with the builder, e.g.
`OpenOptions::new().write(true).create(true).truncate(true)`.

## Example: Read-Only Archive FS

```rust
pub struct ArchiveFs {
    entries: HashMap<String, ArchiveEntry>,
    handles: slab::Slab<ArchiveHandle>,
}

struct ArchiveEntry {
    data: Vec<u8>,
    metadata: Metadata,
}

impl ArchiveFs {
    pub fn from_tar(data: &[u8]) -> io::Result<Self> {
        let mut entries = HashMap::new();

        // Parse tar archive
        for entry in tar::Archive::new(data).entries()? {
            let entry = entry?;
            let path = entry.path()?.to_string_lossy().into_owned();
            let mut data = Vec::new();
            entry.read_to_end(&mut data)?;

            entries.insert(path, ArchiveEntry {
                data,
                metadata: /* from tar header */,
            });
        }

        Ok(Self { entries, handles: slab::Slab::new() })
    }
}

impl FileSystem for ArchiveFs {
    fn write(&mut self, _: FileHandle, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::PermissionDenied, "read-only"))
    }

    // ... other methods
}
```

## Integration

Register filesystem with kernel:

```rust
// In kernel initialization
let my_fs = MyCustomFs::new();
kernel.mount("/custom", Box::new(my_fs));
```

Or use with LayeredFs:

```rust
let base = ArchiveFs::from_tar(system_image)?;
let overlay = MemoryFs::new();
let fs = LayeredFs::new(base, overlay);
```

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_write() {
        let mut fs = SimpleFs::new();

        // Write
        let h = fs
            .open("/test.txt", OpenOptions::new().write(true).create(true).truncate(true))
            .unwrap();
        fs.write(h, b"hello").unwrap();
        fs.close(h).unwrap();

        // Read back
        let h = fs.open("/test.txt", OpenOptions::new().read(true)).unwrap();
        let mut buf = [0u8; 100];
        let n = fs.read(h, &mut buf).unwrap();
        assert_eq!(&buf[..n], b"hello");
    }
}
```

## Best Practices

1. **Normalize paths**: Always normalize to absolute, canonical form
2. **Handle handles**: Use slab or similar for O(1) handle lookup
3. **Atomic operations**: Metadata changes should be atomic
4. **Error mapping**: Map backend errors to appropriate io::ErrorKind
5. **Timestamps**: Update atime/mtime/ctime appropriately

## Related Documentation

- [VFS](../userspace/vfs.md) - VFS overview
- [Layered FS](../userspace/layered-fs.md) - Union filesystem
- [Memory](../kernel/memory.md) - Memory-backed storage
