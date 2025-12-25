# Syscall Interface

The syscall layer is the boundary between user code and the kernel. All resource access goes through syscalls.

## Design Principles

1. **Isolation**: Processes can only access what they have handles to
2. **Auditing**: All operations go through a single point
3. **Safety**: The kernel validates all operations
4. **Simplicity**: Familiar POSIX-like interface

## File Operations

### open

Open a file or device.

```rust
pub fn open(path: &str, flags: OpenFlags) -> SyscallResult<Fd>
```

**OpenFlags** is a struct with boolean fields:
```rust
pub struct OpenFlags {
    pub read: bool,
    pub write: bool,
    pub create: bool,
    pub truncate: bool,
    pub append: bool,
}
```

**Predefined constants:**
- `OpenFlags::READ` - Open for reading only
- `OpenFlags::WRITE` - Open for writing (creates and truncates)
- `OpenFlags::RDWR` - Open for reading and writing
- `OpenFlags::APPEND` - Open for appending (creates but doesn't truncate)

**Special paths:**
- `/dev/console` - System console
- `/dev/null` - Discard writes, EOF on read
- `/dev/zero` - Returns zeros on read

**Example:**
```rust
let fd = syscall::open("/home/user/file.txt", OpenFlags::READ)?;
```

### read

Read from a file descriptor.

```rust
pub fn read(fd: Fd, buf: &mut [u8]) -> SyscallResult<usize>
```

Returns the number of bytes read, or 0 at EOF.

### write

Write to a file descriptor.

```rust
pub fn write(fd: Fd, buf: &[u8]) -> SyscallResult<usize>
```

Returns the number of bytes written.

### close

Close a file descriptor.

```rust
pub fn close(fd: Fd) -> SyscallResult<()>
```

Decrements the reference count on the underlying object. If refcount reaches 0, the object is freed.

### dup

Duplicate a file descriptor.

```rust
pub fn dup(fd: Fd) -> SyscallResult<Fd>
```

Creates a new fd pointing to the same object. Increments refcount.

## Directory Operations

### mkdir

Create a directory.

```rust
pub fn mkdir(path: &str) -> SyscallResult<()>
```

Parent directory must exist.

### readdir

List directory contents.

```rust
pub fn readdir(path: &str) -> SyscallResult<Vec<String>>
```

Returns names of files and subdirectories.

### exists

Check if a path exists.

```rust
pub fn exists(path: &str) -> SyscallResult<bool>
```

### getcwd

Get current working directory.

```rust
pub fn getcwd() -> SyscallResult<PathBuf>
```

### chdir

Change working directory.

```rust
pub fn chdir(path: &str) -> SyscallResult<()>
```

## Process Operations

### getpid

Get current process ID.

```rust
pub fn getpid() -> SyscallResult<Pid>
```

### exit

Exit the current process.

```rust
pub fn exit(code: i32) -> SyscallResult<()>
```

Sets process state to Zombie with the exit code.

## IPC Operations

### pipe

Create a pipe for inter-process communication.

```rust
pub fn pipe() -> SyscallResult<(Fd, Fd)>
```

Returns (read_fd, write_fd). Data written to write_fd can be read from read_fd.

## Memory Operations

### mem_alloc

Allocate a memory region.

```rust
pub fn mem_alloc(size: usize, prot: Protection) -> SyscallResult<RegionId>
```

**Protection flags:**
- `Protection::READ` - Read-only
- `Protection::READ_WRITE` - Read and write
- `Protection::READ_EXEC` - Read and execute

### mem_free

Free a memory region.

```rust
pub fn mem_free(region_id: RegionId) -> SyscallResult<()>
```

### mem_read

Read from a memory region.

```rust
pub fn mem_read(region_id: RegionId, offset: usize, buf: &mut [u8]) -> SyscallResult<usize>
```

### mem_write

Write to a memory region.

```rust
pub fn mem_write(region_id: RegionId, offset: usize, buf: &[u8]) -> SyscallResult<usize>
```

## Shared Memory

### shmget

Create a shared memory segment.

```rust
pub fn shmget(size: usize) -> SyscallResult<ShmId>
```

### shmat

Attach to a shared memory segment.

```rust
pub fn shmat(shm_id: ShmId, prot: Protection) -> SyscallResult<RegionId>
```

Returns a region ID that can be used with `mem_read`/`mem_write`.

### shmdt

Detach from a shared memory segment.

```rust
pub fn shmdt(shm_id: ShmId) -> SyscallResult<()>
```

If refcount reaches 0, the segment is freed.

### shm_sync

Sync local changes to shared memory.

```rust
pub fn shm_sync(shm_id: ShmId) -> SyscallResult<()>
```

### shm_refresh

Refresh local region from shared memory.

```rust
pub fn shm_refresh(shm_id: ShmId) -> SyscallResult<()>
```

## Memory Stats

### memstats

Get memory stats for current process.

```rust
pub fn memstats() -> SyscallResult<MemoryStats>

pub struct MemoryStats {
    pub allocated: usize,    // Current allocation
    pub limit: usize,        // Memory limit (0 = unlimited)
    pub peak: usize,         // Peak usage
    pub region_count: usize, // Number of regions
    pub shm_count: usize,    // Attached shared memory count
}
```

### set_memlimit

Set memory limit for current process.

```rust
pub fn set_memlimit(limit: usize) -> SyscallResult<()>
```

### system_memstats

Get system-wide memory stats.

```rust
pub fn system_memstats() -> SyscallResult<SystemMemoryStats>

pub struct SystemMemoryStats {
    pub total_allocated: usize,
    pub system_limit: usize,
    pub shm_count: usize,
    pub shm_total_size: usize,
}
```

## Window Operations

### window_create

Create a GUI window.

```rust
pub fn window_create(title: &str) -> SyscallResult<Fd>
```

Returns a file descriptor for the window.

## Console Operations

### console_push_input

Push input to the console (for keyboard input).

```rust
pub fn console_push_input(data: &[u8])
```

### console_take_output

Take output from the console.

```rust
pub fn console_take_output() -> Vec<u8>
```

## Timer Operations

### timer_set

Set a one-shot timer.

```rust
pub fn timer_set(delay_ms: f64, wake_task: Option<TaskId>) -> SyscallResult<TimerId>
```

Timer fires after `delay_ms` milliseconds. If `wake_task` is provided, that task is woken when the timer fires.

### timer_interval

Set a repeating interval timer.

```rust
pub fn timer_interval(interval_ms: f64, wake_task: Option<TaskId>) -> SyscallResult<TimerId>
```

Timer fires every `interval_ms` milliseconds until cancelled.

### timer_cancel

Cancel a pending timer.

```rust
pub fn timer_cancel(timer_id: TimerId) -> SyscallResult<bool>
```

Returns `true` if the timer was pending and cancelled.

## Signal Operations

### kill

Send a signal to a process.

```rust
pub fn kill(pid: Pid, signal: Signal) -> SyscallResult<()>
```

**Available signals:**
- `SIGTERM` - Graceful termination
- `SIGKILL` - Immediate termination (cannot be caught)
- `SIGSTOP` - Stop process (cannot be caught)
- `SIGCONT` - Continue stopped process
- `SIGINT`, `SIGQUIT`, `SIGHUP` - Termination signals
- `SIGUSR1`, `SIGUSR2` - User-defined signals
- `SIGCHLD` - Child status changed
- `SIGALRM` - Timer alarm
- `SIGPIPE` - Broken pipe

### signal

Set signal disposition for current process.

```rust
pub fn signal(sig: Signal, action: SignalAction) -> SyscallResult<()>
```

**Actions:**
- `SignalAction::Default` - Use default behavior
- `SignalAction::Ignore` - Ignore the signal
- `SignalAction::Terminate` - Terminate the process
- `SignalAction::Handle` - Custom handler (future)

Note: `SIGKILL` and `SIGSTOP` cannot have their disposition changed.

### sigblock

Block a signal (queue but don't deliver).

```rust
pub fn sigblock(sig: Signal) -> SyscallResult<()>
```

### sigunblock

Unblock a signal.

```rust
pub fn sigunblock(sig: Signal) -> SyscallResult<()>
```

### sigpending

Check if there are pending signals.

```rust
pub fn sigpending() -> SyscallResult<bool>
```

## Tracing Operations

### trace_enable

Enable kernel tracing.

```rust
pub fn trace_enable()
```

### trace_disable

Disable kernel tracing.

```rust
pub fn trace_disable()
```

### trace_enabled

Check if tracing is enabled.

```rust
pub fn trace_enabled() -> bool
```

### trace_summary

Get a summary of kernel statistics.

```rust
pub fn trace_summary() -> TraceSummary
```

Returns uptime, syscall counts, scheduler stats, process counts, and I/O bytes.

### trace_reset

Reset all trace data and statistics.

```rust
pub fn trace_reset()
```

### trace_event

Record a custom trace event.

```rust
pub fn trace_event(category: TraceCategory, name: &str, detail: Option<&str>)
```

## Error Handling

All syscalls return `SyscallResult<T>`, which is `Result<T, SyscallError>`:

```rust
pub enum SyscallError {
    BadFd,            // Invalid file descriptor
    NotFound,         // File or path not found
    PermissionDenied, // Permission denied
    InvalidArgument,  // Invalid argument
    WouldBlock,       // Would block (non-blocking I/O)
    BrokenPipe,       // Pipe/connection closed
    Busy,             // Resource busy
    NoProcess,        // No current process
    Io(String),       // Generic I/O error
    Memory(MemoryError), // Memory error
    Signal(SignalError), // Signal error
    Interrupted,      // Interrupted by signal
}
```

## Standard File Descriptors

Every process starts with:
- `Fd::STDIN` (0) - Standard input
- `Fd::STDOUT` (1) - Standard output
- `Fd::STDERR` (2) - Standard error

All three point to the console by default.

## Usage Example

```rust
use axeberg::kernel::syscall;

// Read a file
let fd = syscall::open("/home/user/data.txt", syscall::OpenFlags::READ)?;
let mut buf = [0u8; 1024];
let n = syscall::read(fd, &mut buf)?;
syscall::close(fd)?;

// Create and write to a file
let fd = syscall::open("/tmp/output.txt", syscall::OpenFlags::WRITE)?;
syscall::write(fd, b"Hello, axeberg!")?;
syscall::close(fd)?;

// Allocate memory
let region = syscall::mem_alloc(4096, Protection::READ_WRITE)?;
syscall::mem_write(region, 0, b"data")?;
syscall::mem_free(region)?;
```
