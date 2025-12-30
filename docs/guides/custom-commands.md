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

### Registration

```rust
// In BUILTINS HashMap
("mycommand", builtin_mycommand as BuiltinFn),
```

### When to Use

- Commands that modify shell state (cd, export, alias)
- Commands that need direct shell access
- Simple utilities

## Program Commands

For more complex commands organized by category.

### Structure

```rust
// src/shell/programs/mymodule.rs

use crate::shell::ProgramContext;

pub fn cmd_mytool(ctx: &mut ProgramContext) -> i32 {
    let args = ctx.args();

    // Parse arguments
    if args.len() < 2 {
        ctx.stderr("Usage: mytool <arg>\n");
        return 1;
    }

    // Access kernel
    let result = ctx.kernel(|k| {
        k.sys_read_file(&args[1])
    });

    match result {
        Ok(data) => {
            ctx.stdout(&String::from_utf8_lossy(&data));
            0
        }
        Err(e) => {
            ctx.stderr(&format!("Error: {}\n", e));
            1
        }
    }
}
```

### Registration

```rust
// src/shell/programs/mod.rs

pub fn register_programs(registry: &mut ProgramRegistry) {
    registry.register("mytool", mymodule::cmd_mytool);
}
```

### ProgramContext API

```rust
impl ProgramContext {
    // Arguments
    fn args(&self) -> &[String];

    // I/O
    fn stdout(&mut self, s: &str);
    fn stderr(&mut self, s: &str);
    fn read_stdin(&mut self) -> Vec<u8>;
    fn read_line(&mut self) -> Option<String>;

    // Kernel access
    fn kernel<F, R>(&mut self, f: F) -> R
    where F: FnOnce(&mut Kernel) -> R;

    // Environment
    fn env(&self) -> &HashMap<String, String>;
    fn cwd(&self) -> &Path;
}
```

## WASM Commands

For external, portable commands.

### ABI

Commands use axeberg's WASM ABI:

```rust
// Exported functions
#[no_mangle]
pub extern "C" fn _start() -> i32;

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
pub extern "C" fn _start() -> i32 {
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

Organize by function:

| Module | Purpose | Examples |
|--------|---------|----------|
| `fs.rs` | File operations | ls, cat, cp, mv, rm |
| `text.rs` | Text processing | grep, sed, wc, sort |
| `process.rs` | Process management | ps, kill, jobs |
| `user.rs` | User management | useradd, passwd, whoami |
| `system.rs` | System info | uname, uptime, free |
| `net.rs` | Networking | ping, curl, nc |

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mytool_basic() {
        let mut ctx = ProgramContext::test_context();
        ctx.set_args(vec!["mytool".into(), "arg1".into()]);

        let code = cmd_mytool(&mut ctx);

        assert_eq!(code, 0);
        assert!(ctx.stdout_content().contains("expected output"));
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
