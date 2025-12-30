# WASM Command Modules

axeberg uses WebAssembly modules as the executable format for user commands. Instead of hardcoding commands in the shell, each command (like `cat`, `ls`, `grep`) is a standalone WASM module loaded from the filesystem.

## Why WASM Modules?

| Approach | Pros | Cons |
|----------|------|------|
| **Hardcoded functions** | Simple, fast | Unmaintainable, no user extensions |
| **JavaScript eval** | Flexible | Security nightmare, no sandboxing |
| **WASM modules** | Isolated, extensible, polyglot | More complex infrastructure |

We chose WASM modules because:

1. **Isolation**: Each command runs in its own WASM sandbox
2. **Extensibility**: Users can add commands by dropping `.wasm` files in `/bin`
3. **Polyglot**: Write commands in Rust, C, Zig, or any language that compiles to WASM
4. **Security**: WASM's capability model limits what commands can access

## ABI Specification (v1)

The Command ABI defines the contract between the kernel and WASM command modules.

### Required Exports

Every command module MUST export:

| Export   | Type                              | Description |
|----------|-----------------------------------|-------------|
| `memory` | Memory                            | Linear memory for data exchange |
| `main`   | `(argc: i32, argv: i32) -> i32`   | Entry point, returns exit code |

Optional exports:

| Export         | Type   | Description |
|----------------|--------|-------------|
| `__heap_base`  | Global | Start of heap for dynamic allocation |

### Syscall Imports

Commands import syscalls from the `env` namespace:

#### File Operations

```
open(path_ptr: i32, path_len: i32, flags: i32) -> i32
  Opens a file. Returns fd >= 0 on success, < 0 on error.
  Flags: 0 = read, 1 = write, 2 = read+write, 4 = create, 8 = truncate

close(fd: i32) -> i32
  Closes a file descriptor. Returns 0 on success, < 0 on error.

read(fd: i32, buf_ptr: i32, len: i32) -> i32
  Reads up to len bytes. Returns bytes read, 0 = EOF, < 0 = error.

write(fd: i32, buf_ptr: i32, len: i32) -> i32
  Writes len bytes. Returns bytes written, < 0 = error.

stat(path_ptr: i32, path_len: i32, stat_buf: i32) -> i32
  Gets file metadata. Returns 0 on success, < 0 on error.
```

#### Directory Operations

```
mkdir(path_ptr: i32, path_len: i32) -> i32
readdir(path_ptr: i32, path_len: i32, buf_ptr: i32, buf_len: i32) -> i32
rmdir(path_ptr: i32, path_len: i32) -> i32
unlink(path_ptr: i32, path_len: i32) -> i32
rename(from_ptr: i32, from_len: i32, to_ptr: i32, to_len: i32) -> i32
```

#### Process Control

```
exit(code: i32) -> !
  Terminates the command with given exit code. Never returns.

getenv(name_ptr: i32, name_len: i32, buf_ptr: i32, buf_len: i32) -> i32
  Gets environment variable. Returns length written, 0 = not found.

getcwd(buf_ptr: i32, buf_len: i32) -> i32
  Gets current working directory. Returns length written.
```

### Standard File Descriptors

| fd | Purpose |
|----|---------|
| 0  | stdin   |
| 1  | stdout  |
| 2  | stderr  |

Commands should write output to fd 1, errors to fd 2, and read input from fd 0.

### Memory Layout for Arguments

When `main(argc, argv)` is called:

```
argv points to an array of i32 pointers:
  argv[0]: pointer to program name (null-terminated)
  argv[1]: pointer to first argument
  ...
  argv[argc-1]: pointer to last argument
  argv[argc]: null (0)

Example for "cat file.txt":
  argc = 2
  Memory at address A:
    A+0:  "cat\0"         (4 bytes)
    A+4:  "file.txt\0"    (9 bytes)
    A+16: ptr to A+0      (i32) <- argv points here
    A+20: ptr to A+4      (i32)
    A+24: 0               (null terminator)
```

