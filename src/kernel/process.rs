//! Process abstraction
//!
//! A process is the fundamental unit of isolation in axeberg.
//! Each process has its own file descriptor table, working directory,
//! environment variables, and runs as an async task in the executor.
//!
//! Inspired by Linux process model:
//! - Process groups (pgid) for job control
//! - Environment variables (inherited on spawn)
//! - Parent/child relationships
//! - Wait/reap semantics for zombie processes

use super::TaskId;
use super::memory::ProcessMemory;
use super::signal::ProcessSignals;
use super::users::{Gid, Uid};
use std::collections::HashMap;
use std::path::PathBuf;

/// Process group identifier (for job control)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pgid(pub u32);

impl Pgid {
    /// Create a PGID from a PID (new process group)
    pub fn from_pid(pid: Pid) -> Self {
        Pgid(pid.0)
    }
}

impl std::fmt::Display for Pgid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pgid:{}", self.0)
    }
}

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

/// Session identifier (for session management like Linux SID)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sid(pub u32);

impl Sid {
    /// Create a SID from a PID (new session leader)
    pub fn from_pid(pid: Pid) -> Self {
        Sid(pid.0)
    }
}

impl std::fmt::Display for Sid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sid:{}", self.0)
    }
}

/// A process in the system
pub struct Process {
    /// Unique process identifier
    pub pid: Pid,

    /// Parent process (None for init)
    pub parent: Option<Pid>,

    /// Process group ID (for job control, like Linux PGID)
    pub pgid: Pgid,

    /// Session ID (for session management, like Linux SID)
    pub sid: Sid,

    /// Real user ID (who started the process)
    pub uid: Uid,

    /// Real group ID
    pub gid: Gid,

    /// Effective user ID (for permission checks, may differ if setuid)
    pub euid: Uid,

    /// Effective group ID (for permission checks)
    pub egid: Gid,

    /// Saved user ID (for privilege dropping and restoration)
    /// When a setuid binary runs, this stores the original euid so
    /// the process can temporarily drop privileges via seteuid()
    /// and regain them later with seteuid(suid).
    pub suid: Uid,

    /// Saved group ID (for privilege dropping and restoration)
    pub sgid: Gid,

    /// Supplementary group IDs
    pub groups: Vec<Gid>,

    /// Current state
    pub state: ProcessState,

    /// File descriptor table
    pub files: FileTable,

    /// Memory tracking
    pub memory: ProcessMemory,

    /// Signal handling
    pub signals: ProcessSignals,

    /// Environment variables (inherited on spawn, like Linux environ)
    pub environ: HashMap<String, String>,

    /// Current working directory
    pub cwd: PathBuf,

    /// The executor task running this process's code
    pub task: Option<TaskId>,

    /// Process name (for debugging/display)
    pub name: String,

    /// Child processes (for waitpid)
    pub children: Vec<Pid>,

    /// Controlling TTY (like Linux ctty)
    pub ctty: Option<String>,

    /// Is this process a session leader?
    pub is_session_leader: bool,
}

impl Process {
    /// Create a new process (defaults to user 1000)
    pub fn new(pid: Pid, name: String, parent: Option<Pid>) -> Self {
        // Process group defaults to own PID (new session leader)
        let pgid = Pgid::from_pid(pid);
        // Session ID defaults to own PID (new session)
        let sid = Sid::from_pid(pid);

        // Default to regular user (uid 1000)
        let uid = Uid(1000);
        let gid = Gid(1000);

        // Default environment with common variables
        let mut environ = HashMap::new();
        environ.insert("HOME".to_string(), "/home/user".to_string());
        environ.insert("USER".to_string(), "user".to_string());
        environ.insert("SHELL".to_string(), "/bin/sh".to_string());
        environ.insert("PATH".to_string(), "/bin:/usr/bin".to_string());
        environ.insert("TERM".to_string(), "xterm-256color".to_string());

        Self {
            pid,
            parent,
            pgid,
            sid,
            uid,
            gid,
            euid: uid,
            egid: gid,
            suid: uid, // Saved IDs start same as real IDs
            sgid: gid,
            groups: vec![gid],
            state: ProcessState::Running,
            files: FileTable::new(),
            memory: ProcessMemory::new(),
            signals: ProcessSignals::new(),
            environ,
            cwd: PathBuf::from("/"),
            task: None,
            name,
            children: Vec::new(),
            ctty: None,
            is_session_leader: true, // New processes are session leaders by default
        }
    }

