# ADR-005: Single WASM Binary Architecture

## Status
Accepted

## Context

Traditional operating systems separate kernel and userspace:
- Kernel: Runs in privileged mode
- Userspace: Runs in unprivileged mode, uses syscalls

In WASM, there is no hardware privilege separation. We must decide:
1. Simulate separation (separate modules, message passing)
2. Embrace single binary (everything in one module)

## Decision

We will compile everything into a **single WASM binary**. The kernel is a Rust struct, and "syscalls" are method calls.

```rust
// Kernel is just a struct
pub struct Kernel {
    // ... all state
}

// "Syscalls" are method calls
impl Kernel {
    pub fn sys_read(&mut self, fd: Fd, buf: &mut [u8]) -> Result<usize> {
        // ...
    }
}

// Shell calls kernel methods directly
let n = kernel.sys_read(fd, &mut buf)?;
```

## Consequences

### Positive

1. **Simplicity**: No IPC for syscalls, just function calls
2. **Performance**: No serialization overhead
3. **Debugging**: Single stack trace, standard tools
4. **Type safety**: Rust compiler checks kernel/shell interface
5. **Deployment**: One file to serve
6. **Tractability**: One codebase to understand

### Negative

1. **No true isolation**: Shell bug could corrupt kernel state
2. **Not realistic**: Real OS uses hardware protection
3. **Testing**: Harder to test kernel in isolation
4. **Extension**: Adding new programs means recompiling

### Mitigated

1. **Isolation**: Rust's ownership model provides some protection
2. **Realism**: We're teaching concepts, not building production OS
3. **Testing**: Unit tests work fine, integration tests test the whole
4. **Extension**: Could add WASM module loading later

## Alternatives Considered

### 1. Separate WASM modules + message passing
- **Pro**: More realistic separation
- **Con**: Complex IPC, serialization overhead, harder to debug

### 2. Web Workers for kernel
- **Pro**: True isolation, parallel execution
- **Con**: Async-only communication, complex state sharing

### 3. Microkernel in main thread, services in Workers
- **Pro**: Best of both worlds
- **Con**: Significant complexity, overkill for demo

### 4. Interpret bytecode for userspace
- **Pro**: True process isolation
- **Con**: Need to design bytecode, much more work

## Implementation Notes

The "separation" is conceptual:

```
src/
├── kernel/     # Kernel code
│   ├── syscall.rs
│   ├── process.rs
│   └── ...
├── shell/      # "Userspace" code
│   ├── executor.rs
│   └── programs/
└── lib.rs      # Entry point
```

Even though it's one binary, the code is organized as if they were separate:
- Kernel has private state
- Shell uses public kernel API
- Programs can't access kernel internals

## WASM Module Loading

We do have WASM module loading for extensibility (from `src/kernel/wasm/loader.rs`):

```rust
pub struct Loader {
    module: Option<Vec<u8>>,
}

impl Loader {
    pub fn load(&mut self, bytes: &[u8]) -> WasmResult<()> {
        ModuleValidator::validate(bytes)?;
        self.module = Some(bytes.to_vec());
        Ok(())
    }

    pub fn execute(&self, args: &[&str]) -> WasmResult<CommandResult> {
        // ...
    }
}
```

But this is for user-provided modules, not kernel/userspace split.

## Lessons Learned

1. Start simple, add complexity only when needed
2. Conceptual separation + good code organization is enough for education
3. Rust's module system provides natural boundaries
4. A tractable system is more valuable than a realistic one
