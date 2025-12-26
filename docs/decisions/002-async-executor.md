# ADR-002: Custom Async Executor

## Status
Accepted

## Context

WASM in the browser runs single-threaded. We need a way to:
- Run multiple "processes" concurrently
- Handle I/O without blocking
- Yield control back to the browser event loop
- Provide scheduling between tasks

Options for concurrency in WASM:
1. Use an existing async runtime (tokio, async-std)
2. Build a custom executor
3. Use Web Workers for true parallelism

## Decision

We will build a **custom async executor** tailored to our needs.

The executor will:
- Use Rust's async/await with custom Future implementations
- Implement priority-based scheduling (Critical > Normal > Background)
- Integrate with browser's event loop via wasm-bindgen-futures
- Support task spawning, sleeping, and waking

```rust
pub struct Executor {
    critical: VecDeque<Task>,
    normal: VecDeque<Task>,
    background: VecDeque<Task>,
}
```

## Consequences

### Positive

1. **Full control**: We understand every line of code
2. **Minimal size**: No large runtime dependencies
3. **Priority scheduling**: Can prioritize interactive tasks
4. **Educational**: Demonstrates how async works
5. **Tailored**: Optimized for our specific needs

### Negative

1. **Maintenance burden**: We own all the code
2. **Subtle bugs**: Async is tricky to get right
3. **Missing features**: No work-stealing, timers, etc. (until we add them)
4. **Reinventing wheels**: Some of this exists in libraries

## Alternatives Considered

### 1. tokio
- **Pro**: Battle-tested, full-featured, well-documented
- **Con**: Huge dependency, designed for async I/O not our use case, pulls in many features we don't need

### 2. async-std
- **Pro**: Cleaner API than tokio
- **Con**: Still large, not designed for WASM

### 3. smol
- **Pro**: Minimal, educational
- **Con**: Still more than we need, async I/O focused

### 4. Web Workers
- **Pro**: True parallelism
- **Con**: Complex message passing, SharedArrayBuffer restrictions, doesn't fit our process model

### 5. No concurrency (sequential execution)
- **Pro**: Simplest
- **Con**: Can't have background tasks, blocking I/O blocks everything

## Implementation Notes

Key components of our executor:

```rust
// Task wrapper
struct Task {
    id: TaskId,
    future: Pin<Box<dyn Future<Output = ()>>>,
    priority: Priority,
}

// Priority levels
enum Priority {
    Critical,   // Interactive, user-facing
    Normal,     // Regular processes
    Background, // Cleanup, persistence
}

// Main loop
fn run_until_idle(&mut self) {
    while let Some(task) = self.next_task() {
        let waker = self.make_waker(task.id);
        let mut cx = Context::from_waker(&waker);

        match task.future.poll(&mut cx) {
            Poll::Ready(()) => { /* task done */ }
            Poll::Pending => { /* will be re-queued on wake */ }
        }
    }
}
```

We later added:
- Timer integration (sleep, timeouts)
- Process-aware scheduling
- Signal delivery points

## Lessons Learned

1. Start simple: Our first version was ~100 lines
2. Add features incrementally: Timers, priorities came later
3. TLA+ helped: We specified the scheduling invariants formally
4. Test extensively: Async bugs are subtle and timing-dependent
