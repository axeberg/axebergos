//! Work stealing executor for parallel task execution
//!
//! This module provides a multi-threaded async executor using work stealing
//! for load balancing. Based on the Chase-Lev deque algorithm.
//!
//! # Architecture
//!
//! ```text
//!                    ┌─────────────────┐
//!                    │ Global Injector │  ← External spawns
//!                    │   (MPMC Queue)  │
//!                    └────────┬────────┘
//!                             │
//!          ┌──────────────────┼──────────────────┐
//!          ▼                  ▼                  ▼
//!   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
//!   │  Worker 0   │    │  Worker 1   │    │  Worker 2   │
//!   │ Local Deque │◄──►│ Local Deque │◄──►│ Local Deque │
//!   │  (LIFO/FIFO)│    │  (LIFO/FIFO)│    │  (LIFO/FIFO)│
//!   └─────────────┘    └─────────────┘    └─────────────┘
//!         │                  │                  │
//!         └──────────────────┴──────────────────┘
//!                      Work Stealing
//!                     (FIFO from top)
//! ```
//!
//! # Key Properties (verified in TLA+ spec)
//!
//! - **W1: No Lost Tasks** - Every spawned task is eventually executed
//! - **W2: No Double Execution** - Each task executes exactly once
//! - **W3: LIFO Local / FIFO Steal** - Owner pops newest, thieves steal oldest
//! - **W4: Linearizability** - All operations appear atomic
//! - **W5: Progress** - System makes progress under fair scheduling
//!
//! # Usage
//!
//! ```ignore
//! use axeberg::kernel::work_stealing::{WorkStealingExecutor, Config};
//!
//! let config = Config::default().num_workers(4);
//! let executor = WorkStealingExecutor::new(config);
//!
//! executor.spawn(async {
//!     println!("Hello from work stealing executor!");
//! });
//!
//! executor.run();
//! ```

mod deque;
mod injector;

pub use deque::{StealResult, Stealer, Worker};
pub use injector::{InjectResult, Injector};

use super::task::{BoxFuture, TaskId};
use super::Priority;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::thread::{self, JoinHandle};

/// Configuration for the work stealing executor
#[derive(Debug, Clone)]
pub struct Config {
    /// Number of worker threads
    pub num_workers: usize,
    /// Capacity of each worker's local deque
    pub local_queue_capacity: usize,
    /// Number of steal attempts before parking
    pub steal_attempts: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            num_workers: num_cpus(),
            local_queue_capacity: 256,
            steal_attempts: 32,
        }
    }
}

impl Config {
    pub fn num_workers(mut self, n: usize) -> Self {
        self.num_workers = n.max(1);
        self
    }

    pub fn local_queue_capacity(mut self, n: usize) -> Self {
        self.local_queue_capacity = n.next_power_of_two();
        self
    }
}

/// Get number of CPUs (fallback to 1)
fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// A task wrapper with metadata
struct ManagedTask {
    id: TaskId,
    /// Priority level for scheduling (Critical > Normal > Background)
    /// Reserved for future priority-aware work distribution.
    /// In work-stealing schedulers, priority is typically handled by:
    /// 1. Preferring local work (cache locality)
    /// 2. Using priority queues within local deques
    /// 3. Weighted stealing based on priority
    #[allow(dead_code)]
    priority: Priority,
    future: BoxFuture,
}

// ManagedTask needs to be Send for cross-thread transfer
unsafe impl Send for ManagedTask {}

/// Shared state for cross-thread waker
struct WakerState {
    task_id: TaskId,
    /// Pointer to executor's shared state for signaling
    shared: Arc<SharedState>,
}

/// State shared between all workers and the executor
struct SharedState {
    /// Global task counter for ID generation
    next_task_id: AtomicU64,

    /// Total number of workers
    num_workers: usize,

    /// Shutdown flag
    shutdown: AtomicBool,

