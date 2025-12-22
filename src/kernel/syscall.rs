//! System call interface
//!
//! This is the boundary between user code and the kernel. All resource
//! access goes through these syscalls. This provides:
//! - Isolation: processes can only access what they have handles to
//! - Auditing: all operations go through a single point
//! - Safety: the kernel validates all operations

use super::memory::{
    MemoryError, MemoryManager, MemoryStats, Protection, RegionId, ShmId, ShmInfo,
    SystemMemoryStats,
};
use super::object::{
    ConsoleObject, FileObject, KernelObject, ObjectTable, PipeObject, WindowId, WindowObject,
};
pub use super::process::{Fd, Handle, OpenFlags, Pid, Process, ProcessState};
use super::signal::{resolve_action, Signal, SignalAction, SignalError};
use super::task::TaskId;
use super::timer::{TimerId, TimerQueue};
use super::trace::{TraceCategory, TraceSummary, Tracer};
use crate::vfs::{FileSystem, MemoryFs, OpenOptions as VfsOpenOptions};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};

/// System call error
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyscallError {
    /// Invalid file descriptor
    BadFd,
    /// File or path not found
    NotFound,
    /// Permission denied
    PermissionDenied,
    /// Invalid argument
    InvalidArgument,
    /// Would block (for non-blocking I/O)
    WouldBlock,
    /// Pipe/connection closed
    BrokenPipe,
    /// Resource busy
    Busy,
    /// Invalid data (e.g., invalid UTF-8)
    InvalidData,
    /// No such process
    NoProcess,
    /// Generic I/O error
    Io(String),
    /// Memory error
    Memory(MemoryError),
    /// Signal error
    Signal(SignalError),
    /// Process was interrupted by signal
    Interrupted,
}

impl std::fmt::Display for SyscallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyscallError::BadFd => write!(f, "bad file descriptor"),
            SyscallError::NotFound => write!(f, "not found"),
            SyscallError::PermissionDenied => write!(f, "permission denied"),
            SyscallError::InvalidArgument => write!(f, "invalid argument"),
            SyscallError::WouldBlock => write!(f, "would block"),
            SyscallError::BrokenPipe => write!(f, "broken pipe"),
            SyscallError::Busy => write!(f, "resource busy"),
            SyscallError::InvalidData => write!(f, "invalid data"),
            SyscallError::NoProcess => write!(f, "no such process"),
            SyscallError::Io(msg) => write!(f, "I/O error: {}", msg),
            SyscallError::Memory(e) => write!(f, "memory error: {}", e),
            SyscallError::Signal(e) => write!(f, "signal error: {}", e),
            SyscallError::Interrupted => write!(f, "interrupted by signal"),
        }
    }
}

impl From<MemoryError> for SyscallError {
    fn from(e: MemoryError) -> Self {
        SyscallError::Memory(e)
    }
}

impl From<SignalError> for SyscallError {
    fn from(e: SignalError) -> Self {
        SyscallError::Signal(e)
    }
}

impl From<std::io::Error> for SyscallError {
    fn from(e: std::io::Error) -> Self {
        use std::io::ErrorKind;
        match e.kind() {
            ErrorKind::NotFound => SyscallError::NotFound,
            ErrorKind::PermissionDenied => SyscallError::PermissionDenied,
            ErrorKind::WouldBlock => SyscallError::WouldBlock,
            ErrorKind::BrokenPipe => SyscallError::BrokenPipe,
            ErrorKind::InvalidInput => SyscallError::InvalidArgument,
            _ => SyscallError::Io(e.to_string()),
        }
    }
}

/// File metadata returned by the metadata syscall
#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub size: u64,
    pub is_dir: bool,
    pub is_file: bool,
}

pub type SyscallResult<T> = Result<T, SyscallError>;

/// The kernel state - manages all processes and objects
pub struct Kernel {
    /// All processes
    processes: HashMap<Pid, Process>,
    /// Next PID to allocate
    next_pid: u32,
    /// Global object table
    objects: ObjectTable,
    /// The current running process
    current: Option<Pid>,
    /// Console object handle (shared by all)
    console_handle: Handle,
    /// The virtual filesystem
    vfs: MemoryFs,
    /// Map from our Handle to VFS FileHandle for open files
    vfs_handles: HashMap<Handle, usize>,
    /// Memory manager for shared memory and accounting
    memory: MemoryManager,
    /// Timer queue
    timers: TimerQueue,
    /// Current monotonic time (updated by tick)
    now: f64,
    /// Tracer for instrumentation and debugging
    tracer: Tracer,
}

impl Kernel {
    pub fn new() -> Self {
        let mut objects = ObjectTable::new();

        // Create the console device
        let console = ConsoleObject::new();
        let console_handle = objects.insert(KernelObject::Console(console));

        // Create and initialize the VFS
        let mut vfs = MemoryFs::new();
        // Create standard directories
        let _ = vfs.create_dir("/dev");
        let _ = vfs.create_dir("/home");
        let _ = vfs.create_dir("/tmp");
        let _ = vfs.create_dir("/etc");

        Self {
            processes: HashMap::new(),
            next_pid: 1, // PID 0 is reserved
            objects,
            current: None,
            console_handle,
            vfs,
            vfs_handles: HashMap::new(),
            memory: MemoryManager::new(),
            timers: TimerQueue::new(),
            now: 0.0,
            tracer: Tracer::new(),
        }
    }

    /// Get a reference to the VFS
    pub fn vfs(&self) -> &MemoryFs {
        &self.vfs
    }

    /// Get a mutable reference to the VFS
    pub fn vfs_mut(&mut self) -> &mut MemoryFs {
        &mut self.vfs
    }

    /// Replace the VFS (for restoring from persistence)
    pub fn set_vfs(&mut self, vfs: MemoryFs) {
        self.vfs = vfs;
    }

    /// Get the currently running process
    pub fn current_process(&self) -> Option<&Process> {
        self.current.and_then(|pid| self.processes.get(&pid))
    }

    /// Get the currently running process mutably
    pub fn current_process_mut(&mut self) -> Option<&mut Process> {
        self.current.and_then(|pid| self.processes.get_mut(&pid))
    }

    /// Set the current process
    pub fn set_current(&mut self, pid: Pid) {
        self.current = Some(pid);
    }

    /// Create a new process
    pub fn spawn_process(&mut self, name: &str, parent: Option<Pid>) -> Pid {
        let pid = Pid(self.next_pid);
        self.next_pid += 1;

        let mut process = Process::new(pid, name.to_string(), parent);

        // Give the process stdin/stdout/stderr pointing to console
        // Retain the console handle for each fd (3 references)
        self.objects.retain(self.console_handle);
        self.objects.retain(self.console_handle);
        self.objects.retain(self.console_handle);

        process.files.insert(Fd::STDIN, self.console_handle);
        process.files.insert(Fd::STDOUT, self.console_handle);
        process.files.insert(Fd::STDERR, self.console_handle);

        self.processes.insert(pid, process);
        pid
    }

    /// Get a process by PID
    pub fn get_process(&self, pid: Pid) -> Option<&Process> {
        self.processes.get(&pid)
    }

    /// Get a process mutably
    pub fn get_process_mut(&mut self, pid: Pid) -> Option<&mut Process> {
        self.processes.get_mut(&pid)
    }

    /// Get the console for input/output
    pub fn console(&mut self) -> Option<&mut ConsoleObject> {
        match self.objects.get_mut(self.console_handle) {
            Some(KernelObject::Console(c)) => Some(c),
            _ => None,
        }
    }

    // ========== TIMER/TICK ==========

