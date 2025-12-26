# ADR-003: In-Memory Filesystem

## Status
Accepted

## Context

We need a filesystem for the OS. Browser storage options:

1. **localStorage**: Simple key-value, 5-10MB limit, synchronous
2. **IndexedDB**: Async, larger storage, complex API
3. **Origin Private File System (OPFS)**: Filesystem-like, fast, large
4. **In-memory**: Fast, simple, no persistence

Requirements:
- Unix-like paths and operations
- Reasonable performance
- Ideally some persistence
- Ability to add virtual filesystems (/proc, /dev, /sys)

## Decision

We will use an **in-memory filesystem** as the primary implementation, with optional OPFS persistence for saving/restoring state.

```rust
pub struct MemoryFs {
    root: Directory,
}

struct Directory {
    entries: HashMap<String, Entry>,
    metadata: Metadata,
}

enum Entry {
    File(FileData),
    Directory(Directory),
    Symlink(String),
}
```

## Consequences

### Positive

1. **Simple implementation**: HashMap-based tree is easy to understand
2. **Fast**: All operations are memory operations
3. **Flexible**: Easy to add virtual filesystems
4. **No async complexity**: Synchronous operations simplify code
5. **Isolation**: Each session starts fresh (feature, not bug)
6. **Testable**: Easy to set up test fixtures

### Negative

1. **No persistence by default**: Data lost on page refresh
2. **Memory limited**: Large files consume browser memory
3. **Not a real filesystem**: No journaling, no block devices

### Mitigated

1. **Persistence**: OPFS snapshot/restore for persistence
2. **Memory**: Practical limit is browser's memory (plenty for demos)

## Alternatives Considered

### 1. IndexedDB-backed
- **Pro**: Persistence, large storage
- **Con**: Async-only API, complex to wrap in sync interface, slow for small operations

### 2. OPFS as primary
- **Pro**: Fast, filesystem-like API, persistent
- **Con**: Still async, browser support varies, more complex

### 3. localStorage-backed
- **Pro**: Simple, synchronous
- **Con**: 5-10MB limit, string-only values

### 4. Emscripten FS
- **Pro**: Mature, many backends
- **Con**: C-based, doesn't fit Rust idioms

## Implementation Details

### Virtual Filesystems

The VFS trait allows plugging in special filesystems:

```rust
pub trait VirtualFs {
    fn read(&self, path: &str) -> Result<Vec<u8>>;
    fn write(&mut self, path: &str, data: &[u8]) -> Result<()>;
    // ...
}

// Mount points
/proc  → ProcFs (process info)
/dev   → DevFs  (devices: null, zero, random, tty)
/sys   → SysFs  (kernel info)
```

### Persistence Strategy

Save/restore entire filesystem to OPFS:

```rust
impl MemoryFs {
    pub async fn save(&self) -> Result<()> {
        let snapshot = self.serialize();
        opfs::write("axeberg_fs.json", &snapshot).await
    }

    pub async fn restore() -> Result<Self> {
        let data = opfs::read("axeberg_fs.json").await?;
        Self::deserialize(&data)
    }
}
```

### File Metadata

Full Unix-style metadata:

```rust
pub struct Metadata {
    pub mode: u16,      // Permission bits
    pub uid: Uid,       // Owner
    pub gid: Gid,       // Group
    pub size: u64,
    pub atime: u64,     // Access time
    pub mtime: u64,     // Modify time
    pub ctime: u64,     // Change time
    pub kind: FileType,
}
```

## Lessons Learned

1. Start with in-memory, add persistence later
2. The VFS trait abstraction was worth it for virtual filesystems
3. JSON serialization is good enough for snapshots
4. Symlinks were tricky to get right (resolution loops, relative paths)
