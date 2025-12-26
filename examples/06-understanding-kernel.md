# Tutorial 6: Understanding the Kernel

A deep dive into axeberg's kernel architecture.

## Overview

The kernel is the core of axeberg. It manages:
- Processes and scheduling
- Memory allocation
- File systems
- Inter-process communication
- Users and permissions

## Entry Point

Everything starts in `src/lib.rs`:

```rust
#[wasm_bindgen(start)]
pub fn main() {
    // Initialize panic handler for better errors
    console_error_panic_hook::set_once();

    // Create and boot the kernel
    let kernel = Kernel::new();
    kernel.boot();
}
```

## The Kernel Struct

Located in `src/kernel/mod.rs`:

```rust
pub struct Kernel {
    // Process management
    process_table: ProcessTable,
    object_table: ObjectTable,

    // Filesystem
    vfs: MemoryFs,

    // Users
    user_db: UserDatabase,
    session_manager: SessionManager,

    // IPC
    pipes: PipeRegistry,
    message_queues: MessageQueueRegistry,
    shared_memory: SharedMemoryRegistry,

    // Time
    timer_queue: TimerQueue,

    // I/O
    console: ConsoleHandle,
}
```

## System Calls

All userspace operations go through syscalls in `src/kernel/syscall.rs`:

```rust
impl Kernel {
    pub fn sys_open(&mut self, path: &str, flags: OpenFlags) -> Result<Fd> {
        // 1. Resolve path
        let resolved = self.vfs.resolve_path(path)?;

        // 2. Check permissions
        self.check_permission(&resolved, flags.into())?;

        // 3. Create/open file
        let file = self.vfs.open(&resolved, flags)?;

        // 4. Allocate file descriptor
        let fd = self.current_process_mut().alloc_fd(file);

        Ok(fd)
    }

    pub fn sys_read(&mut self, fd: Fd, buf: &mut [u8]) -> Result<usize> {
        let file = self.current_process().get_file(fd)?;
        file.read(buf)
    }

    pub fn sys_write(&mut self, fd: Fd, buf: &[u8]) -> Result<usize> {
        let file = self.current_process().get_file(fd)?;
        file.write(buf)
    }
}
```

## Process Model

### Process Structure (`src/kernel/process.rs`)

```rust
pub struct Process {
    pub pid: Pid,
    pub ppid: Pid,           // Parent PID
    pub state: ProcessState,
    pub exit_code: Option<i32>,

    // Credentials
    pub uid: Uid,
    pub gid: Gid,
    pub session: SessionId,

    // Resources
    pub fds: FileDescriptorTable,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,

    // Memory tracking
    pub memory: MemoryStats,
}

pub enum ProcessState {
    Created,
    Running,
    Sleeping,
    Stopped,
    Zombie,
}
```

### Process Lifecycle

```rust
// Spawning a new process
pub async fn spawn(&mut self, command: &str, args: &[String]) -> Result<Pid> {
    // 1. Allocate PID
    let pid = self.process_table.next_pid();

    // 2. Create process
    let process = Process::new(pid, self.current_pid());

    // 3. Inherit environment
    process.env = self.current_process().env.clone();
    process.cwd = self.current_process().cwd.clone();

    // 4. Set up standard FDs (stdin, stdout, stderr)
    process.fds.inherit_from(self.current_process());

    // 5. Add to process table
    self.process_table.insert(process);

    // 6. Schedule execution
    self.executor.spawn(pid, command, args);

    Ok(pid)
}
```

## Async Executor

The executor (`src/kernel/executor.rs`) runs async tasks:

```rust
pub struct Executor {
    // Priority queues
    critical: VecDeque<Task>,
    normal: VecDeque<Task>,
    background: VecDeque<Task>,

    // Waker registry
    wakers: HashMap<TaskId, Waker>,
}

impl Executor {
    pub fn run(&mut self) {
        loop {
            // Get next task (by priority)
            let task = self.critical.pop_front()
                .or_else(|| self.normal.pop_front())
                .or_else(|| self.background.pop_front());

            match task {
                Some(task) => {
                    // Poll the task
                    match task.future.poll(&mut cx) {
                        Poll::Ready(result) => {
                            // Task completed
                            self.complete(task.id, result);
                        }
                        Poll::Pending => {
                            // Task is waiting, will be re-queued when woken
                        }
                    }
                }
                None => {
                    // No tasks ready, wait for events
                    self.wait_for_events();
                }
            }
        }
    }
}
```

## Virtual Filesystem

The VFS (`src/vfs/memory.rs`) provides a unified file interface:

