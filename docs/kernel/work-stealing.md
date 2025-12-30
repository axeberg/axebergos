# Work-Stealing Scheduler

A lock-free parallel task scheduler using the Chase-Lev deque algorithm.

## Architecture

```
                         ┌─────────────────┐
                         │ Global Injector │  ← External task spawns
                         │   (MPMC Queue)  │
                         └────────┬────────┘
                                  │
           ┌──────────────────────┼──────────────────────┐
           ▼                      ▼                      ▼
    ┌─────────────┐        ┌─────────────┐        ┌─────────────┐
    │  Worker 0   │        │  Worker 1   │        │  Worker 2   │
    │ ┌─────────┐ │        │ ┌─────────┐ │        │ ┌─────────┐ │
    │ │  Local  │ │        │ │  Local  │ │        │ │  Local  │ │
    │ │  Deque  │ │◄──────►│ │  Deque  │ │◄──────►│ │  Deque  │ │
    │ │(LIFO/FIFO)│        │ │(LIFO/FIFO)│        │ │(LIFO/FIFO)│
    │ └─────────┘ │        │ └─────────┘ │        │ └─────────┘ │
    └─────────────┘        └─────────────┘        └─────────────┘
           │                      │                      │
           └──────────────────────┴──────────────────────┘
                            Work Stealing
                           (FIFO from top)
```

## Components

### Global Injector

MPMC (multi-producer, multi-consumer) queue for external task spawns.

```rust
pub struct Injector<T> {
    buffer: Box<[AtomicCell<Option<T>>]>,
    head: AtomicUsize,    // Consumer position
    tail: AtomicUsize,    // Producer position
}

impl<T> Injector<T> {
    pub fn push(&self, task: T) -> InjectResult;
    pub fn steal(&self) -> Option<T>;
}
```

### Worker Deque

Per-worker Chase-Lev deque. Owner pushes/pops from bottom (LIFO), thieves steal from top (FIFO).

```rust
pub struct Worker<T> {
    buffer: Box<[AtomicCell<Option<T>>]>,
    bottom: AtomicUsize,  // Owner's position
    top: AtomicUsize,     // Thief's position
}

impl<T> Worker<T> {
    pub fn push(&self, task: T);           // O(1), owner only
    pub fn pop(&self) -> Option<T>;        // O(1), owner only
    pub fn stealer(&self) -> Stealer<T>;   // Get steal handle
}

pub struct Stealer<T> {
    // Cloneable reference to worker's deque
}

impl<T> Stealer<T> {
    pub fn steal(&self) -> StealResult<T>; // O(1), any thread
}
```

## Usage

```rust
use axeberg::kernel::work_stealing::{WorkStealingExecutor, Config};

// Configure executor
let config = Config::default()
    .num_workers(4)
    .local_queue_capacity(256);

let executor = WorkStealingExecutor::new(config);

// Spawn tasks
executor.spawn(async { work_a().await });
executor.spawn(async { work_b().await });

// Run until completion
executor.run();
```

## Scheduling Algorithm

```
Worker Loop:
┌─────────────────────────────────────────────────────────────┐
│  1. Pop from local deque (LIFO - cache locality)            │
│     └─ Found? Execute task, goto 1                          │
│                                                             │
│  2. Steal from global injector                              │
│     └─ Found? Execute task, goto 1                          │
│                                                             │
│  3. Steal from random peer worker (FIFO - load balance)     │
│     └─ Found? Execute task, goto 1                          │
│     └─ All empty? goto 4                                    │
│                                                             │
│  4. Park thread (wait for new work notification)            │
│     └─ Woken? goto 1                                        │
└─────────────────────────────────────────────────────────────┘
```

## Properties

Formally verified in TLA+ specification:

| Property | Description |
|----------|-------------|
| **W1: No Lost Tasks** | Every spawned task is eventually executed |
| **W2: No Double Execution** | Each task executes exactly once |
| **W3: LIFO/FIFO** | Owner pops newest (cache), thieves steal oldest (balance) |
| **W4: Linearizability** | All operations appear atomic |
| **W5: Progress** | System makes progress under fair scheduling |

## Lock-Free Guarantees

- **Wait-free push/pop**: Owner never blocks
- **Lock-free steal**: Thieves use CAS, no blocking
- **ABA-safe**: Generation counters prevent ABA problems

## Memory Ordering

```rust
// Push (owner only)
buffer[bottom].store(task, Ordering::Relaxed);
bottom.fetch_add(1, Ordering::Release);  // Publish to thieves

// Pop (owner only)
bottom.fetch_sub(1, Ordering::SeqCst);   // Sync with steal
let task = buffer[bottom].take();

// Steal (any thread)
let t = top.load(Ordering::Acquire);     // Read before buffer
let task = buffer[t].take();
top.compare_exchange(t, t+1, Ordering::SeqCst);
```

## Configuration

```rust
pub struct Config {
    /// Number of worker threads (default: num_cpus)
    pub num_workers: usize,

    /// Capacity of each worker's local deque (default: 256)
    pub local_queue_capacity: usize,

    /// Steal attempts before parking (default: 32)
    pub steal_attempts: usize,
}
```

## Performance Characteristics

| Operation | Complexity | Contention |
|-----------|------------|------------|
| Push | O(1) | None (owner only) |
| Pop | O(1) | Rare (owner vs stealer) |
| Steal | O(1) | Low (CAS retry) |
| Inject | O(1) | Moderate (MPMC) |

## When to Use

| Context | Executor |
|---------|----------|
| Browser (WASM) | Single-threaded cooperative |
| Native CLI | Work-stealing |
| Server | Work-stealing |
| Tests | Either |

## Example: Parallel Map

```rust
use axeberg::kernel::work_stealing::{WorkStealingExecutor, Config};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

let executor = WorkStealingExecutor::new(Config::default());
let results = Arc::new(Mutex::new(Vec::new()));

for i in 0..1000 {
    let results = results.clone();
    executor.spawn(async move {
        let computed = expensive_compute(i);
        results.lock().unwrap().push((i, computed));
    });
}

executor.run();
```

## Related Documentation

- [Executor](executor.md) - Single-threaded executor
- [Overview](overview.md) - Kernel architecture
- [Processes](processes.md) - Task-process relationship