    /// Create a process with inherited environment and credentials
    #[allow(clippy::too_many_arguments)]
    pub fn with_environ(
        pid: Pid,
        name: String,
        parent: Option<Pid>,
        pgid: Pgid,
        sid: Sid,
        uid: Uid,
        gid: Gid,
        groups: Vec<Gid>,
        environ: HashMap<String, String>,
        cwd: PathBuf,
    ) -> Self {
        Self {
            pid,
            parent,
            pgid,
            sid,
            uid,
            gid,
            euid: uid,
            egid: gid,
            suid: uid,
            sgid: gid,
            groups,
            state: ProcessState::Running,
            files: FileTable::new(),
            memory: ProcessMemory::new(),
            signals: ProcessSignals::new(),
            environ,
            cwd,
            task: None,
            name,
            children: Vec::new(),
            ctty: None,
            is_session_leader: false,
        }
    }

    /// Create a process with a memory limit
    pub fn with_memory_limit(pid: Pid, name: String, parent: Option<Pid>, limit: usize) -> Self {
        let pgid = Pgid::from_pid(pid);
        let sid = Sid::from_pid(pid);
        let uid = Uid(1000);
        let gid = Gid(1000);

        let mut environ = HashMap::new();
        environ.insert("HOME".to_string(), "/home/user".to_string());
        environ.insert("USER".to_string(), "user".to_string());
        environ.insert("SHELL".to_string(), "/bin/sh".to_string());
        environ.insert("PATH".to_string(), "/bin:/usr/bin".to_string());
        environ.insert("TERM".to_string(), "xterm-256color".to_string());

        Self {
            pid,
            parent,
            pgid,
            sid,
            uid,
            gid,
            euid: uid,
            egid: gid,
            suid: uid,
            sgid: gid,
            groups: vec![gid],
            state: ProcessState::Running,
            files: FileTable::new(),
            memory: ProcessMemory::with_limit(limit),
            signals: ProcessSignals::new(),
            environ,
            cwd: PathBuf::from("/"),
            task: None,
            name,
            children: Vec::new(),
            ctty: None,
            is_session_leader: true,
        }
    }

    /// Create a login shell process for a user (like what login(1) does)
    #[allow(clippy::too_many_arguments)]
    pub fn new_login_shell(
        pid: Pid,
        name: String,
        parent: Option<Pid>,
        uid: Uid,
        gid: Gid,
        groups: Vec<Gid>,
        username: &str,
        home: &str,
        shell: &str,
    ) -> Self {
        // Login shells are session leaders with their own PGID and SID
        let pgid = Pgid::from_pid(pid);
        let sid = Sid::from_pid(pid);

        // Set up proper login environment
        let mut environ = HashMap::new();
        environ.insert("HOME".to_string(), home.to_string());
        environ.insert("USER".to_string(), username.to_string());
        environ.insert("LOGNAME".to_string(), username.to_string());
        environ.insert("SHELL".to_string(), shell.to_string());
        environ.insert(
            "PATH".to_string(),
            "/bin:/usr/bin:/usr/local/bin".to_string(),
        );
        environ.insert("TERM".to_string(), "xterm-256color".to_string());
        environ.insert("PWD".to_string(), home.to_string());

        Self {
            pid,
            parent,
            pgid,
            sid,
            uid,
            gid,
            euid: uid,
            egid: gid,
            suid: uid,
            sgid: gid,
            groups,
            state: ProcessState::Running,
            files: FileTable::new(),
            memory: ProcessMemory::new(),
            signals: ProcessSignals::new(),
            environ,
            cwd: PathBuf::from(home),
            task: None,
            name,
            children: Vec::new(),
            ctty: Some("tty1".to_string()),
            is_session_leader: true,
        }
    }

    pub fn is_session_leader(&self) -> bool {
        self.is_session_leader && self.sid.0 == self.pid.0
    }

    pub fn getsid(&self) -> Sid {
        self.sid
    }

    pub fn getenv(&self, name: &str) -> Option<&str> {
        self.environ.get(name).map(|s| s.as_str())
    }

    pub fn setenv(&mut self, name: &str, value: &str) {
        self.environ.insert(name.to_string(), value.to_string());
    }

    pub fn unsetenv(&mut self, name: &str) -> bool {
        self.environ.remove(name).is_some()
    }

    pub fn environ(&self) -> &HashMap<String, String> {
        &self.environ
    }

    pub fn is_alive(&self) -> bool {
        !matches!(self.state, ProcessState::Zombie(_))
    }

    pub fn is_stopped(&self) -> bool {
        matches!(self.state, ProcessState::Stopped)
    }

    pub fn can_run(&self) -> bool {
        matches!(self.state, ProcessState::Running | ProcessState::Sleeping)
    }

