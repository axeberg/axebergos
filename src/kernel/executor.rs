//! Async executor for cooperative multitasking
//!
//! Designed for UI work in WASM:
//! - Tick-based execution (integrates with requestAnimationFrame)
//! - Task identity (tasks have IDs for event routing)
//! - Priority levels (compositor runs before apps)
//! - Proper wake semantics (no busy-waiting)
//! - Timeout support for async operations
//! - Task groups for hierarchical management
//!
//! Tractability > Complexity, but this is the kernel - it needs to be solid.

use super::task::{BoxFuture, TaskId};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
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

// ============================================================================
// Timeout Support
// ============================================================================

/// Error returned when a timeout expires
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeoutError;

impl std::fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "operation timed out")
    }
}

impl std::error::Error for TimeoutError {}

/// Future that wraps another future with a timeout
///
/// If the inner future doesn't complete within the deadline,
/// the Timeout future returns Err(TimeoutError).
pub struct Timeout<F> {
    future: F,
    deadline: f64,
    /// Function to get current time (injected for testability)
    now_fn: fn() -> f64,
}

impl<F> Timeout<F> {
    /// Create a new timeout wrapper
    pub fn new(future: F, deadline: f64) -> Self {
        Self {
            future,
            deadline,
            now_fn: default_now,
        }
    }

    /// Create with custom time function (for testing)
    pub fn with_now_fn(future: F, deadline: f64, now_fn: fn() -> f64) -> Self {
        Self {
            future,
            deadline,
            now_fn,
        }
    }
}

impl<F: Future + Unpin> Future for Timeout<F> {
    type Output = Result<F::Output, TimeoutError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Check timeout first
        let now = (self.now_fn)();
        if now >= self.deadline {
            return Poll::Ready(Err(TimeoutError));
        }

