# Kernel Overview

The axeberg kernel is the core of the operating system, managing processes, memory, and system resources.

## Core Components

### 1. Process Manager

Manages the lifecycle of processes:

```rust
pub struct Process {
    pub pid: Pid,           // Unique identifier
    pub parent: Option<Pid>, // Parent process
    pub state: ProcessState, // Running, Sleeping, Zombie
    pub files: FileTable,    // File descriptors
    pub memory: ProcessMemory, // Memory tracking
    pub cwd: PathBuf,        // Working directory
    pub task: Option<TaskId>, // Associated async task
    pub name: String,        // Process name
}
```

Each process has:
- Its own file descriptor table
- Its own memory accounting
- Its own working directory
- No shared mutable state with other processes

### 2. Object Table

All kernel resources are objects with handles:

```rust
pub enum KernelObject {
    File(FileObject),      // Regular file
    Pipe(PipeObject),      // IPC pipe
    Console(ConsoleObject), // System console
    Window(WindowObject),  // GUI window
    Directory(DirectoryObject), // Directory
}
```

Objects are reference-counted:
- `insert()` creates with refcount 1
- `retain()` increments refcount (for sharing)
- `release()` decrements, removes when 0

### 3. Memory Manager

Tracks all memory allocations:

```rust
// Per-process memory
pub struct ProcessMemory {
    regions: HashMap<RegionId, MemoryRegion>,
    allocated: usize,
    limit: usize,
    peak: usize,
    attached_shm: HashMap<ShmId, RegionId>,
}

// System-wide shared memory
pub struct MemoryManager {
    shared_segments: HashMap<ShmId, SharedMemory>,
    system_limit: usize,
    total_allocated: usize,
}
```

### 4. Executor

Runs async tasks cooperatively:

```rust
pub struct Executor {
    tasks: Vec<Task>,
    ready: VecDeque<TaskId>,
    task_priorities: HashMap<TaskId, Priority>,
}

pub enum Priority {
    Critical,  // UI, input handling
    Normal,    // Regular work
    Background, // Low-priority tasks
}
```

Tasks are scheduled by priority, with all Critical tasks running before Normal tasks.

### 5. VFS (Virtual File System)

Provides a unified file interface:

```rust
pub trait FileSystem {
    fn open(&mut self, path: &str, options: OpenOptions) -> io::Result<FileHandle>;
    fn close(&mut self, handle: FileHandle) -> io::Result<()>;
    fn read(&mut self, handle: FileHandle, buf: &mut [u8]) -> io::Result<usize>;
    fn write(&mut self, handle: FileHandle, buf: &[u8]) -> io::Result<usize>;
    fn create_dir(&mut self, path: &str) -> io::Result<()>;
    fn read_dir(&self, path: &str) -> io::Result<Vec<DirEntry>>;
    // ...
}
```

Currently in-memory (`MemoryFs`), with OPFS persistence planned.

## Kernel State

The kernel maintains global state in a thread-local:

```rust
thread_local! {
    pub static KERNEL: RefCell<Kernel> = RefCell::new(Kernel::new());
}

pub struct Kernel {
    processes: HashMap<Pid, Process>,
    objects: ObjectTable,
    current: Option<Pid>,      // Currently executing process
    console_handle: Handle,    // Shared console
    vfs: MemoryFs,            // Filesystem
    memory: MemoryManager,     // Shared memory manager
}
```

## Syscall Flow

When user code makes a syscall:

```
User Code           Kernel
    │                 │
    ├──open("/file")──►│
    │                 ├── resolve_path()
    │                 ├── open_file()
    │                 ├── create FileObject
    │                 ├── insert into ObjectTable
    │                 ├── allocate fd
    ◄──────Fd(3)──────┤
```

All syscalls:
1. Check for current process
2. Validate arguments
3. Perform operation
4. Update kernel state
5. Return result

## Initialization

Boot sequence:

1. Create kernel with empty state
2. Initialize VFS with standard directories
3. Create init process (PID 1)
4. Set up stdin/stdout/stderr pointing to console
5. Initialize filesystem content
6. Spawn system processes
7. Start runtime loop

## Error Handling

All syscalls return `SyscallResult<T>`:

```rust
pub enum SyscallError {
    BadFd,           // Invalid file descriptor
    NotFound,        // Path not found
    PermissionDenied, // Access denied
    InvalidArgument, // Bad argument
    WouldBlock,      // Non-blocking would block
    BrokenPipe,      // Pipe closed
    Busy,            // Resource busy
    NoProcess,       // No current process
    Io(String),      // Generic I/O error
    Memory(MemoryError), // Memory error
}
```

## Thread Safety

The kernel is designed for single-threaded WASM:
- Uses `RefCell` for interior mutability
- No locks or atomics needed
- `thread_local!` for per-isolate state

If multi-threading is added later, we'd need to replace `RefCell` with proper synchronization.

## Related Documentation

- [Syscall Interface](syscalls.md) - Full syscall reference
- [Process Model](processes.md) - Process details
- [Memory Management](memory.md) - Memory operations
- [Object Table](objects.md) - Object lifecycle