    /// Create a forked copy of this process with COW memory
    ///
    /// The child process gets:
    /// - New PID
    /// - This process as parent
    /// - Same pgid, sid, uid, gid, environ, cwd
    /// - Empty file table (caller must set up fds)
    /// - COW memory (shared pages until written)
    /// - Empty children list
    ///
    /// Returns a new Process that needs its fds and task set up by the caller.
    pub fn cow_fork<F>(
        &self,
        child_pid: Pid,
        region_id_generator: F,
    ) -> (
        Self,
        std::collections::HashMap<super::memory::RegionId, super::memory::RegionId>,
    )
    where
        F: FnMut() -> super::memory::RegionId,
    {
        // COW clone the memory
        let (child_memory, region_mapping) = self.memory.cow_fork(region_id_generator);

        let child = Self {
            pid: child_pid,
            parent: Some(self.pid),
            pgid: self.pgid, // Inherit process group
            sid: self.sid,   // Inherit session
            uid: self.uid,
            gid: self.gid,
            euid: self.euid,
            egid: self.egid,
            suid: self.suid, // Inherit saved UID
            sgid: self.sgid, // Inherit saved GID
            groups: self.groups.clone(),
            state: ProcessState::Running,
            files: FileTable::new(), // Caller sets up fds
            memory: child_memory,
            signals: super::signal::ProcessSignals::new(), // Fresh signal state
            environ: self.environ.clone(),
            cwd: self.cwd.clone(),
            task: None, // Caller sets up task
            name: self.name.clone(),
            children: Vec::new(), // No children yet
            ctty: self.ctty.clone(),
            is_session_leader: false, // Child is not session leader
        };

        (child, region_mapping)
    }
}

/// Maximum file descriptors per process (POSIX default is often 1024)
pub const MAX_FDS_PER_PROCESS: usize = 1024;

/// A process's file descriptor table
pub struct FileTable {
    /// Next fd to allocate
    next_fd: u32,
    /// Map from fd to kernel object handle
    table: HashMap<Fd, Handle>,
    /// Maximum number of file descriptors (can be adjusted per-process)
    max_fds: usize,
}

impl FileTable {
    pub fn new() -> Self {
        Self {
            next_fd: 3, // 0, 1, 2 reserved for stdin/stdout/stderr
            table: HashMap::new(),
            max_fds: MAX_FDS_PER_PROCESS,
        }
    }

    /// Create a new FileTable with a custom fd limit
    pub fn with_limit(max_fds: usize) -> Self {
        Self {
            next_fd: 3,
            table: HashMap::new(),
            max_fds,
        }
    }

    /// Allocate a new file descriptor for a handle
    /// Returns None if the fd limit has been reached
    pub fn alloc(&mut self, handle: Handle) -> Option<Fd> {
        // Check if we've hit the limit
        if self.table.len() >= self.max_fds {
            return None;
        }
        let fd = Fd(self.next_fd);
        self.next_fd += 1;
        self.table.insert(fd, handle);
        Some(fd)
    }

    /// Get the current number of open file descriptors
    pub fn len(&self) -> usize {
        self.table.len()
    }

    /// Check if the file table is empty
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    /// Get the maximum number of file descriptors allowed
    pub fn max_fds(&self) -> usize {
        self.max_fds
    }

    /// Set the maximum number of file descriptors (for rlimit)
    pub fn set_max_fds(&mut self, max: usize) {
        self.max_fds = max;
    }

    pub fn insert(&mut self, fd: Fd, handle: Handle) {
        self.table.insert(fd, handle);
    }

    pub fn get(&self, fd: Fd) -> Option<Handle> {
        self.table.get(&fd).copied()
    }

    pub fn remove(&mut self, fd: Fd) -> Option<Handle> {
        self.table.remove(&fd)
    }

    pub fn contains(&self, fd: Fd) -> bool {
        self.table.contains_key(&fd)
    }

    /// Clone the file table for fork
    /// Returns a new FileTable with the same mappings and fd limit
    pub fn clone_for_fork(&self) -> Self {
        Self {
            next_fd: self.next_fd,
            table: self.table.clone(),
            max_fds: self.max_fds,
        }
    }

    /// Get all file descriptors and their handles
    pub fn iter(&self) -> impl Iterator<Item = (Fd, Handle)> + '_ {
        self.table.iter().map(|(fd, h)| (*fd, *h))
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

        let fd1 = ft.alloc(h1).expect("should allocate fd");
        let fd2 = ft.alloc(h2).expect("should allocate fd");

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
        let fd = ft.alloc(h).expect("should allocate fd");

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

    #[test]
    fn test_file_table_fd_limit() {
        // Create a file table with a small limit for testing
        let mut ft = FileTable::with_limit(5);
        let h = Handle(100);

        // Should be able to allocate up to the limit
        for i in 0..5 {
            let fd = ft.alloc(h);
            assert!(fd.is_some(), "Should allocate fd #{}", i);
        }

        // Should fail when limit is reached
        let fd = ft.alloc(h);
        assert!(fd.is_none(), "Should fail when limit is reached");

        // Verify the count
        assert_eq!(ft.len(), 5);
        assert_eq!(ft.max_fds(), 5);
    }

    #[test]
    fn test_file_table_default_limit() {
        let ft = FileTable::new();
        assert_eq!(ft.max_fds(), MAX_FDS_PER_PROCESS);
    }
}