### Error Codes

Negative return values indicate errors:

| Code | Meaning           |
|------|-------------------|
| -1   | Generic error     |
| -2   | Not found         |
| -3   | Permission denied |
| -4   | Already exists    |
| -5   | Not a directory   |
| -6   | Is a directory    |
| -7   | Invalid argument  |
| -8   | No space left     |
| -9   | I/O error         |
| -10  | Bad file descriptor |
| -11  | Directory not empty |

## Loader Architecture

```
┌────────────────────────────────────────────────────────────┐
│                      Shell                                 │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ user types: cat file.txt                            │   │
│  └─────────────────────────┬───────────────────────────┘   │
│                            │                               │
│                            ▼                               │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ 1. Resolve command: /bin/cat.wasm                   │   │
│  │ 2. Load module bytes from VFS                       │   │
│  │ 3. Validate against ABI                             │   │
│  └─────────────────────────┬───────────────────────────┘   │
│                            │                               │
│                            ▼                               │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ ModuleValidator                                     │   │
│  │ - Check WASM magic/version                          │   │
│  │ - Verify 'memory' export exists                     │   │
│  │ - Verify 'main' export with correct signature       │   │
│  └─────────────────────────┬───────────────────────────┘   │
│                            │                               │
│                            ▼                               │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ Loader                                              │   │
│  │ - Instantiate WASM module                           │   │
│  │ - Bind syscall imports to Runtime                   │   │
│  │ - Setup arguments in memory                         │   │
│  │ - Call main(argc, argv)                             │   │
│  └─────────────────────────┬───────────────────────────┘   │
│                            │                               │
│                            ▼                               │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ Runtime                                             │   │
│  │ - Captures stdout/stderr                            │   │
│  │ - Provides stdin                                    │   │
│  │ - Handles syscall implementations                   │   │
│  │ - Manages file descriptor table                     │   │
│  └─────────────────────────┬───────────────────────────┘   │
│                            │                               │
│                            ▼                               │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ CommandResult { exit_code, stdout, stderr }         │   │
│  └─────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────┘
```

## Writing a Command (Rust Example)

```rust
#![no_std]
#![no_main]

// Syscall imports
#[link(wasm_import_module = "env")]
extern "C" {
    fn write(fd: i32, buf: *const u8, len: i32) -> i32;
    fn exit(code: i32) -> !;
}

#[no_mangle]
pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    let msg = b"Hello from WASM!\n";
    unsafe {
        write(1, msg.as_ptr(), msg.len() as i32);
    }
    0 // Exit code 0 = success
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    unsafe { exit(1) }
}
```

Build with:
```bash
cargo build --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/hello.wasm /bin/hello.wasm
```

## TLA+ Formal Specification

The loader has a formal TLA+ specification in `src/kernel/wasm/WasmLoader.tla` that models:

**State Machine:**
```
INIT → LOADING → READY → RUNNING → TERMINATED
         |          |         |
         +----------+---------+-→ ERROR
```

**Safety Invariants:**
- `MemorySafety`: All memory accesses within bounds
- `FdSafety`: File descriptor table always consistent
- `ExitCodeInvariant`: Exit code only set upon termination
- `TerminationFinal`: Terminal state is final

**Liveness:**
- `EventualTermination`: Running commands eventually terminate or error

## Current Limitations

1. **Builtins still hardcoded**: Core commands like `cd`, `pwd`, `echo` remain builtins for bootstrapping
2. **No dynamic linking**: Each command is fully standalone
3. **No user-space WASM commands yet**: The infrastructure is ready, but no `.wasm` files exist in `/bin`

## Related Documentation

- [Shell](../userspace/shell.md) - Shell command execution
- [Syscall Interface](syscalls.md) - Full syscall reference
- [VFS](../userspace/vfs.md) - Filesystem where commands are stored
- [Work Tracker](../WORK_TRACKER.md) - All work items and planned enhancements
