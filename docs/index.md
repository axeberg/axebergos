# axeberg Documentation

Welcome to axeberg — a personal mini-OS written in Rust, compiled to WebAssembly, running entirely in your browser.

## What is axeberg?

axeberg is an operating system you can understand. The entire codebase is designed to be comprehensible by one person, with clean abstractions and comprehensive tests.

```
┌─────────────────────────────────────────────────────────────────┐
│                         Your Browser                             │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                      axeberg OS                          │    │
│  │  ┌─────────────────────────────────────────────────┐    │    │
│  │  │                    Shell                         │    │    │
│  │  │     $ cat file.txt | grep hello | wc -l         │    │    │
│  │  └─────────────────────────┬───────────────────────┘    │    │
│  │                            │                             │    │
│  │                            ▼                             │    │
│  │  ┌─────────────────────────────────────────────────┐    │    │
│  │  │              WASM Command Loader                 │    │    │
│  │  │   /bin/cat.wasm → /bin/grep.wasm → /bin/wc.wasm │    │    │
│  │  └─────────────────────────┬───────────────────────┘    │    │
│  │                            │                             │    │
│  │                            ▼                             │    │
│  │  ┌─────────────────────────────────────────────────┐    │    │
│  │  │                    Kernel                        │    │    │
│  │  │  ┌─────────┐  ┌─────────┐  ┌─────────────────┐  │    │    │
│  │  │  │Syscalls │  │ Process │  │    VFS          │  │    │    │
│  │  │  │Interface│  │ Manager │  │ (In-Memory FS)  │  │    │    │
│  │  │  └────┬────┘  └────┬────┘  └────────┬────────┘  │    │    │
│  │  │       └────────────┴────────────────┘            │    │    │
│  │  │                                                   │    │    │
│  │  │  ┌─────────┐ ┌─────────┐ ┌───────┐ ┌─────────┐  │    │    │
│  │  │  │Executor │ │   IPC   │ │Timers │ │ Signals │  │    │    │
│  │  │  │ (Async) │ │(Channel)│ │(Queue)│ │ (POSIX) │  │    │    │
│  │  │  └─────────┘ └─────────┘ └───────┘ └─────────┘  │    │    │
│  │  └───────────────────────────────────────────────────┘    │    │
│  │                            │                             │    │
│  │                            ▼                             │    │
│  │  ┌─────────────────────────────────────────────────┐    │    │
│  │  │              Compositor (Canvas2D)               │    │    │
│  │  │                    Terminal                       │    │    │
│  │  └───────────────────────────────────────────────────┘    │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

## Philosophy

Inspired by [Radiant Computer](https://radiant.computer/) and [Oxide's Hubris](https://hubris.oxide.computer/):

| Principle | What it means |
|-----------|---------------|
| **Tractable** | The entire system should be comprehensible by one person |
| **Immediate** | Changes take effect instantly, no waiting |
| **Personal** | Your computing environment, under your control |
| **First Principles** | Simple, elegant solutions built from the ground up |

## Documentation Sections

### Getting Started

- **[Quick Start](#quick-start)** - Build and run in 60 seconds
- **[Architecture Overview](#architecture-overview)** - How the pieces fit together

### Kernel

The kernel is the core of axeberg, managing processes, memory, and system resources.

| Document | Description |
|----------|-------------|
| [Kernel Overview](kernel/overview.md) | High-level architecture |
| [Syscall Interface](kernel/syscalls.md) | The syscall API |
| [WASM Modules](kernel/wasm-modules.md) | **Command executable format and ABI** |
| [Process Model](kernel/processes.md) | Processes, sessions, and isolation |
| [Users & Groups](kernel/users.md) | Multi-user system and permissions |
| [Memory Management](kernel/memory.md) | Allocation and shared memory |
| [Object Table](kernel/objects.md) | Kernel objects and handles |
| [Executor](kernel/executor.md) | Async task execution |
| [IPC](kernel/ipc.md) | Inter-process communication |
| [Timers](kernel/timers.md) | Timer scheduling and async sleep |
| [Signals](kernel/signals.md) | POSIX-like signal system |
| [Tracing](kernel/tracing.md) | Instrumentation and debugging |

### Userspace

User-facing components that run on top of the kernel.

| Document | Description |
|----------|-------------|
| [Shell](userspace/shell.md) | **Command-line interpreter** |
| [VFS](userspace/vfs.md) | Virtual filesystem |
| [Compositor](userspace/compositor.md) | Window management |
| [Standard I/O](userspace/stdio.md) | Console and pipes |

### Development

| Document | Description |
|----------|-------------|
| [Building](development/building.md) | Build and run instructions |
| [Testing](development/testing.md) | Test suite overview |
| [Contributing](development/contributing.md) | Development guidelines |
| [Invariants](development/invariants.md) | System invariants and their tests |

### Learning Resources

| Resource | Description |
|----------|-------------|
| [Examples & Tutorials](../examples/README.md) | Step-by-step learning guides |
| [Architecture Diagrams](../ARCHITECTURE.md) | Visual system overview |
| [Decision Records](decisions/README.md) | Why we made certain choices (ADRs) |

## Quick Start

```bash
# Clone the repository
git clone https://github.com/axeberg/axebergos.git
cd axebergos