    /// Number of active (non-parked) workers
    active_workers: AtomicUsize,

    /// Condition variable for worker parking
    park_mutex: Mutex<()>,
    park_condvar: Condvar,

    /// Stealers for all workers (for cross-worker stealing)
    stealers: Vec<Stealer<ManagedTask>>,

    /// Global injector queue
    injector: Injector<ManagedTask>,

    /// Set of ready task IDs (for wake tracking)
    /// Using a mutex for simplicity; could use lock-free set
    ready_tasks: Mutex<std::collections::HashSet<TaskId>>,
}

impl SharedState {
    fn next_task_id(&self) -> TaskId {
        TaskId(self.next_task_id.fetch_add(1, Ordering::Relaxed))
    }

    fn signal_work_available(&self) {
        // Wake one parked worker
        self.park_condvar.notify_one();
    }

    fn signal_all(&self) {
        self.park_condvar.notify_all();
    }

    fn mark_ready(&self, task_id: TaskId) {
        let mut ready = self.ready_tasks.lock().unwrap();
        ready.insert(task_id);
        drop(ready);
        self.signal_work_available();
    }

    fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed)
    }
}

/// Per-worker thread-local state
struct WorkerState {
    /// Worker ID
    id: usize,

    /// Local deque owner handle
    local: Worker<ManagedTask>,

    /// Shared state
    shared: Arc<SharedState>,

    /// Current random victim for stealing
    steal_rng: usize,
}

impl WorkerState {
    /// Find work: local queue → injector → steal from others
    fn find_work(&mut self) -> Option<ManagedTask> {
        // 1. Try local queue first (LIFO - cache hot)
        if let StealResult::Success(task) = self.local.pop() {
            return Some(task);
        }

        // 2. Try global injector
        if let InjectResult::Success(task) = self.shared.injector.steal() {
            return Some(task);
        }

        // 3. Try stealing from other workers
        self.try_steal()
    }

    /// Attempt to steal from other workers
    fn try_steal(&mut self) -> Option<ManagedTask> {
        let num_workers = self.shared.num_workers;
        if num_workers <= 1 {
            return None;
        }

        // Try each worker (starting from random position)
        for i in 0..num_workers {
            let victim = (self.steal_rng + i) % num_workers;
            if victim == self.id {
                continue;
            }

            loop {
                match self.shared.stealers[victim].steal() {
                    StealResult::Success(task) => {
                        // Update RNG for next steal attempt
                        self.steal_rng = (victim + 1) % num_workers;
                        return Some(task);
                    }
                    StealResult::Empty => break, // Try next victim
                    StealResult::Retry => continue, // Retry same victim
                }
            }
        }

        // Update RNG even on failure
        self.steal_rng = (self.steal_rng + 1) % num_workers;
        None
    }

    /// Park this worker until work is available
    fn park(&self) {
        self.shared.active_workers.fetch_sub(1, Ordering::SeqCst);

        let guard = self.shared.park_mutex.lock().unwrap();
        // Double-check there's no work before parking
        if !self.shared.is_shutdown()
            && self.local.is_empty()
            && self.shared.injector.is_empty()
        {
            let _guard = self.shared.park_condvar.wait(guard);
        }

        self.shared.active_workers.fetch_add(1, Ordering::SeqCst);
    }

    /// Create a waker for the given task
    fn create_waker(&self, task_id: TaskId) -> Waker {
        let state = Box::new(WakerState {
            task_id,
            shared: self.shared.clone(),
        });
        let ptr = Box::into_raw(state) as *const ();
        let raw = RawWaker::new(ptr, &WAKER_VTABLE);
        unsafe { Waker::from_raw(raw) }
    }

    /// Run the worker loop
    fn run(&mut self) {
        while !self.shared.is_shutdown() {
            // Find work
            let task = match self.find_work() {
                Some(t) => t,
                None => {
                    // No work, park
                    self.park();
                    continue;
                }
            };

            // Poll the task
            self.poll_task(task);
        }
    }

