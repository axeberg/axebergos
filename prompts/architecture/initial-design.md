# Initial System Design

## The Prompt

> Build a mini operating system in Rust that compiles to WebAssembly and runs in the browser. It should be:
>
> 1. **Tractable**: Small enough that one person can understand the entire codebase
> 2. **Immediate**: Changes take effect instantly, no long compile cycles
> 3. **Personal**: A computing environment you can fully control
> 4. **First Principles**: Simple, elegant solutions over complex ones
>
> Start with the core kernel: process management, a basic filesystem, and a shell.

## Design Discussion

### Q: What should the process model look like?

The AI considered several approaches:

1. **Full preemptive multitasking** - Too complex for WASM (no true threads)
2. **Cooperative multitasking** - Simpler but requires explicit yields
3. **Async/await based** - Natural fit for JavaScript interop

**Decision**: Use Rust's async/await with a custom executor. Processes are async tasks that can yield at I/O boundaries.

### Q: What about the filesystem?

Options considered:

1. **Browser localStorage** - Limited to 5-10MB
2. **IndexedDB** - Complex API, async only
3. **Origin Private File System (OPFS)** - Modern, fast, up to gigabytes
4. **Pure in-memory** - Simplest, but no persistence

**Decision**: In-memory VFS with optional OPFS persistence. This gives us speed during operation and persistence when needed.

### Q: How Unix-like should it be?

> "Make it Unix-like enough that the patterns are familiar, but don't be a slave to POSIX compliance. Simplify where it makes sense."

This led to:
- File descriptors and handles (familiar concept)
- Pipes and redirects (essential UX)
- Users/groups (security model)
- But NO: signals like SIGKILL (simplified), fork() (use spawn), complex errno

## Initial Structure

```
src/
├── lib.rs              # WASM entry point
├── kernel/
│   ├── mod.rs          # Kernel struct
│   ├── syscall.rs      # System calls
│   ├── process.rs      # Process table
│   └── executor.rs     # Async runtime
├── vfs/
│   └── memory.rs       # In-memory filesystem
└── shell/
    ├── mod.rs          # Shell entry
    ├── parser.rs       # Command parsing
    └── executor.rs     # Command execution
```

## Key Design Decisions

### 1. Single-threaded with async

WASM doesn't have true threads (without SharedArrayBuffer), so we embrace single-threaded async. This simplifies everything:

- No locks needed (mostly)
- No race conditions (mostly)
- Easier to reason about

### 2. Everything in one binary

No separate kernel/userspace binaries. Everything compiles to one WASM module. This keeps deployment simple:

```
index.html + axeberg.wasm = complete OS
```

### 3. Shell-first UX

The primary interface is a terminal. This is:
- Familiar to developers
- Text-based (works everywhere)
- Scriptable

### 4. Kernel as library

The kernel is just a Rust struct with methods. "System calls" are just method calls:

```rust
kernel.read(fd, &mut buf)?;
kernel.write(fd, data)?;
kernel.spawn(command)?;
```

## Evolution

This initial design evolved as we added features:

- Added IPC (pipes, shared memory, message queues)
- Added sessions for multi-user support
- Added proc/sys/dev virtual filesystems
- Added TTY handling for terminal interaction

But the core principles remained: **tractable, simple, Unix-like**.
