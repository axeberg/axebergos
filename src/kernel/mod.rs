//! The kernel - process management, syscalls, and system services
//!
//! Inspired by Oxide's Hubris:
//! - Tasks are specified at build time
//! - Synchronous mental model, async implementation
//! - Small, auditable core
//!
//! Core abstractions:
//! - Process: unit of isolation, has its own file descriptors
//! - Handle/Fd: reference to a kernel object
//! - KernelObject: file, pipe, console, window, etc.
//! - Syscall: the interface between user code and the kernel

pub mod debugger;
pub mod devfs;
pub mod events;
pub mod executor;
pub mod fifo;
pub mod flock;
pub mod init;
pub mod ipc;
pub mod memory;
pub mod memory_persist;
pub mod mount;
pub mod msgqueue;
pub mod object;
pub mod pkg;
pub mod process;
pub mod procfs;
pub mod profiler;
pub mod semaphore;
pub mod signal;
pub mod syscall;
pub mod sysfs;
pub mod task;
pub mod timer;
pub mod trace;
pub mod tty;
pub mod uds;
pub mod users;
pub mod visualizer;
pub mod wasm;
pub mod work_stealing;

#[cfg(target_arch = "wasm32")]
pub mod network;

#[cfg(test)]
mod invariants_test;

pub use debugger::{
    Breakpoint, BreakpointAction, BreakpointCondition, BreakpointId, DebugMode, DebugTarget,
    DebuggerStatus, MemoryView, MemoryWatch, SyscallArg, SyscallRecord, WasmDebugger, WatchType,
};
pub use executor::{Executor, Priority};
pub use fifo::{FifoBuffer, FifoError, FifoRegistry};
pub use flock::{FileLockManager, LockError, LockType, RangeLock};
pub use init::{
    InitSystem, RestartPolicy, Service, ServiceConfig, ServiceState, ServiceStatus, Target,
};
pub use ipc::{
    BoundedReceiver, BoundedRecvFuture, BoundedSendFuture, BoundedSender, Receiver, SendError,
    Sender, TryRecvError, TrySendError, bounded_channel, channel,
};
pub use memory::{
    CowStats, MemoryError, MemoryStats, PAGE_SIZE, ProcessCowStats, Protection, RegionId, ShmId,
    ShmInfo, SystemMemoryStats,
};
pub use memory_persist::{MemoryPersistStats, MemoryPersistence};
pub use mount::{FsType, MountEntry, MountError, MountOptions, MountTable};
pub use msgqueue::{
    Message, MessageQueue, MsgQueueError, MsgQueueId, MsgQueueManager, MsgQueueStats,
};
pub use pkg::{
    Checksum, Dependency, InstalledPackage, PackageDatabase, PackageId, PackageInstaller,
    PackageManager, PackageManifest, PackageRegistry, PkgError, PkgResult, RegistryEntry,
    ResolvedPackage, Version, VersionReq,
};
pub use process::{Fd, Handle, OpenFlags, Pid};
pub use profiler::{
    AllocationEvent, AllocationSizeDistribution, CpuProfile, FlameGraphBuilder, FlameNode,
    MemoryProfile, MemorySnapshot, ProcessMemorySnapshot, ProfileSummary, Profiler, ProfilerState,
    SyscallProfile, TaskSample, TaskSampleState,
};
pub use semaphore::{
    SemAdj, SemError, SemId, SemOpResult, SemSetStats, SemaphoreManager, SemaphoreSet,
};
pub use signal::{Signal, SignalAction, SignalError};
pub use syscall::{SyscallError, SyscallResult};
pub use task::{Task, TaskId, TaskState};
pub use timer::TimerId;
pub use trace::{TraceCategory, TraceEvent, TraceSummary, Tracer};
pub use tty::{Termios, Tty, TtyManager};
pub use uds::{
    SockAddr, SocketError, SocketId, SocketResult, SocketState, SocketType, UnixSocket,
    UnixSocketManager,
};
pub use users::{FileMode, Gid, Group, Uid, User, UserDb};
pub use visualizer::{
    KernelSnapshot, MemoryRegionType, MemoryRegionView, ProcessMemoryLayout, ProcessTree,
    ProcessTreeNode, ResourceDashboard, ResourceLimitView, SchedulerView, SyscallActivity,
    SyscallMonitor, SystemMemoryView, TaskView, TaskViewState,
};
pub use work_stealing::{
    Config as WorkStealingConfig, Injector, StealResult, Stealer, TaskHandle, WorkStealingExecutor,
    Worker,
};

use std::cell::RefCell;

thread_local! {
    /// The executor for running async tasks
    /// Note: The full kernel state (processes, objects) is in syscall::KERNEL
    static EXECUTOR: RefCell<Executor> = RefCell::new(Executor::new());
}

/// Spawn a task with normal priority, returns task ID
pub fn spawn<F>(future: F) -> TaskId
where
    F: std::future::Future<Output = ()> + 'static,
{
    EXECUTOR.with(|e| e.borrow_mut().spawn(future))
}

/// Spawn a task with specified priority
pub fn spawn_with_priority<F>(future: F, priority: Priority) -> TaskId
where
    F: std::future::Future<Output = ()> + 'static,
{
    EXECUTOR.with(|e| e.borrow_mut().spawn_with_priority(future, priority))
}

/// Run one tick of execution (call from requestAnimationFrame)
pub fn tick() -> usize {
    EXECUTOR.with(|e| e.borrow_mut().tick())
}

/// Run the executor until all tasks complete (for non-UI contexts)
pub fn run() {
    EXECUTOR.with(|e| e.borrow_mut().run())
}

/// Check if there are active tasks
pub fn has_tasks() -> bool {
    EXECUTOR.with(|e| e.borrow().has_tasks())
}

/// Get count of active tasks
pub fn task_count() -> usize {
    EXECUTOR.with(|e| e.borrow().task_count())
}
