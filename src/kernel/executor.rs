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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Priority {
    /// System-critical tasks (compositor, input handling)
    Critical = 0,
    /// Normal application tasks
    #[default]
    Normal = 1,
    /// Background tasks (can be starved)
    Background = 2,
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

    /// Wake a task by ID (mark it as ready to be polled)
    ///
    /// Used by timers and other async wake sources.
    /// Returns true if the task exists and was woken.
    pub fn wake_task(&self, task_id: TaskId) -> bool {
        // Check if task exists (either in tasks map or pending spawn)
        let exists = self.tasks.contains_key(&task_id)
            || self.pending_spawn.borrow().iter().any(|t| t.id == task_id);

        if exists {
            self.ready.borrow_mut().insert(task_id);
            true
        } else {
            false
        }
    }

    /// Wake multiple tasks by ID
    ///
    /// Convenience method for batch waking (e.g., from timer queue).
    pub fn wake_tasks(&self, task_ids: &[TaskId]) {
        let mut ready = self.ready.borrow_mut();
        for &task_id in task_ids {
            let exists = self.tasks.contains_key(&task_id)
                || self.pending_spawn.borrow().iter().any(|t| t.id == task_id);
            if exists {
                ready.insert(task_id);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn test_spawn_returns_unique_ids() {
        let mut exec = Executor::new();
        let id1 = exec.spawn(async {});
        let id2 = exec.spawn(async {});
        let id3 = exec.spawn(async {});

        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_task_runs_to_completion() {
        let mut exec = Executor::new();
        let ran = Rc::new(Cell::new(false));
        let ran_clone = ran.clone();

        exec.spawn(async move {
            ran_clone.set(true);
        });

        exec.run();
        assert!(ran.get());
    }

    #[test]
    fn test_multiple_tasks_all_complete() {
        let mut exec = Executor::new();
        let counter = Rc::new(Cell::new(0));

        for _ in 0..10 {
            let counter = counter.clone();
            exec.spawn(async move {
                counter.set(counter.get() + 1);
            });
        }

        exec.run();
        assert_eq!(counter.get(), 10);
    }

    #[test]
    fn test_tick_returns_polled_count() {
        let mut exec = Executor::new();
        exec.spawn(async {});
        exec.spawn(async {});
        exec.spawn(async {});

        let polled = exec.tick();
        assert_eq!(polled, 3);

        // After completion, no tasks left
        assert!(!exec.has_tasks());
    }

    #[test]
    fn test_priority_order() {
        let mut exec = Executor::new();
        let order = Rc::new(RefCell::new(Vec::new()));

        // Spawn in reverse priority order
        {
            let order = order.clone();
            exec.spawn_with_priority(
                async move {
                    order.borrow_mut().push("background");
                },
                Priority::Background,
            );
        }
        {
            let order = order.clone();
            exec.spawn_with_priority(
                async move {
                    order.borrow_mut().push("normal");
                },
                Priority::Normal,
            );
        }
        {
            let order = order.clone();
            exec.spawn_with_priority(
                async move {
                    order.borrow_mut().push("critical");
                },
                Priority::Critical,
            );
        }

        exec.tick();

        let result = order.borrow();
        assert_eq!(result.as_slice(), &["critical", "normal", "background"]);
    }

    #[test]
    fn test_yielding_task_with_run() {
        // run() handles tasks that yield without waking
        let mut exec = Executor::new();
        let counter = Rc::new(Cell::new(0));
        let counter_clone = counter.clone();

        exec.spawn(async move {
            counter_clone.set(counter_clone.get() + 1);
            futures::pending!(); // Yield
            counter_clone.set(counter_clone.get() + 1);
            futures::pending!(); // Yield
            counter_clone.set(counter_clone.get() + 1);
        });

        exec.run();
        assert_eq!(counter.get(), 3);
        assert!(!exec.has_tasks());
    }

    #[test]
    fn test_tick_without_wake_leaves_task_pending() {
        // tick() only polls tasks that are in the ready set
        // A task that yields without waking won't be re-polled
        let mut exec = Executor::new();
        let counter = Rc::new(Cell::new(0));
        let counter_clone = counter.clone();

        exec.spawn(async move {
            counter_clone.set(counter_clone.get() + 1);
            futures::pending!(); // Yield without waking
        });

        // First tick: runs until yield
        exec.tick();
        assert_eq!(counter.get(), 1);
        assert!(exec.has_tasks());

        // Second tick: task is NOT in ready set, so nothing happens
        let polled = exec.tick();
        assert_eq!(polled, 0); // Nothing was polled
        assert_eq!(counter.get(), 1); // Counter unchanged
        assert!(exec.has_tasks()); // Task still exists
    }

    #[test]
    fn test_task_count() {
        let mut exec = Executor::new();
        assert_eq!(exec.task_count(), 0);

        exec.spawn(async {
            futures::pending!();
        });
        exec.spawn(async {
            futures::pending!();
        });

        // Before tick, tasks are in pending_spawn
        assert_eq!(exec.task_count(), 2);

        exec.tick();

        // After tick, still 2 tasks (they yielded)
        assert_eq!(exec.task_count(), 2);
    }

    #[test]
    fn test_spawn_during_tick() {
        let mut exec = Executor::new();
        let spawned = Rc::new(Cell::new(false));
        let spawned_clone = spawned.clone();

        // This is tricky - we can't easily spawn during a tick in this test
        // because we don't have access to the executor from within the future.
        // But we can verify that pending_spawn works correctly.

        exec.spawn(async move {
            spawned_clone.set(true);
        });

        exec.run();
        assert!(spawned.get());
    }

    #[test]
    fn test_wake_task() {
        let mut exec = Executor::new();
        let counter = Rc::new(Cell::new(0));
        let counter_clone = counter.clone();

        let task_id = exec.spawn(async move {
            counter_clone.set(counter_clone.get() + 1);
            futures::pending!(); // Yield without waking
            counter_clone.set(counter_clone.get() + 1);
        });

        // First tick: runs until yield
        exec.tick();
        assert_eq!(counter.get(), 1);
        assert!(exec.has_tasks());

        // Second tick: task not ready, nothing happens
        let polled = exec.tick();
        assert_eq!(polled, 0);
        assert_eq!(counter.get(), 1);

        // Wake the task externally (like a timer would)
        assert!(exec.wake_task(task_id));

        // Third tick: task is now ready again
        let polled = exec.tick();
        assert_eq!(polled, 1);
        assert_eq!(counter.get(), 2);

        // Task completed
        assert!(!exec.has_tasks());
    }

    #[test]
    fn test_wake_nonexistent_task() {
        let exec = Executor::new();
        let fake_id = TaskId(9999);

        // Waking a nonexistent task returns false
        assert!(!exec.wake_task(fake_id));
    }

    #[test]
    fn test_wake_tasks_batch() {
        let mut exec = Executor::new();
        let counter = Rc::new(Cell::new(0));

        let mut task_ids = Vec::new();
        for _ in 0..3 {
            let counter = counter.clone();
            let id = exec.spawn(async move {
                counter.set(counter.get() + 1);
                futures::pending!();
                counter.set(counter.get() + 1);
            });
            task_ids.push(id);
        }

        // First tick: all run once
        exec.tick();
        assert_eq!(counter.get(), 3);

        // Second tick: nothing ready
        exec.tick();
        assert_eq!(counter.get(), 3);

        // Wake all tasks at once
        exec.wake_tasks(&task_ids);

        // Third tick: all run again
        exec.tick();
        assert_eq!(counter.get(), 6);
    }
}
