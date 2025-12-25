# Code Review: axebergos

**Reviewer**: Claude
**Date**: 2025-12-25
**Scope**: Full codebase review (~31,000 lines of Rust)

---

## Executive Summary

axebergos is a well-architected browser-based mini-OS written in Rust, targeting WebAssembly. The code demonstrates strong fundamentals: clear module separation, comprehensive tests, and good documentation. However, there are opportunities for improvement in error handling, code organization, and performance.

**Overall Quality**: 7.5/10
**Test Coverage**: Good
**Documentation**: Good
**Architecture**: Very Good

---

## Critical Issues

### 1. Weak Random Number Generation (Security)
**Location**: `src/kernel/syscall.rs:448-480`

```rust
fn generate_random_bytes(len: usize) -> Vec<u8> {
    // Uses xorshift64 seeded from system time
    let mut state: u64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x123456789ABCDEF0);
```

**Problem**: The PRNG for `/dev/random` and `/dev/urandom` uses xorshift64 seeded from system time. This is:
- Predictable (time-based seed)
- Not cryptographically secure
- Has the `getrandom` crate with the `js` feature already available

**Recommendation**: Use `getrandom` crate which is already a dependency:
```rust
fn generate_random_bytes(len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    getrandom::getrandom(&mut buf).expect("getrandom failed");
    buf
}
```

### 2. Thread-Local Global State Pattern
**Location**: Multiple files (`src/kernel/mod.rs:62-66`, `src/terminal.rs:78-98`)

The extensive use of `thread_local!` with `RefCell` creates:
- Hidden global state that's hard to test
- Potential for borrow panics at runtime
- Difficulty in supporting multi-threaded scenarios (future bare-metal target)

**Recommendation**: Consider a more explicit dependency injection approach or a kernel context object passed through function parameters.

---

## Major Issues

### 3. Duplicate Command Registration
**Location**: `src/shell/executor.rs:149-183`

```rust
reg.register("su", prog_su);
reg.register("sudo", prog_sudo);
// ... 30 lines later ...
reg.register("su", prog_su);     // DUPLICATE
reg.register("sudo", prog_sudo); // DUPLICATE
```

**Problem**: `su` and `sudo` are registered twice, wasting memory and indicating copy-paste errors.

**Recommendation**: Remove the duplicate registrations at lines 149-150.

### 4. Magic String Protocol for Builtins
**Location**: `src/shell/builtins.rs:299`, `src/shell/executor.rs:509-541`

```rust
// builtins.rs
BuiltinResult::Success(format!("__EXPORT__:{}", pairs.join("\x00")))

// executor.rs
if output.starts_with("__EXPORT__:") {
    let pairs = &output["__EXPORT__:".len()..];
```