    /// Process a kernel tick - updates timers and returns tasks to wake
    ///
    /// Call this each frame with the current monotonic time (e.g., performance.now()).
    /// Returns a list of task IDs that should be woken (timers expired).
    pub fn tick(&mut self, now: f64) -> Vec<TaskId> {
        self.now = now;
        self.timers.tick(now)
    }

    /// Get the current kernel time
    pub fn now(&self) -> f64 {
        self.now
    }

    // ========== TRACING ==========

    /// Enable tracing
    pub fn trace_enable(&mut self) {
        self.tracer.enable();
        self.tracer.set_start_time(self.now);
    }

    /// Disable tracing
    pub fn trace_disable(&mut self) {
        self.tracer.disable();
    }

    /// Check if tracing is enabled
    pub fn trace_enabled(&self) -> bool {
        self.tracer.is_enabled()
    }

    /// Get trace summary
    pub fn trace_summary(&self) -> TraceSummary {
        self.tracer.summary(self.now)
    }

    /// Get the tracer (for detailed access)
    pub fn tracer(&self) -> &Tracer {
        &self.tracer
    }

    /// Get the tracer mutably
    pub fn tracer_mut(&mut self) -> &mut Tracer {
        &mut self.tracer
    }

    /// Reset trace data
    pub fn trace_reset(&mut self) {
        self.tracer.reset();
    }

    // ========== SYSCALLS ==========

    /// Open a file or device
    pub fn sys_open(&mut self, path: &str, flags: OpenFlags) -> SyscallResult<Fd> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;

        // Resolve path
        let resolved = self.resolve_path(current, path)?;

        // Handle special paths
        let handle = if resolved.starts_with("/dev/") {
            self.open_device(&resolved, flags)?
        } else {
            self.open_file(&resolved, flags)?
        };

