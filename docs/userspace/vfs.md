# Virtual File System

The VFS provides a unified file interface over different storage backends.

## Architecture

```
┌──────────────────────────────────────────┐
│              Syscall Layer               │
│    open, read, write, close, mkdir...    │
└────────────────────┬─────────────────────┘
                     │
┌────────────────────▼─────────────────────┐
│              VFS Trait                   │
│         FileSystem interface             │
└────────────────────┬─────────────────────┘
                     │
     ┌───────────────┼───────────────┐
     ▼               ▼               ▼
┌─────────┐   ┌─────────────┐   ┌─────────┐
│MemoryFs │   │ Persistence │   │ (Other) │
│         │◄──│   (OPFS)    │   │         │
└─────────┘   └─────────────┘   └─────────┘
```

## FileSystem Trait

All backends implement this trait. The definition below is the full method
set from `src/vfs/mod.rs`, which is the authoritative source:

```rust
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

    /// Get file metadata by path
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
    fn link(&mut self, source: &str, dest: &str) -> io::Result<()>;

    /// Change file mode (permissions)
    fn chmod(&mut self, path: &str, mode: u16) -> io::Result<()>;

    /// Change file owner (uid/gid; None leaves that field unchanged)
    fn chown(&mut self, path: &str, uid: Option<u32>, gid: Option<u32>) -> io::Result<()>;

    /// Get metadata for an open handle (fstat)
    fn fstat(&self, handle: FileHandle) -> io::Result<Metadata>;

    /// Get the resolved path for an open handle
    fn handle_path(&self, handle: FileHandle) -> io::Result<String>;

    /// Set the filesystem clock used for timestamp updates
    fn set_clock(&mut self, now: f64);

    /// Update access and modification times (None = use current clock)
    fn utimes(&mut self, path: &str, atime: Option<f64>, mtime: Option<f64>) -> io::Result<()>;
}
```

## MemoryFs

The current in-memory implementation (from `src/vfs/memory.rs`):

```rust
pub struct MemoryFs {
    nodes: HashMap<String, Node>,
    meta: HashMap<String, NodeMeta>,
    handles: Slab<OpenFile>,
}

enum Node {
    File(Vec<u8>),
    Directory,
    Symlink(String),
}

struct NodeMeta {
    uid: u32,
    gid: u32,
    mode: u16,
}
```

### Characteristics

- **Fast**: No I/O latency (in-memory operations)
- **Simple**: Easy to understand and debug
- **Persistent**: Serializes to OPFS via `Persistence` module (see `src/vfs/persist.rs`)
- **Unlimited**: Only bound by browser memory

### Path Handling

Paths are normalized:
- `/foo/bar/../baz` → `/foo/baz`
- `//foo//bar` → `/foo/bar`
- Trailing slashes removed

## Open Options

```rust
pub struct OpenOptions {
    pub read: bool,    // Open for reading
    pub write: bool,   // Open for writing
    pub create: bool,  // Create if doesn't exist
    pub truncate: bool, // Truncate existing content
}

impl OpenOptions {
    pub fn new() -> Self;
    pub fn read(mut self, read: bool) -> Self;
    pub fn write(mut self, write: bool) -> Self;
    pub fn create(mut self, create: bool) -> Self;
    pub fn truncate(mut self, truncate: bool) -> Self;
}
```

## File Metadata

```rust
pub struct Metadata {
    pub size: u64,
    pub is_dir: bool,
    pub is_file: bool,
    pub is_symlink: bool,
    pub symlink_target: Option<String>,
    pub uid: u32,             // Owner user ID
    pub gid: u32,             // Owner group ID
    pub mode: u16,            // Unix permission mode (0o755, etc.)
    pub atime: f64,           // Access time (ms since epoch)
    pub mtime: f64,           // Modification time
    pub ctime: f64,           // Change time (metadata change)
    pub nlink: u32,           // Hard link count
}
```

Timestamps are updated automatically:
- **atime**: Updated on read
- **mtime**: Updated on write/truncate
- **ctime**: Updated on any metadata change (chmod, chown, write)

## Directory Entry

```rust
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
}
```

## Standard Directories

Created at boot:
- `/dev` - Device files
- `/home` - User home directories
- `/tmp` - Temporary files
- `/etc` - Configuration

## Special Paths

### /dev/console

The system console:
- Read: keyboard input
- Write: terminal output

### /dev/null

Null device:
- Read: always EOF
- Write: discarded

### /dev/zero

Zero device:
- Read: infinite zeros
- Write: discarded

## Usage Examples

### Reading a File

```rust
// Via syscall
let fd = syscall::open("/home/user/file.txt", OpenFlags::READ)?;
let mut buf = [0u8; 1024];
let n = syscall::read(fd, &mut buf)?;
let content = String::from_utf8_lossy(&buf[..n]);
syscall::close(fd)?;
```

### Writing a File

```rust
let fd = syscall::open("/tmp/output.txt", OpenFlags::WRITE)?;
syscall::write(fd, b"Hello, world!")?;
syscall::close(fd)?;
```

### Creating Directories

```rust
syscall::mkdir("/home/user/documents")?;
syscall::mkdir("/home/user/documents/projects")?;
```

### Listing Directories

```rust
let entries = syscall::readdir("/home/user")?;
for name in entries {
    println!("{}", name);
}
```

### Checking Existence

```rust
if syscall::exists("/tmp/cache")? {
    // Use cached data
} else {
    // Generate and cache
}
```

## Convenience Functions

```rust
/// Read entire file to string
pub fn read_to_string<F: FileSystem>(fs: &mut F, path: &str) -> io::Result<String>

/// Write string to file
pub fn write_string<F: FileSystem>(fs: &mut F, path: &str, content: &str) -> io::Result<()>
```

## VFS Integration with Kernel

The kernel wraps VFS operations:

1. Syscall validates arguments
2. Path is resolved relative to process cwd
3. VFS operation is performed
4. FileObject created in ObjectTable
5. Fd returned to process

```rust
pub fn sys_open(&mut self, path: &str, flags: OpenFlags) -> SyscallResult<Fd> {
    let current = self.current.ok_or(SyscallError::NoProcess)?;
    let resolved = self.resolve_path(current, path)?;

    if resolved.starts_with("/dev/") {
        self.open_device(&resolved, flags)
    } else {
        self.open_file(&resolved, flags)
    }
}
```

## File Syncing

Files are synced to VFS on close:

```rust
pub fn sys_close(&mut self, fd: Fd) -> SyscallResult<()> {
    // Sync file content back to VFS
    if let Some(KernelObject::File(_)) = self.objects.get(handle) {
        self.sync_file(handle)?;
    }

    // Release the handle
    self.objects.release(handle);
    Ok(())
}
```

## Persistence

The `Persistence` module (`src/vfs/persist.rs`) provides OPFS-backed storage:

```rust
impl Persistence {
    /// Save filesystem to OPFS
    pub async fn save(fs: &MemoryFs) -> Result<(), String>;

    /// Load filesystem from OPFS
    pub async fn load() -> Result<Option<MemoryFs>, String>;

    /// Check if OPFS is available
    pub async fn is_available() -> bool;

    /// Clear persisted data
    pub async fn clear() -> Result<(), String>;
}
```

Benefits:
- Persistent across sessions
- Larger storage quota than localStorage
- Async operations via wasm-bindgen-futures

## Related Documentation

- [Syscall Interface](../kernel/syscalls.md) - File syscalls
- [Kernel Objects](../kernel/objects.md) - FileObject details
- [Work Tracker](../WORK_TRACKER.md) - All work items and planned enhancements
