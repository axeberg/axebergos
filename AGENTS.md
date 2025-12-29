# Agent Guidelines for AxebergOS

This document provides instructions for LLM agents (Claude, GPT, etc.) working on this codebase.

## Pre-Commit Checklist

**Before every commit, run these commands in order:**

```bash
cargo fmt
cargo clippy --lib -- -D warnings
cargo test --lib
```

Only proceed with `git add` and `git commit` if all three pass. This is **mandatory** - never skip these steps.

---

## Documentation Index

### Core Documentation
| Document | Location | Purpose |
|----------|----------|---------|
| Project Analysis | `docs/PROJECT_ANALYSIS.md` | Full codebase review, security findings, architecture grades |
| Work Tracker | `docs/WORK_TRACKER.md` | Known issues, priorities, progress log, and completed features |
| Invariants | `docs/development/invariants.md` | Critical system invariants (P1-P3, S1-S5, M1-M5, etc.) |
| Contributing Guide | `docs/development/contributing.md` | Code style, commit format, PR process |

### Architecture Decisions (ADRs)
Located in `docs/decisions/`:
| ADR | Decision |
|-----|----------|
| 001 | WebAssembly as primary target |
| 002 | Custom async executor (no tokio/async-std) |
| 003 | In-memory filesystem |
| 004 | Unix-like interface design |
| 005 | Single WASM binary architecture |
| 006 | Cooperative multitasking |

### Subsystem Documentation
| System | Documentation | Source |
|--------|--------------|--------|
| Kernel Overview | `docs/kernel/overview.md` | `src/kernel/` |
| Syscalls | `docs/kernel/syscalls.md` | `src/kernel/syscall.rs` |
| Processes | `docs/kernel/processes.md` | `src/kernel/process.rs` |
| Memory | `docs/kernel/memory.md` | `src/kernel/memory.rs` |
| Signals | `docs/kernel/signals.md` | `src/kernel/signal.rs` |
| IPC | `docs/kernel/ipc.md` | `src/kernel/ipc.rs` |
| Users | `docs/kernel/users.md` | `src/kernel/users.rs` |
| Timers | `docs/kernel/timers.md` | `src/kernel/timer.rs` |
| Executor | `docs/kernel/executor.md` | `src/kernel/executor.rs` |
| WASM Modules | `docs/kernel/wasm-modules.md` | `src/kernel/wasm/` |
| VFS | `docs/userspace/vfs.md` | `src/vfs/` |
| Shell | `docs/userspace/shell.md` | `src/shell/` |
| Compositor | `docs/userspace/compositor.md` | `src/compositor/` |

### Formal Specifications
TLA+ specifications in `specs/tla/`:
- `ProcessStateMachine.tla` - Process lifecycle verification
- `SignalDelivery.tla` - Signal system correctness
- `TimerQueue.tla` - Timer queue ordering
- `PathValidation.tla` - VFS path handling
- `HistoryBuffer.tla` - Terminal history

---

## Project Structure

```
src/
├── kernel/                 # Kernel subsystems (~21k lines)
│   ├── syscall.rs         # System call implementations
│   ├── process.rs         # Process management
│   ├── memory.rs          # Memory management
│   ├── signal.rs          # Signal handling
│   ├── users.rs           # User/group management
│   ├── ipc.rs             # Inter-process communication
│   ├── executor.rs        # Async task executor
│   ├── timer.rs           # Timer management
│   ├── wasm/              # WASM module loading
│   ├── pkg/               # Package manager
│   ├── work_stealing/     # Work-stealing scheduler
│   └── ...
├── shell/                  # Shell implementation (~3k lines)
│   ├── executor.rs        # Command execution
│   ├── parser.rs          # Command parsing
│   ├── builtins.rs        # Built-in commands
│   └── programs/          # Individual command implementations
├── vfs/                    # Virtual filesystem
│   ├── memory.rs          # In-memory FS
│   ├── layered.rs         # Layered/overlay FS
│   └── persist.rs         # Persistence layer
├── compositor/             # WebGPU compositor
│   ├── surface.rs         # GPU surface management
│   ├── layout.rs          # BSP tiling layout
│   ├── text.rs            # Text rendering
│   └── ...
└── platform/               # Platform abstraction
    ├── web.rs             # Browser/WASM platform
    └── wasi.rs            # WASI platform
```

---

## Code Standards

### Rust Patterns

**Error Handling - Never Panic in Production Code**
```rust
// GOOD: Use Result with ? operator
fn get_current_process(&self) -> SyscallResult<&Process> {
    let current = self.current.ok_or(SyscallError::NoProcess)?;
    self.processes.get(&current).ok_or(SyscallError::NoProcess)
}

// BAD: Never use unwrap() in kernel code
fn bad_example(&self) -> &Process {
    self.processes.get(&self.current.unwrap()).unwrap()  // DON'T DO THIS
}
```

**Interior Mutability**
```rust
// Single-threaded, use RefCell
use std::cell::RefCell;
thread_local! {
    static KERNEL: RefCell<Kernel> = RefCell::new(Kernel::new());
}

// Access pattern
KERNEL.with(|k| k.borrow_mut().sys_call())
```

**Syscall Pattern**
```rust
// In Kernel impl
pub fn sys_new_call(&mut self, arg: Type) -> SyscallResult<Result> {
    let current = self.current.ok_or(SyscallError::NoProcess)?;
    let process = self.get_current_process()?;
    // Implementation using ? for all fallible operations
}

// Public wrapper function
pub fn new_call(arg: Type) -> SyscallResult<Result> {
    KERNEL.with(|k| k.borrow_mut().sys_new_call(arg))
}
```