        // Add to process file table
        let process = self.processes.get_mut(&current).unwrap();
        let fd = process.files.alloc(handle);
        Ok(fd)
    }

    /// Read from a file descriptor
    pub fn sys_read(&mut self, fd: Fd, buf: &mut [u8]) -> SyscallResult<usize> {
        let handle = self.get_handle(fd)?;
        let obj = self.objects.get_mut(handle).ok_or(SyscallError::BadFd)?;
        Ok(obj.read(buf)?)
    }

    /// Write to a file descriptor
    pub fn sys_write(&mut self, fd: Fd, buf: &[u8]) -> SyscallResult<usize> {
        let handle = self.get_handle(fd)?;
        let obj = self.objects.get_mut(handle).ok_or(SyscallError::BadFd)?;
        Ok(obj.write(buf)?)
    }

    /// Close a file descriptor
    pub fn sys_close(&mut self, fd: Fd) -> SyscallResult<()> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let process = self.processes.get_mut(&current).unwrap();

        let handle = process.files.remove(fd).ok_or(SyscallError::BadFd)?;

        // Sync file to VFS if it's a file (before potential release)
        if let Some(KernelObject::File(_)) = self.objects.get(handle) {
            self.sync_file(handle)?;
        }

        // Release the handle (decrements refcount)
        // If refcount drops to 0, the object is removed
        if let Some(_removed_object) = self.objects.release(handle) {
            // Object was deallocated - clean up VFS handle if present
            if let Some(vh) = self.vfs_handles.remove(&handle) {
                let _ = self.vfs.close(vh);
            }
        }

        Ok(())
    }

    /// Seek within a file
    pub fn sys_seek(&mut self, fd: Fd, pos: SeekFrom) -> SyscallResult<u64> {
        let handle = self.get_handle(fd)?;
        let obj = self.objects.get_mut(handle).ok_or(SyscallError::BadFd)?;
        Ok(obj.seek(pos)?)
    }

    /// Create a pipe (returns read_fd, write_fd)
    pub fn sys_pipe(&mut self) -> SyscallResult<(Fd, Fd)> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;

        // Create pipe object
        let pipe = PipeObject::new(4096);
        let handle = self.objects.insert(KernelObject::Pipe(pipe));

        // Allocate two fds pointing to same pipe
        // (In a real OS these would be separate read/write ends)
        let process = self.processes.get_mut(&current).unwrap();
        let read_fd = process.files.alloc(handle);
        let write_fd = process.files.alloc(handle);

        Ok((read_fd, write_fd))
    }

    /// Create a window (returns fd for the window)
    pub fn sys_window_create(&mut self, _title: &str) -> SyscallResult<Fd> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;

        // For now, use a placeholder window ID
        // The compositor integration will make this real
        static NEXT_WINDOW_ID: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(1);
        let window_id = WindowId(NEXT_WINDOW_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed));

        let window = WindowObject::new(window_id);
        let handle = self.objects.insert(KernelObject::Window(window));

        let process = self.processes.get_mut(&current).unwrap();
        let fd = process.files.alloc(handle);

        Ok(fd)
    }

    /// Duplicate a file descriptor
    pub fn sys_dup(&mut self, fd: Fd) -> SyscallResult<Fd> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let process = self.processes.get_mut(&current).unwrap();

        // Get the handle for the existing fd
        let handle = process.files.get(fd).ok_or(SyscallError::BadFd)?;

        // Retain the object (increment refcount)
        if !self.objects.retain(handle) {
            return Err(SyscallError::BadFd);
        }

        // Allocate a new fd pointing to the same handle
        let new_fd = process.files.alloc(handle);
        Ok(new_fd)
    }

    /// Get current working directory
    pub fn sys_getcwd(&self) -> SyscallResult<PathBuf> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let process = self.processes.get(&current).unwrap();
        Ok(process.cwd.clone())
    }

    /// Change working directory
    pub fn sys_chdir(&mut self, path: &str) -> SyscallResult<()> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let resolved = self.resolve_path(current, path)?;

        // TODO: verify path exists and is a directory

        let process = self.processes.get_mut(&current).unwrap();
        process.cwd = resolved;
        Ok(())
    }

    /// Exit the current process
    pub fn sys_exit(&mut self, code: i32) -> SyscallResult<()> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let process = self.processes.get_mut(&current).unwrap();
        process.state = ProcessState::Zombie(code);
        Ok(())
    }

    /// Get current process ID
    pub fn sys_getpid(&self) -> SyscallResult<Pid> {
        self.current.ok_or(SyscallError::NoProcess)
    }

    // ========== HELPERS ==========

    /// Get a handle from the current process's file table
    fn get_handle(&self, fd: Fd) -> SyscallResult<Handle> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let process = self.processes.get(&current).unwrap();
        process.files.get(fd).ok_or(SyscallError::BadFd)
    }

    /// Resolve a path relative to a process's cwd
    fn resolve_path(&self, pid: Pid, path: &str) -> SyscallResult<PathBuf> {
        let process = self.processes.get(&pid).ok_or(SyscallError::NoProcess)?;

        let path = Path::new(path);
        if path.is_absolute() {
            Ok(path.to_path_buf())
        } else {
            Ok(process.cwd.join(path))
        }
    }

    /// Open a device (paths starting with /dev/)
    fn open_device(&mut self, path: &Path, _flags: OpenFlags) -> SyscallResult<Handle> {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or(SyscallError::NotFound)?;

        match name {
            "console" => Ok(self.console_handle),
            "null" => {
                // /dev/null - discard all writes, return EOF on read
                // For now, just return a file object that does nothing special
                let file = FileObject::new(path.to_path_buf(), Vec::new(), true, true);
                Ok(self.objects.insert(KernelObject::File(file)))
            }
            "zero" => {
                // /dev/zero - returns infinite zeros
                // For now, return a file with some zeros
                let file = FileObject::new(path.to_path_buf(), vec![0; 4096], true, false);
                Ok(self.objects.insert(KernelObject::File(file)))
            }
            _ => Err(SyscallError::NotFound),
        }
    }

    /// Open a regular file
    fn open_file(&mut self, path: &Path, flags: OpenFlags) -> SyscallResult<Handle> {
        let path_str = path.to_str().ok_or(SyscallError::InvalidArgument)?;

        // Convert our flags to VFS options
        // Note: We always need read access in VFS to read existing content into FileObject,
        // but the actual permissions are tracked separately in the FileObject
        let vfs_opts = VfsOpenOptions {
            read: true, // Always need to read existing content
            write: flags.write,
            create: flags.create,
            truncate: flags.truncate,
        };

        // Open via VFS
        let vfs_handle = self.vfs.open(path_str, vfs_opts)?;

        // Read the file contents
        let meta = self.vfs.metadata(path_str)?;
        let mut data = vec![0u8; meta.size as usize];
        if !data.is_empty() {
            self.vfs.read(vfs_handle, &mut data)?;
        }

        // For append mode, seek to end
        if flags.append {
            self.vfs.seek(vfs_handle, SeekFrom::End(0))?;
        } else {
            // Seek back to start
            self.vfs.seek(vfs_handle, SeekFrom::Start(0))?;
        }

        // Create a FileObject that mirrors the VFS file
        // For append mode, position at end
        let mut file = FileObject::new(path.to_path_buf(), data, flags.read, flags.write);
        if flags.append {
            file.position = file.data.len() as u64;
        }

        let handle = self.objects.insert(KernelObject::File(file));

        // Track the VFS handle for sync/close
        self.vfs_handles.insert(handle, vfs_handle);

        Ok(handle)
    }

    /// Sync a file back to the VFS (on close or explicit sync)
    fn sync_file(&mut self, handle: Handle) -> SyscallResult<()> {
        let vfs_handle = self.vfs_handles.get(&handle).copied();

        if let Some(vh) = vfs_handle {
            // Get the file data
            if let Some(KernelObject::File(file)) = self.objects.get(handle) {
                let data = file.data.clone();
                let path = file.path.clone();

                // Write back to VFS
                self.vfs.seek(vh, SeekFrom::Start(0))?;

                // Truncate and write
                let path_str = path.to_str().ok_or(SyscallError::InvalidArgument)?;

                // Close old handle and reopen with truncate
                let _ = self.vfs.close(vh);
                let new_vh = self.vfs.open(
                    path_str,
                    VfsOpenOptions {
                        read: false,
                        write: true,
                        create: true,
                        truncate: true,
                    },
                )?;
                self.vfs.write(new_vh, &data)?;
                self.vfs_handles.insert(handle, new_vh);
            }
        }
        Ok(())
    }

    /// Create a directory
    pub fn sys_mkdir(&mut self, path: &str) -> SyscallResult<()> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let resolved = self.resolve_path(current, path)?;
        let path_str = resolved.to_str().ok_or(SyscallError::InvalidArgument)?;
        self.vfs.create_dir(path_str)?;
        Ok(())
    }

    /// List directory contents
    pub fn sys_readdir(&mut self, path: &str) -> SyscallResult<Vec<String>> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let resolved = self.resolve_path(current, path)?;
        let path_str = resolved.to_str().ok_or(SyscallError::InvalidArgument)?;
        let entries = self.vfs.read_dir(path_str)?;
        Ok(entries.into_iter().map(|e| e.name).collect())
    }

    /// Check if a path exists
    pub fn sys_exists(&self, path: &str) -> SyscallResult<bool> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let resolved = self.resolve_path(current, path)?;
        let path_str = resolved.to_str().ok_or(SyscallError::InvalidArgument)?;
        Ok(self.vfs.exists(path_str))
    }

    /// Get file/directory metadata
    pub fn sys_metadata(&self, path: &str) -> SyscallResult<FileMetadata> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let resolved = self.resolve_path(current, path)?;
        let path_str = resolved.to_str().ok_or(SyscallError::InvalidArgument)?;
        let meta = self.vfs.metadata(path_str)?;
        Ok(FileMetadata {
            size: meta.size,
            is_dir: meta.is_dir,
            is_file: meta.is_file,
        })
    }

    /// Remove a file
    pub fn sys_remove_file(&mut self, path: &str) -> SyscallResult<()> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let resolved = self.resolve_path(current, path)?;
        let path_str = resolved.to_str().ok_or(SyscallError::InvalidArgument)?;
        self.vfs.remove_file(path_str)?;
        Ok(())
    }

    /// Remove a directory (must be empty)
    pub fn sys_remove_dir(&mut self, path: &str) -> SyscallResult<()> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let resolved = self.resolve_path(current, path)?;
        let path_str = resolved.to_str().ok_or(SyscallError::InvalidArgument)?;
        self.vfs.remove_dir(path_str)?;
        Ok(())
    }

    /// Rename/move a file or directory
    pub fn sys_rename(&mut self, from: &str, to: &str) -> SyscallResult<()> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let from_resolved = self.resolve_path(current, from)?;
        let to_resolved = self.resolve_path(current, to)?;
        let from_str = from_resolved.to_str().ok_or(SyscallError::InvalidArgument)?;
        let to_str = to_resolved.to_str().ok_or(SyscallError::InvalidArgument)?;
        self.vfs.rename(from_str, to_str)?;
        Ok(())
    }

    /// Copy a file
    pub fn sys_copy_file(&mut self, from: &str, to: &str) -> SyscallResult<u64> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let from_resolved = self.resolve_path(current, from)?;
        let to_resolved = self.resolve_path(current, to)?;
        let from_str = from_resolved.to_str().ok_or(SyscallError::InvalidArgument)?;
        let to_str = to_resolved.to_str().ok_or(SyscallError::InvalidArgument)?;
        let size = self.vfs.copy_file(from_str, to_str)?;
        Ok(size)
    }

    // ========== MEMORY SYSCALLS ==========

    /// Allocate a memory region for the current process
    pub fn sys_alloc(&mut self, size: usize, prot: Protection) -> SyscallResult<RegionId> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let process = self.processes.get_mut(&current).ok_or(SyscallError::NoProcess)?;

        let region_id = self.memory.alloc_region_id();
        process.memory.allocate(region_id, size, prot)?;

        Ok(region_id)
    }

    /// Free a memory region
    pub fn sys_free(&mut self, region_id: RegionId) -> SyscallResult<()> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let process = self.processes.get_mut(&current).ok_or(SyscallError::NoProcess)?;

        process.memory.free(region_id)?;
        Ok(())
    }

    /// Read from a memory region
    pub fn sys_mem_read(
        &mut self,
        region_id: RegionId,
        offset: usize,
        buf: &mut [u8],
    ) -> SyscallResult<usize> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let process = self.processes.get_mut(&current).ok_or(SyscallError::NoProcess)?;

        let region = process.memory.get(region_id).ok_or(SyscallError::Memory(
            MemoryError::InvalidRegion,
        ))?;

        Ok(region.read(offset, buf)?)
    }

    /// Write to a memory region
    pub fn sys_mem_write(
        &mut self,
        region_id: RegionId,
        offset: usize,
        buf: &[u8],
    ) -> SyscallResult<usize> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let process = self.processes.get_mut(&current).ok_or(SyscallError::NoProcess)?;

        let region = process.memory.get_mut(region_id).ok_or(SyscallError::Memory(
            MemoryError::InvalidRegion,
        ))?;

        Ok(region.write(offset, buf)?)
    }

    /// Create a shared memory segment
    pub fn sys_shmget(&mut self, size: usize) -> SyscallResult<ShmId> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        Ok(self.memory.shmget(size, current)?)
    }

    /// Attach to a shared memory segment
    pub fn sys_shmat(&mut self, shm_id: ShmId, prot: Protection) -> SyscallResult<RegionId> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;

        // Get the shared memory region from the manager
        let region = self.memory.shmat(shm_id, current, prot)?;
        let region_id = region.id;

        // Attach to the process memory
        let process = self.processes.get_mut(&current).ok_or(SyscallError::NoProcess)?;
        process.memory.attach_shm(shm_id, region)?;

        Ok(region_id)
    }

    /// Detach from a shared memory segment
    pub fn sys_shmdt(&mut self, shm_id: ShmId) -> SyscallResult<()> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;

        // Sync changes back to shared memory before detaching
        let process = self.processes.get_mut(&current).ok_or(SyscallError::NoProcess)?;

        // Get the region data before detaching
        if let Some(region_id) = process.memory.shm_region(shm_id) {
            if let Some(region) = process.memory.get(region_id) {
                let data = region.as_slice().to_vec();
                self.memory.shm_sync(shm_id, &data)?;
            }
        }

        // Detach from process memory
        process.memory.detach_shm(shm_id)?;

        // Detach from global shared memory (may remove if refcount hits 0)
        self.memory.shmdt(shm_id, current)?;

        Ok(())
    }

    /// Sync shared memory region (write local changes to shared segment)
    pub fn sys_shm_sync(&mut self, shm_id: ShmId) -> SyscallResult<()> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let process = self.processes.get(&current).ok_or(SyscallError::NoProcess)?;

        // Get the region data
        let region_id = process.memory.shm_region(shm_id).ok_or(SyscallError::Memory(
            MemoryError::NotAttached,
        ))?;

        let region = process.memory.get(region_id).ok_or(SyscallError::Memory(
            MemoryError::InvalidRegion,
        ))?;

        let data = region.as_slice().to_vec();
        self.memory.shm_sync(shm_id, &data)?;

        Ok(())
    }

    /// Refresh local shared memory region from shared segment
    pub fn sys_shm_refresh(&mut self, shm_id: ShmId) -> SyscallResult<()> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;

        // Get the latest shared data
        let data = self.memory.shm_read(shm_id)?.to_vec();

        // Update local region
        let process = self.processes.get_mut(&current).ok_or(SyscallError::NoProcess)?;
        let region_id = process.memory.shm_region(shm_id).ok_or(SyscallError::Memory(
            MemoryError::NotAttached,
        ))?;

        let region = process.memory.get_mut(region_id).ok_or(SyscallError::Memory(
            MemoryError::InvalidRegion,
        ))?;

        region.write(0, &data)?;
        Ok(())
    }

    /// Get shared memory info
    pub fn sys_shm_info(&self, shm_id: ShmId) -> SyscallResult<ShmInfo> {
        Ok(self.memory.shm_info(shm_id)?)
    }

    /// List all shared memory segments
    pub fn sys_shm_list(&self) -> SyscallResult<Vec<ShmInfo>> {
        Ok(self.memory.shm_list())
    }

    /// Get memory stats for current process
    pub fn sys_memstats(&self) -> SyscallResult<MemoryStats> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let process = self.processes.get(&current).ok_or(SyscallError::NoProcess)?;
        Ok(process.memory.stats())
    }

    /// Set memory limit for current process
    pub fn sys_set_memlimit(&mut self, limit: usize) -> SyscallResult<()> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let process = self.processes.get_mut(&current).ok_or(SyscallError::NoProcess)?;
        process.memory.set_limit(limit);
        Ok(())
    }

    /// Get system-wide memory stats
    pub fn sys_system_memstats(&self) -> SyscallResult<SystemMemoryStats> {
        Ok(self.memory.system_stats())
    }

    // ========== TIMER SYSCALLS ==========

    /// Get current kernel time
    pub fn sys_now(&self) -> f64 {
        self.now
    }

    /// Set current kernel time (called from runtime with rAF timestamp)
    pub fn set_time(&mut self, now: f64) {
        self.now = now;
    }

    /// Schedule a one-shot timer
    pub fn sys_timer_set(&mut self, delay_ms: f64, wake_task: Option<TaskId>) -> SyscallResult<TimerId> {
        if delay_ms < 0.0 {
            return Err(SyscallError::InvalidArgument);
        }
        Ok(self.timers.schedule(delay_ms, self.now, wake_task))
    }

    /// Schedule a repeating interval timer
    pub fn sys_timer_interval(&mut self, interval_ms: f64, wake_task: Option<TaskId>) -> SyscallResult<TimerId> {
        if interval_ms <= 0.0 {
            return Err(SyscallError::InvalidArgument);
        }
        Ok(self.timers.schedule_interval(interval_ms, self.now, wake_task))
    }

    /// Cancel a timer
    pub fn sys_timer_cancel(&mut self, timer_id: TimerId) -> SyscallResult<bool> {
        Ok(self.timers.cancel(timer_id))
    }

    /// Check if a timer is pending
    pub fn sys_timer_pending(&self, timer_id: TimerId) -> SyscallResult<bool> {
        Ok(self.timers.is_pending(timer_id))
    }

    /// Get time until next timer fires (for sleep optimization)
    pub fn time_until_next_timer(&self) -> Option<f64> {
        self.timers.time_until_next(self.now)
    }

    /// Get pending timer count
    pub fn pending_timer_count(&self) -> usize {
        self.timers.pending_count()
    }

    /// Tick timers, returning tasks to wake
    pub fn tick_timers(&mut self) -> Vec<TaskId> {
        self.timers.tick(self.now)
    }

    /// Set an alarm for the current process (SIGALRM after delay)
    pub fn sys_alarm(&mut self, delay_ms: f64) -> SyscallResult<TimerId> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let process = self.processes.get(&current).ok_or(SyscallError::NoProcess)?;
        let task = process.task;
        self.sys_timer_set(delay_ms, task)
    }

    // ========== SIGNAL SYSCALLS ==========

    /// Send a signal to a process
    pub fn sys_kill(&mut self, pid: Pid, signal: Signal) -> SyscallResult<()> {
        let process = self.processes.get_mut(&pid).ok_or(SyscallError::NoProcess)?;

        // Can't signal zombies
        if matches!(process.state, ProcessState::Zombie(_)) {
            return Err(SyscallError::NoProcess);
        }

        // Queue the signal
        process.signals.send(signal);

        Ok(())
    }

    /// Set signal handler for current process
    pub fn sys_signal(&mut self, signal: Signal, action: SignalAction) -> SyscallResult<SignalAction> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let process = self.processes.get_mut(&current).ok_or(SyscallError::NoProcess)?;

        let old_action = process.signals.disposition.get_action(signal);
        process.signals.disposition.set_action(signal, action)?;

        Ok(old_action)
    }

    /// Block a signal for current process
    pub fn sys_sigblock(&mut self, signal: Signal) -> SyscallResult<()> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let process = self.processes.get_mut(&current).ok_or(SyscallError::NoProcess)?;
        process.signals.block(signal)?;
        Ok(())
    }

    /// Unblock a signal for current process
    pub fn sys_sigunblock(&mut self, signal: Signal) -> SyscallResult<()> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let process = self.processes.get_mut(&current).ok_or(SyscallError::NoProcess)?;
        process.signals.unblock(signal);
        Ok(())
    }

    /// Check if current process has pending signals
    pub fn sys_sigpending(&self) -> SyscallResult<bool> {
        let current = self.current.ok_or(SyscallError::NoProcess)?;
        let process = self.processes.get(&current).ok_or(SyscallError::NoProcess)?;
        Ok(process.signals.has_pending())
    }

    /// Process pending signals for a process, returns action to take
    pub fn process_signals(&mut self, pid: Pid) -> Option<(Signal, SignalAction)> {
        let process = self.processes.get_mut(&pid)?;

        // Get next pending signal
        let signal = process.signals.next_pending()?;

        // Resolve the action
        let action = resolve_action(signal, &process.signals.disposition);

        // Apply immediate actions
        match action {
            SignalAction::Kill | SignalAction::Terminate => {
                process.state = ProcessState::Zombie(-(signal.num() as i32));
            }
            SignalAction::Stop => {
                process.state = ProcessState::Stopped;
                process.signals.stop();
            }
            SignalAction::Continue => {
                if process.state == ProcessState::Stopped {
                    process.state = ProcessState::Running;
                }
                process.signals.cont();
            }
            SignalAction::Ignore => {
                // Nothing to do
            }
            SignalAction::Handle | SignalAction::Default => {
                // Handler will be called by caller
            }
        }

        Some((signal, action))
    }

    /// Get process state
    pub fn get_process_state(&self, pid: Pid) -> Option<ProcessState> {
        self.processes.get(&pid).map(|p| p.state.clone())
    }

    /// List all processes
    pub fn list_processes(&self) -> Vec<(Pid, String, ProcessState)> {
        self.processes
            .values()
            .map(|p| (p.pid, p.name.clone(), p.state.clone()))
            .collect()
    }
}

