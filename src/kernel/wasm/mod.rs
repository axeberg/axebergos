//! WASM Command Module Loader
//!
//! This module provides the infrastructure for loading and executing WASM
//! command modules - the axeberg equivalent of Unix executables in /bin.
//!
//! # Design Philosophy
//!
//! Instead of hardcoding shell commands, axeberg treats each command as a
//! standalone WASM module. This provides:
//!
//! - **Isolation**: Each command runs in its own WASM sandbox
//! - **Extensibility**: Users can add new commands by dropping .wasm files
//! - **Portability**: Commands can be written in any language that compiles to WASM
//! - **Security**: WASM's capability-based security model limits what commands can do
//!
//! # ABI Specification v1
//!
//! ## Overview
//!
//! A WASM command module is a WebAssembly module that conforms to the axeberg
//! Command ABI. This ABI defines:
//!
//! 1. Required exports from the command module
//! 2. Syscall imports provided by the kernel
//! 3. Memory layout conventions
//! 4. Argument passing protocol
//!
//! ## Required Exports
//!
//! Every command module MUST export:
//!
//! | Export       | Type                            | Description                    |
//! |-------------|--------------------------------|--------------------------------|
//! | `memory`    | Memory                         | Linear memory for data exchange|
//! | `main`      | `(argc: i32, argv: i32) -> i32`| Entry point, returns exit code |
//!
//! Optional exports:
//!
//! | Export        | Type   | Description                          |
//! |--------------|--------|--------------------------------------|
//! | `__heap_base`| Global | Start of heap for dynamic allocation |
//!
//! ## Syscall Imports
//!
//! The kernel provides these syscalls in the `env` namespace:
//!
//! ### File Operations
//!
//! ```text
//! open(path_ptr: i32, path_len: i32, flags: i32) -> i32
//!   Opens a file. Returns fd >= 0 on success, < 0 on error.
//!   Flags: 0 = read, 1 = write, 2 = read+write, 4 = create, 8 = truncate
//!
//! close(fd: i32) -> i32
//!   Closes a file descriptor. Returns 0 on success, < 0 on error.
//!
//! read(fd: i32, buf_ptr: i32, len: i32) -> i32
//!   Reads up to len bytes. Returns bytes read, 0 = EOF, < 0 = error.
//!
//! write(fd: i32, buf_ptr: i32, len: i32) -> i32
//!   Writes len bytes. Returns bytes written, < 0 = error.
//!
//! stat(path_ptr: i32, path_len: i32, stat_buf: i32) -> i32
//!   Gets file metadata. stat_buf is 32 bytes:
//!   [0..4]: size (u32)
//!   [4..8]: is_dir (u32, 0 or 1)
//!   [8..16]: modified_time (u64, unix timestamp)
//!   [16..24]: created_time (u64, unix timestamp)
//!   [24..32]: reserved
//! ```
//!
//! ### Directory Operations
//!
//! ```text
//! mkdir(path_ptr: i32, path_len: i32) -> i32
//!   Creates a directory. Returns 0 on success, < 0 on error.
//!
//! readdir(path_ptr: i32, path_len: i32, buf_ptr: i32, buf_len: i32) -> i32
//!   Lists directory entries. Returns bytes written to buf, < 0 = error.
//!   Format: null-terminated strings concatenated.
//!
//! rmdir(path_ptr: i32, path_len: i32) -> i32
//!   Removes empty directory. Returns 0 on success, < 0 on error.
//!
//! unlink(path_ptr: i32, path_len: i32) -> i32
//!   Removes a file. Returns 0 on success, < 0 on error.
//!
//! rename(from_ptr: i32, from_len: i32, to_ptr: i32, to_len: i32) -> i32
//!   Renames/moves a file. Returns 0 on success, < 0 on error.
//! ```
//!
//! ### Process Control
//!
//! ```text
//! exit(code: i32) -> !
//!   Terminates the command with given exit code. Never returns.
//!
//! getenv(name_ptr: i32, name_len: i32, buf_ptr: i32, buf_len: i32) -> i32
//!   Gets environment variable. Returns length written, 0 = not found, < 0 = error.
//!
//! getcwd(buf_ptr: i32, buf_len: i32) -> i32
//!   Gets current working directory. Returns length written, < 0 = error.
//! ```
//!
//! ## Memory Layout for Arguments
//!
//! When `main(argc, argv)` is called:
//!
//! ```text
//! argv points to an array of i32 pointers:
//!   argv[0]: pointer to program name (null-terminated string)
//!   argv[1]: pointer to first argument
//!   ...
//!   argv[argc-1]: pointer to last argument
//!   argv[argc]: null (0)
//!
//! Example for "cat file.txt":
//!   argc = 2
//!   Memory layout at some address A:
//!     A+0:  "cat\0"         (4 bytes)
//!     A+4:  "file.txt\0"    (9 bytes)
//!     A+16: ptr to A+0      (i32 = 4 bytes) <- argv points here
//!     A+20: ptr to A+4      (i32 = 4 bytes)
//!     A+24: 0               (null terminator)
//! ```
//!
//! ## Standard File Descriptors
//!
//! | fd | Purpose        |
//! |----|----------------|
//! | 0  | stdin          |
//! | 1  | stdout         |
//! | 2  | stderr         |
//!
//! Commands should write output to fd 1, errors to fd 2, and read input from fd 0.
//!
//! ## Error Codes
//!
//! Negative return values indicate errors:
//!
//! | Code | Meaning           |
//! |------|-------------------|
//! | -1   | Generic error     |
//! | -2   | Not found         |
//! | -3   | Permission denied |
//! | -4   | Already exists    |
//! | -5   | Not a directory   |
//! | -6   | Is a directory    |
//! | -7   | Invalid argument  |
//! | -8   | No space left     |
//! | -9   | I/O error         |
//!
//! ## Example Command (Rust)
//!
//! ```rust,ignore
//! #![no_std]
//! #![no_main]
//!
//! // Syscall imports
//! #[link(wasm_import_module = "env")]
//! extern "C" {
//!     fn write(fd: i32, buf: *const u8, len: i32) -> i32;
//!     fn exit(code: i32) -> !;
//! }
//!
//! #[no_mangle]
//! pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
//!     let msg = b"Hello from WASM!\n";
//!     unsafe {
//!         write(1, msg.as_ptr(), msg.len() as i32);
//!     }
//!     0
//! }
//!
//! #[panic_handler]
//! fn panic(_: &core::panic::PanicInfo) -> ! {
//!     unsafe { exit(1) }
//! }
//! ```
//!
//! # TLA+ Specification (Informal)
//!
//! The loader maintains these invariants:
//!
//! 1. **Memory Safety**: All pointer accesses are bounds-checked against
//!    the module's memory size before reading/writing.
//!
//! 2. **Isolation**: A command cannot access memory of other commands or
//!    the kernel (WASM sandboxing guarantees this).
//!
//! 3. **File Descriptor Validity**: Syscalls validate fd arguments against
//!    the command's open file descriptor table.
//!
//! 4. **Termination**: The `exit` syscall always terminates the command.
//!    Commands may also return from `main`, which calls exit implicitly.
//!
//! 5. **Sequential Consistency**: Syscalls are processed in order, with
//!    full consistency between reads and writes.
//!
//! State transitions:
//! ```text
//! INIT -> LOADING -> READY -> RUNNING -> TERMINATED
//!                      |        |
//!                      +--------+-- (on error) --> ERROR
//! ```

mod abi;
mod command;
mod error;
mod executor;
mod loader;
mod runtime;

pub use abi::*;
pub use command::*;
pub use error::*;
pub use executor::*;
pub use loader::*;
pub use runtime::*;

#[cfg(test)]
mod tests;
