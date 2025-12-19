//! The kernel - task execution, IPC, and system services
//!
//! Inspired by Oxide's Hubris:
//! - Tasks are specified at build time
//! - Synchronous mental model, async implementation
//! - Small, auditable core
//!
//! Extended for UI work:
//! - Tick-based execution for frame-by-frame rendering
//! - Priority levels for compositor vs apps
//! - Event queue for input handling

pub mod events;
pub mod executor;
pub mod ipc;
pub mod task;

pub use executor::{Executor, Priority};
pub use ipc::{channel, Receiver, Sender};
pub use task::{Task, TaskId, TaskState};

use std::cell::RefCell;

thread_local! {
    /// The global kernel instance
    static KERNEL: RefCell<Kernel> = RefCell::new(Kernel::new());
}

/// The kernel manages all system state
pub struct Kernel {
    executor: Executor,
}

impl Kernel {
    pub fn new() -> Self {
        Self {
            executor: Executor::new(),
        }
    }
}

impl Default for Kernel {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawn a task with normal priority, returns task ID
pub fn spawn<F>(future: F) -> TaskId
where
    F: std::future::Future<Output = ()> + 'static,
{
    KERNEL.with(|k| k.borrow_mut().executor.spawn(future))
}

/// Spawn a task with specified priority
pub fn spawn_with_priority<F>(future: F, priority: Priority) -> TaskId
where
    F: std::future::Future<Output = ()> + 'static,
{
    KERNEL.with(|k| {
        k.borrow_mut()
            .executor
            .spawn_with_priority(future, priority)
    })
}

/// Run one tick of execution (call from requestAnimationFrame)
pub fn tick() -> usize {
    KERNEL.with(|k| k.borrow_mut().executor.tick())
}

/// Run the executor until all tasks complete (for non-UI contexts)
pub fn run() {
    KERNEL.with(|k| k.borrow_mut().executor.run())
}

/// Check if there are active tasks
pub fn has_tasks() -> bool {
    KERNEL.with(|k| k.borrow().executor.has_tasks())
}

/// Get count of active tasks
pub fn task_count() -> usize {
    KERNEL.with(|k| k.borrow().executor.task_count())
}