    /// Poll a task once
    fn poll_task(&mut self, mut task: ManagedTask) {
        let waker = self.create_waker(task.id);
        let mut cx = Context::from_waker(&waker);

        match task.future.as_mut().poll(&mut cx) {
            Poll::Ready(()) => {
                // Task completed, remove from ready set
                let mut ready = self.shared.ready_tasks.lock().unwrap();
                ready.remove(&task.id);
            }
            Poll::Pending => {
                // Task yielded, check if it's still ready
                let ready = self.shared.ready_tasks.lock().unwrap();
                let is_ready = ready.contains(&task.id);
                drop(ready);

                if is_ready {
                    // Re-queue for immediate re-poll
                    let _ = self.local.push(task);
                } else {
                    // Store task for later wake
                    // For now, we re-queue it (a more sophisticated impl would use a parking lot)
                    let _ = self.local.push(task);
                }
            }
        }
    }
}

/// Waker vtable implementation
const WAKER_VTABLE: RawWakerVTable =
    RawWakerVTable::new(waker_clone, waker_wake, waker_wake_by_ref, waker_drop);

unsafe fn waker_clone(ptr: *const ()) -> RawWaker {
    // Safety: ptr came from Box::into_raw in create_waker
    let state = unsafe { &*(ptr as *const WakerState) };
    let cloned = Box::new(WakerState {
        task_id: state.task_id,
        shared: state.shared.clone(),
    });
    RawWaker::new(Box::into_raw(cloned) as *const (), &WAKER_VTABLE)
}

unsafe fn waker_wake(ptr: *const ()) {
    // Safety: ptr came from Box::into_raw in create_waker, and this consumes it
    let state = unsafe { Box::from_raw(ptr as *mut WakerState) };
    state.shared.mark_ready(state.task_id);
}

unsafe fn waker_wake_by_ref(ptr: *const ()) {
    // Safety: ptr came from Box::into_raw in create_waker
    let state = unsafe { &*(ptr as *const WakerState) };
    state.shared.mark_ready(state.task_id);
}

unsafe fn waker_drop(ptr: *const ()) {
    // Safety: ptr came from Box::into_raw in create_waker
    drop(unsafe { Box::from_raw(ptr as *mut WakerState) });
}

/// Handle returned when spawning a task
#[derive(Debug, Clone, Copy)]
pub struct TaskHandle {
    id: TaskId,
}

impl TaskHandle {
    pub fn id(&self) -> TaskId {
        self.id
    }
}

/// The work stealing executor
pub struct WorkStealingExecutor {
    /// Shared state
    shared: Arc<SharedState>,

    /// Worker thread handles
    workers: Vec<JoinHandle<()>>,

    /// Local workers for pushing (kept for spawning from main thread)
    local_pushers: Vec<Worker<ManagedTask>>,
}

impl WorkStealingExecutor {
    /// Create a new work stealing executor with the given configuration
    pub fn new(config: Config) -> Self {
        let num_workers = config.num_workers;

        // Create worker deques
        let mut local_pushers = Vec::with_capacity(num_workers);
        let mut stealers = Vec::with_capacity(num_workers);

        for _ in 0..num_workers {
            let (worker, stealer) = Worker::new(config.local_queue_capacity);
            local_pushers.push(worker);
            stealers.push(stealer);
        }

        // Create shared state
        let shared = Arc::new(SharedState {
            next_task_id: AtomicU64::new(0),
            num_workers,
            shutdown: AtomicBool::new(false),
            active_workers: AtomicUsize::new(num_workers),
            park_mutex: Mutex::new(()),
            park_condvar: Condvar::new(),
            stealers,
            injector: Injector::new(),
            ready_tasks: Mutex::new(std::collections::HashSet::new()),
        });

        // Note: Workers are spawned lazily in run() or explicitly via spawn_workers()
        WorkStealingExecutor {
            shared,
            workers: Vec::with_capacity(num_workers),
            local_pushers,
        }
    }

