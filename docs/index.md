# axeberg Kernel Documentation

Welcome to the axeberg kernel documentation. axeberg is a personal mini-OS written in Rust, compiled to WebAssembly, and running in the browser.

## Philosophy

Inspired by [Radiant Computer](https://radiant.computer/) and [Oxide's Hubris](https://hubris.oxide.computer/):

- **Tractable**: The entire system should be comprehensible by one person
- **Immediate**: Changes take effect instantly, no waiting
- **Personal**: Your computing environment, under your control
- **First Principles**: Simple, elegant solutions built from the ground up

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                        Browser/WASM                          │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
│  │  Compositor │  │   Runtime   │  │    Boot     │         │
│  │  (Canvas2D) │  │ (rAF loop)  │  │  Sequence   │         │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘         │
│         │                │                │                 │
│  ┌──────┴────────────────┴────────────────┴──────┐         │
│  │                    Kernel                      │         │
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐       │         │
│  │  │ Syscall │  │ Process │  │  Memory │       │         │
│  │  │Interface│  │ Manager │  │ Manager │       │         │
│  │  └────┬────┘  └────┬────┘  └────┬────┘       │         │
│  │       │            │            │             │         │
│  │  ┌────┴────────────┴────────────┴────┐       │         │
│  │  │          Object Table             │       │         │
│  │  │   (Files, Pipes, Console, etc.)   │       │         │
│  │  └────────────────┬──────────────────┘       │         │
│  │                   │                           │         │
│  │  ┌────────────────┴──────────────────┐       │         │
│  │  │              VFS                   │       │         │
│  │  │    (In-memory filesystem)          │       │         │
│  │  └────────────────────────────────────┘       │         │
│  │                                               │         │
│  │  ┌─────────────┐  ┌─────────────┐            │         │
│  │  │  Executor   │  │     IPC     │            │         │
│  │  │ (Async/rAF) │  │  (Channels) │            │         │
│  │  └─────────────┘  └─────────────┘            │         │
│  └───────────────────────────────────────────────┘         │
└─────────────────────────────────────────────────────────────┘
```

## Documentation Sections

### Kernel

- [Kernel Overview](kernel/overview.md) - High-level architecture
- [Syscall Interface](kernel/syscalls.md) - The syscall API
- [Process Model](kernel/processes.md) - Processes and isolation
- [Memory Management](kernel/memory.md) - Allocation and shared memory
- [Object Table](kernel/objects.md) - Kernel objects and handles
- [Executor](kernel/executor.md) - Async task execution
- [IPC](kernel/ipc.md) - Inter-process communication

### Userspace

- [VFS](userspace/vfs.md) - Virtual filesystem
- [Compositor](userspace/compositor.md) - Window management
- [Standard I/O](userspace/stdio.md) - Console and pipes

### Development

- [Building](development/building.md) - Build and run instructions
- [Testing](development/testing.md) - Test suite overview
- [Contributing](development/contributing.md) - Development guidelines

## Quick Start

```bash
# Build the WASM binary
wasm-pack build --target web

# Start the dev server
cargo run --bin serve

# Open in browser
open http://localhost:8080
```

## Design Decisions

### Why Monolithic?

axeberg uses a monolithic kernel architecture despite being inspired by microkernels like Hubris. This is pragmatic for our environment:

1. **WASM Constraints**: No hardware MMU, no privilege levels, single address space
2. **Microkernel Benefits Lost**: Memory isolation impossible in WASM
3. **Simplicity Wins**: Direct function calls are faster than message passing
4. **Modularity via Design**: Clean interfaces between components, not IPC

### Why Async?

The browser's event loop drives everything. We use cooperative async multitasking:

1. `requestAnimationFrame` calls `kernel::tick()`
2. Tasks yield at natural points
3. No preemption (not needed for our use case)
4. Priority-based scheduling (Critical > Normal > Background)

### Why Reference Counting?

Kernel objects (files, pipes, windows) use reference counting:

1. Multiple processes can share objects
2. Objects are freed when no references remain
3. Simple, predictable lifetime management
4. Works well with Rust's ownership model

## Test Coverage

The kernel has comprehensive tests covering:

- Executor scheduling and priorities
- Process creation and lifecycle
- Memory allocation and limits
- Shared memory operations
- File operations via VFS
- IPC channels
- Object reference counting

Run tests with: `cargo test`

## Version

axeberg v0.1.0 - Early development

## License

MIT