**Documentation Style**
```rust
/// Allocate a memory region for the current process.
///
/// # Arguments
/// * `size` - Size in bytes
/// * `prot` - Protection flags
///
/// # Returns
/// A `RegionId` on success, or an error if allocation fails.
pub fn mem_alloc(size: usize, prot: Protection) -> SyscallResult<RegionId>
```

### Commit Message Format

Use conventional commits:
```
type: brief description

Longer explanation if needed.
- Bullet points for multiple changes
- Reference issues: Fixes #123
```

Types: `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `security`

Examples:
```
fix: handle empty file reads correctly

Files with zero size were returning errors instead
of empty buffers. Now correctly returns 0 bytes read.
```

```
security: fix critical password hashing vulnerability

- Replace DJB2 hash with salted key-stretching
- Add constant-time comparison
- Remove hardcoded root password
```

---

## Testing

### Running Tests
```bash
# Run all tests
cargo test --lib

# Run specific module tests
cargo test --lib syscall::
cargo test --lib vfs::memory::
cargo test --lib users::

# Run with output
cargo test --lib -- --nocapture
```

### Test Structure
```rust
#[test]
fn test_feature_basic() {
    // Happy path - feature works as expected
}

#[test]
fn test_feature_error_case() {
    // Verify proper error handling
}

#[test]
fn test_feature_edge_case() {
    // Boundary conditions, empty inputs, max values
}
```

### Test Requirements
- All new code must have tests
- Cover happy path, error cases, and edge cases
- Tests may use `.unwrap()` (production code may not)
- Current test count: 660+ tests

---

## Build Targets

```bash
# Native (for testing)
cargo build
cargo test --lib

# WASM (production)
wasm-pack build --target web --release

# Type check only (fast)
cargo check --lib --target wasm32-unknown-unknown
```

---

## Critical Invariants

From `docs/development/invariants.md` - these must NEVER be violated:

### Process Invariants
- **P1**: Valid state transitions only (Running → Sleeping/Blocked/Stopped/Zombie)
- **P2**: Zombie is terminal - no transitions out
- **P3**: Parent must exist (or be None for init)

### Signal Invariants
- **S1**: SIGKILL always terminates (cannot be caught/blocked/ignored)
- **S2**: SIGSTOP always stops (cannot be caught/blocked/ignored)
- **S3**: Signals coalesce (except realtime)

### Memory Invariants
- **M1**: Cannot exceed memory limits
- **M2**: All access within region bounds
- **M5**: No use-after-free

### File Descriptor Invariants
- **F1**: Refcount = number of handles
- **F2**: Free when refcount hits 0
- **F3**: Closed FD returns BadFd, not UB

---

## Security Guidelines

### Current Security Status
See `docs/PROJECT_ANALYSIS.md` Appendix B for full details.

**Fixed Issues:**
- SEC-001: Hardcoded root password removed
- SEC-002: Secure password hashing with salt + key stretching
- SEC-003: Kernel panic points fixed (32+ unwrap calls)
- SEC-004: Symlink loop detection (MAX_DEPTH = 40)

**Remaining Issues (HIGH):**
- TOCTOU race conditions in file operations
- Missing setuid bit processing
- No privilege dropping support

### Security Patterns
```rust
// Password hashing (src/kernel/users.rs)
// - 16-byte random salt per password
// - 10,000 rounds of key stretching
// - Constant-time comparison

// Symlink resolution (src/vfs/memory.rs)
// - MAX_SYMLINK_DEPTH = 40 (POSIX standard)
// - Component-by-component resolution
// - Detect and reject loops

// Error handling
// - Never leak full paths in error messages
// - Validate all user input at syscall boundary
// - Check permissions before operations
```

---

## Current Work

See `docs/WORK_TRACKER.md` for:
- Known issues and their status (57 items tracked)
- Priority order for fixes
- Progress log with dates

---

## Project Philosophy

From `docs/development/contributing.md`:

1. **Tractable**: Code should be understandable by one person
2. **Simple**: Prefer simple solutions over clever ones
3. **Complete**: Features should be fully implemented, not partial
4. **Tested**: All code needs tests

---

## Common Tasks

### Adding a New Syscall
1. Add method to `Kernel` struct in `src/kernel/syscall.rs`
2. Add public wrapper function
3. Update `docs/kernel/syscalls.md`
4. Add comprehensive tests
5. Run pre-commit checklist

### Adding a Shell Command
1. Add function to appropriate file in `src/shell/programs/`
2. Register in `ProgramRegistry::new()` in `src/shell/executor.rs`
3. Add man page in `docs/man/`
4. Run pre-commit checklist

### Fixing a Security Issue
1. Check `docs/WORK_TRACKER.md` for existing tracking
2. Implement fix following security patterns above
3. Add tests that verify the fix
4. Update `docs/PROJECT_ANALYSIS.md` to mark as fixed
5. Update `docs/WORK_TRACKER.md` progress log
6. Run pre-commit checklist

---

## Quick Reference

| Task | Command |
|------|---------|
| Format code | `cargo fmt` |
| Lint code | `cargo clippy --lib -- -D warnings` |
| Run all tests | `cargo test --lib` |
| Build native | `cargo build` |
| Build WASM | `wasm-pack build --target web --release` |
| Check types only | `cargo check --lib --target wasm32-unknown-unknown` |

---

*Last updated: 2025-12-28*
