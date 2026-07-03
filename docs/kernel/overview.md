# Kernel Overview

The axeberg kernel manages processes, memory, filesystems, and system resources.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                                Kernel                                       │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                         Syscall Interface                           │    │
│  │  open() read() write() fork() exec() kill() mmap() socket() ...     │    │
│  └──────────────────────────────────┬──────────────────────────────────┘    │
│                                     │                                       │
│  ┌──────────────────────────────────┴──────────────────────────────────┐    │
│  │                           Subsystems                                │    │
│  │  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐         │    │
│  │  │   Process      │  │      VFS       │  │      IPC       │         │    │
│  │  │  • processes   │  │  • vfs         │  │  • fifos       │         │    │
│  │  │  • next_pid    │  │  • handles     │  │  • msgqueues   │         │    │
│  │  │  • current     │  │  • procfs      │  │  • semaphores  │         │    │
│  │  │  • signals     │  │  • devfs       │  │  • uds         │         │    │
│  │  │  • caps        │  │  • sysfs       │  │  • flocks      │         │    │
│  │  │                │  │  • mounts      │  │                │         │    │
│  │  └────────────────┘  └────────────────┘  └────────────────┘         │    │
│  │  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐         │    │
│  │  │     Time       │  │    Memory      │  │     Users      │         │    │
│  │  │  • timers      │  │  • regions     │  │  • users       │         │    │
│  │  │  • now         │  │  • shm         │  │  • groups      │         │    │
│  │  │  • alarms      │  │  • mmap        │  │  • sessions    │         │    │
│  │  └────────────────┘  └────────────────┘  └────────────────┘         │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                     │                                       │
│  ┌──────────────────────────────────┴──────────────────────────────────┐    │
│  │                           Executor                                  │    │
│  │  Single-threaded (WASM) │ Work-stealing (native)                    │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Subsystems

### Process Subsystem

Manages process lifecycle, scheduling, and credentials.

```rust
pub struct ProcessSubsystem {
    processes: HashMap<Pid, Process>,
    next_pid: u32,
    current: Option<Pid>,
}
```

**Process structure:**

```rust
pub struct Process {
    // Identity
    pub pid: Pid,
    pub parent: Option<Pid>,
    pub pgid: Pgid,              // Process group (job control)
    pub sid: Sid,                // Session ID

    // Credentials
    pub uid: Uid,                // Real user ID
    pub gid: Gid,                // Real group ID
    pub euid: Uid,               // Effective UID
    pub egid: Gid,               // Effective GID
    pub suid: Uid,               // Saved UID
    pub sgid: Gid,               // Saved GID
    pub groups: Vec<Gid>,        // Supplementary groups
    pub capabilities: ProcessCapabilities,

    // State
    pub state: ProcessState,     // Running, Sleeping, Stopped, Zombie
    pub files: FileTable,        // File descriptors
    pub memory: ProcessMemory,   // Memory regions
    pub signals: ProcessSignals, // Signal handling
    pub rlimits: ResourceLimits, // Resource limits
    pub nice: i8,                // Scheduling priority

    // Environment
    pub cwd: PathBuf,
    pub environ: HashMap<String, String>,
    pub umask: u32,
    pub jail_root: Option<PathBuf>,  // chroot jail
}
```

### VFS Subsystem

Manages filesystems and file operations.

```rust
pub struct VfsSubsystem {
    vfs: Box<dyn FileSystem>,    // Primary filesystem
    vfs_handles: Slab<VfsHandle>,
    procfs: ProcFs,              // /proc
    devfs: DevFs,                // /dev
    sysfs: SysFs,                // /sys
    mounts: MountTable,
}
```

**Filesystem trait** (abridged; `src/vfs/mod.rs` is authoritative):

```rust
pub trait FileSystem {
    fn open(&mut self, path: &str, options: OpenOptions) -> io::Result<FileHandle>;
    fn close(&mut self, handle: FileHandle) -> io::Result<()>;
    fn read(&mut self, handle: FileHandle, buf: &mut [u8]) -> io::Result<usize>;
    fn write(&mut self, handle: FileHandle, buf: &[u8]) -> io::Result<usize>;
    fn seek(&mut self, handle: FileHandle, pos: io::SeekFrom) -> io::Result<u64>;
    fn metadata(&self, path: &str) -> io::Result<Metadata>;
    fn fstat(&self, handle: FileHandle) -> io::Result<Metadata>;
    fn create_dir(&mut self, path: &str) -> io::Result<()>;
    fn read_dir(&self, path: &str) -> io::Result<Vec<DirEntry>>;
    fn remove_file(&mut self, path: &str) -> io::Result<()>;
    fn remove_dir(&mut self, path: &str) -> io::Result<()>;
    fn rename(&mut self, from: &str, to: &str) -> io::Result<()>;
    fn copy_file(&mut self, from: &str, to: &str) -> io::Result<u64>;
    fn exists(&self, path: &str) -> bool;
    fn symlink(&mut self, target: &str, link_path: &str) -> io::Result<()>;
    fn read_link(&self, path: &str) -> io::Result<String>;
    fn link(&mut self, source: &str, dest: &str) -> io::Result<()>;
    fn chmod(&mut self, path: &str, mode: u16) -> io::Result<()>;
    fn chown(&mut self, path: &str, uid: Option<u32>, gid: Option<u32>) -> io::Result<()>;
    fn handle_path(&self, handle: FileHandle) -> io::Result<String>;
    fn set_clock(&mut self, now: f64);
    fn utimes(&mut self, path: &str, atime: Option<f64>, mtime: Option<f64>) -> io::Result<()>;
}
```

