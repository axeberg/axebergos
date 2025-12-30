# Adding Syscalls

Guide to extending the kernel with new system calls.

## Overview

Syscalls are the interface between userspace and kernel. Adding a new syscall involves:

1. Define syscall number
2. Implement handler function
3. Register in dispatch
4. Add WASM ABI bindings (if needed)
5. Test

## Step 1: Define Syscall Number

```rust
// src/kernel/syscall.rs

syscall_names! {
    // ... existing syscalls ...

    // Add your new syscall
    MyNewSyscall = 400,
}
```

The `syscall_names!` macro generates both the enum variant and name lookup.

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

## Step 3: Register in Dispatch

```rust
// src/kernel/syscall.rs, in syscall() method

pub fn syscall(&mut self, nr: SyscallNr, args: &[SyscallArg]) -> SyscallResult<SyscallArg> {
    match nr {
        // ... existing syscalls ...

        SyscallNr::MyNewSyscall => {
            let arg1 = args.get(0).map(|a| a.as_i32()).unwrap_or(0);
            let arg2 = args.get(1).map(|a| a.as_str()).unwrap_or("");
            self.sys_my_new_syscall(arg1, arg2).map(SyscallArg::Int)
        }
    }
}
```

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

## Syscall Argument Types

```rust
pub enum SyscallArg {
    Int(i32),
    Long(i64),
    Ptr(u32),
    Str(String),
    Bytes(Vec<u8>),
}

impl SyscallArg {
    pub fn as_i32(&self) -> i32;
    pub fn as_i64(&self) -> i64;
    pub fn as_u32(&self) -> u32;
    pub fn as_str(&self) -> &str;
    pub fn as_bytes(&self) -> &[u8];
}
```

## Error Types

```rust
pub enum SyscallError {
    BadFd,              // Invalid file descriptor
    NotFound,           // Path/resource not found
    PermissionDenied,   // Access denied
    InvalidArgument,    // Bad argument value
    WouldBlock,         // Non-blocking would block
    BrokenPipe,         // Pipe has no readers
    TooManyOpenFiles,   // FD limit reached
    NoProcess,          // No current process
    TooBig,             // Size overflow
    Busy,               // Resource busy
    Io(String),         // Generic I/O error
}

impl SyscallError {
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::BadFd => -9,          // EBADF
            Self::NotFound => -2,       // ENOENT
            Self::PermissionDenied => -13, // EACCES
            Self::InvalidArgument => -22,  // EINVAL
            Self::WouldBlock => -11,    // EAGAIN
            // ...
        }
    }
}
```

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

```markdown
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
```

## Related Documentation

- [Syscalls](../kernel/syscalls.md) - Syscall reference
- [Overview](../kernel/overview.md) - Kernel architecture
- [WASM Modules](../kernel/wasm-modules.md) - WASM ABI
