# Memory Management

axeberg provides memory accounting, allocation tracking, shared memory for inter-process communication, and copy-on-write (COW) for efficient process forking.

## Design Philosophy

In WASM, we cannot provide hardware-level memory isolation (no MMU, no page tables). Instead, we provide:

1. **Tracking**: Know exactly what's allocated and by whom
2. **Limits**: Prevent runaway processes from consuming all memory
3. **Shared Memory**: Efficient zero-copy IPC
4. **Visibility**: Full insight into system memory state
5. **Copy-on-Write**: Efficient fork without copying memory upfront

## Memory Regions

A region is a tracked memory allocation:

```rust
pub struct MemoryRegion {
    /// Unique identifier
    pub id: RegionId,

    /// Size in bytes
    pub size: usize,

    /// Protection flags
    pub protection: Protection,

    /// The actual data
    data: Vec<u8>,

    /// Is this region part of shared memory?
    shared: Option<ShmId>,
}
```

### Protection Flags

```rust
pub struct Protection {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl Protection {
    pub const NONE: Protection;
    pub const READ: Protection;
    pub const READ_WRITE: Protection;
    pub const READ_EXEC: Protection;
}
```

Protection is enforced at the syscall level:

```rust
// Reading from read-only is OK
region.read(offset, &mut buf)?;

// Writing to read-only fails
region.write(offset, &buf);  // Err(PermissionDenied)
```

### Region Operations

```rust
// Allocate a region
let region = mem_alloc(1024, Protection::READ_WRITE)?;

// Write data
mem_write(region, 0, b"hello")?;

// Read data
let mut buf = [0u8; 10];
mem_read(region, 0, &mut buf)?;

// Free when done
mem_free(region)?;
```

## Per-Process Memory

Each process has a `ProcessMemory` that tracks allocations:

```rust
pub struct ProcessMemory {
    /// Memory regions owned by this process
    regions: HashMap<RegionId, MemoryRegion>,

    /// Total bytes allocated
    allocated: usize,

    /// Memory limit (0 = unlimited)
    limit: usize,

    /// Peak memory usage
    peak: usize,

    /// Shared memory segments attached
    attached_shm: HashMap<ShmId, RegionId>,
}
```

### Memory Limits

Processes can have memory limits:

```rust
// Set a 1MB limit
set_memlimit(1024 * 1024)?;

// Allocations that would exceed limit fail
mem_alloc(2 * 1024 * 1024, Protection::READ_WRITE);
// Returns Err(Memory(OutOfMemory))
```

### Memory Stats

Get detailed memory information:

```rust
let stats = memstats()?;
println!("Allocated: {} bytes", stats.allocated);
println!("Limit: {} bytes", stats.limit);
println!("Peak: {} bytes", stats.peak);
println!("Regions: {}", stats.region_count);
println!("Shared: {}", stats.shm_count);
```

## Shared Memory

Shared memory enables efficient IPC between processes.

### Creating Shared Memory

```rust
// Process 1: Create a shared memory segment
let shm_id = shmget(4096)?;  // 4KB segment

// Attach to get a region
let region = shmat(shm_id, Protection::READ_WRITE)?;

// Write data
mem_write(region, 0, b"shared data")?;

// Sync to shared segment
shm_sync(shm_id)?;
```

### Attaching from Another Process

```rust
// Process 2: Attach to existing segment
let region = shmat(shm_id, Protection::READ)?;

// Refresh to get latest data
shm_refresh(shm_id)?;

// Read data
let mut buf = [0u8; 20];
mem_read(region, 0, &mut buf)?;
```

### Detaching

```rust
// When done, detach
shmdt(shm_id)?;

// If all processes detach, segment is freed
```

### Shared Memory Lifecycle

1. `shmget()` creates segment (refcount = 0)
2. `shmat()` attaches process (refcount++)
3. Each attached process has a local region
4. `shm_sync()` writes local changes to shared
5. `shm_refresh()` reads shared changes to local
6. `shmdt()` detaches (refcount--)
7. When refcount = 0, segment is freed

### Listing Shared Memory

```rust
let list = shm_list()?;
for info in list {
    println!("ShmId: {:?}", info.id);
    println!("  Size: {} bytes", info.size);
    println!("  Attached: {} processes", info.attached_count);
    println!("  Creator: {:?}", info.creator);
}
```

## Copy-on-Write (COW)

axeberg implements copy-on-write semantics for efficient process forking. When a process forks, memory pages are shared between parent and child until one of them writes, at which point only the modified page is copied.

### Page-Based Memory

Memory regions are divided into 4KB pages internally:

```rust
pub const PAGE_SIZE: usize = 4096;

pub struct Page {
    /// Data is reference-counted via Arc
    data: Arc<Vec<u8>>,
}
```