    /// Spawn a future onto the executor
    pub fn spawn<F>(&self, future: F) -> TaskHandle
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.spawn_with_priority(future, Priority::Normal)
    }

    /// Spawn a future with a specific priority
    pub fn spawn_with_priority<F>(&self, future: F, priority: Priority) -> TaskHandle
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let id = self.shared.next_task_id();

        let task = ManagedTask {
            id,
            priority,
            future: Box::pin(future),
        };

        // Mark as ready
        {
            let mut ready = self.shared.ready_tasks.lock().unwrap();
            ready.insert(id);
        }

        // Push to global injector
        self.shared.injector.push(task);

        // Signal workers
        self.shared.signal_work_available();

        TaskHandle { id }
    }

    /// Spawn worker threads
    fn spawn_workers(&mut self) {
        // Take the local pushers and create worker threads
        let local_pushers = std::mem::take(&mut self.local_pushers);

        for (id, local) in local_pushers.into_iter().enumerate() {
            let shared = self.shared.clone();

            let handle = thread::Builder::new()
                .name(format!("work-stealer-{}", id))
                .spawn(move || {
                    let mut worker = WorkerState {
                        id,
                        local,
                        shared,
                        steal_rng: id,
                    };
                    worker.run();
                })
                .expect("Failed to spawn worker thread");

            self.workers.push(handle);
        }
    }

    /// Run the executor until all tasks complete
    pub fn run(&mut self) {
        // Spawn worker threads if not already running
        if self.workers.is_empty() && !self.local_pushers.is_empty() {
            self.spawn_workers();
        }

        // Wait for all work to complete
        loop {
            // Check if there's any work left
            let has_work = !self.shared.injector.is_empty()
                || self.shared.stealers.iter().any(|s| !s.is_empty());

            if !has_work {
                // Check if all workers are parked (no in-flight work)
                let active = self.shared.active_workers.load(Ordering::SeqCst);
                if active == 0 {
                    break;
                }
            }

            // Brief sleep to avoid busy-wait
            std::thread::sleep(std::time::Duration::from_micros(100));
        }
    }

    /// Shutdown the executor
    pub fn shutdown(&mut self) {
        self.shared.shutdown.store(true, Ordering::SeqCst);
        self.shared.signal_all();

        // Wait for all workers to finish
        for handle in self.workers.drain(..) {
            let _ = handle.join();
        }
    }

    /// Get the number of pending tasks
    pub fn pending_tasks(&self) -> usize {
        self.shared.ready_tasks.lock().unwrap().len()
    }
}

impl Drop for WorkStealingExecutor {
    fn drop(&mut self) {
        if !self.workers.is_empty() {
            self.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn test_spawn_and_run() {
        let config = Config::default().num_workers(2);
        let mut executor = WorkStealingExecutor::new(config);

        let counter = Arc::new(AtomicUsize::new(0));

        for _ in 0..10 {
            let counter = counter.clone();
            executor.spawn(async move {
                counter.fetch_add(1, Ordering::SeqCst);
            });
        }

        executor.run();
        executor.shutdown();

        assert_eq!(counter.load(Ordering::SeqCst), 10);
    }

    #[test]
    fn test_work_stealing() {
        // Force imbalanced work
        let config = Config::default().num_workers(4);
        let mut executor = WorkStealingExecutor::new(config);

        let counter = Arc::new(AtomicUsize::new(0));

        // Spawn many tasks - they should be distributed via stealing
        for _ in 0..100 {
            let counter = counter.clone();
            executor.spawn(async move {
                // Small delay to allow stealing
                std::thread::yield_now();
                counter.fetch_add(1, Ordering::SeqCst);
            });
        }

        executor.run();
        executor.shutdown();

        assert_eq!(counter.load(Ordering::SeqCst), 100);
    }
}
