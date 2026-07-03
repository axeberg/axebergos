# Adding Syscalls

Guide to extending the kernel with new system calls.

## Overview

Syscalls are the interface between userspace and kernel. Adding a new syscall involves:

1. Define the syscall number (optional, for tracing/ABI)
2. Implement the handler as a `sys_*` method on `Kernel`
3. Add a public wrapper function
4. Add WASM ABI bindings (if needed)
5. Test

There is no central `syscall(nr, args)` dispatch table and no `SyscallArg`
enum. Each syscall is just a method plus a thin wrapper.

## Step 1: Define Syscall Number

```rust
// src/kernel/syscall.rs

syscall_names! {
    // ... existing syscalls ...

    // Add your new syscall
    MyNewSyscall = 400,
}
```

The `syscall_names!` macro only generates the syscall-number enum and a
name lookup (used by tracing and the WASM ABI). It does **not** generate any
dispatch logic — calls reach the kernel directly through the wrapper in
Step 3. This step is optional and only needed if the syscall should appear
in traces or be exposed to WASM modules.

## Step 2: Implement Handler

```rust
// src/kernel/syscall.rs

impl Kernel {
    /// My new syscall - does something useful
    ///
    /// # Arguments
    /// * `arg1` - First argument description
    /// * `arg2` - Second argument description
    ///
    /// # Returns
    /// * `Ok(result)` - On success
    /// * `Err(SyscallError)` - On failure
    pub fn sys_my_new_syscall(
        &mut self,
        arg1: i32,
        arg2: &str,
    ) -> SyscallResult<i32> {
        // Get current process
        let process = self.get_current_process()?;

        // Validate arguments
        if arg1 < 0 {
            return Err(SyscallError::InvalidArgument);
        }

        // Permission check if needed
        if !process.capabilities.has(Capability::SysAdmin) {
            return Err(SyscallError::PermissionDenied);
        }

        // Implement logic
        let result = self.do_something(arg1, arg2)?;

        Ok(result)
    }
}
```

## Step 3: Add a Public Wrapper

Userspace code (shell programs, etc.) does not call `sys_*` methods
directly. Instead, each syscall has a free function that borrows the
thread-local kernel and forwards the call. Add yours next to the others in
`src/kernel/syscall.rs`:

```rust
// src/kernel/syscall.rs (free functions, outside the `impl Kernel` block)

/// My new syscall - does something useful.
pub fn my_new_syscall(arg1: i32, arg2: &str) -> SyscallResult<i32> {
    KERNEL.with(|k| k.borrow_mut().sys_my_new_syscall(arg1, arg2))
}
```

That wrapper is the entire "registration" — there is no match arm to update
and no argument-boxing enum to thread the call through.

## Step 4: WASM ABI Bindings

If the syscall needs to be callable from WASM modules:

```rust
// src/kernel/wasm/runtime.rs

impl WasmRuntime {
    fn syscall_my_new_syscall(
        &mut self,
        arg1: i32,
        arg2_ptr: i32,
        arg2_len: i32,
    ) -> i32 {
        let arg2 = self.read_string(arg2_ptr, arg2_len);

        match self.kernel.sys_my_new_syscall(arg1, &arg2) {
            Ok(result) => result,
            Err(e) => e.to_errno(),
        }
    }
}

// Register in WASM imports
fn register_imports(linker: &mut Linker) {
    linker.func_wrap("env", "syscall_my_new_syscall",
        |caller: Caller, arg1: i32, arg2_ptr: i32, arg2_len: i32| -> i32 {
            // ...
        }
    );
}
```

## Step 5: Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_new_syscall_basic() {
        let mut kernel = Kernel::new();
        kernel.init_for_test();

        let result = kernel.sys_my_new_syscall(42, "test");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected_value);
    }

    #[test]
    fn test_my_new_syscall_permission_denied() {
        let mut kernel = Kernel::new();
        kernel.init_for_test();

        // Switch to unprivileged user
        kernel.with_current_process(|p| {
            p.euid = Uid(1000);
            p.capabilities = ProcessCapabilities::empty();
        });

        let result = kernel.sys_my_new_syscall(42, "test");
        assert_eq!(result, Err(SyscallError::PermissionDenied));
    }
}
```

## Argument Types

Syscall methods take ordinary Rust types directly — `i32`, `&str`,
`&[u8]`, `Fd`, etc. There is no `SyscallArg` enum boxing arguments; the
method signature is the contract.

## Error Types

Syscalls return `SyscallResult<T>` (`Result<T, SyscallError>`). The full set
of error variants (`src/kernel/syscall.rs`):

```rust
pub enum SyscallError {
    BadFd,               // Invalid file descriptor
    NotFound,            // File or path not found
    PermissionDenied,    // Permission denied
    InvalidArgument,     // Invalid argument
    WouldBlock,          // Would block (non-blocking I/O)
    BrokenPipe,          // Pipe/connection closed
    Busy,                // Resource busy
    InvalidData,         // Invalid data (e.g. invalid UTF-8)
    NoProcess,           // No current process
    Io(String),          // Generic I/O error
    Memory(MemoryError), // Memory error
    Signal(SignalError), // Signal error
    Interrupted,         // Interrupted by signal
    NotADirectory,       // Not a directory
    IsADirectory,        // Is a directory
    AlreadyExists,       // Already exists
    TooManyOpenFiles,    // FD limit reached (EMFILE)
    TooBig,              // Value too big for type (E2BIG/EFBIG)
}
```

`SyscallError` provides a `to_errno()` mapping to negative errno values for
the WASM ABI.

## Common Patterns

### Resource Access

```rust
pub fn sys_resource_op(&mut self, id: u32) -> SyscallResult<()> {
    // Get current process
    let process = self.get_current_process()?;

    // Get resource, check ownership
    let resource = self.resources.get_mut(id)
        .ok_or(SyscallError::NotFound)?;

    if resource.owner != process.uid && process.euid != Uid(0) {
        return Err(SyscallError::PermissionDenied);
    }

    // Operate on resource
    Ok(())
}
```

### Capability Check

```rust
pub fn sys_privileged_op(&mut self) -> SyscallResult<()> {
    let process = self.get_current_process()?;

    if !process.capabilities.has(Capability::SysAdmin) {
        return Err(SyscallError::PermissionDenied);
    }

    // Do privileged operation
    Ok(())
}
```

### Path Resolution

```rust
pub fn sys_path_op(&mut self, path: &str) -> SyscallResult<()> {
    let process = self.get_current_process()?;

    // Resolve relative to cwd, respecting jail
    let resolved = self.resolve_path(path)?;

    // Check traversal permissions
    self.check_path_traversal(&resolved)?;

    // Operate on path
    Ok(())
}
```

## Documentation

Add to syscalls.md:

````markdown
### my_new_syscall

Does something useful.

**Signature**: `my_new_syscall(arg1: i32, arg2: *const u8, arg2_len: i32) -> i32`

**Arguments**:
- `arg1`: First argument description
- `arg2`: Pointer to string data
- `arg2_len`: Length of string

**Returns**:
- On success: result value
- On error: negative errno

**Errors**:
- `EINVAL`: Invalid argument
- `EPERM`: Permission denied

**Example**:
```rust
let result = syscall_my_new_syscall(42, "test".as_ptr(), 4);
```
````

## Related Documentation

- [Syscalls](../kernel/syscalls.md) - Syscall reference
- [Overview](../kernel/overview.md) - Kernel architecture
- [WASM Modules](../kernel/wasm-modules.md) - WASM ABI