```rust
pub trait Filesystem {
    fn open(&self, path: &str, flags: OpenFlags) -> Result<File>;
    fn read(&self, path: &str) -> Result<Vec<u8>>;
    fn write(&mut self, path: &str, data: &[u8]) -> Result<()>;
    fn create_dir(&mut self, path: &str) -> Result<()>;
    fn remove(&mut self, path: &str) -> Result<()>;
    fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>>;
    fn metadata(&self, path: &str) -> Result<Metadata>;
    // ...
}
```

### Special Filesystems

- **procfs** (`/proc`): Process information
- **devfs** (`/dev`): Device files (null, zero, random, tty)
- **sysfs** (`/sys`): System information

```rust
// Reading /proc/self/status
pub fn read_proc_status(&self, pid: Pid) -> String {
    let process = self.process_table.get(pid)?;
    format!(
        "Name: {}\nPid: {}\nUid: {}\nState: {:?}\n",
        process.name, process.pid, process.uid, process.state
    )
}
```

## Inter-Process Communication

### Pipes (`src/kernel/ipc/pipe.rs`)

```rust
pub struct Pipe {
    buffer: VecDeque<u8>,
    capacity: usize,
    read_end_open: bool,
    write_end_open: bool,
    readers_waiting: VecDeque<Waker>,
    writers_waiting: VecDeque<Waker>,
}

impl Pipe {
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        loop {
            if !self.buffer.is_empty() {
                // Data available, read it
                let n = min(buf.len(), self.buffer.len());
                for i in 0..n {
                    buf[i] = self.buffer.pop_front().unwrap();
                }
                // Wake any waiting writers
                self.wake_writers();
                return Ok(n);
            }

            if !self.write_end_open {
                // EOF
                return Ok(0);
            }

            // Wait for data
            self.readers_waiting.push_back(current_waker());
            yield_now().await;
        }
    }
}
```

### Shared Memory (`src/kernel/ipc/shm.rs`)

```rust
pub struct SharedMemory {
    data: Vec<u8>,
    protection: Protection,
    attached: HashSet<Pid>,
}

bitflags! {
    pub struct Protection: u8 {
        const READ = 0b001;
        const WRITE = 0b010;
        const EXEC = 0b100;
    }
}
```

## Signals

Signal delivery (`src/kernel/signals.rs`):

```rust
pub fn send_signal(&mut self, pid: Pid, signal: Signal) -> Result<()> {
    let process = self.process_table.get_mut(pid)?;

    match signal {
        Signal::SIGKILL => {
            // Cannot be caught or ignored
            process.terminate(128 + 9);
        }
        Signal::SIGSTOP => {
            process.state = ProcessState::Stopped;
        }
        Signal::SIGCONT => {
            if process.state == ProcessState::Stopped {
                process.state = ProcessState::Running;
            }
        }
        _ => {
            // Queue for delivery
            process.pending_signals.insert(signal);
        }
    }

    Ok(())
}
```

## Tracing with strace

You can trace syscalls:

```bash
$ strace cat /etc/passwd
open("/etc/passwd", O_RDONLY) = 3
read(3, "root:x:0:0:root:/root:/bin/sh\n", 4096) = 31
write(1, "root:x:0:0:root:/root:/bin/sh\n", 31) = 31
close(3) = 0
exit(0)
```

Implementation in `src/kernel/tracing.rs`:

```rust
pub fn trace_syscall(&self, name: &str, args: &[&dyn Debug], result: &dyn Debug) {
    if self.current_process().traced {
        eprintln!("{}({}) = {:?}", name, format_args(args), result);
    }
}
```

## Exercises

### Exercise 1: Trace a Pipeline

```bash
$ strace sh -c 'cat /etc/passwd | grep root'
```

Observe:
- How are pipes created?
- Which process reads/writes which end?
- When do processes wait?

### Exercise 2: Explore /proc

```bash
$ ls /proc/self/
$ cat /proc/self/status
$ cat /proc/self/cmdline
$ ls -la /proc/self/fd/
```

### Exercise 3: Read the Source

Open these files and trace through:
1. `src/kernel/syscall.rs` - Find `sys_read`
2. `src/kernel/process.rs` - Understand `ProcessState`
3. `src/kernel/executor.rs` - See how tasks are scheduled

## Key Insights

1. **Everything is async**: The kernel uses Rust's async/await for cooperative multitasking
2. **Single-threaded**: No true parallelism (WASM limitation), but concurrent I/O
3. **Object handles**: Files, pipes, etc. are kernel objects with reference counting
4. **Unix-like but simpler**: Familiar patterns without POSIX complexity

## Further Reading

- [Kernel Overview](../docs/kernel/overview.md)
- [Syscall Reference](../docs/kernel/syscalls.md)
- [TLA+ Specifications](../specs/tla/README.md)
