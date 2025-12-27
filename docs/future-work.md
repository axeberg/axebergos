# Future Work

Consolidated list of planned features and enhancements for axeberg.

## VFS

| Feature | Description | Complexity | Status |
|---------|-------------|------------|--------|
| Layered Filesystem | Union mount for read-only base + writable overlay | Medium | ✅ Done |

*Source: [docs/userspace/vfs.md](userspace/vfs.md)*

> **Layered Filesystem Implemented**: Union mount with read-only base layer and
> writable overlay. Features copy-on-write semantics, whiteout markers for deletions,
> merged directory listings, and opaque directory support.
> See `src/vfs/layered.rs` for LayeredFs implementation.

## Executor

| Feature | Description | Complexity | Status |
|---------|-------------|------------|--------|
| Task cancellation | Cancel running tasks by ID | Low | ✅ Done |
| Timeouts | Automatic timeout for blocking operations | Medium | ✅ Done |
| Work stealing | Multi-threaded executor for parallelism | High | ✅ Done |
| Task groups | Hierarchical task management | Medium | ✅ Done |

*Source: [docs/kernel/executor.md](kernel/executor.md)*

> **Work Stealing Implemented**: Lock-free Chase-Lev deque with TLA+ verification.
> See `src/kernel/work_stealing/` and `specs/tla/WorkStealing.tla`.
>
> **Task Cancellation Implemented**: `cancel_task(task_id)` and `cancel_tasks(&[task_id])`
> methods to remove tasks from the executor. See `src/kernel/executor.rs`.
>
> **Timeouts Implemented**: `Timeout<F>` wrapper that returns `TimeoutError` if the
> inner future exceeds the deadline. Supports custom time functions for testing.
> See `src/kernel/executor.rs` for Timeout and TimeoutError.
>
> **Task Groups Implemented**: `TaskGroupManager` provides hierarchical task management
> with parent-child relationships. Tasks can be added to groups, and entire groups
> can be cancelled at once. See `src/kernel/executor.rs` for TaskGroup and TaskGroupManager.

## WASM Modules

| Feature | Description | Complexity | Status |
|---------|-------------|------------|--------|
| Port commands to WASM | Convert builtin commands to standalone `.wasm` modules | Medium | ✅ Done |
| Package manager | Install commands from external sources | High | ✅ Done |
| Package registry | Server infrastructure to host packages | High | 📝 RFD |
| WASI Preview2 | Broader compatibility with WASI ecosystem | Medium | ✅ Done |

*Source: [docs/kernel/wasm-modules.md](kernel/wasm-modules.md)*

> **Package Manager Implemented**: Full-featured WASM package manager with semantic versioning,
> dependency resolution, checksums, and CLI interface.
> See `src/kernel/pkg/` and use `pkg --help` for usage.
>
> **Package Registry RFD**: Design document for the registry server infrastructure.
> See [RFD 0001](../rfd/0001-package-registry.md) for the proposed design based on
> Cargo's sparse index protocol with OIDC trusted publishing.
>
> **WASM Command Infrastructure**: Complete ABI v1 with extensive syscall support:
> file ops (open, close, read, write, stat, seek), directory ops (mkdir, readdir,
> rmdir, unlink, rename, copy), process control (exit, getenv, getcwd, dup).
> Commands are discovered in /bin, /usr/bin, /usr/local/bin.
> See `src/kernel/wasm/` for loader, executor, runtime, and command runner.
>
> **WASI Preview2 Implemented**: Component Model-based WASI 0.2 interfaces including:
> - `wasi:io/streams` - Input/output streams with async support
> - `wasi:io/poll` - Pollable resources for async I/O
> - `wasi:clocks/wall-clock` - Wall clock time
> - `wasi:clocks/monotonic-clock` - Monotonic time for measurements
> - `wasi:random` - Secure random number generation
> - `wasi:filesystem` - File system access with descriptors and preopens
> - `wasi:cli` - Command-line interface (args, env, exit, streams)
> See `src/kernel/wasm/wasi_preview2.rs` for the complete implementation.

## Memory

| Feature | Description | Complexity | Status |
|---------|-------------|------------|--------|
| Memory-mapped files | Map VFS files into memory regions | Medium | ✅ Done |
| Copy-on-write | Efficient fork via COW pages | High | ✅ Done |
| Memory pools | Arena allocation for performance | Medium | ✅ Done |
| OPFS persistence | Persist memory regions to disk | Low | ✅ Done |

*Source: [docs/kernel/memory.md](kernel/memory.md)*