impl Default for Kernel {
    fn default() -> Self {
        Self::new()
    }
}

// Global kernel instance
thread_local! {
    pub static KERNEL: RefCell<Kernel> = RefCell::new(Kernel::new());
}

// ========== PUBLIC API ==========
// These functions provide the syscall interface to user code

/// Open a file or device
pub fn open(path: &str, flags: OpenFlags) -> SyscallResult<Fd> {
    KERNEL.with(|k| k.borrow_mut().sys_open(path, flags))
}

/// Read from a file descriptor
pub fn read(fd: Fd, buf: &mut [u8]) -> SyscallResult<usize> {
    KERNEL.with(|k| k.borrow_mut().sys_read(fd, buf))
}

/// Write to a file descriptor
pub fn write(fd: Fd, buf: &[u8]) -> SyscallResult<usize> {
    KERNEL.with(|k| k.borrow_mut().sys_write(fd, buf))
}

/// Close a file descriptor
pub fn close(fd: Fd) -> SyscallResult<()> {
    KERNEL.with(|k| k.borrow_mut().sys_close(fd))
}

/// Create a pipe
pub fn pipe() -> SyscallResult<(Fd, Fd)> {
    KERNEL.with(|k| k.borrow_mut().sys_pipe())
}

/// Create a window
pub fn window_create(title: &str) -> SyscallResult<Fd> {
    KERNEL.with(|k| k.borrow_mut().sys_window_create(title))
}