Pages track their reference count:
- `ref_count() == 1`: Page is private (owned by single process)
- `ref_count() > 1`: Page is shared (COW - copy before writing)

### COW Semantics

When a process writes to a shared page:

1. Check if page has `ref_count > 1` (shared)
2. If shared, clone the page data (COW fault)
3. Replace page with private copy
4. Write to the private copy

This is transparent to the process:

```rust
// Both parent and child see this region
let region = mem_alloc(4096, Protection::READ_WRITE)?;
mem_write(region, 0, b"initial data")?;

// After fork, child has COW copy of all regions
let child_pid = fork()?;

// Parent writes - triggers COW on parent's page
mem_write(region, 0, b"parent data")?;

// Child writes - triggers COW on child's page
mem_write(region, 0, b"child data")?;

// Now parent and child have independent copies
```

### Fork System Call

The `fork()` syscall creates a child process with COW memory:

```rust
// Fork the current process
let child_pid = fork()?;

// Returns child PID to parent
// Child inherits:
// - COW memory (shared until written)
// - File descriptors (reference counted)
// - Environment variables
// - Current working directory
// - Process group and session
```

### COW Statistics

Get COW statistics for a region or process:

```rust
// Per-region stats
let stats = region.cow_stats();
println!("Total pages: {}", stats.total_pages);
println!("Shared pages: {}", stats.shared_pages);
println!("Private pages: {}", stats.private_pages);
println!("COW faults: {}", stats.cow_faults);

// Per-process stats
let stats = process_memory.cow_stats();
println!("Regions with COW: {}", stats.regions_with_cow);
```

### Benefits

1. **Fast fork**: No immediate memory copy needed
2. **Memory efficient**: Pages only copied when modified
3. **Read sharing**: Unmodified pages stay shared forever
4. **Lazy copying**: Copy cost spread over time

### Implementation Notes

- Page size is 4KB (standard page size)
- Reference counting via `Arc<Vec<u8>>`
- COW applies to private memory only (not shared memory segments)
- COW faults are counted for monitoring

## Memory Manager

The kernel has a global `MemoryManager`:

```rust
pub struct MemoryManager {
    /// Next region ID
    next_region_id: AtomicU64,

    /// Next shared memory ID
    next_shm_id: AtomicU64,

    /// Shared memory segments
    shared_segments: HashMap<ShmId, SharedMemory>,

    /// System memory limit (0 = unlimited)
    system_limit: usize,

    /// Total memory allocated
    total_allocated: usize,
}
```

### System-Wide Stats

```rust
let stats = system_memstats()?;
println!("Total allocated: {} bytes", stats.total_allocated);
println!("System limit: {} bytes", stats.system_limit);
println!("Shared segments: {}", stats.shm_count);
println!("Shared total: {} bytes", stats.shm_total_size);
```

## Error Handling

Memory operations can fail:

```rust
pub enum MemoryError {
    /// Out of memory (quota exceeded)
    OutOfMemory,

    /// Invalid region ID
    InvalidRegion,

    /// Permission denied (protection violation)
    PermissionDenied,

    /// Access out of bounds
    OutOfBounds,

    /// Shared memory segment not found
    ShmNotFound,

    /// Already attached to this shared memory
    AlreadyAttached,

    /// Not attached to this shared memory
    NotAttached,

    /// Invalid size
    InvalidSize,
}
```

## Best Practices

1. **Always free regions**: Prevent memory leaks
2. **Set appropriate limits**: Protect against runaway allocations
3. **Use shared memory for large data**: Avoid copying
4. **Sync shared memory explicitly**: Don't assume automatic sync
5. **Check protection**: Don't try to write to read-only regions

## Example: Producer-Consumer

```rust
// Producer
async fn producer(shm_id: ShmId) {
    let region = shmat(shm_id, Protection::READ_WRITE)?;

    for i in 0..100 {
        let data = format!("message {}", i);
        mem_write(region, 0, data.as_bytes())?;
        shm_sync(shm_id)?;

        // Signal consumer somehow...
        yield_now().await;
    }

    shmdt(shm_id)?;
}

// Consumer
async fn consumer(shm_id: ShmId) {
    let region = shmat(shm_id, Protection::READ)?;

    loop {
        shm_refresh(shm_id)?;

        let mut buf = [0u8; 256];
        mem_read(region, 0, &mut buf)?;

        if buf.starts_with(b"done") {
            break;
        }

        // Process message...
        yield_now().await;
    }

    shmdt(shm_id)?;
}
```

## Related Documentation

- [Syscall Interface](syscalls.md) - Memory syscalls
- [Process Model](processes.md) - Per-process memory
- [IPC](ipc.md) - Communication patterns
- [Work Tracker](../WORK_TRACKER.md) - All work items and planned enhancements