> **Copy-on-Write Implemented**: Page-based COW with Arc reference counting.
> Fork syscall creates child processes with shared memory pages that are
> copied only when written (copy-on-write semantics).
> See `src/kernel/memory.rs` for Page, CowMemory, and cow_fork implementations.
>
> **OPFS Persistence Implemented**: Named data storage API for persisting memory
> regions to browser's Origin Private File System. Supports save, load, list,
> delete operations with full async support.
> See `src/kernel/memory_persist.rs` for MemoryPersistence API.
>
> **Memory-Mapped Files Implemented**: `MmapManager` provides file-to-memory mappings
> with support for private (COW), shared, and anonymous mappings. Tracks dirty
> pages for msync operations. See `src/kernel/memory.rs` for MmapManager and MmapRegion.
>
> **Memory Pools Implemented**: `PoolManager` provides arena-style allocation with
> O(1) alloc/free operations. Pools are sized for specific object sizes with
> free list management. See `src/kernel/memory.rs` for MemoryPool and PoolManager.

## IPC

| Feature | Description | Complexity | Status |
|---------|-------------|------------|--------|
| Bounded channels | Back-pressure for producers | Low | ✅ Done |
| Waker-based async | Register wakers for efficient wake-up | Medium | ✅ Done |

*Source: [docs/kernel/ipc.md](kernel/ipc.md)*

> **Bounded Channels Implemented**: `bounded_channel(capacity)` creates channels
> with capacity limits. `try_send` returns `TrySendError::Full` when full,
> `send().await` yields until space is available.
> See `src/kernel/ipc.rs` for BoundedSender/BoundedReceiver.
>
> **Waker-Based Async Implemented**: Both bounded and unbounded channels now register
> wakers for efficient notification. Receivers register wakers when the channel is
> empty, senders (bounded) register wakers when full. Wakers are invoked when
> data/space becomes available, avoiding busy-polling.
> See `src/kernel/ipc.rs` for RecvFuture and BoundedSendFuture/BoundedRecvFuture.

## Compositor

> **Compositor Implemented**: WebGPU-based window compositor with BSP tiling layout.
> See `src/compositor/` for the implementation.

The core compositor is now implemented. These enhancements are planned:

| Feature | Description | Complexity | Status |
|---------|-------------|------------|--------|
| Text rendering | GPU-accelerated text via glyph atlas | Medium | ✅ Done |
| Window decorations | Close/minimize/maximize buttons, drag, resize | Medium | ✅ Done |
| Animations | Window open/close, layout transitions | Medium | ✅ Done |
| Themes | User-configurable colors, light/dark mode | Low | ✅ Done |

*Source: [docs/plans/compositor.md](plans/compositor.md)*

> **Text Rendering Implemented**: GPU-accelerated text rendering with glyph atlas.
> Features include font metrics (monospace/proportional), text layout with alignment
> (left/center/right, top/middle/bottom), word/character wrapping, and positioned
> glyphs with UV coordinates for GPU rendering.
> See `src/compositor/text.rs` for TextRenderer, GlyphAtlas, and TextLayout.
>
> **Themes Implemented**: Multiple theme presets (dark, light, high-contrast, monokai, nord)
> with `Theme::by_name()` lookup and `Theme::available_themes()` discovery.
> See `src/compositor/mod.rs` for Theme implementation.
>
> **Animations Implemented**: Animation framework with easing functions (linear, ease-in,
> ease-out, ease-in-out), `Animation` type with property interpolation, and
> `WindowAnimationState` for managing window open/close animations.
> See `src/compositor/mod.rs` for animation types and presets.
>
> **Window Decorations Implemented**: `DecorationButton` enum (Close, Maximize, Minimize),
> `decoration_button_rect()` for button positioning, and `DecorationColors` for theming.
> See `src/compositor/mod.rs` for decoration support.

## Summary

### Completed ✅
| Feature | Category |
|---------|----------|
| Task cancellation | Executor |
| Timeouts | Executor |
| Work stealing executor | Executor |
| Task groups | Executor |
| Bounded channels | IPC |
| Waker-based async | IPC |
| OPFS persistence | Memory |
| Copy-on-write memory | Memory |
| Memory-mapped files | Memory |
| Memory pools | Memory |
| Layered filesystem | VFS |
| Package manager | WASM |
| WASM command infrastructure | WASM |
| WASI Preview2 | WASM |
| WebGPU compositor | Compositor |
| Text rendering | Compositor |
| Themes | Compositor |
| Animations | Compositor |
| Window decorations | Compositor |

### Remaining Work

| Category | Feature | Complexity |
|----------|---------|------------|
| **Registry** | Package registry | High (📝 RFD) |
