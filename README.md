# axeberg

A personal mini-OS written in Rust, compiled to WebAssembly, running entirely in your browser.

## Overview

axeberg is an operating system you can understand. The entire codebase is designed to be comprehensible by one person, with clean abstractions and comprehensive tests.

**Key Features:**
- Unix-like shell with pipes, redirects, and job control
- Multi-user system with `/etc/passwd` persistence
- Session management (login, logout, su, sudo)
- In-memory VFS with proc/sys/dev virtual filesystems
- 50+ built-in commands
- WASM module loader for extensibility

## Quick Start

```bash
# Build and run
wasm-pack build --target web
cargo run --bin serve

# Open http://localhost:8080
```

## Documentation

| Section | Description |
|---------|-------------|
| [Documentation Index](docs/index.md) | Full documentation |
| [Kernel Overview](docs/kernel/overview.md) | Architecture and components |
| [Syscall Reference](docs/kernel/syscalls.md) | System call API |
| [Shell Guide](docs/userspace/shell.md) | Command-line usage |
| [Building](docs/development/building.md) | Build instructions |

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                      Browser                             │
│  ┌───────────────────────────────────────────────────┐  │
│  │                   axeberg OS                       │  │
│  │  ┌─────────────────────────────────────────────┐  │  │
│  │  │  Shell: cat file | grep pattern | wc -l     │  │  │
│  │  └──────────────────────┬──────────────────────┘  │  │
│  │                         ▼                          │  │
│  │  ┌─────────────────────────────────────────────┐  │  │
│  │  │                  Kernel                      │  │  │
│  │  │  • Process Manager   • Memory Manager        │  │  │
│  │  │  • VFS (MemoryFs)    • User/Group DB        │  │  │
│  │  │  • IPC (pipes/shm)   • TTY/Session Mgmt     │  │  │
│  │  │  • Signals           • Timers               │  │  │
│  │  └──────────────────────┬──────────────────────┘  │  │
│  │                         ▼                          │  │
│  │  ┌─────────────────────────────────────────────┐  │  │
│  │  │         Compositor (Canvas2D/Terminal)       │  │  │
│  │  └─────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

## Multi-User System

Linux-like user management with proper session isolation:

```bash
$ login root root          # Spawns new session as root
$ whoami                   # root
$ useradd alice            # Create user
$ passwd alice secret      # Set password
$ cat /etc/passwd          # View user database
$ logout                   # End session
```

Users persist to `/etc/passwd`, `/etc/shadow`, `/etc/group`.

## Deployment

### Prerequisites

```bash
# Install wasm-pack (required for all deployment methods)
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
```

### Build

```bash
# Build WASM binary
wasm-pack build --target web

# This generates:
# - pkg/axeberg.js      (JavaScript bindings)
# - pkg/axeberg_bg.wasm (WebAssembly binary)
```

### Option 1: Local Development

```bash
cargo run --bin serve
# Open http://localhost:8080
```

### Option 2: Static Hosting

Deploy to any static host (GitHub Pages, Netlify, Vercel, S3, etc.):

```
Required files:
├── index.html
└── pkg/
    ├── axeberg.js
    └── axeberg_bg.wasm
```

### Option 3: Docker

```dockerfile
FROM rust:latest as builder
RUN curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
WORKDIR /app
COPY . .
RUN wasm-pack build --target web --release

FROM nginx:alpine
COPY --from=builder /app/index.html /usr/share/nginx/html/
COPY --from=builder /app/pkg /usr/share/nginx/html/pkg/
EXPOSE 80
```

### Option 4: Embed in Other Apps

```javascript
import init from './pkg/axeberg.js';
await init();  // Terminal auto-attaches to DOM
```


## Project Structure

```
axebergos/
├── src/
│   ├── kernel/           # Core OS kernel
│   │   ├── syscall.rs    # System calls
│   │   ├── process.rs    # Process management
│   │   ├── users.rs      # User/group database
│   │   └── ...
│   ├── shell/            # Command interpreter
│   │   ├── executor.rs   # Command execution
│   │   └── parser.rs     # Shell parsing
│   ├── vfs/              # Virtual filesystem
│   └── compositor/       # Display/terminal
├── docs/                 # Documentation
├── tests/                # Integration tests
└── index.html            # Browser entry
```

## Testing

```bash
cargo test              # All tests
cargo test --lib        # Unit tests only
cargo test integration  # Integration tests
```

## License

MIT

## Links

- [Full Documentation](docs/index.md)
- [Architecture](docs/kernel/overview.md)
- [Shell Commands](docs/userspace/shell.md)
