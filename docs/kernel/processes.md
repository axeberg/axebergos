# Process Model

Processes are the fundamental unit of isolation in axeberg.

## Process Structure

```rust
pub struct Process {
    // Identity
    pub pid: Pid,              // Unique process identifier
    pub parent: Option<Pid>,   // Parent process (None for init)
    pub pgid: Pgid,            // Process group ID (job control)
    pub sid: Sid,              // Session ID

    // Credentials (Linux-like)
    pub uid: Uid,              // Real user ID
    pub gid: Gid,              // Real group ID
    pub euid: Uid,             // Effective user ID
    pub egid: Gid,             // Effective group ID
    pub groups: Vec<Gid>,      // Supplementary groups

    // State
    pub state: ProcessState,
    pub files: FileTable,      // File descriptors
    pub memory: ProcessMemory, // Memory tracking
    pub environ: HashMap<String, String>, // Environment
    pub cwd: PathBuf,          // Working directory

    // Session
    pub ctty: Option<String>,  // Controlling TTY
    pub is_session_leader: bool,

    // Execution
    pub task: Option<TaskId>,
    pub name: String,
    pub children: Vec<Pid>,
}
```

## Sessions and Process Groups

Like Linux, axeberg supports proper session management:

```
Session (sid=100)
├── Process Group (pgid=100) - foreground
│   ├── bash (pid=100, session leader)
│   └── vim (pid=101)
└── Process Group (pgid=102) - background
    └── make (pid=102)
```

**Session Leader**: First process in a session (typically login shell)
- Created by `login` command or `setsid()` syscall
- Has controlling TTY
- Death sends SIGHUP to all session processes

**Process Groups**: Used for job control (fg/bg)

## User Credentials

Each process has Linux-like credentials:

| Field | Purpose |
|-------|---------|
| `uid` | Real user ID (who started process) |
| `gid` | Real group ID |
| `euid` | Effective UID (for permission checks) |
| `egid` | Effective GID |
| `groups` | Supplementary groups |

### Permission Checking

```rust
// Kernel checks effective credentials for file access
if euid == 0 {
    // Root can do anything
} else if euid == file_uid {
    // Owner permissions
} else if egid == file_gid || groups.contains(&file_gid) {
    // Group permissions
} else {
    // Other permissions
}
```

## Process States

```rust
pub enum ProcessState {
    Running,       // Ready to run or currently running
    Sleeping,      // Waiting for I/O or timer
    Blocked(Pid),  // Waiting for another process
    Zombie(i32),   // Exited, waiting to be reaped
}
```

## Login Shells

The `login` command creates a proper session:

```rust
pub fn spawn_login_shell(
    username: &str,
    uid: Uid,
    gid: Gid,
    home: &str,
    shell: &str,
) -> Pid {
    // Creates new process with:
    // - New session ID (becomes session leader)
    // - New process group
    // - Proper credentials (uid/gid)
    // - Environment (HOME, USER, SHELL, etc.)
    // - Controlling TTY
}
```

Usage:
```bash
$ login alice password
Login successful: alice
  PID: 5, SID: 5, PGID: 5
  UID: 1001, GID: 1001
  Home: /home/alice
  TTY: tty1
```

## File Descriptor Table

Each process has its own file descriptor table:

```rust
pub struct FileTable {
    next_fd: u32,
    table: HashMap<Fd, Handle>,
}
```

- FDs 0, 1, 2 are stdin/stdout/stderr
- New FDs allocated from 3
- Each FD maps to a kernel object Handle

## Process Lifecycle

### Creation

```rust
// Regular process
let pid = spawn_process("name", Some(parent_pid));

// Login shell (with credentials)
let pid = spawn_login_shell("user", uid, gid, "/home/user", "/bin/sh");
```

### Termination

```rust
pub fn exit(code: i32) -> SyscallResult<()>
```

When a process exits:
1. State changes to `Zombie(code)`
2. File descriptors closed
3. Memory regions freed
4. Parent notified
5. If session leader, SIGHUP sent to session

### Logout

```bash
$ logout
Session 5 ended for user 'alice' (PID 5)
Returned to parent process.
```

Logout terminates the session and returns to parent.

## Environment Variables

Processes inherit environment on spawn:

```rust
environ.insert("HOME", "/home/user");
environ.insert("USER", "user");
environ.insert("SHELL", "/bin/sh");
environ.insert("PATH", "/bin:/usr/bin");
environ.insert("TERM", "xterm-256color");
```

## Isolation Model

### What's Isolated

- **File descriptors**: Per-process table
- **Memory regions**: Per-process tracking
- **Working directory**: Separate per process
- **Environment**: Inherited copy
- **Credentials**: Per-process uid/gid

### What's Shared

- **Kernel objects**: Via handle reference counting
- **Shared memory**: Explicit via shm* syscalls
- **User database**: System-wide `/etc/passwd`

## Session Syscalls

| Syscall | Description |
|---------|-------------|
| `setsid()` | Create new session, become leader |
| `getsid(pid)` | Get session ID |
| `getpgid(pid)` | Get process group ID |
| `setpgid(pid, pgid)` | Set process group |

## Related Documentation

- [Memory Management](memory.md) - Process memory
- [Syscall Interface](syscalls.md) - All syscalls
- [Signals](signals.md) - Signal handling
- [IPC](ipc.md) - Inter-process communication