/// Get current working directory
pub fn getcwd() -> SyscallResult<PathBuf> {
    KERNEL.with(|k| k.borrow().sys_getcwd())
}

/// Change working directory
pub fn chdir(path: &str) -> SyscallResult<()> {
    KERNEL.with(|k| k.borrow_mut().sys_chdir(path))
}

/// Exit the current process
pub fn exit(code: i32) -> SyscallResult<()> {
    KERNEL.with(|k| k.borrow_mut().sys_exit(code))
}

/// Get current process ID
pub fn getpid() -> SyscallResult<Pid> {
    KERNEL.with(|k| k.borrow().sys_getpid())
}

/// Create a directory
pub fn mkdir(path: &str) -> SyscallResult<()> {
    KERNEL.with(|k| k.borrow_mut().sys_mkdir(path))
}

/// List directory contents
pub fn readdir(path: &str) -> SyscallResult<Vec<String>> {
    KERNEL.with(|k| k.borrow_mut().sys_readdir(path))
}

/// Check if path exists
pub fn exists(path: &str) -> SyscallResult<bool> {
    KERNEL.with(|k| k.borrow().sys_exists(path))
}

/// Get file/directory metadata
pub fn metadata(path: &str) -> SyscallResult<FileMetadata> {
    KERNEL.with(|k| k.borrow().sys_metadata(path))
}

/// Remove a file
pub fn remove_file(path: &str) -> SyscallResult<()> {
    KERNEL.with(|k| k.borrow_mut().sys_remove_file(path))
}

/// Remove a directory (must be empty)
pub fn remove_dir(path: &str) -> SyscallResult<()> {
    KERNEL.with(|k| k.borrow_mut().sys_remove_dir(path))
}

/// Rename/move a file or directory
pub fn rename(from: &str, to: &str) -> SyscallResult<()> {
    KERNEL.with(|k| k.borrow_mut().sys_rename(from, to))
}

/// Copy a file
pub fn copy_file(from: &str, to: &str) -> SyscallResult<u64> {
    KERNEL.with(|k| k.borrow_mut().sys_copy_file(from, to))
}

/// Read entire file contents as string (convenience function)
pub fn read_file(path: &str) -> SyscallResult<String> {
    let fd = open(path, OpenFlags::READ)?;
    let mut contents = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = read(fd, &mut buf)?;
        if n == 0 {
            break;
        }
        contents.extend_from_slice(&buf[..n]);
    }
    close(fd)?;
    String::from_utf8(contents).map_err(|_| SyscallError::InvalidData)
}

/// Write string to file (convenience function)
pub fn write_file(path: &str, content: &str) -> SyscallResult<()> {
    let fd = open(path, OpenFlags::WRITE)?;
    write(fd, content.as_bytes())?;
    close(fd)?;
    Ok(())
}

/// Get file stat (wrapper around metadata)
pub fn stat(path: &str) -> SyscallResult<FileStat> {
    let meta = metadata(path)?;
    Ok(FileStat {
        is_dir: meta.is_dir,
        size: meta.size,
    })
}

/// Simple stat result
pub struct FileStat {
    pub is_dir: bool,
    pub size: u64,
}

/// Duplicate a file descriptor
pub fn dup(fd: Fd) -> SyscallResult<Fd> {
    KERNEL.with(|k| k.borrow_mut().sys_dup(fd))
}

/// Spawn a new process (internal, will be expanded)
pub fn spawn_process(name: &str) -> Pid {
    KERNEL.with(|k| k.borrow_mut().spawn_process(name, None))
}

/// Set the current running process
pub fn set_current_process(pid: Pid) {
    KERNEL.with(|k| k.borrow_mut().set_current(pid))
}

/// Push input to the console
pub fn console_push_input(data: &[u8]) {
    KERNEL.with(|k| {
        if let Some(console) = k.borrow_mut().console() {
            console.push_input(data);
        }
    })
}

/// Take console output
pub fn console_take_output() -> Vec<u8> {
    KERNEL.with(|k| {
        k.borrow_mut()
            .console()
            .map(|c| c.take_output())
            .unwrap_or_default()
    })
}

// ========== MEMORY API ==========

/// Allocate a memory region
pub fn mem_alloc(size: usize, prot: Protection) -> SyscallResult<RegionId> {
    KERNEL.with(|k| k.borrow_mut().sys_alloc(size, prot))
}

/// Free a memory region
pub fn mem_free(region_id: RegionId) -> SyscallResult<()> {
    KERNEL.with(|k| k.borrow_mut().sys_free(region_id))
}

/// Read from a memory region
pub fn mem_read(region_id: RegionId, offset: usize, buf: &mut [u8]) -> SyscallResult<usize> {
    KERNEL.with(|k| k.borrow_mut().sys_mem_read(region_id, offset, buf))
}

/// Write to a memory region
pub fn mem_write(region_id: RegionId, offset: usize, buf: &[u8]) -> SyscallResult<usize> {
    KERNEL.with(|k| k.borrow_mut().sys_mem_write(region_id, offset, buf))
}

