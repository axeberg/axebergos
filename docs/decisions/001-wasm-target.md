# ADR-001: WebAssembly as Primary Target

## Status
Accepted

## Context

We want to build a mini operating system that is:
- Accessible to anyone with a browser
- Requires no installation
- Runs in a sandboxed environment
- Demonstrates OS concepts without requiring bare metal

Traditional OS development requires:
- Bootable images
- Hardware or emulators
- Complex toolchains
- Platform-specific code

## Decision

We will target WebAssembly (wasm32-unknown-unknown) as the primary compilation target, with the browser as the runtime environment.

The OS will:
- Compile to a single .wasm file
- Run in any modern browser
- Use browser APIs for I/O (console, storage, networking)
- Be embeddable in any web page

## Consequences

### Positive

1. **Zero installation**: Users just open a URL
2. **Cross-platform**: Works on any device with a modern browser
3. **Sandboxed**: Can't damage the host system
4. **Shareable**: Easy to demo, link, embed
5. **Modern tooling**: wasm-pack, wasm-bindgen are mature
6. **Rust ecosystem**: Full access to Rust crates (with wasm support)

### Negative

1. **No true threads**: WASM has limited threading support
2. **No direct hardware**: Must use browser APIs
3. **No bare metal path**: Can't boot on real hardware (without rewrite)
4. **Performance ceiling**: Browser adds overhead
5. **Browser differences**: Subtle API variations

## Alternatives Considered

### 1. Native Binary (Linux target)
- **Pro**: Full OS capabilities, real scheduling
- **Con**: Requires VM/emulator, harder to share

### 2. UEFI Application
- **Pro**: Closer to real OS, direct hardware
- **Con**: Complex toolchain, testing requires reboot/VM

### 3. Microkernel on Embedded
- **Pro**: Real hardware, constrained environment
- **Con**: Specialized hardware, harder to access

### 4. Educational OS Framework (e.g., xv6, MINIX)
- **Pro**: Well-documented, proven for learning
- **Con**: Not Rust, less modern

## Notes

The browser environment actually provides a surprisingly good OS substrate:
- Console → Terminal I/O
- Origin Private File System → Persistent storage
- WebSocket → Networking
- Crypto API → Random numbers
- requestAnimationFrame → Timer events

This decision shapes everything else: async execution model, IPC design, filesystem abstraction.