### IPC Subsystem

Inter-process communication mechanisms.

```rust
pub struct IpcSubsystem {
    fifos: FifoRegistry,              // Named pipes
    msgqueues: MsgQueueManager,       // Message queues
    semaphores: SemaphoreManager,     // Counting semaphores
    uds: UnixSocketManager,           // Unix domain sockets
    flocks: FileLockManager,          // File locking (flock/fcntl)
}
```

### Time Subsystem

Timer and alarm management.

```rust
pub struct TimeSubsystem {
    timers: TimerQueue,
    now: f64,                         // Current time (ms)
}
```

## Object Table

Kernel resources are reference-counted objects:

```rust
pub enum KernelObject {
    File(FileObject),
    Pipe(PipeObject),
    Console(ConsoleObject),
    Window(WindowObject),
    Directory(DirectoryObject),
}
```

Operations:
- `insert()` - Create with refcount 1
- `retain()` - Increment refcount
- `release()` - Decrement, remove when 0

## Syscall Flow

```
User Code                        Kernel
    │                              │
    ├──sys_open("/etc/passwd")────►│
    │                              ├─ get_current_process()
    │                              ├─ check_path_traversal()
    │                              ├─ resolve_jailed_path()
    │                              ├─ check_permission_with_caps()
    │                              ├─ vfs.open()
    │                              ├─ create FileObject
    │                              ├─ objects.insert()
    │                              ├─ files.alloc()
    ◄──────────Ok(Fd(3))───────────┤
```

## Security Model

### Capabilities

Linux-style POSIX capabilities for fine-grained permissions:

```rust
pub enum Capability {
    DacOverride,      // Bypass file permission checks
    DacReadSearch,    // Bypass read/search permission
    Fowner,           // Bypass ownership checks
    Kill,             // Send signals to any process
    Setuid,           // Manipulate process UIDs
    Setgid,           // Manipulate process GIDs
    SysChroot,        // Use chroot()
    SysAdmin,         // System administration
    SysResource,      // Override resource limits
    // ... 24 total capabilities
}
```

### Process Jails (chroot)

Processes can be confined to a subtree:

```rust
// Syscall: sys_chroot("/jail")
// Requires: CAP_SYS_CHROOT

// After chroot:
// - "/" resolves to /jail
// - ".." cannot escape jail
// - Children inherit jail
```

### Permission Checking

```rust
fn check_permission_with_caps(process, file, access) {
    // Root or CAP_DAC_OVERRIDE bypasses all checks
    if process.euid == 0 || process.capabilities.has(DacOverride) {
        return Ok(());
    }
    // Otherwise check owner/group/other permissions
}
```

## Error Handling

All syscalls return `SyscallResult<T>`:

```rust
pub enum SyscallError {
    BadFd,
    NotFound,
    PermissionDenied,
    InvalidArgument,
    WouldBlock,
    BrokenPipe,
    Busy,
    InvalidData,
    NoProcess,
    Io(String),
    Memory(MemoryError),
    Signal(SignalError),
    Interrupted,
    NotADirectory,
    IsADirectory,
    AlreadyExists,
    TooManyOpenFiles,
    TooBig,
}
```

## Initialization

Boot sequence:

1. Create kernel with empty state
2. Initialize VFS with standard directories
3. Create init process (PID 1)
4. Set up stdin/stdout/stderr
5. Mount procfs, devfs, sysfs
6. Initialize user database
7. Start runtime loop

## Thread Safety

Single-threaded WASM design:
- `RefCell` for interior mutability
- `thread_local!` for per-isolate state
- No locks required

For multi-threaded contexts, use the work-stealing executor.

## Related Documentation

- [Syscalls](syscalls.md) - System call reference
- [Processes](processes.md) - Process model
- [Memory](memory.md) - Memory management
- [Users](users.md) - Multi-user system
- [Signals](signals.md) - Signal handling
- [IPC](ipc.md) - Inter-process communication
- [Work Stealing](work-stealing.md) - Parallel scheduler