/// Create a shared memory segment
pub fn shmget(size: usize) -> SyscallResult<ShmId> {
    KERNEL.with(|k| k.borrow_mut().sys_shmget(size))
}

/// Attach to a shared memory segment
pub fn shmat(shm_id: ShmId, prot: Protection) -> SyscallResult<RegionId> {
    KERNEL.with(|k| k.borrow_mut().sys_shmat(shm_id, prot))
}

/// Detach from a shared memory segment
pub fn shmdt(shm_id: ShmId) -> SyscallResult<()> {
    KERNEL.with(|k| k.borrow_mut().sys_shmdt(shm_id))
}

/// Sync local changes to shared memory
pub fn shm_sync(shm_id: ShmId) -> SyscallResult<()> {
    KERNEL.with(|k| k.borrow_mut().sys_shm_sync(shm_id))
}

/// Refresh local region from shared memory
pub fn shm_refresh(shm_id: ShmId) -> SyscallResult<()> {
    KERNEL.with(|k| k.borrow_mut().sys_shm_refresh(shm_id))
}

/// Get shared memory info
pub fn shm_info(shm_id: ShmId) -> SyscallResult<ShmInfo> {
    KERNEL.with(|k| k.borrow().sys_shm_info(shm_id))
}

/// List all shared memory segments
pub fn shm_list() -> SyscallResult<Vec<ShmInfo>> {
    KERNEL.with(|k| k.borrow().sys_shm_list())
}

/// Get memory stats for current process
pub fn memstats() -> SyscallResult<MemoryStats> {
    KERNEL.with(|k| k.borrow().sys_memstats())
}

/// Set memory limit for current process
pub fn set_memlimit(limit: usize) -> SyscallResult<()> {
    KERNEL.with(|k| k.borrow_mut().sys_set_memlimit(limit))
}

/// Get system-wide memory stats
pub fn system_memstats() -> SyscallResult<SystemMemoryStats> {
    KERNEL.with(|k| k.borrow().sys_system_memstats())
}

// ========== TIMER API ==========

/// Get current kernel time (monotonic ms)
pub fn now() -> f64 {
    KERNEL.with(|k| k.borrow().sys_now())
}

/// Set current time (called from runtime)
pub fn set_time(time: f64) {
    KERNEL.with(|k| k.borrow_mut().set_time(time))
}

/// Schedule a one-shot timer
pub fn timer_set(delay_ms: f64, wake_task: Option<TaskId>) -> SyscallResult<TimerId> {
    KERNEL.with(|k| k.borrow_mut().sys_timer_set(delay_ms, wake_task))
}

/// Schedule a repeating interval timer
pub fn timer_interval(interval_ms: f64, wake_task: Option<TaskId>) -> SyscallResult<TimerId> {
    KERNEL.with(|k| k.borrow_mut().sys_timer_interval(interval_ms, wake_task))
}

/// Cancel a timer
pub fn timer_cancel(timer_id: TimerId) -> SyscallResult<bool> {
    KERNEL.with(|k| k.borrow_mut().sys_timer_cancel(timer_id))
}

/// Check if a timer is pending
pub fn timer_pending(timer_id: TimerId) -> SyscallResult<bool> {
    KERNEL.with(|k| k.borrow().sys_timer_pending(timer_id))
}

/// Set an alarm (sends SIGALRM to current process after delay)
pub fn alarm(delay_ms: f64) -> SyscallResult<TimerId> {
    KERNEL.with(|k| k.borrow_mut().sys_alarm(delay_ms))
}

/// Get time until next timer fires
pub fn time_until_next_timer() -> Option<f64> {
    KERNEL.with(|k| k.borrow().time_until_next_timer())
}

/// Get pending timer count
pub fn pending_timer_count() -> usize {
    KERNEL.with(|k| k.borrow().pending_timer_count())
}

/// Tick timers and return tasks to wake (call from runtime)
pub fn tick_timers() -> Vec<TaskId> {
    KERNEL.with(|k| k.borrow_mut().tick_timers())
}

// ========== SIGNAL API ==========

/// Send a signal to a process
pub fn kill(pid: Pid, signal: Signal) -> SyscallResult<()> {
    KERNEL.with(|k| k.borrow_mut().sys_kill(pid, signal))
}

/// Set signal handler for current process
pub fn signal(sig: Signal, action: SignalAction) -> SyscallResult<SignalAction> {
    KERNEL.with(|k| k.borrow_mut().sys_signal(sig, action))
}

/// Block a signal
pub fn sigblock(sig: Signal) -> SyscallResult<()> {
    KERNEL.with(|k| k.borrow_mut().sys_sigblock(sig))
}

/// Unblock a signal
pub fn sigunblock(sig: Signal) -> SyscallResult<()> {
    KERNEL.with(|k| k.borrow_mut().sys_sigunblock(sig))
}

/// Check for pending signals
pub fn sigpending() -> SyscallResult<bool> {
    KERNEL.with(|k| k.borrow().sys_sigpending())
}

/// Process signals for a process (call from runtime)
pub fn process_signals(pid: Pid) -> Option<(Signal, SignalAction)> {
    KERNEL.with(|k| k.borrow_mut().process_signals(pid))
}

/// Get process state
pub fn get_process_state(pid: Pid) -> Option<ProcessState> {
    KERNEL.with(|k| k.borrow().get_process_state(pid))
}

/// List all processes
pub fn list_processes() -> Vec<(Pid, String, ProcessState)> {
    KERNEL.with(|k| k.borrow().list_processes())
}

// ========== Tracing API ==========

/// Enable tracing
pub fn trace_enable() {
    KERNEL.with(|k| k.borrow_mut().trace_enable())
}

/// Disable tracing
pub fn trace_disable() {
    KERNEL.with(|k| k.borrow_mut().trace_disable())
}

/// Check if tracing is enabled
pub fn trace_enabled() -> bool {
    KERNEL.with(|k| k.borrow().trace_enabled())
}

/// Get trace summary
pub fn trace_summary() -> TraceSummary {
    KERNEL.with(|k| k.borrow().trace_summary())
}

/// Reset trace data
pub fn trace_reset() {
    KERNEL.with(|k| k.borrow_mut().trace_reset())
}

/// Trace a custom event
pub fn trace_event(category: TraceCategory, name: &str, detail: Option<&str>) {
    KERNEL.with(|k| {
        let mut kernel = k.borrow_mut();
        let now = kernel.now;
        if let Some(d) = detail {
            kernel.tracer_mut().trace_detail(now, category, name, d);
        } else {
            kernel.tracer_mut().trace_instant(now, category, name);
        }
    })
}

// ========== PERSISTENCE API ==========

/// Get a JSON snapshot of the VFS for persistence
pub fn vfs_snapshot() -> std::io::Result<Vec<u8>> {
    KERNEL.with(|k| k.borrow().vfs().to_json())
}

