# Executor

The executor is axeberg's async task scheduler, designed for browser integration.

## Design Goals

1. **Browser-native**: Works with requestAnimationFrame
2. **Cooperative**: No preemption needed
3. **Priority-aware**: Critical tasks run first
4. **Tick-based**: Runs in discrete frames

## Architecture

```rust
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
```

## Tasks

The Task trait defines the interface for programs/modules:

```rust
pub trait Task: Send + 'static {
    /// Human-readable name for this task
    fn name(&self) -> &'static str;

    /// The task's main execution. Returns a future that drives the task.
    fn run(&mut self) -> Pin<Box<dyn Future<Output = ()> + '_>>;
}

// Tasks are compiled in and registered with the kernel at boot.
// For spawned futures, the executor uses BoxFuture:
pub type BoxFuture = Pin<Box<dyn Future<Output = ()> + 'static>>;
```

## Priority Levels

```rust
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Critical = 0,    // High priority (UI, input)
    Normal = 1,      // Default
    Background = 2,  // Low priority
}
```

Priority affects scheduling order:
- All Critical tasks run before Normal
- All Normal tasks run before Background
- Within a priority, FIFO order

## Spawning Tasks

```rust
// Normal priority (default)
let task_id = kernel::spawn(async {
    println!("Hello from task!");
});

// With explicit priority
let task_id = kernel::spawn_with_priority(
    async {
        // Handle user input
    },
    Priority::Critical,
);
```

## The Tick Loop

The executor is driven by `requestAnimationFrame`:

```rust
// Called ~60 times per second (from src/kernel/executor.rs)
pub fn tick(&mut self) -> usize {
    self.integrate_pending();

    // Collect ready task IDs, sorted by priority
    let mut ready_ids: Vec<TaskId> = self.ready.borrow().iter().copied().collect();
    ready_ids.sort_by_key(|id| {
        self.tasks.get(id).map(|t| t.priority).unwrap_or(Priority::Background)
    });

    let mut polled = 0;

    for task_id in ready_ids {
        self.ready.borrow_mut().remove(&task_id);

        let Some(mut task) = self.tasks.remove(&task_id) else {
            continue;
        };

        let waker = self.create_waker(task_id);
        let mut cx = Context::from_waker(&waker);

        match task.future.as_mut().poll(&mut cx) {
            Poll::Ready(()) => {
                // Task completed, don't re-insert
                polled += 1;
            }
            Poll::Pending => {
                // Task yielded, put it back
                self.tasks.insert(task_id, task);
                polled += 1;
            }
        }
    }

    polled
}
```

## Waker Semantics

When a task blocks, it needs to be woken later:

```rust
// Waker adds task ID back to ready set
fn wake(&mut self, task_id: TaskId) {
    if self.tasks.contains_key(&task_id) {
        self.ready.borrow_mut().insert(task_id);
    }
}
```

## Run Mode

For non-UI contexts (tests, CLI), run all tasks to completion:

```rust
pub fn run(&mut self) {
    while self.has_tasks() {
        self.tick();
    }
}
```

## Task Lifecycle

```
spawn() ──► Ready ──► Running ──► Completed
               ▲         │
               │         │ Poll::Pending
               │         ▼
               └─── Blocked
                (wake re-queues)
```

## Browser Integration

The runtime connects the executor to the browser:

```rust
// In runtime.rs
fn tick_loop() {
    let timestamp = /* from rAF */;

    // Push frame event
    events::push_system(events::SystemEvent::Frame { timestamp });

    // Process input events
    process_compositor_events();

    // Tick the kernel (runs tasks)
    kernel::tick();

    // Render compositor
    compositor::render();

    // Schedule next frame
    request_animation_frame();
}
```

## Example: Yielding

Tasks can yield to allow other tasks to run:

```rust
kernel::spawn(async {
    for i in 0..100 {
        do_work(i);

        // Yield control, resume next tick
        futures::pending!();
    }
});
```

## Example: Multiple Priorities

```rust
// Background indexing
kernel::spawn_with_priority(
    async {
        index_files().await;
    },
    Priority::Background,
);

// User input handling
kernel::spawn_with_priority(
    async {
        loop {
            let event = get_next_event().await;
            handle_event(event);
        }
    },
    Priority::Critical,
);

// Normal application work
kernel::spawn(async {
    process_data().await;
});
```

Input handling runs before normal work, which runs before indexing.

## Testing the Executor

```rust
#[test]
fn test_priority_order() {
    let mut executor = Executor::new();

    let order = Rc::new(RefCell::new(Vec::new()));

    // Spawn in reverse priority order
    let o = order.clone();
    executor.spawn_with_priority(async move {
        o.borrow_mut().push("background");
    }, Priority::Background);

    let o = order.clone();
    executor.spawn_with_priority(async move {
        o.borrow_mut().push("critical");
    }, Priority::Critical);

    let o = order.clone();
    executor.spawn(async move {
        o.borrow_mut().push("normal");
    });

    executor.run();

    // Critical ran first, then Normal, then Background
    assert_eq!(*order.borrow(), vec!["critical", "normal", "background"]);
}
```

## Limitations (Single-Threaded Executor)

1. **No preemption**: Long-running sync code blocks everything
2. **Single-threaded**: No parallelism
3. **No deadlock detection**: Circular waits hang forever
4. **Trust required**: Tasks must yield cooperatively

## Work Stealing Executor

For native/multi-threaded contexts, axeberg provides a lock-free work stealing
executor in `kernel::work_stealing`:

```rust
use axeberg::kernel::{WorkStealingExecutor, WorkStealingConfig, Priority};

// Configure with 4 worker threads
let config = WorkStealingConfig::default().num_workers(4);
let mut executor = WorkStealingExecutor::new(config);

// Spawn tasks (distributed across workers)
for i in 0..100 {
    executor.spawn(async move {
        println!("Task {} running!", i);
    });
}

// Run until all complete
executor.run();
```

### Architecture

```text
                   ┌─────────────────┐
                   │ Global Injector │  ← External spawns
                   │   (MPMC Queue)  │
                   └────────┬────────┘
                            │
         ┌──────────────────┼──────────────────┐
         ▼                  ▼                  ▼
  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
  │  Worker 0   │    │  Worker 1   │    │  Worker 2   │
  │ Local Deque │◄──►│ Local Deque │◄──►│ Local Deque │
  │  (LIFO/FIFO)│    │  (LIFO/FIFO)│    │  (LIFO/FIFO)│
  └─────────────┘    └─────────────┘    └─────────────┘
        │                  │                  │
        └──────────────────┴──────────────────┘
                     Work Stealing
                    (FIFO from top)
```

### Key Properties (TLA+ Verified)

The implementation is formally verified in `specs/tla/WorkStealing.tla`:

- **W1: No Lost Tasks** - Every spawned task is eventually executed
- **W2: No Double Execution** - Each task executes exactly once
- **W3: LIFO Local / FIFO Steal** - Owner pops newest, thieves steal oldest
- **W4: Linearizability** - All operations appear atomic
- **W5: Progress** - System makes progress under fair scheduling

### Lock-Free Data Structures

The Chase-Lev work stealing deque provides:
- **O(1) push/pop** for the owner thread (LIFO)
- **O(1) steal** for thief threads (FIFO)
- **Lock-free**: No thread blocks another indefinitely
- **ABA-safe**: Generation counters prevent ABA problems

## Related Documentation

- [Kernel Overview](overview.md) - Executor's role in the kernel
- [Process Model](processes.md) - Process-task relationship
- [IPC](ipc.md) - Async communication primitives
- [Future Work](../future-work.md) - Planned enhancements
