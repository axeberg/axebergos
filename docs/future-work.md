# Future Work

Consolidated list of planned features and enhancements for axeberg.

## VFS

| Feature | Description | Complexity |
|---------|-------------|------------|
| Layered Filesystem | Union mount for read-only base + writable overlay | Medium |

*Source: [docs/userspace/vfs.md](userspace/vfs.md)*

## Executor

| Feature | Description | Complexity | Status |
|---------|-------------|------------|--------|
| Task cancellation | Cancel running tasks by ID | Low | ✅ Done |
| Timeouts | Automatic timeout for blocking operations | Medium | Planned |
| Work stealing | Multi-threaded executor for parallelism | High | ✅ Done |
| Task groups | Hierarchical task management | Medium | Planned |

*Source: [docs/kernel/executor.md](kernel/executor.md)*

> **Work Stealing Implemented**: Lock-free Chase-Lev deque with TLA+ verification.
> See `src/kernel/work_stealing/` and `specs/tla/WorkStealing.tla`.
>
> **Task Cancellation Implemented**: `cancel_task(task_id)` and `cancel_tasks(&[task_id])`
> methods to remove tasks from the executor. See `src/kernel/executor.rs`.

## WASM Modules

| Feature | Description | Complexity | Status |
|---------|-------------|------------|--------|
| Port commands to WASM | Convert builtin commands to standalone `.wasm` modules | Medium | Planned |
| Package manager | Install commands from external sources | High | ✅ Done |
| Package registry | Server infrastructure to host packages | High | 📝 RFD |
| WASI preview2 | Broader compatibility with WASI ecosystem | Medium | Planned |

*Source: [docs/kernel/wasm-modules.md](kernel/wasm-modules.md)*

> **Package Manager Implemented**: Full-featured WASM package manager with semantic versioning,
> dependency resolution, checksums, and CLI interface.
> See `src/kernel/pkg/` and use `pkg --help` for usage.
>
> **Package Registry RFD**: Design document for the registry server infrastructure.
> See [RFD 0001](../rfd/0001-package-registry.md) for the proposed design based on
> Cargo's sparse index protocol with OIDC trusted publishing.

## Memory

| Feature | Description | Complexity | Status |
|---------|-------------|------------|--------|
| Memory-mapped files | Map VFS files into memory regions | Medium | Planned |
| Copy-on-write | Efficient fork via COW pages | High | ✅ Done |
| Memory pools | Arena allocation for performance | Medium | Planned |
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

## IPC

| Feature | Description | Complexity | Status |
|---------|-------------|------------|--------|
| Bounded channels | Back-pressure for producers | Low | ✅ Done |
| Waker-based async | Register wakers for efficient wake-up | Medium | Planned |

*Source: [docs/kernel/ipc.md](kernel/ipc.md)*

> **Bounded Channels Implemented**: `bounded_channel(capacity)` creates channels
> with capacity limits. `try_send` returns `TrySendError::Full` when full,
> `send().await` yields until space is available.
> See `src/kernel/ipc.rs` for BoundedSender/BoundedReceiver.

## Compositor

> **Compositor Implemented**: WebGPU-based window compositor with BSP tiling layout.
> See `src/compositor/` for the implementation.

The core compositor is now implemented. These enhancements are planned:

| Feature | Description | Complexity |
|---------|-------------|------------|
| Text rendering | GPU-accelerated text via glyph atlas | Medium |
| Window decorations | Close/minimize/maximize buttons, drag, resize | Medium |
| Animations | Window open/close, layout transitions | Medium |
| Themes | User-configurable colors, light/dark mode | Low |

*Source: [docs/plans/compositor.md](plans/compositor.md)*

## Priority Recommendations

### Quick Wins (Low Complexity)
1. ~~Task cancellation~~ ✅ Done
2. ~~Bounded channels~~ ✅ Done
3. ~~OPFS persistence for memory~~ ✅ Done

### Medium Impact
1. Timeouts for executor
2. Memory-mapped files
3. Port commands to WASM
4. Waker-based async for IPC

### Major Features
1. ~~Work stealing executor~~ ✅ Done
2. ~~Copy-on-write memory~~ ✅ Done
3. ~~Package manager~~ ✅ Done
4. ~~Compositor implementation~~ ✅ Done (WebGPU)
5. Task groups for hierarchical task management
6. Text rendering for compositor
