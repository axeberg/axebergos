# Kernel Structure Design

## The Prompt

> Now let's design the kernel module structure. I want clear separation of concerns but not so many files that it's hard to navigate. What modules do we need?

## Module Discussion

### Core Modules (Essential)

| Module | Responsibility | Why Essential |
|--------|---------------|---------------|
| `syscall.rs` | System call dispatch | Entry point for all kernel operations |
| `process.rs` | Process lifecycle | Core abstraction of computation |
| `memory.rs` | Memory tracking | Per-process limits and accounting |
| `executor.rs` | Async runtime | Runs the async tasks |

### IPC Modules (Unix-like communication)

| Module | Responsibility |
|--------|---------------|
| `ipc/pipe.rs` | Anonymous and named pipes |
| `ipc/queue.rs` | Message queues (POSIX-style) |
| `ipc/shm.rs` | Shared memory regions |
| `ipc/semaphore.rs` | Counting semaphores |

### User Management

| Module | Responsibility |
|--------|---------------|
| `users.rs` | User/group database |
| `session.rs` | Login sessions |

### Virtual Filesystems

| Module | Responsibility |
|--------|---------------|
| `procfs.rs` | /proc filesystem |
| `devfs.rs` | /dev filesystem |
| `sysfs.rs` | /sys filesystem |

### Miscellaneous

| Module | Responsibility |
|--------|---------------|
| `signals.rs` | Signal delivery |
| `timers.rs` | Timer queue |
| `objects.rs` | Kernel object table |
| `tracing.rs` | Debug output |

## Design Decisions

### Q: Why not put everything in one big file?

> "Modules should be cohesive - each one has a single clear purpose. But they shouldn't be so small that you're jumping between 50 files to understand one feature."

Guidelines we followed:
- 200-800 lines per module is ideal
- If > 1000 lines, consider splitting
- If < 100 lines, consider merging

### Q: How should modules interact?

```rust
// Bad: Modules reach into each other's internals
kernel.process_table.table[pid].memory.allocated

// Good: Clean APIs between modules
kernel.process(pid)?.memory_allocated()
```

### Q: What about the kernel struct itself?

```rust
pub struct Kernel {
    // Core state
    process_table: ProcessTable,
    object_table: ObjectTable,
    vfs: MemoryFs,

    // User management
    user_db: UserDatabase,
    session_manager: SessionManager,

    // IPC
    pipes: PipeRegistry,
    message_queues: MessageQueueRegistry,
    shared_memory: SharedMemoryRegistry,

    // Time
    timer_queue: TimerQueue,

    // Misc
    console: ConsoleHandle,
}
```

Each component is its own type with its own impl block. The Kernel struct composes them.

## File Organization Result

```
src/kernel/
├── mod.rs              # Kernel struct, re-exports
├── syscall.rs          # ~500 lines - syscall dispatch
├── process.rs          # ~400 lines - process table
├── memory.rs           # ~300 lines - memory tracking
├── executor.rs         # ~400 lines - async runtime
├── objects.rs          # ~200 lines - handle table
├── signals.rs          # ~300 lines - signal delivery
├── timers.rs           # ~250 lines - timer queue
├── users.rs            # ~400 lines - user/group DB
├── session.rs          # ~200 lines - sessions
├── procfs.rs           # ~300 lines - /proc
├── devfs.rs            # ~200 lines - /dev
├── sysfs.rs            # ~150 lines - /sys
├── tracing.rs          # ~100 lines - debug output
├── ipc/
│   ├── mod.rs          # IPC types
│   ├── pipe.rs         # ~300 lines
│   ├── queue.rs        # ~250 lines
│   ├── shm.rs          # ~200 lines
│   └── semaphore.rs    # ~150 lines
└── wasm/
    ├── mod.rs          # WASM loading
    └── abi.rs          # ABI definitions
```

## Lessons

1. **Start small, split later**: We started with fewer files and split when they grew
2. **Consistent patterns**: Each module has similar structure (types, impl, tests)
3. **Tests alongside code**: Each module has a `#[cfg(test)] mod tests` section
