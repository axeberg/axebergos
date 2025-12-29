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

/// Resource limit type (like Linux RLIMIT_*)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum RlimitResource {
    /// Maximum number of open file descriptors
    NoFile = 0,
    /// Maximum number of processes for this user
    NProc = 1,
    /// Maximum file size that can be created (bytes)
    FSize = 2,
    /// Maximum stack size (bytes)
    Stack = 3,
    /// Maximum CPU time (seconds)
    Cpu = 4,
    /// Maximum size of core dump file (bytes)
    Core = 5,
    /// Maximum data segment size (bytes)
    Data = 6,
    /// Maximum address space (bytes)
    As = 7,
}

impl RlimitResource {
    pub fn from_u32(n: u32) -> Option<Self> {
        match n {
            0 => Some(RlimitResource::NoFile),
            1 => Some(RlimitResource::NProc),
            2 => Some(RlimitResource::FSize),
            3 => Some(RlimitResource::Stack),
            4 => Some(RlimitResource::Cpu),
            5 => Some(RlimitResource::Core),
            6 => Some(RlimitResource::Data),
            7 => Some(RlimitResource::As),
            _ => None,
        }
    }
}

/// A single resource limit with soft and hard values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rlimit {
    /// Current (soft) limit - can be raised up to hard limit
    pub soft: u64,
    /// Maximum (hard) limit - only root can raise
    pub hard: u64,
}

impl Rlimit {
    pub const INFINITY: u64 = u64::MAX;

    pub fn new(soft: u64, hard: u64) -> Self {
        Self { soft, hard }
    }

    pub fn unlimited() -> Self {
        Self {
            soft: Self::INFINITY,
            hard: Self::INFINITY,
        }
    }
}

/// Resource limits for a process (like Linux rlimit)
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum number of open file descriptors (RLIMIT_NOFILE)
    pub nofile: Rlimit,
    /// Maximum number of processes for this user (RLIMIT_NPROC)
    pub nproc: Rlimit,
    /// Maximum file size (RLIMIT_FSIZE)
    pub fsize: Rlimit,
    /// Maximum stack size (RLIMIT_STACK)
    pub stack: Rlimit,
    /// Maximum CPU time in seconds (RLIMIT_CPU)
    pub cpu: Rlimit,
    /// Maximum core dump size (RLIMIT_CORE)
    pub core: Rlimit,
    /// Maximum data segment size (RLIMIT_DATA)
    pub data: Rlimit,
    /// Maximum address space (RLIMIT_AS)
    pub address_space: Rlimit,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            nofile: Rlimit::new(1024, 4096),
            nproc: Rlimit::new(1024, 4096),
            fsize: Rlimit::unlimited(),
            stack: Rlimit::new(8 * 1024 * 1024, Rlimit::INFINITY), // 8MB soft
            cpu: Rlimit::unlimited(),
            core: Rlimit::new(0, Rlimit::INFINITY), // Core dumps disabled by default
            data: Rlimit::unlimited(),
            address_space: Rlimit::unlimited(),
        }
    }
}

impl ResourceLimits {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, resource: RlimitResource) -> Rlimit {
        match resource {
            RlimitResource::NoFile => self.nofile,
            RlimitResource::NProc => self.nproc,
            RlimitResource::FSize => self.fsize,
            RlimitResource::Stack => self.stack,
            RlimitResource::Cpu => self.cpu,
            RlimitResource::Core => self.core,
            RlimitResource::Data => self.data,
            RlimitResource::As => self.address_space,
        }
    }

    pub fn set(&mut self, resource: RlimitResource, limit: Rlimit) {
        match resource {
            RlimitResource::NoFile => self.nofile = limit,
            RlimitResource::NProc => self.nproc = limit,
            RlimitResource::FSize => self.fsize = limit,
            RlimitResource::Stack => self.stack = limit,
            RlimitResource::Cpu => self.cpu = limit,
            RlimitResource::Core => self.core = limit,
            RlimitResource::Data => self.data = limit,
            RlimitResource::As => self.address_space = limit,
        }
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

    /// Resource limits (like Linux rlimit)
    pub rlimits: ResourceLimits,

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

    /// File mode creation mask (umask)
    /// Bits set in umask are cleared from the mode when creating files
    pub umask: u16,
}

/// Builder pattern for creating Process instances
///
/// This provides a more ergonomic way to create processes with many optional parameters.
///
/// # Example
/// ```ignore
/// let process = ProcessBuilder::new(pid, "my_process")
///     .parent(Some(parent_pid))
///     .uid(Uid::ROOT)
///     .gid(Gid::ROOT)
///     .cwd("/root")
///     .build();
/// ```
pub struct ProcessBuilder {
    pid: Pid,
    name: String,
    parent: Option<Pid>,
    pgid: Option<Pgid>,
    sid: Option<Sid>,
    uid: Uid,
    gid: Gid,
    euid: Option<Uid>,
    egid: Option<Gid>,
    groups: Vec<Gid>,
    environ: HashMap<String, String>,
    cwd: PathBuf,
    memory_limit: Option<usize>,
    is_session_leader: bool,
    umask: u16,
    ctty: Option<String>,
}