/// Restore VFS from a JSON snapshot
pub fn vfs_restore(data: &[u8]) -> std::io::Result<()> {
    let vfs = MemoryFs::from_json(data)?;
    KERNEL.with(|k| k.borrow_mut().set_vfs(vfs));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_kernel() {
        KERNEL.with(|k| {
            *k.borrow_mut() = Kernel::new();
            let pid = k.borrow_mut().spawn_process("test", None);
            k.borrow_mut().set_current(pid);
        });
    }

    #[test]
    fn test_open_console() {
        setup_test_kernel();

        let fd = open("/dev/console", OpenFlags::RDWR).unwrap();
        assert!(fd.0 >= 3); // After stdin/stdout/stderr
    }

    #[test]
    fn test_write_stdout() {
        setup_test_kernel();

        let n = write(Fd::STDOUT, b"Hello").unwrap();
        assert_eq!(n, 5);

        let output = console_take_output();
        assert_eq!(&output, b"Hello");
    }

    #[test]
    fn test_getpid() {
        setup_test_kernel();

        let pid = getpid().unwrap();
        assert_eq!(pid, Pid(1));
    }

    #[test]
    fn test_chdir_getcwd() {
        setup_test_kernel();

        assert_eq!(getcwd().unwrap(), PathBuf::from("/"));

        chdir("/home").unwrap();
        assert_eq!(getcwd().unwrap(), PathBuf::from("/home"));
    }

    #[test]
    fn test_pipe() {
        setup_test_kernel();

        let (read_fd, write_fd) = pipe().unwrap();

        write(write_fd, b"test").unwrap();

        let mut buf = [0u8; 10];
        let n = read(read_fd, &mut buf).unwrap();
        assert_eq!(n, 4);
        assert_eq!(&buf[..n], b"test");
    }

    #[test]
    fn test_close() {
        setup_test_kernel();

        let fd = open("/dev/console", OpenFlags::RDWR).unwrap();
        assert!(close(fd).is_ok());

        // Can't read from closed fd
        let mut buf = [0u8; 10];
        assert!(read(fd, &mut buf).is_err());
    }

    #[test]
    fn test_file_write_read() {
        setup_test_kernel();

        // Create and write to a file
        let fd = open("/tmp/test.txt", OpenFlags::WRITE).unwrap();
        write(fd, b"Hello, VFS!").unwrap();
        close(fd).unwrap();

        // Reopen and read
        let fd = open("/tmp/test.txt", OpenFlags::READ).unwrap();
        let mut buf = [0u8; 20];
        let n = read(fd, &mut buf).unwrap();
        assert_eq!(n, 11);
        assert_eq!(&buf[..n], b"Hello, VFS!");
        close(fd).unwrap();
    }

    #[test]
    fn test_mkdir_readdir() {
        setup_test_kernel();

        // Create a directory
        mkdir("/home/user").unwrap();

        // Create a file in it
        let fd = open("/home/user/file.txt", OpenFlags::WRITE).unwrap();
        write(fd, b"content").unwrap();
        close(fd).unwrap();

        // List directory
        let entries = readdir("/home/user").unwrap();
        assert!(entries.contains(&"file.txt".to_string()));
    }

    #[test]
    fn test_exists() {
        setup_test_kernel();

        assert!(exists("/tmp").unwrap());
        assert!(!exists("/nonexistent").unwrap());

        mkdir("/tmp/testdir").unwrap();
        assert!(exists("/tmp/testdir").unwrap());
    }

    #[test]
    fn test_dup() {
        setup_test_kernel();

        // Open a file
        let fd1 = open("/tmp/dup_test.txt", OpenFlags::WRITE).unwrap();
        write(fd1, b"hello").unwrap();

        // Dup the fd
        let fd2 = dup(fd1).unwrap();
        assert_ne!(fd1, fd2);

        // Both fds can write
        write(fd2, b" world").unwrap();

        // Close fd1 - file should still be accessible via fd2
        close(fd1).unwrap();

        // fd2 should still work
        write(fd2, b"!").unwrap();
        close(fd2).unwrap();

        // Verify content was written
        let fd = open("/tmp/dup_test.txt", OpenFlags::READ).unwrap();
        let mut buf = [0u8; 20];
        let n = read(fd, &mut buf).unwrap();
        assert_eq!(&buf[..n], b"hello world!");
        close(fd).unwrap();
    }

    #[test]
    fn test_dup_invalid_fd() {
        setup_test_kernel();

        // Can't dup an invalid fd
        assert!(dup(Fd(99)).is_err());
    }

    #[test]
    fn test_refcount_with_stdio() {
        setup_test_kernel();

        // stdout is shared (refcount > 1)
        // Writing should work
        write(Fd::STDOUT, b"test").unwrap();

        // Dup stdout
        let fd = dup(Fd::STDOUT).unwrap();

        // Both can write
        write(Fd::STDOUT, b" more").unwrap();
        write(fd, b" stuff").unwrap();

        // Close the dup - stdout should still work
        close(fd).unwrap();
        write(Fd::STDOUT, b"!").unwrap();
    }

    // ========== MEMORY TESTS ==========

    #[test]
    fn test_mem_alloc_free() {
        setup_test_kernel();

        let region = mem_alloc(1024, Protection::READ_WRITE).unwrap();
        assert!(region.0 > 0);

        // Check stats
        let stats = memstats().unwrap();
        assert_eq!(stats.allocated, 1024);
        assert_eq!(stats.region_count, 1);

        // Free
        mem_free(region).unwrap();

        let stats = memstats().unwrap();
        assert_eq!(stats.allocated, 0);
        assert_eq!(stats.region_count, 0);
    }

    #[test]
    fn test_mem_read_write() {
        setup_test_kernel();

        let region = mem_alloc(100, Protection::READ_WRITE).unwrap();

        // Write to the region
        let written = mem_write(region, 0, b"hello world").unwrap();
        assert_eq!(written, 11);

        // Read back
        let mut buf = [0u8; 20];
        let n = mem_read(region, 0, &mut buf).unwrap();
        assert_eq!(n, 20);
        assert_eq!(&buf[..11], b"hello world");

        mem_free(region).unwrap();
    }

    #[test]
    fn test_mem_limit() {
        setup_test_kernel();

        // Set a limit
        set_memlimit(1000).unwrap();

        // Allocate within limit
        let r1 = mem_alloc(500, Protection::READ_WRITE).unwrap();
        let r2 = mem_alloc(400, Protection::READ_WRITE).unwrap();

        // This should fail
        let result = mem_alloc(200, Protection::READ_WRITE);
        assert!(result.is_err());

        mem_free(r1).unwrap();
        mem_free(r2).unwrap();
    }

    #[test]
    fn test_shm_basic() {
        setup_test_kernel();

        // Create shared memory
        let shm_id = shmget(1024).unwrap();
        assert!(shm_id.0 > 0);

        // Get info - not attached until shmat
        let info = shm_info(shm_id).unwrap();
        assert_eq!(info.size, 1024);
        assert_eq!(info.attached_count, 0);

        // Attach to get a region
        let region = shmat(shm_id, Protection::READ_WRITE).unwrap();

        // Now we're attached
        let info = shm_info(shm_id).unwrap();
        assert_eq!(info.attached_count, 1);

        // Write to region
        mem_write(region, 0, b"shared data").unwrap();

        // Sync to shared memory
        shm_sync(shm_id).unwrap();

        // Detach
        shmdt(shm_id).unwrap();

        let stats = memstats().unwrap();
        assert_eq!(stats.shm_count, 0);
    }

    #[test]
    fn test_shm_list() {
        setup_test_kernel();

        let shm1 = shmget(1000).unwrap();
        let shm2 = shmget(2000).unwrap();

        let list = shm_list().unwrap();
        assert_eq!(list.len(), 2);

        // To clean up, we need to attach first (then detach)
        let _r1 = shmat(shm1, Protection::READ_WRITE).unwrap();
        let _r2 = shmat(shm2, Protection::READ_WRITE).unwrap();
        shmdt(shm1).unwrap();
        shmdt(shm2).unwrap();

        let list = shm_list().unwrap();
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn test_system_memstats() {
        setup_test_kernel();

        let stats = system_memstats().unwrap();
        assert_eq!(stats.shm_count, 0);

        let _shm = shmget(1024).unwrap();

        let stats = system_memstats().unwrap();
        assert_eq!(stats.shm_count, 1);
        assert_eq!(stats.shm_total_size, 1024);
    }

    #[test]
    fn test_mem_protection() {
        setup_test_kernel();

        // Read-only region
        let region = mem_alloc(100, Protection::READ).unwrap();

        // Read should work
        let mut buf = [0u8; 10];
        assert!(mem_read(region, 0, &mut buf).is_ok());

        // Write should fail
        let result = mem_write(region, 0, b"test");
        assert!(result.is_err());

        mem_free(region).unwrap();
    }

    // ========== Timer Tests ==========

    #[test]
    fn test_timer_set() {
        setup_test_kernel();

        // Set a timer (no task to wake, just testing the timer itself)
        let timer_id = timer_set(100.0, None).unwrap();
        assert!(timer_id.0 > 0);

        // Timer should be pending
        KERNEL.with(|k| {
            let kernel = k.borrow();
            assert!(kernel.timers.is_pending(timer_id));
        });

        // Cancel it
        assert!(timer_cancel(timer_id).unwrap());

        // Timer should no longer be pending
        KERNEL.with(|k| {
            let kernel = k.borrow();
            assert!(!kernel.timers.is_pending(timer_id));
        });
    }

    #[test]
    fn test_timer_tick() {
        setup_test_kernel();

        // Set timers at different times (no tasks to wake)
        let _t1 = timer_set(50.0, None).unwrap();
        let _t2 = timer_set(100.0, None).unwrap();
        let _t3 = timer_set(150.0, None).unwrap();

        // Tick at time 75 - t1 should fire (but no task woken since wake_task=None)
        KERNEL.with(|k| {
            let mut kernel = k.borrow_mut();
            let woken = kernel.tick(75.0);
            assert_eq!(woken.len(), 0); // No tasks because we passed None
            assert_eq!(kernel.pending_timer_count(), 2); // 2 timers left
        });

        // Tick at time 125 - t2 should fire
        KERNEL.with(|k| {
            let mut kernel = k.borrow_mut();
            kernel.tick(125.0);
            assert_eq!(kernel.pending_timer_count(), 1); // 1 timer left
        });

        // Tick at time 200 - t3 should fire
        KERNEL.with(|k| {
            let mut kernel = k.borrow_mut();
            kernel.tick(200.0);
            assert_eq!(kernel.pending_timer_count(), 0); // No timers left
        });
    }

    #[test]
    fn test_timer_interval() {
        setup_test_kernel();

        // Set an interval timer (fires every 50ms)
        let timer_id = timer_interval(50.0, None).unwrap();

        // Tick at 50 - fires
        KERNEL.with(|k| {
            let mut kernel = k.borrow_mut();
            kernel.tick(50.0);
            // Timer should still be pending (it repeats)
            assert!(kernel.timers.is_pending(timer_id));
        });

        // Tick at 100 - fires again
        KERNEL.with(|k| {
            let mut kernel = k.borrow_mut();
            kernel.tick(100.0);
            assert!(kernel.timers.is_pending(timer_id));
        });

        // Cancel it
        timer_cancel(timer_id).unwrap();

        // Timer should not be pending anymore
        KERNEL.with(|k| {
            let kernel = k.borrow();
            assert!(!kernel.timers.is_pending(timer_id));
        });
    }

    // ========== Signal Tests ==========

    #[test]
    fn test_signal_basic() {
        setup_test_kernel();

        // Create another process to signal
        let target_pid = KERNEL.with(|k| {
            let mut kernel = k.borrow_mut();
            kernel.spawn_process("target", None)
        });

        // Send SIGUSR1 to the target
        kill(target_pid, Signal::SIGUSR1).unwrap();

        // Check that the signal is pending
        KERNEL.with(|k| {
            let kernel = k.borrow();
            let process = kernel.get_process(target_pid).unwrap();
            assert!(process.signals.has_pending());
        });
    }

    #[test]
    fn test_signal_block_unblock() {
        setup_test_kernel();

        // Block SIGUSR1
        sigblock(Signal::SIGUSR1).unwrap();

        // Self-signal
        let my_pid = getpid().unwrap();
        kill(my_pid, Signal::SIGUSR1).unwrap();

        // Should not have pending (blocked signals don't count as "has_pending")
        KERNEL.with(|k| {
            let kernel = k.borrow();
            let process = kernel.get_process(my_pid).unwrap();
            assert!(!process.signals.has_pending());
        });

        // Unblock
        sigunblock(Signal::SIGUSR1).unwrap();

        // Now should have pending
        KERNEL.with(|k| {
            let kernel = k.borrow();
            let process = kernel.get_process(my_pid).unwrap();
            assert!(process.signals.has_pending());
        });
    }

    #[test]
    fn test_signal_disposition() {
        setup_test_kernel();

        // Set SIGTERM to ignore
        signal(Signal::SIGTERM, SignalAction::Ignore).unwrap();

        KERNEL.with(|k| {
            let kernel = k.borrow();
            let pid = kernel.current.unwrap();
            let process = kernel.get_process(pid).unwrap();
            assert_eq!(
                process.signals.disposition.get_action(Signal::SIGTERM),
                SignalAction::Ignore
            );
        });

        // Cannot set SIGKILL disposition
        let result = signal(Signal::SIGKILL, SignalAction::Ignore);
        assert!(result.is_err());
    }

    #[test]
    fn test_sigkill_terminates() {
        setup_test_kernel();

        // Create a process to kill
        let target_pid = KERNEL.with(|k| {
            let mut kernel = k.borrow_mut();
            kernel.spawn_process("victim", None)
        });

        // Send SIGKILL
        kill(target_pid, Signal::SIGKILL).unwrap();

        // Process should be a zombie now
        KERNEL.with(|k| {
            let kernel = k.borrow();
            let process = kernel.get_process(target_pid).unwrap();
            matches!(process.state, ProcessState::Zombie(_));
        });
    }

    #[test]
    fn test_sigpending() {
        setup_test_kernel();

        let my_pid = getpid().unwrap();

        // No pending signals initially
        let has_pending = sigpending().unwrap();
        assert!(!has_pending);

        // Send a signal
        kill(my_pid, Signal::SIGUSR1).unwrap();

        // Now has pending
        let has_pending = sigpending().unwrap();
        assert!(has_pending);
    }

    // ========== Tracing Tests ==========

    #[test]
    fn test_trace_enable_disable() {
        setup_test_kernel();

        assert!(!trace_enabled());

        trace_enable();
        assert!(trace_enabled());

        trace_disable();
        assert!(!trace_enabled());
    }

    #[test]
    fn test_trace_summary() {
        setup_test_kernel();

        trace_enable();

        // Get summary
        let summary = trace_summary();
        assert!(summary.enabled);
        assert_eq!(summary.syscall_count, 0);
    }

    #[test]
    fn test_trace_reset() {
        setup_test_kernel();

        trace_enable();

        // Record some trace events
        trace_event(TraceCategory::Custom, "test", Some("detail"));

        KERNEL.with(|k| {
            assert!(k.borrow().tracer().events().len() > 0);
        });

        trace_reset();

        KERNEL.with(|k| {
            assert_eq!(k.borrow().tracer().events().len(), 0);
        });
    }

    #[test]
    fn test_trace_custom_event() {
        setup_test_kernel();

        trace_enable();

        trace_event(TraceCategory::Custom, "my_event", Some("custom detail"));
        trace_event(TraceCategory::Syscall, "open", None);

        KERNEL.with(|k| {
            let events = k.borrow().tracer().events().clone();
            assert_eq!(events.len(), 2);
            assert_eq!(events[0].name, "my_event");
            assert_eq!(events[0].detail, Some("custom detail".to_string()));
            assert_eq!(events[1].name, "open");
            assert!(events[1].detail.is_none());
        });
    }
}