# Build the WASM binary
wasm-pack build --target web

# Start the dev server
cargo run --bin serve

# Open in browser
open http://localhost:8080
```

You'll see a terminal where you can run commands like:

```bash
$ help                    # Show available commands
$ ls                      # List files
$ cat /etc/welcome.txt    # Read a file
$ echo "hello" > test.txt # Write to a file
$ cat test.txt | wc       # Pipe commands
```

## Architecture Overview

### How Commands Execute

When you type `cat file.txt | grep hello`:

1. **Terminal** captures your input
2. **Parser** tokenizes and builds a pipeline structure
3. **Executor** iterates through the pipeline:
   - For each command, looks up in registry or loads WASM module
   - Connects stdout → stdin between commands (pipes)
   - Applies any redirections (`>`, `<`)
4. **Programs** read from VFS, write to stdout/stderr
5. **Results** flow back to terminal for display

### Key Design Decisions

| Decision | Why |
|----------|-----|
| **Monolithic kernel** | WASM has no MMU/privilege levels; microkernel IPC overhead not worth it |
| **Async cooperative** | Browser event loop drives everything; tasks yield voluntarily |
| **WASM command modules** | Isolation, extensibility, polyglot support |
| **Reference-counted objects** | Simple lifetime management, works with Rust ownership |
| **In-memory VFS** | Fast, simple; OPFS persistence planned for later |

### Test Coverage

The kernel has 674 tests covering:

- Executor scheduling and priorities
- Process creation and lifecycle
- Memory allocation and limits
- Shared memory operations
- File operations via VFS
- WASM module validation and loading
- Shell parsing and execution
- IPC channels
- Object reference counting

Run tests with: `cargo test`

## Project Structure

```
axebergos/
├── src/
│   ├── boot.rs              # System initialization
│   ├── runtime.rs           # Event loop integration
│   ├── kernel/
│   │   ├── mod.rs           # Kernel entry points
│   │   ├── syscall.rs       # Syscall implementations
│   │   ├── process.rs       # Process management
│   │   ├── memory.rs        # Memory management
│   │   ├── wasm/            # WASM module loader
│   │   │   ├── mod.rs       # ABI documentation
│   │   │   ├── abi.rs       # ABI types
│   │   │   ├── loader.rs    # Module validation/loading
│   │   │   ├── runtime.rs   # Syscall runtime
│   │   │   └── WasmLoader.tla  # TLA+ specification
│   │   └── ...
│   ├── shell/
│   │   ├── parser.rs        # Command parsing
│   │   ├── executor.rs      # Pipeline execution
│   │   ├── builtins.rs      # Built-in commands
│   │   └── terminal.rs      # Terminal emulator
│   └── vfs/
│       ├── mod.rs           # VFS traits
│       └── memory.rs        # In-memory filesystem
├── docs/                    # This documentation
├── index.html               # Browser entry point
└── Cargo.toml
```

## Formal Specifications

Critical subsystems have TLA+ specifications:

| Spec | Location | What it models |
|------|----------|----------------|
| WASM Loader | `src/kernel/wasm/WasmLoader.tla` | Command lifecycle, syscall semantics, memory safety |

## Version

**axeberg v0.1.0**

Current capabilities:
- ✅ Working shell with pipes, redirects, and job control
- ✅ 98 Unix-like commands
- ✅ Multi-user system with sessions (`login`, `logout`, `su`, `sudo`)
- ✅ Permission enforcement (Unix rwx model)
- ✅ User persistence (`/etc/passwd`, `/etc/shadow`, `/etc/group`)
- ✅ In-memory VFS with proc/sys/dev virtual filesystems
- ✅ WASM build and deployment verified
- 🚧 WASM command modules (ABI defined, execution in progress)

## License

MIT