impl ProcessBuilder {
    /// Create a new process builder with required parameters
    pub fn new(pid: Pid, name: impl Into<String>) -> Self {
        Self {
            pid,
            name: name.into(),
            parent: None,
            pgid: None,
            sid: None,
            uid: Uid(1000),
            gid: Gid(1000),
            euid: None,
            egid: None,
            groups: Vec::new(),
            environ: HashMap::new(),
            cwd: PathBuf::from("/"),
            memory_limit: None,
            is_session_leader: true,
            umask: 0o022,
            ctty: None,
        }
    }

    /// Set the parent process
    pub fn parent(mut self, parent: Option<Pid>) -> Self {
        self.parent = parent;
        self
    }

    /// Set the process group ID
    pub fn pgid(mut self, pgid: Pgid) -> Self {
        self.pgid = Some(pgid);
        self
    }

    /// Set the session ID
    pub fn sid(mut self, sid: Sid) -> Self {
        self.sid = Some(sid);
        self
    }

    /// Set the real user ID
    pub fn uid(mut self, uid: Uid) -> Self {
        self.uid = uid;
        self
    }

    /// Set the real group ID
    pub fn gid(mut self, gid: Gid) -> Self {
        self.gid = gid;
        self
    }

    /// Set the effective user ID (defaults to uid if not set)
    pub fn euid(mut self, euid: Uid) -> Self {
        self.euid = Some(euid);
        self
    }

    /// Set the effective group ID (defaults to gid if not set)
    pub fn egid(mut self, egid: Gid) -> Self {
        self.egid = Some(egid);
        self
    }

    /// Set supplementary groups
    pub fn groups(mut self, groups: Vec<Gid>) -> Self {
        self.groups = groups;
        self
    }

    /// Set the environment variables
    pub fn environ(mut self, environ: HashMap<String, String>) -> Self {
        self.environ = environ;
        self
    }

    /// Add a single environment variable
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.environ.insert(key.into(), value.into());
        self
    }

    /// Set the current working directory
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = cwd.into();
        self
    }

    /// Set the memory limit
    pub fn memory_limit(mut self, limit: usize) -> Self {
        self.memory_limit = Some(limit);
        self
    }

    /// Set whether this is a session leader
    pub fn session_leader(mut self, is_leader: bool) -> Self {
        self.is_session_leader = is_leader;
        self
    }

    /// Set the file creation mask (umask)
    pub fn umask(mut self, umask: u16) -> Self {
        self.umask = umask;
        self
    }

    /// Set the controlling TTY
    pub fn ctty(mut self, ctty: impl Into<String>) -> Self {
        self.ctty = Some(ctty.into());
        self
    }

    /// Build the Process instance
    pub fn build(self) -> Process {
        let pgid = self.pgid.unwrap_or_else(|| Pgid::from_pid(self.pid));
        let sid = self.sid.unwrap_or_else(|| Sid::from_pid(self.pid));
        let euid = self.euid.unwrap_or(self.uid);
        let egid = self.egid.unwrap_or(self.gid);
        let groups = if self.groups.is_empty() {
            vec![self.gid]
        } else {
            self.groups
        };

        let memory = if let Some(limit) = self.memory_limit {
            ProcessMemory::with_limit(limit)
        } else {
            ProcessMemory::new()
        };

        Process {
            pid: self.pid,
            parent: self.parent,
            pgid,
            sid,
            uid: self.uid,
            gid: self.gid,
            euid,
            egid,
            suid: self.uid,
            sgid: self.gid,
            groups,
            state: ProcessState::Running,
            files: FileTable::new(),
            memory,
            signals: ProcessSignals::new(),
            rlimits: ResourceLimits::new(),
            environ: self.environ,
            cwd: self.cwd,
            task: None,
            name: self.name,
            children: Vec::new(),
            ctty: self.ctty,
            is_session_leader: self.is_session_leader,
            umask: self.umask,
        }
    }
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
            rlimits: ResourceLimits::new(),
            environ,
            cwd: PathBuf::from("/"),
            task: None,
            name,
            children: Vec::new(),
            ctty: None,
            is_session_leader: true, // New processes are session leaders by default
            umask: 0o022,            // Default umask (files=644, dirs=755)
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
            rlimits: ResourceLimits::new(),
            environ,
            cwd,
            task: None,
            name,
            children: Vec::new(),
            ctty: None,
            is_session_leader: false,
            umask: 0o022,
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
            rlimits: ResourceLimits::new(),
            environ,
            cwd: PathBuf::from("/"),
            task: None,
            name,
            children: Vec::new(),
            ctty: None,
            is_session_leader: true,
            umask: 0o022,
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
            rlimits: ResourceLimits::new(),
            environ,
            cwd: PathBuf::from(home),
            task: None,
            name,
            children: Vec::new(),
            ctty: Some("tty1".to_string()),
            is_session_leader: true,
            umask: 0o022,
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
            rlimits: self.rlimits.clone(),                 // Inherit resource limits
            environ: self.environ.clone(),
            cwd: self.cwd.clone(),
            task: None, // Caller sets up task
            name: self.name.clone(),
            children: Vec::new(), // No children yet
            ctty: self.ctty.clone(),
            is_session_leader: false, // Child is not session leader
            umask: self.umask,        // Inherit umask
        };

        (child, region_mapping)
    }
}

