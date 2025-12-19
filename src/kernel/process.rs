//! Process abstraction
//!
//! A process is the fundamental unit of isolation in axeberg.
//! Each process has its own file descriptor table, working directory,
//! and runs as an async task in the executor.

use super::memory::ProcessMemory;
use super::signal::ProcessSignals;
use super::TaskId;
use std::collections::HashMap;
use std::path::PathBuf;

/// Process identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pid(pub u32);

impl std::fmt::Display for Pid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pid:{}", self.0)
    }
}

/// Process state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessState {
    /// Process is ready to run or currently running
    Running,
    /// Process is waiting for I/O or a timer
    Sleeping,
    /// Process is blocked waiting for another process
    Blocked(Pid),
    /// Process is stopped (by signal)
    Stopped,
    /// Process has exited with a status code
    Zombie(i32),
}

/// File descriptor - an index into a process's file table
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fd(pub u32);

impl Fd {
    pub const STDIN: Fd = Fd(0);
    pub const STDOUT: Fd = Fd(1);
    pub const STDERR: Fd = Fd(2);
}

impl std::fmt::Display for Fd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "fd:{}", self.0)
    }
}

/// Flags for opening files
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenFlags {
    pub read: bool,
    pub write: bool,
    pub create: bool,
    pub truncate: bool,
    pub append: bool,
}

impl OpenFlags {
    pub const READ: OpenFlags = OpenFlags {
        read: true,
        write: false,
        create: false,
        truncate: false,
        append: false,
    };

    pub const WRITE: OpenFlags = OpenFlags {
        read: false,
        write: true,
        create: true,
        truncate: true,
        append: false,
    };

    pub const RDWR: OpenFlags = OpenFlags {
        read: true,
        write: true,
        create: false,
        truncate: false,
        append: false,
    };

    pub const APPEND: OpenFlags = OpenFlags {
        read: false,
        write: true,
        create: true,
        truncate: false,
        append: true,
    };
}

/// A process in the system
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

    /// Signal handling
    pub signals: ProcessSignals,

    /// Current working directory
    pub cwd: PathBuf,

    /// The executor task running this process's code
    pub task: Option<TaskId>,

    /// Process name (for debugging/display)
    pub name: String,
}

impl Process {
    /// Create a new process
    pub fn new(pid: Pid, name: String, parent: Option<Pid>) -> Self {
        Self {
            pid,
            parent,
            state: ProcessState::Running,
            files: FileTable::new(),
            memory: ProcessMemory::new(),
            signals: ProcessSignals::new(),
            cwd: PathBuf::from("/"),
            task: None,
            name,
        }
    }

    /// Create a process with a memory limit
    pub fn with_memory_limit(pid: Pid, name: String, parent: Option<Pid>, limit: usize) -> Self {
        Self {
            pid,
            parent,
            state: ProcessState::Running,
            files: FileTable::new(),
            memory: ProcessMemory::with_limit(limit),
            signals: ProcessSignals::new(),
            cwd: PathBuf::from("/"),
            task: None,
            name,
        }
    }

    /// Check if process is alive (not a zombie)
    pub fn is_alive(&self) -> bool {
        !matches!(self.state, ProcessState::Zombie(_))
    }

    /// Check if process is stopped
    pub fn is_stopped(&self) -> bool {
        matches!(self.state, ProcessState::Stopped)
    }

    /// Check if process can run (not stopped, not zombie)
    pub fn can_run(&self) -> bool {
        matches!(self.state, ProcessState::Running | ProcessState::Sleeping)
    }
}

/// A process's file descriptor table
pub struct FileTable {
    /// Next fd to allocate
    next_fd: u32,
    /// Map from fd to kernel object handle
    table: HashMap<Fd, Handle>,
}

impl FileTable {
    pub fn new() -> Self {
        Self {
            next_fd: 3, // 0, 1, 2 reserved for stdin/stdout/stderr
            table: HashMap::new(),
        }
    }

    /// Allocate a new file descriptor
    pub fn alloc(&mut self, handle: Handle) -> Fd {
        let fd = Fd(self.next_fd);
        self.next_fd += 1;
        self.table.insert(fd, handle);
        fd
    }

    /// Insert at a specific fd (for stdin/stdout/stderr)
    pub fn insert(&mut self, fd: Fd, handle: Handle) {
        self.table.insert(fd, handle);
    }

    /// Get a handle by fd
    pub fn get(&self, fd: Fd) -> Option<Handle> {
        self.table.get(&fd).copied()
    }

    /// Remove a file descriptor
    pub fn remove(&mut self, fd: Fd) -> Option<Handle> {
        self.table.remove(&fd)
    }

    /// Check if fd exists
    pub fn contains(&self, fd: Fd) -> bool {
        self.table.contains_key(&fd)
    }
}

impl Default for FileTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle to a kernel object
/// This is what's stored in the file table - a reference to an object in the kernel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handle(pub u64);

impl Handle {
    pub const NULL: Handle = Handle(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_creation() {
        let proc = Process::new(Pid(1), "init".to_string(), None);
        assert_eq!(proc.pid, Pid(1));
        assert!(proc.parent.is_none());
        assert!(proc.is_alive());
        assert_eq!(proc.cwd, PathBuf::from("/"));
    }

    #[test]
    fn test_file_table_alloc() {
        let mut ft = FileTable::new();
        let h1 = Handle(100);
        let h2 = Handle(200);

        let fd1 = ft.alloc(h1);
        let fd2 = ft.alloc(h2);

        assert_eq!(fd1, Fd(3)); // First user fd after stdin/stdout/stderr
        assert_eq!(fd2, Fd(4));
        assert_eq!(ft.get(fd1), Some(h1));
        assert_eq!(ft.get(fd2), Some(h2));
    }

    #[test]
    fn test_file_table_insert_stdio() {
        let mut ft = FileTable::new();
        let console = Handle(1);

        ft.insert(Fd::STDIN, console);
        ft.insert(Fd::STDOUT, console);
        ft.insert(Fd::STDERR, console);

        assert_eq!(ft.get(Fd::STDIN), Some(console));
        assert_eq!(ft.get(Fd::STDOUT), Some(console));
    }

    #[test]
    fn test_file_table_remove() {
        let mut ft = FileTable::new();
        let h = Handle(100);
        let fd = ft.alloc(h);

        assert!(ft.contains(fd));
        let removed = ft.remove(fd);
        assert_eq!(removed, Some(h));
        assert!(!ft.contains(fd));
    }

    #[test]
    fn test_process_zombie() {
        let mut proc = Process::new(Pid(1), "test".to_string(), None);
        assert!(proc.is_alive());

        proc.state = ProcessState::Zombie(0);
        assert!(!proc.is_alive());
    }
}
