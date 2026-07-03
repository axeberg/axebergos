# Writing Custom Commands

Guide to implementing shell commands in axeberg.

## Overview

Commands can be implemented as:
1. **Built-in commands** - Rust functions in `shell/builtins.rs`
2. **Program commands** - Rust functions in `shell/programs/`
3. **WASM modules** - External `.wasm` files in `/bin/`

## Built-in Commands

Simplest approach for shell-integrated commands (cd, export, etc.).

### Structure

```rust
// src/shell/builtins.rs

pub fn builtin_mycommand(
    args: &[String],
    state: &mut ShellState,
    _stdin: &mut dyn BufRead,
    stdout: &mut dyn Write,
    _stderr: &mut dyn Write,
) -> i32 {
    // args[0] is command name
    // Return exit code (0 = success)

    writeln!(stdout, "Hello from mycommand!").ok();
    0
}
```

### Dispatch

Builtins are not stored in a table. They are dispatched from `builtins.rs`:
`builtins::is_builtin(name)` reports whether a name is a builtin, and a
`match` on the name runs the corresponding handler, returning a
`BuiltinResult`.

### When to Use

- Commands that modify shell state (cd, export, alias)
- Commands that need direct shell access
- Simple utilities

## Program Commands

For more complex commands organized by category.

### Structure

Programs are plain functions matching the `ProgramFn` signature
(`src/shell/executor.rs`):

```rust
pub type ProgramFn =
    fn(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32;
```

- `args` — command-line arguments (`args[0]` is the command name)
- `stdin` — standard input, already collected to a string
- `stdout` / `stderr` — buffers you append output to
- returns the exit code (0 = success)

The kernel is reached through the free functions in `crate::kernel::syscall`,
each of which wraps `KERNEL.with(|k| k.borrow_mut().sys_xxx(...))`.

```rust
// src/shell/programs/mymodule.rs

use crate::kernel::syscall;

pub fn prog_mytool(
    args: &[String],
    _stdin: &str,
    stdout: &mut String,
    stderr: &mut String,
) -> i32 {
    // args[0] is the command name
    if args.len() < 2 {
        stderr.push_str("Usage: mytool <file>\n");
        return 1;
    }

    let fd = match syscall::open(&args[1], syscall::OpenFlags::READ) {
        Ok(fd) => fd,
        Err(e) => {
            stderr.push_str(&format!("mytool: {}\n", e));
            return 1;
        }
    };

    let mut buf = [0u8; 4096];
    loop {
        match syscall::read(fd, &mut buf) {
            Ok(0) => break,
            Ok(n) => stdout.push_str(&String::from_utf8_lossy(&buf[..n])),
            Err(e) => {
                stderr.push_str(&format!("mytool: {}\n", e));
                let _ = syscall::close(fd);
                return 1;
            }
        }
    }
    let _ = syscall::close(fd);
    0
}
```

### Registration

Register the function inside `ProgramRegistry::new()` in
`src/shell/executor.rs`:

```rust
// inside ProgramRegistry::new()
reg.register("mytool", programs::prog_mytool);
```

`register` takes a `&str` name and a `ProgramFn`. There is no separate
`register_programs(registry)` entry point — every program is registered in
this one constructor.

## WASM Commands

For external, portable commands.

### ABI

Commands use axeberg's WASM ABI:

```rust
// Exported entry point (the kernel calls the export named "main";
// see MAIN in src/kernel/wasm/abi.rs)
#[no_mangle]
pub extern "C" fn main() -> i32;

// Imported syscalls
extern "C" {
    fn syscall_read(fd: i32, buf: *mut u8, len: i32) -> i32;
    fn syscall_write(fd: i32, buf: *const u8, len: i32) -> i32;
    fn syscall_open(path: *const u8, path_len: i32, flags: i32) -> i32;
    fn syscall_close(fd: i32) -> i32;
    fn syscall_exit(code: i32) -> !;
    // ... see kernel/wasm/abi.rs for full list
}
```

### Example

```rust
// my_command/src/main.rs

#[no_mangle]
pub extern "C" fn main() -> i32 {
    let args = get_args();

    if args.len() < 2 {
        write_stderr("Usage: mycommand <file>\n");
        return 1;
    }

    match read_file(&args[1]) {
        Ok(content) => {
            write_stdout(&content);
            0
        }
        Err(e) => {
            write_stderr(&format!("Error: {}\n", e));
            1
        }
    }
}
```

### Building

```bash
# Build as WASM
cargo build --target wasm32-unknown-unknown --release

# Copy to /bin
cp target/wasm32-unknown-unknown/release/mycommand.wasm /bin/
```

### WASI Support

axeberg supports WASI Preview2 for compatibility:

```rust
// Use standard WASI imports
use wasi::*;

fn main() {
    // Standard Rust main works with WASI
    let args: Vec<String> = std::env::args().collect();
    // ...
}
```

## Command Categories

Organize by function. The command names below are illustrative groupings,
not a guarantee that every one is implemented — check `src/shell/programs/`
for the current set (and `ProgramRegistry::new()` for what is registered):

| Module | Purpose | Example commands |
|--------|---------|----------|
| `fs.rs` | File operations | ls, cat, cp, mv, rm |
| `text.rs` | Text processing | grep, wc, sort |
| `process.rs` | Process management | ps, kill, jobs |
| `user.rs` | User management | useradd, passwd, whoami |
| `system.rs` | System info | uname, uptime, free |

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mytool_basic() {
        let args = vec!["mytool".to_string(), "/etc/hostname".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let code = prog_mytool(&args, "", &mut stdout, &mut stderr);

        assert_eq!(code, 0);
        assert!(stdout.contains("expected output"));
    }
}
```

## Best Practices

1. **Exit codes**: 0 for success, non-zero for errors
2. **Error messages**: Write to stderr, not stdout
3. **Help text**: Support `-h` and `--help` flags
4. **Streaming**: Process input line-by-line when possible
5. **Signals**: Handle SIGINT/SIGPIPE gracefully

## Related Documentation

- [Shell](../userspace/shell.md) - Shell architecture
- [WASM Modules](../kernel/wasm-modules.md) - WASM ABI details
- [Syscalls](../kernel/syscalls.md) - Available syscalls