/// Maximum file descriptors per process (POSIX default is often 1024)
pub const MAX_FDS_PER_PROCESS: usize = 1024;

/// File descriptor flags (for fcntl F_GETFD/F_SETFD)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FdFlags {
    /// Close file descriptor on exec (FD_CLOEXEC)
    pub cloexec: bool,
}

impl FdFlags {
    pub const CLOEXEC: u32 = 1;

    pub fn from_bits(bits: u32) -> Self {
        Self {
            cloexec: bits & Self::CLOEXEC != 0,
        }
    }

    pub fn to_bits(self) -> u32 {
        if self.cloexec { Self::CLOEXEC } else { 0 }
    }
}

/// A process's file descriptor table
pub struct FileTable {
    /// Next fd to allocate
    next_fd: u32,
    /// Map from fd to kernel object handle
    table: HashMap<Fd, Handle>,
    /// Map from fd to fd flags (FD_CLOEXEC, etc.)
    flags: HashMap<Fd, FdFlags>,
    /// Maximum number of file descriptors (can be adjusted per-process)
    max_fds: usize,
}

impl FileTable {
    pub fn new() -> Self {
        Self {
            next_fd: 3, // 0, 1, 2 reserved for stdin/stdout/stderr
            table: HashMap::new(),
            flags: HashMap::new(),
            max_fds: MAX_FDS_PER_PROCESS,
        }
    }

    /// Create a new FileTable with a custom fd limit
    pub fn with_limit(max_fds: usize) -> Self {
        Self {
            next_fd: 3,
            table: HashMap::new(),
            flags: HashMap::new(),
            max_fds,
        }
    }

    /// Allocate a new file descriptor for a handle
    /// Returns None if the fd limit has been reached
    pub fn alloc(&mut self, handle: Handle) -> Option<Fd> {
        self.alloc_with_flags(handle, FdFlags::default())
    }

    /// Allocate a new file descriptor for a handle with specific flags
    /// Returns None if the fd limit has been reached
    pub fn alloc_with_flags(&mut self, handle: Handle, fd_flags: FdFlags) -> Option<Fd> {
        // Check if we've hit the limit
        if self.table.len() >= self.max_fds {
            return None;
        }
        let fd = Fd(self.next_fd);
        self.next_fd += 1;
        self.table.insert(fd, handle);
        self.flags.insert(fd, fd_flags);
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
        self.flags.insert(fd, FdFlags::default());
    }

    pub fn get(&self, fd: Fd) -> Option<Handle> {
        self.table.get(&fd).copied()
    }

    pub fn remove(&mut self, fd: Fd) -> Option<Handle> {
        self.flags.remove(&fd);
        self.table.remove(&fd)
    }

    pub fn contains(&self, fd: Fd) -> bool {
        self.table.contains_key(&fd)
    }

    /// Get the flags for a file descriptor
    pub fn get_flags(&self, fd: Fd) -> Option<FdFlags> {
        self.flags.get(&fd).copied()
    }

    /// Set the flags for a file descriptor
    pub fn set_flags(&mut self, fd: Fd, fd_flags: FdFlags) -> bool {
        if self.table.contains_key(&fd) {
            self.flags.insert(fd, fd_flags);
            true
        } else {
            false
        }
    }

    /// Clone the file table for fork
    /// Returns a new FileTable with the same mappings, flags, and fd limit
    pub fn clone_for_fork(&self) -> Self {
        Self {
            next_fd: self.next_fd,
            table: self.table.clone(),
            flags: self.flags.clone(),
            max_fds: self.max_fds,
        }
    }

    /// Clone the file table for exec, excluding FDs with FD_CLOEXEC set
    /// Returns a new FileTable with only non-CLOEXEC file descriptors
    pub fn clone_for_exec(&self) -> Self {
        let mut new_table = HashMap::new();
        let mut new_flags = HashMap::new();

        for (fd, handle) in &self.table {
            let fd_flags = self.flags.get(fd).copied().unwrap_or_default();
            if !fd_flags.cloexec {
                new_table.insert(*fd, *handle);
                new_flags.insert(*fd, fd_flags);
            }
        }

        Self {
            next_fd: self.next_fd,
            table: new_table,
            flags: new_flags,
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
