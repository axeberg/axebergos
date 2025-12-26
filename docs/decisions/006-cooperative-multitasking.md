# ADR-006: Cooperative Multitasking

## Status
Accepted

## Context

Operating systems typically use one of two multitasking models:

1. **Preemptive**: OS interrupts tasks via timer, forces context switch
2. **Cooperative**: Tasks voluntarily yield control

In WASM:
- No timer interrupts (no hardware access)
- Single-threaded (no threads to preempt)
- Must return control to browser event loop periodically

## Decision

We will use **cooperative multitasking** based on Rust's async/await.

Tasks yield at natural suspension points:
- Waiting for I/O (read from pipe, wait for input)
- Sleeping (timers)
- Explicit yield

```rust
// Task yields when it awaits
async fn my_task() {
    let data = pipe.read().await;  // Yields here
    process(data);
    sleep(Duration::from_secs(1)).await;  // Yields here
}
```

## Consequences

### Positive

1. **Natural for WASM**: Fits the browser execution model
2. **No interrupts needed**: Tasks yield when appropriate
3. **Rust ecosystem**: Leverages async/await, futures, pinning
4. **Predictable**: No surprising preemption mid-operation
5. **Simpler kernel**: No complex scheduler or context switching
6. **Efficient**: No timer overhead, tasks run to yield point

### Negative

1. **Starvation risk**: Misbehaving task can hog CPU
2. **Must remember to yield**: CPU-bound tasks block everything
3. **Not realistic**: Real OS uses preemption
4. **Learning gap**: Users may not understand yield points

### Mitigated

1. **Starvation**: We add yield points in long-running commands
2. **Yield discipline**: Document where yields happen
3. **Realism**: We explain the difference in documentation

## How It Works

### Yield Points

```rust
// I/O operations yield
pipe.read(&mut buf).await      // Yields until data available
pipe.write(data).await         // Yields if buffer full

// Time operations yield
sleep(duration).await          // Yields until timer fires

// Explicit yield
yield_now().await              // Yields unconditionally
```

### No Yield = Blocking

```rust
// This blocks everything!
fn cpu_bound() {
    for i in 0..1_000_000_000 {
        // No await, no yield
    }
}

// Fixed version
async fn cpu_bound_friendly() {
    for i in 0..1_000_000_000 {
        if i % 10000 == 0 {
            yield_now().await;  // Give others a chance
        }
    }
}
```

### Scheduler

```rust
impl Executor {
    fn run_one_task(&mut self) -> bool {
        if let Some(task) = self.ready_queue.pop_front() {
            match task.poll() {
                Poll::Pending => {
                    // Task yielded, will be re-queued when woken
                    true
                }
                Poll::Ready(()) => {
                    // Task completed
                    true
                }
            }
        } else {
            false  // No ready tasks
        }
    }
}
```

## Alternatives Considered

### 1. True preemption (somehow)
- **Pro**: Realistic, prevents starvation
- **Con**: Not possible in standard WASM

### 2. Web Workers for parallelism
- **Pro**: True concurrent execution
- **Con**: Complex message passing, different memory spaces

### 3. Time-sliced cooperative
- **Pro**: Limits per-task execution time
- **Con**: Adds complexity, still requires explicit yields

### 4. Actor model
- **Pro**: Clear message boundaries
- **Con**: Different programming model, less Unix-like

## Comparison with Real OS

| Aspect | Real OS | axeberg |
|--------|---------|---------|
| Scheduling | Preemptive (timer interrupt) | Cooperative (await) |
| Context switch | Save/restore registers | Poll future |
| Starvation | Timer prevents | Must yield voluntarily |
| Priority | Scheduler enforced | Priority queues |
| Blocking | Doesn't block others | Blocks everything if no await |

## Lessons Learned

1. Cooperative multitasking is natural for async Rust
2. Document yield points clearly
3. Add defensive yields in CPU-bound code
4. The async/await abstraction hides most complexity
5. Users familiar with async JS/Python adapt quickly
