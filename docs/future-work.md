# Future Work

Consolidated list of planned features and enhancements for axeberg.

## VFS

| Feature | Description | Complexity |
|---------|-------------|------------|
| Layered Filesystem | Union mount for read-only base + writable overlay | Medium |

*Source: [docs/userspace/vfs.md](userspace/vfs.md)*

## Executor

| Feature | Description | Complexity |
|---------|-------------|------------|
| Task cancellation | Cancel running tasks by ID | Low |
| Timeouts | Automatic timeout for blocking operations | Medium |
| Work stealing | Multi-threaded executor for parallelism | High |
| Task groups | Hierarchical task management | Medium |

*Source: [docs/kernel/executor.md](kernel/executor.md)*

## WASM Modules

| Feature | Description | Complexity |
|---------|-------------|------------|
| Port commands to WASM | Convert builtin commands to standalone `.wasm` modules | Medium |
| Package manager | Install commands from external sources | High |
| WASI preview2 | Broader compatibility with WASI ecosystem | Medium |

*Source: [docs/kernel/wasm-modules.md](kernel/wasm-modules.md)*

## Memory

| Feature | Description | Complexity |
|---------|-------------|------------|
| Memory-mapped files | Map VFS files into memory regions | Medium |
| Copy-on-write | Efficient fork via COW pages | High |
| Memory pools | Arena allocation for performance | Medium |
| OPFS persistence | Persist memory regions to disk | Low |

*Source: [docs/kernel/memory.md](kernel/memory.md)*

## IPC

| Feature | Description | Complexity |
|---------|-------------|------------|
| Bounded channels | Back-pressure for producers | Low |
| Waker-based async | Register wakers for efficient wake-up | Medium |

*Source: [docs/kernel/ipc.md](kernel/ipc.md)*

## Compositor (Planned Feature)

The compositor itself is not yet implemented. Once implemented, these enhancements are planned:

| Feature | Description | Complexity |
|---------|-------------|------------|
| WebGPU backend | Higher performance rendering | High |
| Window decorations | Close/minimize/maximize buttons, drag, resize | Medium |
| Animations | Window open/close, layout transitions | Medium |
| Themes | User-configurable colors, light/dark mode | Low |

*Source: [docs/plans/compositor.md](plans/compositor.md)*

## Priority Recommendations

### Quick Wins (Low Complexity)
1. Task cancellation
2. Bounded channels
3. OPFS persistence for memory

### Medium Impact
1. Timeouts for executor
2. Memory-mapped files
3. Port commands to WASM

### Major Features
1. Work stealing executor
2. Copy-on-write memory
3. Package manager
4. Compositor implementation
