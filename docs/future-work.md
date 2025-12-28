# Future Work

Consolidated list of planned features and remaining work for axeberg.

## Remaining Work

| Category | Feature | Description | Complexity | Status |
|----------|---------|-------------|------------|--------|
| Registry | Package registry | Server infrastructure for hosting WASM packages | High | 📝 [RFD](../rfd/0001-package-registry.md) |

The package registry design is documented in RFD 0001, proposing Cargo's sparse index
protocol with OIDC trusted publishing.

---

## Completed Features

All other planned features have been implemented. See below for details.

### Executor

| Feature | Source |
|---------|--------|
| Task cancellation | `src/kernel/executor.rs` |
| Timeouts | `src/kernel/executor.rs` |
| Work stealing | `src/kernel/work_stealing/` |
| Task groups | `src/kernel/executor.rs` |

### IPC

| Feature | Source |
|---------|--------|
| Bounded channels | `src/kernel/ipc.rs` |
| Waker-based async | `src/kernel/ipc.rs` |

### Memory

| Feature | Source |
|---------|--------|
| Memory-mapped files | `src/kernel/memory.rs` |
| Copy-on-write | `src/kernel/memory.rs` |
| Memory pools | `src/kernel/memory.rs` |
| OPFS persistence | `src/kernel/memory_persist.rs` |

### VFS

| Feature | Source |
|---------|--------|
| Layered filesystem | `src/vfs/layered.rs` |

### WASM

| Feature | Source |
|---------|--------|
| Command ABI v1 | `src/kernel/wasm/abi.rs` |
| WASM loader/executor | `src/kernel/wasm/` |
| Package manager | `src/kernel/pkg/` |
| WASI Preview2 | `src/kernel/wasm/wasi_preview2.rs` |

### Compositor

| Feature | Source |
|---------|--------|
| WebGPU rendering | `src/compositor/surface.rs` |
| BSP tiling layout | `src/compositor/layout.rs` |
| Text rendering | `src/compositor/text.rs` |
| Themes | `src/compositor/mod.rs` |
| Animations | `src/compositor/mod.rs` |
| Window decorations | `src/compositor/mod.rs` |

---

## Implementation Notes

For detailed documentation on each subsystem, see:

- [Executor](kernel/executor.md)
- [IPC](kernel/ipc.md)
- [Memory](kernel/memory.md)
- [VFS](userspace/vfs.md)
- [WASM Modules](kernel/wasm-modules.md)
- [Compositor](plans/compositor.md)