**Problem**: Using magic string prefixes (`__EXPORT__:`, `__UNSET__:`, `__ALIAS__:`, `__UNALIAS__:`) for inter-module communication is:
- Fragile (typos won't be caught at compile time)
- Not type-safe
- Hard to extend

**Recommendation**: Use proper enum variants:
```rust
pub enum BuiltinResult {
    Success(String),
    Ok,
    Error(String),
    Exit(i32),
    Cd(PathBuf),
    Export(Vec<(String, String)>),  // NEW
    Unset(Vec<String>),             // NEW
    SetAlias(Vec<(String, String)>), // NEW
    UnsetAlias(Vec<String>),         // NEW
}
```

### 5. Large File: `shell/executor.rs` (7,473 lines)
**Location**: `src/shell/executor.rs`

This file contains:
- The `Executor` struct
- `ProgramRegistry`
- 80+ individual program implementations (`prog_cat`, `prog_ls`, etc.)

**Problem**: Monolithic files are hard to:
- Navigate and understand
- Test in isolation
- Maintain over time

**Recommendation**: Split into multiple modules:
```
src/shell/
├── executor.rs       (core execution logic)
├── registry.rs       (program registry)
├── programs/
│   ├── mod.rs
│   ├── file.rs       (cat, ls, cp, mv, rm, etc.)
│   ├── text.rs       (grep, head, tail, sort, uniq, wc)
│   ├── system.rs     (ps, kill, uptime, free)
│   ├── user.rs       (id, whoami, su, sudo, passwd)
│   └── misc.rs       (date, cal, sleep, etc.)
```

### 6. Stdin Passed via Magic Argument
**Location**: `src/shell/executor.rs:363-365`, `src/shell/executor.rs:456-458`

```rust
// Pass pipe input via special arg
let mut args = expanded_args;
if !pipe_input.is_empty() {
    args.insert(0, format!("__STDIN__:{}", pipe_input));
}
```

**Problem**: Passing stdin content as a specially-prefixed first argument is:
- Unusual API design
- Limits stdin size (must fit in a String)
- Requires every program to check for and strip this prefix

**Recommendation**: Change `ProgramFn` signature to include stdin:
```rust
pub type ProgramFn = fn(
    args: &[String],
    stdin: &str,
    stdout: &mut String,
    stderr: &mut String,
) -> i32;
```

---

## Medium Issues

### 7. Inconsistent Error Handling in Boot
**Location**: `src/boot.rs:38-43`

```rust
Err(e) => {
    console_log!("[boot] Filesystem error: {}, using fresh", e);
    init_filesystem();
}
```

**Problem**: Filesystem restore failures are silently swallowed, potentially losing user data without explicit notification.

**Recommendation**: Surface this error more prominently (e.g., show a message in the terminal).

### 8. Unbounded History in Terminal
**Location**: `src/terminal.rs:84-85`

```rust
static HISTORY: RefCell<Vec<String>> = RefCell::new(Vec::new());
```

**Problem**: Command history grows unbounded and is stored entirely in memory.

**Recommendation**: Add a maximum history size (e.g., 1000 entries):
```rust
const MAX_HISTORY: usize = 1000;

fn add_to_history(cmd: String) {
    HISTORY.with(|h| {
        let mut history = h.borrow_mut();
        if history.len() >= MAX_HISTORY {
            history.remove(0);
        }
        history.push(cmd);
    });
}
```

### 9. Missing Validation in Path Normalization
**Location**: `src/vfs/memory.rs:103-127`

The `normalize_path` function doesn't validate for:
- Path components longer than reasonable limits
- Total path length limits
- Null bytes in paths

**Recommendation**: Add validation:
```rust
fn normalize_path(path: &str) -> Result<String, io::Error> {
    if path.contains('\0') {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "null in path"));
    }
    if path.len() > 4096 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "path too long"));
    }
    // ... rest of normalization
}
```

### 10. Signal Numbers Don't Match POSIX
**Location**: `src/kernel/signal.rs:14-38`

```rust
pub enum Signal {
    SIGTERM = 1,  // POSIX: 15
    SIGKILL = 2,  // POSIX: 9
    SIGSTOP = 3,  // POSIX: 19
    SIGCONT = 4,  // POSIX: 18
    SIGINT = 5,   // POSIX: 2
    // ...
}
```

**Problem**: Signal numbers don't match POSIX conventions, which could cause confusion when users try to use familiar signal numbers.

**Recommendation**: Either:
1. Match POSIX numbers, or
2. Document prominently that these are axeberg-specific

### 11. Potential Integer Overflow in Seek
**Location**: `src/vfs/memory.rs:376-392`

```rust
SeekFrom::End(n) => {
    if n >= 0 {
        size + n as u64  // Potential overflow if n is very large
    } else {
        size.saturating_sub((-n) as u64)
    }
}
```

**Recommendation**: Use `checked_add` and return an error on overflow.

---

## Minor Issues

### 12. Unused Import Warning Potential
**Location**: `src/kernel/object.rs:13`

```rust
use std::io::{self, Read, Seek, SeekFrom, Write};
```

`Read`, `Write`, `Seek` are traits used as bounds - this is fine, but ensure CI catches actual unused imports.

### 13. Inconsistent Error Messages
Some error messages include the command name, others don't:
```rust
// Good
return BuiltinResult::Error("cd: too many arguments".into());

// Could be improved
return Err(io::Error::new(io::ErrorKind::NotFound, "File not found"));
// Better: include path
```

### 14. Clone Instead of Reference in Several Places
**Location**: `src/vfs/memory.rs:307`, `src/vfs/memory.rs:341`, etc.

```rust
let path = file.path.clone();  // Cloning for borrow checker
```

Consider restructuring to avoid cloning where possible.

### 15. No Timeout on async Operations
WASM async operations (fetch, etc.) have no timeout, which could cause hangs.

---

## Code Quality Observations

### Strengths

1. **Excellent Documentation**: Design principles documented, good module-level docs, clear comments explaining "why"

2. **Comprehensive Testing**:
   - Unit tests in most modules
   - Integration tests
   - Invariant tests documenting system guarantees
   - TLA+ formal specifications

3. **Clean Architecture**:
   - Clear separation between kernel, shell, VFS, platform
   - Syscall interface provides clean boundary
   - Platform abstraction allows future bare-metal target

4. **Good Rust Idioms**:
   - Proper use of `Result` and `Option`
   - Derive macros for common traits
   - Builder patterns where appropriate

5. **Thoughtful Feature Set**: Impressive number of Unix utilities implemented (80+)

### Areas for Improvement

1. **Test Organization**: Tests are inline (`#[cfg(test)] mod tests`) which is fine, but some files have very long test modules

2. **Logging**: Uses `console_log!` macro but no log levels - consider using the `log` crate

3. **Metrics/Observability**: Limited visibility into system performance

4. **Configuration**: Hardcoded values (autosave interval, history size) should be configurable

---

## Performance Observations

### Potential Bottlenecks

1. **VFS Path Lookup**: Linear scan through HashMap for each path component could be slow for deep directories

2. **Signal Coalescing**: Uses `VecDeque` with linear search for signal priority - could use `BinaryHeap`

3. **Pipe Buffer**: `VecDeque<u8>` for pipe - consider using a ring buffer for better cache locality

4. **History Search**: Linear search through history for Ctrl+R - could use suffix array for large histories

### Memory Usage

1. **FileObject stores full content**: Each open file holds a copy of its data, which could be memory-intensive for large files

2. **Shared Memory is copied**: The `shmat` implementation copies data rather than sharing, losing the efficiency benefit

---

## Recommendations Summary

### High Priority
1. Fix PRNG to use `getrandom` crate
2. Remove duplicate command registrations
3. Replace magic string protocol with proper enum variants

### Medium Priority
4. Split `executor.rs` into smaller modules
5. Change `ProgramFn` signature to include stdin parameter
6. Add history size limit
7. Add path validation

### Low Priority
8. Add log levels
9. Make configuration values configurable
10. Consider signal numbers alignment with POSIX
11. Optimize VFS path lookup

---

## Conclusion

axebergos is a well-crafted project that demonstrates deep understanding of operating system concepts. The code is generally clean and well-documented. The main areas for improvement are:

1. **Security**: Use proper cryptographic RNG
2. **Maintainability**: Split large files, use type-safe inter-module communication
3. **API Design**: Cleaner function signatures for programs

The project successfully achieves its stated goal of being "tractable" - the code is indeed comprehensible and well-organized despite its scope.
