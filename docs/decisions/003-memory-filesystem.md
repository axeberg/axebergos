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
// From src/vfs/memory.rs
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

The FileSystem trait allows plugging in special filesystems:

```rust
pub trait FileSystem {
    fn open(&mut self, path: &str, options: OpenOptions) -> io::Result<FileHandle>;
    fn read(&mut self, handle: FileHandle, buf: &mut [u8]) -> io::Result<usize>;
    fn write(&mut self, handle: FileHandle, data: &[u8]) -> io::Result<usize>;
    // ...
}

// Mount points
/proc  → ProcFs (process info)
/dev   → DevFs  (devices: null, zero, random, tty)
/sys   → SysFs  (kernel info)
```

### Persistence Strategy

Snapshot/restore via serialization (from `src/vfs/memory.rs`):

```rust
#[derive(Serialize, Deserialize)]
pub struct FsSnapshot {
    nodes: HashMap<String, Node>,
    meta: HashMap<String, NodeMeta>,
    version: u32,
}

impl MemoryFs {
    pub fn snapshot(&self) -> FsSnapshot { ... }
    pub fn restore(snapshot: FsSnapshot) -> Self { ... }
}
```

Persistence to OPFS is handled by `src/vfs/persist.rs`.

## Lessons Learned

1. Start with in-memory, add persistence later
2. The FileSystem trait abstraction was worth it for virtual filesystems
3. JSON serialization is good enough for snapshots
4. Symlinks were tricky to get right (resolution loops, relative paths)
