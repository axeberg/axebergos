# axeberg Documentation

A mini operating system written in Rust, compiled to WebAssembly.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                                  Browser                                     │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                             axeberg OS                                 │  │
│  │                                                                        │  │
│  │  ┌──────────────────────────────────────────────────────────────────┐ │  │
│  │  │ Shell                     $ cat file | grep pattern | wc -l      │ │  │
│  │  └─────────────────────────────────────┬────────────────────────────┘ │  │
│  │                                        │                              │  │
│  │                                        ▼                              │  │
│  │  ┌──────────────────────────────────────────────────────────────────┐ │  │
│  │  │ WASM Loader         /bin/cat.wasm → /bin/grep.wasm → /bin/wc.wasm│ │  │
│  │  └─────────────────────────────────────┬────────────────────────────┘ │  │
│  │                                        │                              │  │
│  │                                        ▼                              │  │
│  │  ┌──────────────────────────────────────────────────────────────────┐ │  │
│  │  │                            Kernel                                 │ │  │
│  │  │  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌────────────┐  │ │  │
│  │  │  │ Syscalls   │  │ Processes  │  │   VFS      │  │  Memory    │  │ │  │
│  │  │  │ (300+)     │  │ + Signals  │  │ MemoryFs   │  │  COW/mmap  │  │ │  │
│  │  │  └────────────┘  └────────────┘  └────────────┘  └────────────┘  │ │  │
│  │  │  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌────────────┐  │ │  │
│  │  │  │ Executor   │  │    IPC     │  │  Timers    │  │   Users    │  │ │  │
│  │  │  │ Async/WS   │  │ pipe/shm   │  │  Async     │  │ + Caps     │  │ │  │
│  │  │  └────────────┘  └────────────┘  └────────────┘  └────────────┘  │ │  │
│  │  └──────────────────────────────────────────────────────────────────┘ │  │
│  │                                        │                              │  │
│  │                                        ▼                              │  │
│  │  ┌──────────────────────────────────────────────────────────────────┐ │  │
│  │  │                    Compositor (WebGPU) / Terminal                 │ │  │
│  │  └──────────────────────────────────────────────────────────────────┘ │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Design Principles

| Principle | Description |
|-----------|-------------|
| **Tractable** | Entire system comprehensible by one person |
| **Immediate** | Changes take effect instantly |
| **Personal** | Your computing environment, under your control |

## Statistics

| Metric | Value |
|--------|-------|
| Lines of Rust | ~62,000 |
| Tests | ~1,000+ |
| Shell commands | 80+ |
| Syscalls | 300+ |

## Documentation

### Kernel

| Document | Description |
|----------|-------------|
| [Overview](kernel/overview.md) | High-level architecture |
| [Syscalls](kernel/syscalls.md) | System call API reference |
| [Processes](kernel/processes.md) | Process model, states, sessions |
| [Memory](kernel/memory.md) | Allocation, COW, mmap |
| [Executor](kernel/executor.md) | Async task execution |
| [Work Stealing](kernel/work-stealing.md) | Parallel scheduler |
| [Users & Groups](kernel/users.md) | Multi-user system, capabilities |
| [Signals](kernel/signals.md) | Signal system |
| [IPC](kernel/ipc.md) | Inter-process communication |
| [Timers](kernel/timers.md) | Timer scheduling |
| [Objects](kernel/objects.md) | Kernel object reference counting |
| [WASM Modules](kernel/wasm-modules.md) | Command format and ABI |
| [Tracing](kernel/tracing.md) | Instrumentation |

### Userspace

| Document | Description |
|----------|-------------|
| [Shell](userspace/shell.md) | Command-line interpreter |
| [VFS](userspace/vfs.md) | Virtual filesystem |
| [Layered FS](userspace/layered-fs.md) | Union filesystem |
| [Standard I/O](userspace/stdio.md) | Console and pipes |

### Development

| Document | Description |
|----------|-------------|
| [Building](development/building.md) | Build instructions |
| [Testing](development/testing.md) | Test suite |
| [Contributing](development/contributing.md) | Development guidelines |
| [Invariants](development/invariants.md) | System invariants |

### Guides

| Document | Description |
|----------|-------------|
| [Custom Commands](guides/custom-commands.md) | Writing shell commands |
| [VFS Backends](guides/vfs-backends.md) | Implementing filesystems |
| [Adding Syscalls](guides/adding-syscalls.md) | Extending the kernel |

### Architecture

| Document | Description |
|----------|-------------|
| [Decision Records](decisions/README.md) | Architecture decisions (ADRs) |
| [Bare Metal](plans/bare-metal-boot.md) | Future: x86_64 port |
| [Compositor](plans/compositor.md) | WebGPU window management |

## Quick Start

```bash
# Build
wasm-pack build --target web

# Run
cargo run --bin serve
# Open http://localhost:8080

# Test
cargo test
```

## Command Execution Flow

```
User Input: "cat file.txt | grep hello"
     │
     ▼
┌─────────────────────────────────────────────────────────────────────┐
│ Parser                                                               │
│  ├─ Tokenize: ["cat", "file.txt", "|", "grep", "hello"]             │
│  └─ Build: Pipeline { commands: [SimpleCommand, SimpleCommand] }     │
└─────────────────────────────────────────────────────────────────────┘
     │
     ▼
┌─────────────────────────────────────────────────────────────────────┐
│ Executor                                                             │
│  ├─ Create pipe for stdout → stdin                                   │
│  ├─ Spawn process for "cat" with redirected stdout                   │
│  └─ Spawn process for "grep" with redirected stdin                   │
└─────────────────────────────────────────────────────────────────────┘
     │
     ▼
┌─────────────────────────────────────────────────────────────────────┐
│ Kernel                                                               │
│  ├─ Process 1: cat reads /file.txt via VFS, writes to pipe          │
│  └─ Process 2: grep reads from pipe, writes matches to terminal     │
└─────────────────────────────────────────────────────────────────────┘
```

## Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| Monolithic kernel | WASM lacks MMU/privilege levels |
| Async cooperative | Browser event loop integration |
| WASM commands | Isolation, extensibility |
| Reference-counted objects | Simple lifetime management |
| Layered VFS | In-memory with OPFS persistence |

## License

MIT
