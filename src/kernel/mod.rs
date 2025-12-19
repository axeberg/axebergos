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

pub mod events;
pub mod executor;
pub mod ipc;
pub mod object;
pub mod process;
pub mod syscall;
pub mod task;

pub use executor::{Executor, Priority};
pub use ipc::{channel, Receiver, Sender};
pub use process::{Fd, Handle, OpenFlags, Pid};
pub use syscall::{SyscallError, SyscallResult};
pub use task::{Task, TaskId, TaskState};

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
