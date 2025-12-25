# Timers

The timer system provides time-based scheduling for the kernel, enabling async sleep operations and periodic callbacks.

## Overview

Timers are managed by a `TimerQueue` that uses a min-heap for efficient scheduling. When a timer fires, it wakes the associated task (if any).

```
┌─────────────────────────────────────────┐
│              TimerQueue                  │
│  ┌─────────────────────────────────┐    │
│  │         Min-Heap                │    │
│  │  ┌─────┐ ┌─────┐ ┌─────┐       │    │
│  │  │ 50ms│ │100ms│ │150ms│  ...  │    │
│  │  └─────┘ └─────┘ └─────┘       │    │
│  └─────────────────────────────────┘    │
│                                          │
│  tick(now) → Vec<TaskId> to wake        │
└─────────────────────────────────────────┘
```

## Timer Types

### One-shot Timers

Fire once after a delay, then are removed:

```rust
use axeberg::kernel::syscall::{timer_set, timer_cancel};

// Set timer for 100ms from now
let timer_id = timer_set(100.0, Some(my_task_id))?;

// Cancel if no longer needed
timer_cancel(timer_id)?;
```

### Interval Timers

Fire repeatedly at a fixed interval:

```rust
use axeberg::kernel::syscall::timer_interval;

// Fire every 50ms
let timer_id = timer_interval(50.0, Some(my_task_id))?;

// Cancel to stop
timer_cancel(timer_id)?;
```

## Timer States

```rust
pub enum TimerState {
    Pending,   // Waiting to fire
    Fired,     // Has fired (one-shot only)
    Cancelled, // Was cancelled
}
```

## Kernel Integration

The kernel processes timers during each tick:

```rust
// In the main loop (e.g., requestAnimationFrame)
let now = performance_now();
let tasks_to_wake = kernel.tick(now);
executor.wake_tasks(&tasks_to_wake);
executor.tick();
```

## API Reference

### Syscalls

| Function | Description |
|----------|-------------|
| `timer_set(delay_ms, wake_task)` | Set a one-shot timer |
| `timer_interval(interval_ms, wake_task)` | Set a repeating timer |
| `timer_cancel(timer_id)` | Cancel a pending timer |

### Kernel Methods

| Method | Description |
|--------|-------------|
| `kernel.tick(now)` | Process timers, return tasks to wake |
| `kernel.time_until_next_timer()` | Time until next timer (for sleep) |
| `kernel.pending_timer_count()` | Number of pending timers |

## Implementation Details

### Min-Heap Scheduling

The timer queue uses a `BinaryHeap` with fire times as keys. This provides O(log n) insertion and O(log n) extraction of the next timer to fire.

### Monotonic Time

The kernel maintains a monotonic `now` field updated each tick. All timer fire times are absolute values relative to this clock.

### Task Wake Integration

When a timer fires:
1. The timer is removed from the heap (one-shot) or rescheduled (interval)
2. If `wake_task` was specified, the task ID is collected
3. The kernel returns all task IDs to wake
4. The executor's `wake_tasks()` method marks them as ready

## Example: Async Sleep

```rust
async fn sleep(ms: f64) {
    let task_id = current_task_id();
    timer_set(ms, Some(task_id)).unwrap();
    // Yield until woken
    futures::pending!();
}

async fn my_task() {
    println!("Starting...");
    sleep(1000.0).await;  // Sleep 1 second
    println!("Done!");
}
```
