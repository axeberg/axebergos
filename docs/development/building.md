# Building axeberg

Instructions for building and running axeberg.

## Prerequisites

- **Rust**: Latest stable (1.80+)
- **wasm-pack**: For building WASM
- **Modern browser**: Chrome, Firefox, Safari, or Edge

### Installing Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

### Installing wasm-pack

```bash
cargo install wasm-pack
```

## Project Structure

```
axebergos/
├── src/
│   ├── lib.rs          # Library entry point
│   ├── boot.rs         # Boot sequence
│   ├── kernel/         # Kernel components
│   │   ├── mod.rs
│   │   ├── executor.rs # Task scheduler
│   │   ├── process.rs  # Process abstraction
│   │   ├── memory.rs   # Memory management
│   │   ├── syscall.rs  # Syscall interface
│   │   ├── object.rs   # Kernel objects
│   │   ├── ipc.rs      # Inter-process communication
│   │   ├── events.rs   # Event handling
│   │   └── task.rs     # Task abstraction
│   ├── vfs/            # Virtual filesystem
│   │   ├── mod.rs
│   │   └── memory.rs   # In-memory backend
│   └── bin/
│       └── serve.rs    # Dev server
├── index.html          # HTML shell (project root)
├── docs/               # Documentation
├── Cargo.toml
└── README.md
```

## Building for Web

### Debug Build

```bash
wasm-pack build --target web
```

Output in `pkg/`:
- `axeberg.js` - JavaScript glue
- `axeberg_bg.wasm` - WebAssembly binary
- `axeberg.d.ts` - TypeScript definitions

### Release Build

```bash
wasm-pack build --target web --release
```

Smaller, faster WASM binary.

## Running the Dev Server

axeberg includes a built-in dev server:

```bash
cargo run --bin serve
```

Then open: http://localhost:8080

The server:
- Serves static files from project root
- Serves WASM from `pkg/`
- Supports hot reload (rebuild and refresh)

## Development Workflow

1. Make changes to Rust code
2. Rebuild WASM: `wasm-pack build --target web`
3. Refresh browser

Or use watch mode:

```bash
# Terminal 1: Watch and rebuild
cargo watch -s "wasm-pack build --target web"

# Terminal 2: Serve
cargo run --bin serve
```

## Running Tests

### Unit Tests

```bash
cargo test
```

Currently runs 99 tests covering:
- Executor scheduling
- Process management
- Memory allocation
- File operations
- IPC channels
- Object lifecycle

### Running Specific Tests

```bash
# Run tests matching a pattern
cargo test syscall

# Run a specific test
cargo test test_mem_alloc_free

# Show output
cargo test -- --nocapture
```

### Test Coverage

```bash
cargo install cargo-tarpaulin  # Optional tool
cargo tarpaulin
```

**Note**: cargo-watch and cargo-tarpaulin are optional development tools that can enhance the development workflow but are not required for building or testing the project.

## Code Quality

### Formatting

```bash
cargo fmt
```

### Linting

```bash
cargo clippy
```

### Documentation

```bash
cargo doc --open
```

## Troubleshooting

### WASM build fails

Ensure wasm-pack is installed:
```bash
cargo install wasm-pack
```

### "Can't find wasm file"

Make sure you ran wasm-pack build:
```bash
wasm-pack build --target web
```

### Canvas not found

Check that index.html has the canvas element:
```html
<canvas id="canvas"></canvas>
```

### Console errors

Open browser DevTools (F12) to see JavaScript errors.

## Configuration

### Cargo.toml

Key settings:

```toml
[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2"
web-sys = { version = "0.3", features = [...] }
js-sys = "0.3"
futures = "0.3"
wasm-bindgen-futures = "0.4"
```

### web-sys Features

Required features for browser APIs:

```toml
[dependencies.web-sys]
version = "0.3"
features = [
    "Window",
    "Document",
    "HtmlCanvasElement",
    "CanvasRenderingContext2d",
    "MouseEvent",
    "KeyboardEvent",
    # ...
]
```

## Next Steps

- [Testing](testing.md) - Test suite details
- [Contributing](contributing.md) - Development guidelines
