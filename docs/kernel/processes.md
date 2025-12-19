# Process Model

Processes are the fundamental unit of isolation in axeberg.

## Process Structure

```rust
pub struct Process {
    /// Unique process identifier
    pub pid: Pid,

    /// Parent process (None for init)
    pub parent: Option<Pid>,

    /// Current state
    pub state: ProcessState,

    /// File descriptor table
    pub files: FileTable,

    /// Memory tracking
    pub memory: ProcessMemory,

    /// Current working directory
    pub cwd: PathBuf,

    /// The executor task running this process's code
    pub task: Option<TaskId>,

    /// Process name (for debugging/display)
    pub name: String,
}
```

## Process States

```rust
pub enum ProcessState {
    /// Process is ready to run or currently running
    Running,

    /// Process is waiting for I/O or a timer
    Sleeping,

    /// Process is blocked waiting for another process
    Blocked(Pid),

    /// Process has exited with a status code
    Zombie(i32),
}
```

## Process Identification

Each process has a unique `Pid`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pid(pub u32);
```

Special PIDs:
- `Pid(0)` - Reserved (unused)
- `Pid(1)` - Init process

## File Descriptor Table

Each process has its own file descriptor table:

```rust
pub struct FileTable {
    next_fd: u32,
    table: HashMap<Fd, Handle>,
}
```

- FDs 0, 1, 2 are reserved for stdin/stdout/stderr
- New FDs are allocated starting from 3
- Each FD maps to a kernel object Handle

## Process Lifecycle

### Creation

```rust
// Internal kernel function
pub fn spawn_process(&mut self, name: &str, parent: Option<Pid>) -> Pid {
    let pid = Pid(self.next_pid);
    self.next_pid += 1;

    let mut process = Process::new(pid, name.to_string(), parent);

    // Set up stdio (console for all three)
    self.objects.retain(self.console_handle);  // stdin
    self.objects.retain(self.console_handle);  // stdout
    self.objects.retain(self.console_handle);  // stderr

    process.files.insert(Fd::STDIN, self.console_handle);
    process.files.insert(Fd::STDOUT, self.console_handle);
    process.files.insert(Fd::STDERR, self.console_handle);

    self.processes.insert(pid, process);
    pid
}
```

### Termination

```rust
pub fn exit(code: i32) -> SyscallResult<()>
```

When a process exits:
1. State changes to `Zombie(code)`
2. File descriptors are closed (decrements refcounts)
3. Memory regions are freed
4. Parent can retrieve exit status

## Isolation Model

### What's Isolated

- **File descriptors**: Each process has its own table
- **Memory regions**: Per-process allocation tracking
- **Working directory**: Separate cwd per process

### What's Shared

- **Kernel objects**: Via handle reference counting
- **Shared memory**: Explicitly via shm* syscalls
- **Console**: All processes write to same console

### Note on Memory Isolation

In WASM, we cannot achieve true memory isolation:
- No hardware MMU
- Single address space
- All code shares the same heap

Our "isolation" is logical:
- Processes can only access memory they explicitly allocate
- Kernel validates all access through syscalls
- But a malicious process could still read/write arbitrary memory

This is acceptable for a personal OS where you control all code.

## Process Memory

Each process tracks its memory usage:

```rust
pub struct ProcessMemory {
    regions: HashMap<RegionId, MemoryRegion>,
    allocated: usize,
    limit: usize,
    peak: usize,
    attached_shm: HashMap<ShmId, RegionId>,
}
```

Features:
- **Allocation tracking**: Know exactly how much is allocated
- **Memory limits**: Prevent runaway processes
- **Peak tracking**: Understand memory high-water mark
- **Shared memory**: Track attached segments

## Context Switching

axeberg uses cooperative multitasking:

1. Tasks are async Rust futures
2. Tasks yield at `await` points
3. Executor selects next task by priority
4. No preemption (no timer interrupts)

```rust
// Example process code
kernel::spawn(async {
    // This process runs until it awaits or completes
    let fd = syscall::open("/file", OpenFlags::READ)?;

    // If read blocks, we yield here
    let n = syscall::read(fd, &mut buf).await?;

    syscall::close(fd)?;
});
```

## Init Process

The first process (PID 1) is special:
- Created during boot
- Parent of all other processes
- Sets up initial filesystem
- Spawns system daemons

```rust
pub fn boot() {
    let init_pid = syscall::spawn_process("init");
    syscall::set_current_process(init_pid);

    // init's work...
    init_filesystem();
    spawn_init_processes();

    runtime::start();
}
```

## Process Communication

Processes can communicate via:

1. **Pipes**: `pipe()` syscall
2. **Shared Memory**: `shmget/shmat/shmdt`
3. **Files**: Write to VFS, read from VFS
4. **Channels**: Kernel IPC (higher-level abstraction)

## Future Work

- **Process hierarchy**: Proper parent-child relationships
- **Signal handling**: Inter-process signaling
- **Process groups**: Job control
- **Fork/exec**: Traditional Unix semantics (maybe)

## Related Documentation

- [Memory Management](memory.md) - Process memory details
- [Syscall Interface](syscalls.md) - Process syscalls
- [IPC](ipc.md) - Inter-process communication
