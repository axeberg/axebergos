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

## Deployment Options

### 1. Local Development Server

```bash
cargo run --bin serve
# Open http://localhost:8080
```

### 2. Static Web Hosting

Build and deploy to any static host (GitHub Pages, Netlify, Vercel, S3):

```bash
# Build WASM
wasm-pack build --target web --release

# Files to deploy:
# - index.html
# - pkg/axeberg.js
# - pkg/axeberg_bg.wasm
```

**GitHub Pages:**
```bash
# In your repo settings, enable Pages from main branch
git add index.html pkg/
git commit -m "Deploy axeberg"
git push
```

**Netlify/Vercel:**
```bash
# Just connect your repo - auto-deploys on push
# Set build command: wasm-pack build --target web
# Publish directory: .
```

### 3. Docker Container

```dockerfile
FROM rust:latest as builder
RUN cargo install wasm-pack
WORKDIR /app
COPY . .
RUN wasm-pack build --target web --release

FROM nginx:alpine
COPY --from=builder /app/index.html /usr/share/nginx/html/
COPY --from=builder /app/pkg /usr/share/nginx/html/pkg/
EXPOSE 80
```

```bash
docker build -t axeberg .
docker run -p 8080:80 axeberg
```

### 4. Embedded in Other Apps

Import as ES module:

```javascript
import init, { boot } from './pkg/axeberg.js';

async function startOS() {
    await init();
    const terminal = document.getElementById('terminal');
    boot(terminal);
}
```

### 5. Electron Desktop App

```javascript
// main.js
const { app, BrowserWindow } = require('electron');

app.whenReady().then(() => {
    const win = new BrowserWindow({ width: 800, height: 600 });
    win.loadFile('index.html');
});
```

### 6. PWA (Progressive Web App)

Add to `index.html`:
```html
<link rel="manifest" href="manifest.json">
<script>
if ('serviceWorker' in navigator) {
    navigator.serviceWorker.register('sw.js');
}
</script>
```

With `manifest.json`:
```json
{
    "name": "axeberg",
    "display": "standalone",
    "start_url": "/",
    "icons": [{"src": "icon.png", "sizes": "512x512"}]
}
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
