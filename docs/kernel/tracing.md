# Tracing and Instrumentation

The instrumentation system provides debugging, profiling, and monitoring capabilities for the kernel.

## Overview

The `Tracer` collects:
- **Events**: Timestamped records of kernel operations
- **Performance Counters**: Timing and count statistics per operation
- **Kernel Statistics**: Process, signal, timer, and I/O metrics

```
┌────────────────────────────────────────────────────────┐
│                      Tracer                            │
│  ┌──────────────────────────────────────────────┐      │
│  │              Event Ring Buffer               │      │
│  │  [Event 1] [Event 2] [Event 3] ... [Event N] │      │
│  │  (max 1000 events, oldest evicted first)     │      │
│  └──────────────────────────────────────────────┘      │
│                                                        │
│  ┌──────────────┐  ┌────────────────┐  ┌────────────┐  │
│  │ SyscallStats │  │ SchedulerStats │  │KernelStats │  │
│  │ open: 42     │  │ ticks: 1000    │  │ spawned: 5 │  │
│  │ read: 156    │  │ avg: 0.5ms     │  │ signals: 3 │  │
│  │ write: 89    │  │ max: 2.1ms     │  │ timers: 12 │  │
│  └──────────────┘  └────────────────┘  └────────────┘  │
└────────────────────────────────────────────────────────┘
```

## Quick Start

```rust
use axeberg::kernel::syscall::{trace_enable, trace_summary, trace_event};
use axeberg::kernel::TraceCategory;

// Enable tracing
trace_enable();

// Record custom events
trace_event(TraceCategory::Custom, "startup", Some("initializing"));

// Get summary
let summary = trace_summary();
println!("{}", summary);  // Pretty-printed stats
```

## Event Categories

```rust
pub enum TraceCategory {
    Syscall,     // Syscall entry/exit
    Process,     // Process lifecycle (spawn, exit, signal)
    Memory,      // Memory operations (alloc, free, shm)
    Timer,       // Timer events (set, fire, cancel)
    Signal,      // Signal delivery
    Scheduler,   // Task scheduling
    File,        // File operations
    Ipc,         // IPC operations
    Compositor,  // Compositor/window events
    Custom,      // User-defined events
}
```

## API Reference

### Control Functions

| Function | Description |
|----------|-------------|
| `trace_enable()` | Enable tracing |
| `trace_disable()` | Disable tracing |
| `trace_enabled()` | Check if tracing is on |
| `trace_reset()` | Clear all events and stats |

### Event Recording

```rust
// Record a simple event
trace_event(TraceCategory::File, "open", None);

// Record with detail
trace_event(TraceCategory::File, "open", Some("/etc/passwd"));
```

### Getting Statistics

```rust
let summary = trace_summary();

println!("Uptime: {:.2}s", summary.uptime / 1000.0);
println!("Syscalls: {}", summary.syscall_count);
println!("Errors: {}", summary.syscall_errors);
println!("Avg tick: {:.3}ms", summary.avg_tick_time);
```

## TraceSummary Fields

| Field | Type | Description |
|-------|------|-------------|
| `uptime` | f64 | Time since trace_enable (ms) |
| `enabled` | bool | Whether tracing is on |
| `event_count` | usize | Events in buffer |
| `syscall_count` | u64 | Total syscalls |
| `syscall_errors` | u64 | Failed syscalls |
| `tick_count` | u64 | Scheduler ticks |
| `avg_tick_time` | f64 | Average tick duration (ms) |
| `max_tick_time` | f64 | Maximum tick duration (ms) |
| `processes_spawned` | u64 | Processes created |
| `processes_exited` | u64 | Processes exited |
| `signals_delivered` | u64 | Signals sent |
| `timers_fired` | u64 | Timers that fired |
| `bytes_read` | u64 | Total bytes read |
| `bytes_written` | u64 | Total bytes written |

## Performance Counters

Each syscall category has performance counters:

```rust
pub struct PerfCounters {
    pub count: u64,       // Total calls
    pub total_time: f64,  // Total time (ms)
    pub min_time: f64,    // Minimum call time
    pub max_time: f64,    // Maximum call time
    pub errors: u64,      // Error count
}

impl PerfCounters {
    pub fn avg_time(&self) -> f64;      // Average time per call
    pub fn success_rate(&self) -> f64;  // Success ratio (0.0-1.0)
}
```

## Event Filtering

Filter events by category:

```rust
KERNEL.with(|k| {
    let tracer = k.borrow().tracer();

    // Get only syscall events
    let syscalls = tracer.events_by_category(TraceCategory::Syscall);

    // Get events for a specific process
    let pid1_events = tracer.events_by_pid(1);
});
```

## Category Filtering

Only trace specific categories:

```rust
KERNEL.with(|k| {
    let tracer = k.borrow_mut().tracer_mut();

    // Only trace syscalls and memory operations
    tracer.set_filter(Some(vec![
        TraceCategory::Syscall,
        TraceCategory::Memory,
    ]));
});
```

## Ring Buffer

The event buffer holds up to 1000 events. When full, oldest events are evicted:

```rust
const TRACE_BUFFER_SIZE: usize = 1000;

// Events are stored in a VecDeque
// New events push to the back
// When full, pop from the front
```

## Pretty-Printed Output

`TraceSummary` implements `Display`:

```
=== Kernel Statistics ===
Uptime: 1.50s
Tracing: ON
Events buffered: 42

--- Syscalls ---
Total: 156
Errors: 2

--- Scheduler ---
Ticks: 1000
Avg tick: 0.500ms
Max tick: 2.100ms

--- Processes ---
Spawned: 5
Exited: 2

--- Events ---
Signals: 3
Timers: 12

--- I/O ---
Read: 4096 bytes
Written: 1024 bytes
```

## Use Cases

### Debugging

```rust
trace_enable();
// ... run problematic code ...
let events = KERNEL.with(|k| k.borrow().tracer().events().clone());
for event in events {
    println!("[{:.2}] {} {}: {}",
        event.timestamp,
        event.category,
        event.name,
        event.detail.unwrap_or_default());
}
```

### Performance Analysis

```rust
trace_enable();
// ... run workload ...
let summary = trace_summary();

if summary.max_tick_time > 16.0 {
    println!("Warning: Tick exceeded frame budget!");
}

println!("Syscall rate: {:.1}/tick",
    summary.syscall_count as f64 / summary.tick_count as f64);
```

### Monitoring

```rust
// Periodic health check
fn health_check() {
    let summary = trace_summary();

    if summary.syscall_errors > 100 {
        warn!("High error rate detected");
    }

    if summary.avg_tick_time > 10.0 {
        warn!("Scheduler is slow");
    }
}
```

## Implementation Notes

- Tracing is disabled by default (zero overhead when off)
- Events are only recorded when tracing is enabled
- Statistics are always collected (minimal overhead)
- The ring buffer prevents unbounded memory growth
- All timestamps are in milliseconds from kernel time
