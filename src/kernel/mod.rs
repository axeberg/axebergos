//! The kernel - task execution, IPC, and system services
//!
//! Inspired by Oxide's Hubris:
//! - Tasks are specified at build time
//! - Synchronous mental model, async implementation
//! - Small, auditable core

pub mod executor;
pub mod ipc;
pub mod task;

pub use executor::Executor;
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
    next_task_id: u64,
}

impl Kernel {
    pub fn new() -> Self {
        Self {
            executor: Executor::new(),
            next_task_id: 0,
        }
    }

    /// Generate a unique task ID
    pub fn next_task_id(&mut self) -> TaskId {
        let id = TaskId(self.next_task_id);
        self.next_task_id += 1;
        id
    }
}

impl Default for Kernel {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawn a task on the global executor
pub fn spawn<F>(future: F)
where
    F: std::future::Future<Output = ()> + 'static,
{
    KERNEL.with(|k| {
        k.borrow_mut().executor.spawn(future);
    });
}

/// Run the executor until all tasks complete
pub fn run() {
    KERNEL.with(|k| {
        k.borrow_mut().executor.run();
    });
}

/// Get the next available task ID
pub fn allocate_task_id() -> TaskId {
    KERNEL.with(|k| k.borrow_mut().next_task_id())
}
