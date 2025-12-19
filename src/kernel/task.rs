//! Task abstraction
//!
//! A Task is the unit of execution in axeberg. Unlike traditional OS processes,
//! tasks are cooperative and defined at build time. This follows Hubris's model:
//! predictable, auditable, no dynamic spawning chaos.

use std::future::Future;
use std::pin::Pin;

/// Unique identifier for a task
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(pub u64);

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Task({})", self.0)
    }
}

/// Task execution state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// Ready to run
    Ready,
    /// Currently executing
    Running,
    /// Waiting on I/O or another task
    Blocked,
    /// Finished execution
    Completed,
}

/// The Task trait - implement this to create a module/program
///
/// Tasks are the "programs" of axeberg. They're defined at compile time
/// and registered with the kernel during boot.
pub trait Task: Send + 'static {
    /// Human-readable name for this task
    fn name(&self) -> &'static str;

    /// The task's main execution. Returns a future that drives the task.
    fn run(&mut self) -> Pin<Box<dyn Future<Output = ()> + '_>>;
}

/// A boxed future representing a spawned task
pub type BoxFuture = Pin<Box<dyn Future<Output = ()> + 'static>>;
