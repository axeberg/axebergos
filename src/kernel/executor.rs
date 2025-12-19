//! Async executor for cooperative multitasking
//!
//! Designed for UI work in WASM:
//! - Tick-based execution (integrates with requestAnimationFrame)
//! - Task identity (tasks have IDs for event routing)
//! - Priority levels (compositor runs before apps)
//! - Proper wake semantics (no busy-waiting)
//!
//! Tractability > Complexity, but this is the kernel - it needs to be solid.

use super::task::{BoxFuture, TaskId};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::future::Future;
use std::rc::Rc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

/// Task priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    /// System-critical tasks (compositor, input handling)
    Critical = 0,
    /// Normal application tasks
    Normal = 1,
    /// Background tasks (can be starved)
    Background = 2,
}

impl Default for Priority {
    fn default() -> Self {
        Priority::Normal
    }
}

/// A managed task with metadata
struct ManagedTask {
    id: TaskId,
    priority: Priority,
    future: BoxFuture,
}

/// Shared state for waker to signal task readiness
struct WakerState {
    task_id: TaskId,
    ready_set: Rc<RefCell<HashSet<TaskId>>>,
}

/// The executor - runs async tasks cooperatively, one tick at a time
pub struct Executor {
    /// All tasks, indexed by ID
    tasks: BTreeMap<TaskId, ManagedTask>,

    /// Tasks that are ready to be polled (signaled by waker)
    ready: Rc<RefCell<HashSet<TaskId>>>,

    /// Tasks waiting to be spawned (added during tick)
    pending_spawn: RefCell<VecDeque<ManagedTask>>,

    /// Next task ID
    next_id: u64,
}

impl Executor {
    pub fn new() -> Self {
        Self {
            tasks: BTreeMap::new(),
            ready: Rc::new(RefCell::new(HashSet::new())),
            pending_spawn: RefCell::new(VecDeque::new()),
            next_id: 0,
        }
    }

    /// Spawn a future with default (Normal) priority, returns task ID
    pub fn spawn<F>(&mut self, future: F) -> TaskId
    where
        F: Future<Output = ()> + 'static,
    {
        self.spawn_with_priority(future, Priority::Normal)
    }

    /// Spawn a future with specified priority
    pub fn spawn_with_priority<F>(&mut self, future: F, priority: Priority) -> TaskId
    where
        F: Future<Output = ()> + 'static,
    {
        let id = TaskId(self.next_id);
        self.next_id += 1;

        let task = ManagedTask {
            id,
            priority,
            future: Box::pin(future),
        };

        // If we're in the middle of a tick, queue for later
        // Otherwise add directly
        self.pending_spawn.borrow_mut().push_back(task);

        // Mark as ready to run immediately
        self.ready.borrow_mut().insert(id);

        id
    }

    /// Integrate pending spawns into the task map
    fn integrate_pending(&mut self) {
        let mut pending = self.pending_spawn.borrow_mut();
        while let Some(task) = pending.pop_front() {
            self.tasks.insert(task.id, task);
        }
    }

    /// Run one tick of execution
    ///
    /// Polls all ready tasks once, in priority order.
    /// Returns the number of tasks that were polled.
    ///
    /// Call this from requestAnimationFrame for UI work.
    pub fn tick(&mut self) -> usize {
        // First, integrate any tasks spawned since last tick
        self.integrate_pending();

        // Collect ready task IDs, sorted by priority
        let mut ready_ids: Vec<TaskId> = self.ready.borrow().iter().copied().collect();

        // Sort by priority (Critical first, then Normal, then Background)
        ready_ids.sort_by_key(|id| {
            self.tasks
                .get(id)
                .map(|t| t.priority)
                .unwrap_or(Priority::Background)
        });

        let mut polled = 0;

        for task_id in ready_ids {
            // Remove from ready set before polling
            self.ready.borrow_mut().remove(&task_id);

            // Get the task (need to remove to get mutable access to future)
            let Some(mut task) = self.tasks.remove(&task_id) else {
                continue;
            };

            // Create waker for this task
            let waker = self.create_waker(task_id);
            let mut cx = Context::from_waker(&waker);

            match task.future.as_mut().poll(&mut cx) {
                Poll::Ready(()) => {
                    // Task completed, don't re-insert
                    polled += 1;
                }
                Poll::Pending => {
                    // Task yielded, put it back (but NOT in ready set)
                    // It will be re-added to ready set when waker is called
                    self.tasks.insert(task_id, task);
                    polled += 1;
                }
            }
        }

        // Integrate any tasks spawned during this tick
        self.integrate_pending();

        polled
    }

    /// Run until all tasks complete (for non-UI contexts)
    pub fn run(&mut self) {
        loop {
            self.integrate_pending();
            if self.tasks.is_empty() && self.pending_spawn.borrow().is_empty() {
                break;
            }

            // If no tasks are ready, mark all as ready (prevents deadlock in simple cases)
            if self.ready.borrow().is_empty() {
                for id in self.tasks.keys() {
                    self.ready.borrow_mut().insert(*id);
                }
            }

            self.tick();
        }
    }

    /// Check if there are any active tasks
    pub fn has_tasks(&self) -> bool {
        !self.tasks.is_empty() || !self.pending_spawn.borrow().is_empty()
    }

    /// Get count of active tasks
    pub fn task_count(&self) -> usize {
        self.tasks.len() + self.pending_spawn.borrow().len()
    }

    /// Create a waker that marks a task as ready
    fn create_waker(&self, task_id: TaskId) -> Waker {
        let state = Box::new(WakerState {
            task_id,
            ready_set: self.ready.clone(),
        });
        let ptr = Box::into_raw(state) as *const ();
        let raw = RawWaker::new(ptr, &WAKER_VTABLE);
        unsafe { Waker::from_raw(raw) }
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

// Waker implementation that properly signals task readiness

const WAKER_VTABLE: RawWakerVTable =
    RawWakerVTable::new(waker_clone, waker_wake, waker_wake_by_ref, waker_drop);

unsafe fn waker_clone(ptr: *const ()) -> RawWaker {
    unsafe {
        let state = &*(ptr as *const WakerState);
        let cloned = Box::new(WakerState {
            task_id: state.task_id,
            ready_set: state.ready_set.clone(),
        });
        RawWaker::new(Box::into_raw(cloned) as *const (), &WAKER_VTABLE)
    }
}

unsafe fn waker_wake(ptr: *const ()) {
    unsafe {
        let state = Box::from_raw(ptr as *mut WakerState);
        state.ready_set.borrow_mut().insert(state.task_id);
        // Box is dropped here
    }
}

unsafe fn waker_wake_by_ref(ptr: *const ()) {
    unsafe {
        let state = &*(ptr as *const WakerState);
        state.ready_set.borrow_mut().insert(state.task_id);
    }
}

unsafe fn waker_drop(ptr: *const ()) {
    unsafe {
        drop(Box::from_raw(ptr as *mut WakerState));
    }
}