        // Poll the inner future
        match Pin::new(&mut self.future).poll(cx) {
            Poll::Ready(result) => Poll::Ready(Ok(result)),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Wrap a future with a timeout
///
/// Returns `Ok(result)` if the future completes before the deadline,
/// or `Err(TimeoutError)` if the deadline passes.
///
/// # Example
/// ```ignore
/// let result = timeout(my_future, 1000.0, now_ms()).await;
/// match result {
///     Ok(value) => println!("Got value: {:?}", value),
///     Err(TimeoutError) => println!("Timed out!"),
/// }
/// ```
pub fn timeout<F: Future + Unpin>(future: F, timeout_ms: f64, now: f64) -> Timeout<F> {
    Timeout::new(future, now + timeout_ms)
}

/// Default time function - returns 0.0 (use with platform time)
fn default_now() -> f64 {
    // In WASM, this would use performance.now()
    // For non-WASM tests, we use 0.0 as a placeholder
    0.0
}

// ============================================================================
// Task Groups
// ============================================================================

/// Unique identifier for a task group
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskGroupId(pub u64);

/// A task group manages a collection of related tasks
///
/// Task groups enable hierarchical task management:
/// - Cancel all tasks in a group at once
/// - Wait for all tasks in a group to complete
/// - Track group membership for tasks
#[derive(Debug)]
pub struct TaskGroup {
    /// Unique identifier
    pub id: TaskGroupId,
    /// Tasks belonging to this group
    tasks: HashSet<TaskId>,
    /// Parent group (for hierarchy)
    parent: Option<TaskGroupId>,
    /// Child groups
    children: HashSet<TaskGroupId>,
}

impl TaskGroup {
    /// Create a new task group
    pub fn new(id: TaskGroupId) -> Self {
        Self {
            id,
            tasks: HashSet::new(),
            parent: None,
            children: HashSet::new(),
        }
    }

    /// Create a task group with a parent
    pub fn with_parent(id: TaskGroupId, parent: TaskGroupId) -> Self {
        Self {
            id,
            tasks: HashSet::new(),
            parent: Some(parent),
            children: HashSet::new(),
        }
    }

    /// Add a task to this group
    pub fn add_task(&mut self, task_id: TaskId) {
        self.tasks.insert(task_id);
    }

    /// Remove a task from this group
    pub fn remove_task(&mut self, task_id: TaskId) -> bool {
        self.tasks.remove(&task_id)
    }

    /// Check if a task belongs to this group
    pub fn contains(&self, task_id: TaskId) -> bool {
        self.tasks.contains(&task_id)
    }

    /// Get all tasks in this group
    pub fn tasks(&self) -> &HashSet<TaskId> {
        &self.tasks
    }

    /// Get the number of tasks in this group
    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    /// Check if this group is empty
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Get parent group ID
    pub fn parent(&self) -> Option<TaskGroupId> {
        self.parent
    }

    /// Add a child group
    pub fn add_child(&mut self, child_id: TaskGroupId) {
        self.children.insert(child_id);
    }

    /// Remove a child group
    pub fn remove_child(&mut self, child_id: TaskGroupId) -> bool {
        self.children.remove(&child_id)
    }

    /// Get child group IDs
    pub fn children(&self) -> &HashSet<TaskGroupId> {
        &self.children
    }
}

/// Manager for task groups
#[derive(Debug, Default)]
pub struct TaskGroupManager {
    /// All groups by ID
    groups: HashMap<TaskGroupId, TaskGroup>,
    /// Task to group mapping
    task_groups: HashMap<TaskId, TaskGroupId>,
    /// Next group ID
    next_id: u64,
}

impl TaskGroupManager {
    /// Create a new task group manager
    pub fn new() -> Self {
        Self {
            groups: HashMap::new(),
            task_groups: HashMap::new(),
            next_id: 1,
        }
    }

    /// Create a new task group, returns its ID
    pub fn create_group(&mut self) -> TaskGroupId {
        let id = TaskGroupId(self.next_id);
        self.next_id += 1;
        self.groups.insert(id, TaskGroup::new(id));
        id
    }

    /// Create a child group under a parent
    pub fn create_child_group(&mut self, parent_id: TaskGroupId) -> Option<TaskGroupId> {
        if !self.groups.contains_key(&parent_id) {
            return None;
        }

        let id = TaskGroupId(self.next_id);
        self.next_id += 1;

        let group = TaskGroup::with_parent(id, parent_id);
        self.groups.insert(id, group);

        // Add to parent's children
        if let Some(parent) = self.groups.get_mut(&parent_id) {
            parent.add_child(id);
        }

        Some(id)
    }

    /// Add a task to a group
    pub fn add_task_to_group(&mut self, task_id: TaskId, group_id: TaskGroupId) -> bool {
        if let Some(group) = self.groups.get_mut(&group_id) {
            group.add_task(task_id);
            self.task_groups.insert(task_id, group_id);
            true
        } else {
            false
        }
    }

    /// Remove a task from its group
    pub fn remove_task(&mut self, task_id: TaskId) -> Option<TaskGroupId> {
        if let Some(group_id) = self.task_groups.remove(&task_id) {
            if let Some(group) = self.groups.get_mut(&group_id) {
                group.remove_task(task_id);
            }
            Some(group_id)
        } else {
            None
        }
    }

    /// Get the group a task belongs to
    pub fn get_task_group(&self, task_id: TaskId) -> Option<TaskGroupId> {
        self.task_groups.get(&task_id).copied()
    }

    /// Get a group by ID
    pub fn get_group(&self, group_id: TaskGroupId) -> Option<&TaskGroup> {
        self.groups.get(&group_id)
    }

    /// Get all task IDs in a group (including descendants)
    pub fn get_all_tasks(&self, group_id: TaskGroupId) -> Vec<TaskId> {
        let mut tasks = Vec::new();
        self.collect_tasks_recursive(group_id, &mut tasks);
        tasks
    }

    fn collect_tasks_recursive(&self, group_id: TaskGroupId, tasks: &mut Vec<TaskId>) {
        if let Some(group) = self.groups.get(&group_id) {
            tasks.extend(group.tasks().iter().copied());
            for &child_id in group.children() {
                self.collect_tasks_recursive(child_id, tasks);
            }
        }
    }

    /// Delete a group (does not cancel tasks, just removes grouping)
    pub fn delete_group(&mut self, group_id: TaskGroupId) -> bool {
        if let Some(group) = self.groups.remove(&group_id) {
            // Remove task mappings
            for task_id in group.tasks() {
                self.task_groups.remove(task_id);
            }

            // Remove from parent's children
            if let Some(parent_id) = group.parent()
                && let Some(parent) = self.groups.get_mut(&parent_id)
            {
                parent.remove_child(group_id);
            }

            // Recursively delete children
            for child_id in group.children().clone() {
                self.delete_group(child_id);
            }

            true
        } else {
            false
        }
    }

    /// Get the number of groups
    pub fn group_count(&self) -> usize {
        self.groups.len()
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

    /// Cancel a running task by ID
    ///
    /// Removes the task from the executor entirely. The task's future
    /// is dropped immediately (cleanup via Drop trait).
    /// Returns true if the task existed and was cancelled.
    pub fn cancel_task(&mut self, task_id: TaskId) -> bool {
        // Remove from ready set
        self.ready.borrow_mut().remove(&task_id);

        // Try to remove from pending spawn queue
        let mut pending = self.pending_spawn.borrow_mut();
        let pending_pos = pending.iter().position(|t| t.id == task_id);
        if let Some(pos) = pending_pos {
            pending.remove(pos);
            return true;
        }
        drop(pending);

        // Try to remove from active tasks
        self.tasks.remove(&task_id).is_some()
    }

    /// Cancel multiple tasks by ID
    ///
    /// Returns the number of tasks that were actually cancelled.
    pub fn cancel_tasks(&mut self, task_ids: &[TaskId]) -> usize {
        task_ids.iter().filter(|&&id| self.cancel_task(id)).count()
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

    #[test]
    fn test_cancel_task() {
        let mut exec = Executor::new();
        let counter = Rc::new(Cell::new(0));
        let counter_clone = counter.clone();

        let task_id = exec.spawn(async move {
            counter_clone.set(counter_clone.get() + 1);
            futures::pending!(); // Yield
            counter_clone.set(counter_clone.get() + 1); // Should never run
        });

        // First tick: runs until yield
        exec.tick();
        assert_eq!(counter.get(), 1);
        assert!(exec.has_tasks());

        // Cancel the task
        assert!(exec.cancel_task(task_id));
        assert!(!exec.has_tasks());

        // Verify task is gone
        assert!(!exec.cancel_task(task_id)); // Already cancelled
    }

    #[test]
    fn test_cancel_pending_task() {
        let mut exec = Executor::new();
        let ran = Rc::new(Cell::new(false));
        let ran_clone = ran.clone();

        let task_id = exec.spawn(async move {
            ran_clone.set(true);
        });

        // Cancel before first tick (task is still in pending_spawn)
        assert!(exec.cancel_task(task_id));

        // Run should do nothing - task was cancelled
        exec.run();
        assert!(!ran.get());
    }

    #[test]
    fn test_cancel_nonexistent_task() {
        let mut exec = Executor::new();
        let fake_id = TaskId(9999);

        assert!(!exec.cancel_task(fake_id));
    }

    #[test]
    fn test_cancel_tasks_batch() {
        let mut exec = Executor::new();
        let counter = Rc::new(Cell::new(0));

        let mut task_ids = Vec::new();
        for _ in 0..5 {
            let counter = counter.clone();
            let id = exec.spawn(async move {
                counter.set(counter.get() + 1);
                futures::pending!();
            });
            task_ids.push(id);
        }

        // First tick: all run once
        exec.tick();
        assert_eq!(counter.get(), 5);
        assert_eq!(exec.task_count(), 5);

        // Cancel 3 of them
        let cancelled = exec.cancel_tasks(&task_ids[0..3]);
        assert_eq!(cancelled, 3);
        assert_eq!(exec.task_count(), 2);

        // Cancel including some already cancelled
        let cancelled = exec.cancel_tasks(&task_ids);
        assert_eq!(cancelled, 2); // Only 2 remaining
        assert_eq!(exec.task_count(), 0);
    }

    // ========================================================================
    // Timeout Tests
    // ========================================================================

    #[test]
    fn test_timeout_success() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static TIME: AtomicU64 = AtomicU64::new(0);

        fn mock_now() -> f64 {
            TIME.load(Ordering::SeqCst) as f64
        }

        // Future that completes immediately (std::future::ready implements Unpin)
        let future = std::future::ready(42);
        let mut timeout_fut = Timeout::with_now_fn(future, 1000.0, mock_now);

        // Poll should complete immediately
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        match Pin::new(&mut timeout_fut).poll(&mut cx) {
            Poll::Ready(Ok(42)) => {}
            other => panic!("Expected Ready(Ok(42)), got {:?}", other),
        }
    }

    #[test]
    fn test_timeout_expires() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static TIME: AtomicU64 = AtomicU64::new(2000); // Already past deadline

        fn mock_now() -> f64 {
            TIME.load(Ordering::SeqCst) as f64
        }

        // Future that would complete (std::future::ready implements Unpin)
        let future = std::future::ready(42);
        let mut timeout_fut = Timeout::with_now_fn(future, 1000.0, mock_now);

        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        match Pin::new(&mut timeout_fut).poll(&mut cx) {
            Poll::Ready(Err(TimeoutError)) => {}
            other => panic!("Expected Ready(Err(TimeoutError)), got {:?}", other),
        }
    }

    #[test]
    fn test_timeout_error_display() {
        let err = TimeoutError;
        assert_eq!(format!("{}", err), "operation timed out");
    }

    // ========================================================================
    // Task Group Tests
    // ========================================================================

    #[test]
    fn test_task_group_create() {
        let mut manager = TaskGroupManager::new();
        let group_id = manager.create_group();

        assert_eq!(manager.group_count(), 1);
        assert!(manager.get_group(group_id).is_some());
    }

    #[test]
    fn test_task_group_add_task() {
        let mut manager = TaskGroupManager::new();
        let group_id = manager.create_group();
        let task_id = TaskId(1);

        assert!(manager.add_task_to_group(task_id, group_id));
        assert_eq!(manager.get_task_group(task_id), Some(group_id));

        let group = manager.get_group(group_id).unwrap();
        assert!(group.contains(task_id));
        assert_eq!(group.task_count(), 1);
    }

    #[test]
    fn test_task_group_remove_task() {
        let mut manager = TaskGroupManager::new();
        let group_id = manager.create_group();
        let task_id = TaskId(1);

        manager.add_task_to_group(task_id, group_id);
        assert_eq!(manager.remove_task(task_id), Some(group_id));
        assert_eq!(manager.get_task_group(task_id), None);

        let group = manager.get_group(group_id).unwrap();
        assert!(!group.contains(task_id));
    }

    #[test]
    fn test_task_group_hierarchy() {
        let mut manager = TaskGroupManager::new();
        let parent_id = manager.create_group();
        let child_id = manager.create_child_group(parent_id).unwrap();

        // Add tasks to both groups
        manager.add_task_to_group(TaskId(1), parent_id);
        manager.add_task_to_group(TaskId(2), parent_id);
        manager.add_task_to_group(TaskId(3), child_id);
        manager.add_task_to_group(TaskId(4), child_id);

        // Get all tasks from parent (should include child tasks)
        let all_tasks = manager.get_all_tasks(parent_id);
        assert_eq!(all_tasks.len(), 4);

        // Get tasks from child only
        let child_tasks = manager.get_all_tasks(child_id);
        assert_eq!(child_tasks.len(), 2);
    }

    #[test]
    fn test_task_group_delete() {
        let mut manager = TaskGroupManager::new();
        let group_id = manager.create_group();
        let task_id = TaskId(1);

        manager.add_task_to_group(task_id, group_id);
        assert!(manager.delete_group(group_id));

        assert!(manager.get_group(group_id).is_none());
        assert_eq!(manager.get_task_group(task_id), None);
        assert_eq!(manager.group_count(), 0);
    }

    #[test]
    fn test_task_group_delete_with_children() {
        let mut manager = TaskGroupManager::new();
        let parent_id = manager.create_group();
        let child_id = manager.create_child_group(parent_id).unwrap();
        let grandchild_id = manager.create_child_group(child_id).unwrap();

        manager.add_task_to_group(TaskId(1), grandchild_id);

        // Delete parent should cascade
        assert!(manager.delete_group(parent_id));

        assert!(manager.get_group(parent_id).is_none());
        assert!(manager.get_group(child_id).is_none());
        assert!(manager.get_group(grandchild_id).is_none());
        assert_eq!(manager.group_count(), 0);
    }

    #[test]
    fn test_task_group_add_to_nonexistent() {
        let mut manager = TaskGroupManager::new();
        let fake_group = TaskGroupId(999);

        assert!(!manager.add_task_to_group(TaskId(1), fake_group));
    }
}
