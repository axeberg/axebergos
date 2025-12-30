# Layered Filesystem

A union filesystem with copy-on-write semantics.

## Overview

LayeredFs provides overlay mount functionality: a writable upper layer on top of a read-only lower layer. Changes are isolated to the upper layer while the lower layer remains immutable.

```
┌─────────────────────────────────────────────────────────────┐
│                     Unified View                            │
│  /                                                          │
│  ├── bin/         (from lower)                              │
│  ├── etc/         (merged: lower + upper modifications)     │
│  ├── home/        (from upper, created after mount)         │
│  └── tmp/         (from upper)                              │
└─────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┴───────────────┐
              ▼                               ▼
┌──────────────────────────┐     ┌─────────────────────────┐
│     Upper (writable)     │     │     Lower (read-only)   │
│  /etc/passwd (modified)  │     │  /bin/cat               │
│  /home/alice/            │     │  /etc/passwd (original) │
│  /.wh.deleted_file       │     │  /etc/shadow            │
└──────────────────────────┘     └─────────────────────────┘
```

## Semantics

### Read Operations

1. Check upper layer first
2. If not found (and no whiteout), check lower layer
3. Return first match

```
read("/etc/passwd")
    │
    ├─ Upper has /etc/passwd? ──Yes──► Return upper version
    │
    ├─ Upper has /.wh.etc/passwd? ──Yes──► Return NotFound
    │
    └─ Lower has /etc/passwd? ──Yes──► Return lower version
                              │
                              └─No──► Return NotFound
```

### Write Operations

All writes go to upper layer (copy-on-write):

```
write("/etc/passwd", data)
    │
    ├─ File exists in upper? ──Yes──► Modify in place
    │
    └─ File exists in lower? ──Yes──► Copy to upper, then modify
                             │
                             └─No──► Create in upper
```

### Delete Operations

Deletions create whiteout markers:

```
delete("/etc/shadow")
    │
    ├─ Create /.wh.etc/shadow in upper (whiteout marker)
    │
    └─ File now appears deleted from unified view
```

## Whiteout Markers

Special files that mark deletions:

| Marker | Purpose |
|--------|---------|
| `.wh.<name>` | File/dir `<name>` is deleted |
| `.wh..wh..opq` | Directory is opaque (hide lower contents) |

## Usage

```rust
use axeberg::vfs::{LayeredFs, MemoryFs};

// Create base layer with initial content
let mut lower = MemoryFs::new();
lower.create_dir("/bin")?;
lower.write_file("/bin/cat", b"...")?;
lower.write_file("/etc/passwd", b"root:x:0:0::/root:/bin/sh")?;

// Create empty upper layer
let upper = MemoryFs::new();

// Combine into layered filesystem
let mut fs = LayeredFs::new(lower, upper);

// Reads come from lower
let data = fs.read_file("/bin/cat")?;

// Writes go to upper (copy-on-write)
fs.write_file("/etc/passwd", b"modified")?;

// Original lower layer unchanged
assert_eq!(fs.lower().read_file("/etc/passwd")?, b"root:x:0:0::/root:/bin/sh");

// Upper has the modification
assert_eq!(fs.upper().read_file("/etc/passwd")?, b"modified");
```

## Directory Listing

Merged from both layers, excluding whiteouts:

```rust
// Lower: /dir/a, /dir/b
// Upper: /dir/c, /.wh.dir/b

fs.read_dir("/dir")?
// Returns: [a, c]
// (b is hidden by whiteout)
```

## Opaque Directories

Mark a directory opaque to hide all lower layer contents:

```rust
// Create opaque marker
fs.write_file("/etc/.wh..wh..opq", b"")?;

// Now /etc only shows upper layer contents
// Lower layer /etc/* completely hidden
```

## API

```rust
impl LayeredFs {
    /// Create with both layers
    pub fn new(lower: MemoryFs, upper: MemoryFs) -> Self;

    /// Create with empty upper layer
    pub fn with_base(lower: MemoryFs) -> Self;

    /// Access upper layer directly
    pub fn upper(&self) -> &MemoryFs;
    pub fn upper_mut(&mut self) -> &mut MemoryFs;

    /// Access lower layer directly
    pub fn lower(&self) -> &MemoryFs;
}
```

## FileSystem Trait Implementation

LayeredFs implements the full FileSystem trait:

| Method | Behavior |
|--------|----------|
| `open` | Upper first, then lower |
| `read` | From opened handle's layer |
| `write` | Copy-up if needed, write to upper |
| `create_dir` | Always in upper |
| `remove_file` | Create whiteout |
| `remove_dir` | Create whiteout |
| `metadata` | Upper first, then lower |
| `read_dir` | Merge, exclude whiteouts |
| `chmod` | Copy-up if needed |
| `chown` | Copy-up if needed |

## Use Cases

### Container Filesystems

```
┌─────────────────────────────────────────┐
│           Container View                │
└─────────────────────────────────────────┘
                    │
    ┌───────────────┼───────────────┐
    ▼               ▼               ▼
┌───────┐     ┌───────────┐   ┌───────────┐
│ Upper │     │  Image    │   │   Base    │
│(r/w)  │     │  Layer    │   │   Image   │
│       │     │  (r/o)    │   │   (r/o)   │
└───────┘     └───────────┘   └───────────┘
```

### Safe Experimentation

```rust
// Experiment without modifying base
let base = load_system_fs();
let scratch = MemoryFs::new();
let mut sandbox = LayeredFs::new(base, scratch);

// All changes isolated to scratch layer
sandbox.write_file("/etc/config", b"experimental")?;

// Discard changes by dropping sandbox
// Base filesystem unchanged
```

### Snapshot/Restore

```rust
// Take snapshot by cloning upper layer
let snapshot = fs.upper().clone();

// Later: restore by replacing upper
*fs.upper_mut() = snapshot;
```

## Limitations

- No true hard links across layers
- Whiteouts consume space in upper layer
- No deduplication between layers
- Copy-up copies entire file (no block-level COW)

## Related Documentation

- [VFS](vfs.md) - Virtual filesystem overview
- [Memory](../kernel/memory.md) - Memory management
