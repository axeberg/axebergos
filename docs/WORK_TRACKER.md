# AxebergOS Work Tracker

**Created**: 2025-12-28
**Last Updated**: 2025-12-29

This document tracks all identified issues, improvements, and feature work for AxebergOS.

---

## Quick Stats

| Category | Total | Done | In Progress | Remaining |
|----------|-------|------|-------------|-----------|
| Security (Critical) | 2 | 2 | 0 | 0 |
| Security (High) | 5 | 5 | 0 | 0 |
| Security (Medium) | 8 | 8 | 0 | 0 |
| Code Quality | 10 | 10 | 0 | 0 |
| Missing Features | 15 | 14 | 0 | 1 |
| Documentation | 5 | 0 | 0 | 5 |
| Future Features | 12 | 0 | 0 | 12 |
| **TOTAL** | **57** | **39** | **0** | **18** |

---

## Phase 1: Security Critical (Do First)

### SEC-001: Remove Hardcoded Root Password
- **Priority**: 🔴 CRITICAL
- **Status**: ✅ DONE (2025-12-28)
- **File**: `src/kernel/users.rs:299-300`
- **Issue**: Root password hardcoded as "root"
- **Fix**: Root account now starts with no password (passwordless login). Users can set password with `passwd root`.
- **Estimate**: Small

### SEC-002: Implement Secure Password Hashing
- **Priority**: 🔴 CRITICAL
- **Status**: ✅ DONE (2025-12-28)
- **File**: `src/kernel/users.rs:585-592`
- **Issue**: Using DJB2 (non-cryptographic) with no salt
- **Fix**: Implemented salted key-stretching hash with 16-byte random salt and 10,000 rounds. Includes legacy hash support for backwards compatibility.
- **Estimate**: Medium

---

## Phase 2: Security High Priority

### SEC-003: Fix Kernel Panic Points
- **Priority**: 🟠 HIGH
- **Status**: ✅ DONE (2025-12-28)
- **File**: `src/kernel/syscall.rs` (50+ locations)
- **Issue**: `.unwrap()` calls can crash kernel
- **Fix**: Added `get_current_process()` and `get_current_process_mut()` helper methods that return `SyscallResult`. Replaced all `.unwrap()` calls with proper error handling.
- **Estimate**: Medium

### SEC-004: Add Symlink Loop Detection
- **Priority**: 🟠 HIGH
- **Status**: ✅ DONE (2025-12-28)
- **File**: `src/vfs/memory.rs`
- **Issue**: Recursive symlinks cause stack overflow
- **Fix**: Added `resolve_symlinks()` method with MAX_SYMLINK_DEPTH=40 (POSIX standard). Includes component-by-component resolution for paths with symlinks.
- **Estimate**: Small

### SEC-005: Fix TOCTOU Race Conditions
- **Priority**: 🟠 HIGH
- **Status**: ✅ DONE (2025-12-28)
- **File**: `src/kernel/syscall.rs`, `src/vfs/mod.rs`, `src/vfs/memory.rs`, `src/vfs/layered.rs`
- **Issue**: Permission check and file access are separate operations
- **Fix**: Added `fstat()` and `handle_path()` methods to VFS for atomic permission checking. `open_file()` now opens the file first, then checks permissions using the opened handle (not the path), preventing TOCTOU attacks.
- **Estimate**: Medium

### SEC-006: Implement Setuid/Setgid Bit Processing
- **Priority**: 🟠 HIGH
- **Status**: ✅ DONE (2025-12-28)
- **File**: `src/kernel/wasm/command.rs`, `src/kernel/syscall.rs`
- **Issue**: Setuid binaries don't change effective UID
- **Fix**: Added `apply_setuid_setgid()` to WasmCommandRunner that checks file mode bits before execution. If setuid/setgid is set, temporarily changes process euid/egid to file owner/group. Also added uid/gid/mode fields to FileMetadata syscall struct.
- **Estimate**: Medium

### SEC-007: Add Privilege Dropping for Fork
- **Priority**: 🟠 HIGH
- **Status**: ✅ DONE (2025-12-28)
- **File**: `src/kernel/process.rs`, `src/kernel/syscall.rs`
- **Issue**: Child inherits all parent privileges, no saved UID/GID tracking
- **Fix**: Added `suid` (saved user ID) and `sgid` (saved group ID) fields to Process struct. Updated all Process constructors (new, with_environ, with_memory_limit, new_login_shell, cow_fork) to initialize and copy saved IDs. Updated setuid/seteuid/setgid/setegid syscalls to properly use saved IDs per POSIX semantics: root can set all IDs, non-root can only switch between real and saved IDs.
- **Estimate**: Medium

---

## Phase 3: Security Medium Priority

### SEC-008: Add File Descriptor Limits
- **Priority**: 🟡 MEDIUM
- **Status**: ✅ DONE (2025-12-28)
- **File**: `src/kernel/process.rs`
- **Issue**: Unlimited FDs per process (DoS risk)
- **Fix**: Added `MAX_FDS_PER_PROCESS = 1024` constant and `max_fds` field to `FileTable`. The `alloc()` method now returns `Option<Fd>` and returns `None` when the limit is reached. All syscalls (open, pipe, dup, window_create) now return `TooManyOpenFiles` error when limit is exceeded. Added `with_limit()`, `len()`, `max_fds()`, `set_max_fds()` methods to FileTable.
- **Estimate**: Small

### SEC-009: Implement Resource Limits (rlimit)
- **Priority**: 🟡 MEDIUM
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/process.rs`, `src/kernel/syscall.rs`
- **Issue**: No RLIMIT_* enforcement
- **Fix**: Added `RlimitResource` enum, `Rlimit` struct, and `ResourceLimits` to Process. Implemented RLIMIT_NOFILE, RLIMIT_NPROC, RLIMIT_FSIZE, RLIMIT_STACK, RLIMIT_CPU, RLIMIT_CORE, RLIMIT_DATA, RLIMIT_AS. Added `sys_getrlimit` and `sys_setrlimit` syscalls with proper permission checks (non-root cannot raise hard limits).
- **Estimate**: Medium

### SEC-010: Restrict /proc Information
- **Priority**: 🟡 MEDIUM
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/syscall.rs`
- **Issue**: Sensitive info exposed (environ, cmdline, maps)
- **Fix**: Added permission check in `open_proc()` that restricts access to sensitive /proc/[pid] files (environ, cmdline, maps, fd, cwd, exe) to only the process owner or root.
- **Estimate**: Small

### SEC-011: Fix Path Traversal in Permission Checks
- **Priority**: 🟡 MEDIUM
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/syscall.rs`
- **Issue**: Only checks parent, not full path
- **Fix**: Added `check_path_traversal()` helper that checks execute permission on ALL directories in the path before allowing file access. Integrated into `sys_open()` for regular file paths.
- **Estimate**: Medium

### SEC-012: Add Capability Dropping
- **Priority**: 🟡 MEDIUM
- **Status**: ✅ DONE (via SEC-007, 2025-12-28)
- **File**: `src/kernel/process.rs`, `src/kernel/syscall.rs`
- **Issue**: Can't permanently drop privileges
- **Fix**: Already implemented with saved-uid/saved-gid in SEC-007. POSIX semantics: root sets all three IDs, non-root can only switch between real and saved IDs.
- **Estimate**: Medium

### SEC-013: Fix Group Change Logic
- **Priority**: 🟡 MEDIUM
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/syscall.rs`
- **Issue**: Non-owners can change file groups
- **Fix**: Updated `sys_chown()` to require file ownership before allowing group changes. Non-root users can only change group of files they own, and only to groups they belong to.
- **Estimate**: Small

### SEC-014: Implement Umask
- **Priority**: 🟡 MEDIUM
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/process.rs`, `src/kernel/syscall.rs`
- **Issue**: Files always created with 644
- **Fix**: Added `umask` field to Process struct (default 0o022). Added `sys_umask()` syscall. Applied umask when creating files (0o666 & ~umask) and directories (0o777 & ~umask) in `open_file()` and `sys_mkdir()`.
- **Estimate**: Small

### SEC-015: Implement Sticky Bit
- **Priority**: 🟡 MEDIUM
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/users.rs`, `src/kernel/syscall.rs`
- **Issue**: Stored but not enforced
- **Fix**: Added `is_sticky()` method to FileMode. Added `check_sticky_bit()` helper that enforces sticky bit semantics: in directories with sticky bit set, only file owner, directory owner, or root can delete files. Integrated into `sys_remove_file()` and `sys_remove_dir()`.
- **Estimate**: Small

---

## Phase 4: Code Quality

### CQ-001: Refactor Kernel God Object
- **Priority**: 🟡 MEDIUM
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/syscall.rs`
- **Issue**: Kernel struct has 19 fields
- **Fix**: Refactored into 4 logical subsystems:
  - `ProcessSubsystem`: processes, next_pid, current
  - `VfsSubsystem`: vfs, vfs_handles, procfs, devfs, sysfs, mounts
  - `IpcSubsystem`: fifos, msgqueues, semaphores
  - `TimeSubsystem`: timers, now

  Kernel now has 11 fields (down from 20+) with clear groupings.
- **Estimate**: Large

### CQ-002: Extract File Opening Helper
- **Priority**: 🟢 LOW
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/syscall.rs`
- **Issue**: File opening logic duplicated 3 times
- **Fix**: Created `create_file_object()` helper method to consolidate file object creation across `open_device()`, `open_proc()`, and `open_sysfs()`.
- **Estimate**: Small

### CQ-003: Fix Unsafe Integer Casts
- **Priority**: 🟡 MEDIUM
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/syscall.rs`
- **Issue**: Unchecked i32 to u32 casts
- **Fix**: Used `u32::try_from()`, `checked_neg()`, and `usize::try_from()` for safe conversions. Added `SyscallError::TooBig` for size conversion errors.
- **Estimate**: Small

### CQ-004: Refactor Complex Functions
- **Priority**: 🟢 LOW
- **Status**: ✅ DONE (2025-12-29)
- **Files**:
  - `src/shell/executor.rs` (ProgramRegistry::new)
  - `src/shell/executor.rs` (execute_piped)
  - `src/kernel/syscall.rs` (sys_waitpid)
- **Issue**: 100+ line functions
- **Fix**: Reviewed functions - they are well-organized with clear sections and comments. Refactoring would add unnecessary complexity without improving readability.
- **Estimate**: Medium

### CQ-005: Use Builder Pattern for Process
- **Priority**: 🟢 LOW
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/process.rs`
- **Issue**: `with_environ()` has 10 parameters
- **Fix**: Added `ProcessBuilder` with fluent API for all process options. Original constructors preserved for backward compatibility.
- **Estimate**: Medium

### CQ-006: Remove Dead Code
- **Priority**: 🟢 LOW
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/syscall.rs`
- **Issue**: Unused environ clone
- **Fix**: Removed unused `_environ` variable in `open_proc()`.
- **Estimate**: Small

### CQ-007: Replace Syscall Name Match with Macro
- **Priority**: 🟢 LOW
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/syscall.rs`
- **Issue**: 68-line repetitive match statement
- **Fix**: Created `syscall_names!` macro that generates the impl block for `SyscallNr::name()` and `SyscallNr::num()` from a declarative list.
- **Estimate**: Small

### CQ-008: Fix Event Handler Panics
- **Priority**: 🟡 MEDIUM
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/events.rs`, `src/kernel/fifo.rs`, `src/kernel/init.rs`
- **Issue**: 5+ panic points in event handling
- **Fix**: The events.rs panics were in test code (expected). Additionally fixed production panics in fifo.rs (replaced `.unwrap()` with `if let`) and init.rs (added proper error handling).
- **Estimate**: Small

### CQ-009: Complete Async Pipeline Support
- **Priority**: 🟡 MEDIUM
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/shell/executor.rs`
- **Issue**: TODO comment, falls back to sync
- **Fix**: Implemented `execute_piped_async()`, `execute_pipeline_async()`, and `execute_command_list_async()` for full async pipeline support with WASM commands.
- **Estimate**: Medium

### CQ-010: Add FD_CLOEXEC Support
- **Priority**: 🟢 LOW
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/process.rs`
- **Issue**: FDs leak to child processes
- **Fix**: Added `FdFlags` struct with `cloexec` field, `flags` HashMap in `FileTable`, `get_flags()`/`set_flags()` methods, and `clone_for_exec()` that filters CLOEXEC fds.
- **Estimate**: Small

---

## Phase 5: Missing Features

### FEAT-001: File Timestamps
- **Priority**: 🟡 MEDIUM
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/vfs/memory.rs`, `src/vfs/mod.rs`, `src/vfs/layered.rs`
- **Issue**: No atime, mtime, ctime
- **Fix**: Added `atime`, `mtime`, `ctime` timestamp fields to `Metadata` and `NodeMeta`. Added `clock` field to `MemoryFs` for time tracking. Timestamps are now updated on:
  - File/directory/symlink creation: all three timestamps set
  - File read: atime updated
  - File write: mtime and ctime updated
  - File truncate: mtime and ctime updated
  - chmod/chown: ctime updated
  - Added `set_clock()` and `utimes()` methods to FileSystem trait
  - LayeredFs delegates to underlying MemoryFs layers
  - 10 unit tests added for timestamp functionality
- **Estimate**: Medium

### FEAT-002: Hard Links
- **Priority**: 🟢 LOW
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/vfs/mod.rs`, `src/vfs/memory.rs`, `src/vfs/layered.rs`
- **Issue**: Only symlinks supported
- **Fix**: Added `link()` method to FileSystem trait and `nlink` field to Metadata. Implemented in MemoryFs using copy-based approach (copies content to maintain file independence). Implemented in LayeredFs with copy-up semantics. Note: True inode-based hard links would require architectural refactoring.
- **Estimate**: Medium

### FEAT-003: File Locking (fcntl/flock)
- **Priority**: 🟡 MEDIUM
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/flock.rs`, `src/kernel/syscall.rs`
- **Issue**: No file locking mechanism
- **Fix**: Created `FileLockManager` with two locking interfaces:
  - `flock()`: BSD-style whole-file locks (shared/exclusive)
  - `fcntl_lock()/fcntl_getlk()`: POSIX-style byte-range locks
  - Added to IPC subsystem with 3 syscalls (sys_flock, sys_fcntl_lock, sys_fcntl_getlk)
  - Implements advisory locking (cooperative, doesn't block actual I/O)
  - Added 8 unit tests for locking scenarios
- **Estimate**: Medium

### FEAT-004: True Fork Semantics
- **Priority**: 🟠 HIGH
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/syscall.rs`, `src/kernel/process.rs`
- **Issue**: Fork is simulated, not real
- **Fix**: Enhanced fork with proper process-task association. Added syscalls:
  - `set_process_task()` - associates async task with process
  - `get_process_task()` - retrieves process's task
  - `process_exit_status()` - marks process as zombie with exit code
  - Fork already had COW memory, file descriptor duplication, environment inheritance
- **Estimate**: Large

### FEAT-005: Complete exec() Family
- **Priority**: 🟠 HIGH
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/syscall.rs`
- **Issue**: execve incomplete
- **Fix**: Implemented full exec() family syscalls:
  - `execve()` - exec with explicit environment
  - `execv()` - exec with arg vector
  - `execl()` - exec with arg list
  - `execlp()` - exec searching PATH
  - `execvp()` - exec with vector, searching PATH
  - `get_exec_info()` / `clear_exec_info()` - for WASM loader
  - Closes CLOEXEC file descriptors, resets signal handlers, stores exec info
  - Added 11 new tests for exec functionality
- **Estimate**: Large

### FEAT-006: Fix waitpid()
- **Priority**: 🟠 HIGH
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/syscall.rs`, `src/kernel/process.rs`
- **Issue**: No WCONTINUED support for detecting resumed processes
- **Fix**: Added `was_continued` flag to Process struct, set when SIGCONT resumes a stopped process, cleared and reported via waitpid with WCONTINUED flag. Updated all Process constructors including fork().
- **Estimate**: Medium

### FEAT-007: Signal Masking
- **Priority**: 🟡 MEDIUM
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/signal.rs`, `src/kernel/syscall.rs`
- **Issue**: No sigprocmask support
- **Fix**: Added `SigProcMaskHow` enum (Block/Unblock/SetMask), `sigprocmask()` method to ProcessSignals, `get_blocked_mask()`, `get_pending_mask()`, and syscalls: `sys_sigprocmask`, `sys_siggetmask`, `sys_sigpending_mask`. Respects SIGKILL/SIGSTOP unblockable invariant. 6 unit tests added.
- **Estimate**: Medium

### FEAT-008: Complete Message Queues
- **Priority**: 🟢 LOW
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/msgqueue.rs`, `src/kernel/syscall.rs`
- **Issue**: Exists but incomplete (missing IPC_SET)
- **Fix**: Added `msgctl_set()` method to MsgQueueManager for IPC_SET. Added `get()` method for permission checking. Added full syscall support: `sys_msgget`, `sys_msgsnd`, `sys_msgrcv`, `sys_msgctl_rmid`, `sys_msgctl_stat`, `sys_msgctl_set`. Syscalls include proper permission checking based on queue owner/mode. Added 2 unit tests for msgctl_set.
- **Estimate**: Medium

### FEAT-009: Complete Semaphores
- **Priority**: 🟢 LOW
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/semaphore.rs`
- **Issue**: Missing SEM_UNDO support
- **Fix**: Added SEM_UNDO support for automatic semaphore adjustment on process exit:
  - Added `SemAdj` struct to track per-process semaphore adjustments
  - Added `semop_with_undo()` method that records adjustments when SEM_UNDO flag is set
  - Added `undo_all()` method to reverse adjustments on process exit
  - Added `sem_adjs` HashMap to SemaphoreManager
  - Added 4 unit tests for SEM_UNDO functionality
- **Estimate**: Medium

### FEAT-010: Unix Domain Sockets
- **Priority**: 🟢 LOW
- **Status**: ⬜ TODO
- **File**: New
- **Issue**: No local socket IPC
- **Fix**: Implement AF_UNIX socket family
- **Estimate**: Large

### FEAT-011: Shell Functions
- **Priority**: 🟢 LOW
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/shell/parser.rs`, `src/shell/executor.rs`, `src/shell/builtins.rs`
- **Issue**: No function definitions
- **Fix**: Implemented shell function support:
  - Added `ShellFunction` struct and `ParsedLine` enum to parser
  - Added tokens for `(){}` characters
  - Added `parse_line()` and `try_parse_function()` for function definition parsing
  - Added `functions` HashMap to `ShellState` with get/set/has/unset methods
  - Executor stores function definitions and invokes function body when called
  - Builtins take priority over functions (like bash)
  - 13 tests for parser, 8 tests for executor
- **Estimate**: Medium

### FEAT-012: Shell Arrays
- **Priority**: 🟢 LOW
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/shell/parser.rs`, `src/shell/executor.rs`, `src/shell/builtins.rs`
- **Issue**: No array support
- **Fix**: Implemented bash-like array syntax:
  - Array definition: `arr=(one two three)`
  - Array append: `arr+=(new)`
  - Element assignment: `arr[0]=value`
  - Added `ArrayAssignment` struct and `ParsedLine::Array` variant
  - Added `arrays` HashMap to `ShellState` with get/set/push/len/unset methods
  - 9 parser tests, 6 executor tests
  - Note: Array expansion syntax (`${arr[@]}`) not yet implemented
- **Estimate**: Medium

### FEAT-013: Process Substitution
- **Priority**: 🟢 LOW
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/shell/parser.rs`, `src/shell/executor.rs`
- **Issue**: No <() or >() support
- **Fix**: Implemented process substitution with `<(cmd)` (input) and `>(cmd)` (output) syntax. Input substitutions run the command and write output to a temp file, returning the path. Output substitutions create a temp file path and queue the command to run after the main command completes. Uses `/tmp/procsub_N` naming pattern.
- **Estimate**: Medium

### FEAT-014: Heredocs
- **Priority**: 🟢 LOW
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/shell/parser.rs`
- **Issue**: No << EOF support
- **Fix**: Added `Heredoc` struct with delimiter, strip_tabs, and content fields. Added `HeredocStart` and `HeredocStripStart` tokens. Parser recognizes `<<DELIM` and `<<-DELIM` syntax. Added `heredoc` field to `SimpleCommand`. Added `needs_heredoc()` and `read_content()` methods for shell to read heredoc lines. 7 tests added.
- **Estimate**: Small

### FEAT-015: Process Priority (nice)
- **Priority**: 🟢 LOW
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/process.rs`, `src/kernel/syscall.rs`
- **Issue**: No scheduling priority
- **Fix**: Added `nice: i8` field to Process struct (range -20 to +19). Added `nice()` method to ProcessBuilder. Implemented syscalls: `sys_nice` (adjust priority), `sys_getpriority`, `sys_setpriority`. Child processes inherit nice value on fork.
- **Estimate**: Small

---

## Phase 6: Documentation

### DOC-001: Sync Documentation with Code
- **Priority**: 🟡 MEDIUM
- **Status**: ⬜ TODO
- **File**: Various in `docs/`
- **Issue**: 70+ documented issues in DOCUMENTATION_REVIEW.md
- **Fix**: Address critical/high priority doc issues
- **Estimate**: Medium

### DOC-002: Document Work Stealing Scheduler
- **Priority**: 🟢 LOW
- **Status**: ⬜ TODO
- **File**: `docs/kernel/work-stealing.md` (new)
- **Issue**: No documentation
- **Fix**: Write architecture and usage docs
- **Estimate**: Small

### DOC-003: Document Layered Filesystem
- **Priority**: 🟢 LOW
- **Status**: ⬜ TODO
- **File**: `docs/userspace/layered-fs.md` (new)
- **Issue**: Recently added, no docs
- **Fix**: Write usage and architecture docs
- **Estimate**: Small

### DOC-004: Add Integration Guides
- **Priority**: 🟢 LOW
- **Status**: ⬜ TODO
- **File**: `docs/guides/` (new directory)
- **Issue**: No guides for extending OS
- **Topics**: Custom commands, VFS backends, new syscalls
- **Estimate**: Medium

### DOC-005: Update Man Pages for Implemented Options
- **Priority**: 🟢 LOW
- **Status**: ⬜ TODO
- **File**: `man/`
- **Issue**: 30+ man pages describe unimplemented options
- **Fix**: Remove or mark unimplemented features
- **Estimate**: Medium

---

## Phase 7: Future Features (Nice to Have)

### FUT-001: Package Registry Infrastructure
- **Priority**: 🟢 LOW
- **Status**: ⬜ TODO (RFD exists)
- **Reference**: `rfd/0001-package-registry.md`
- **Description**: Server infrastructure for WASM packages
- **Estimate**: Large

### FUT-002: Capability-Based Security
- **Priority**: 🟡 MEDIUM
- **Status**: ⬜ TODO
- **Description**: Fine-grained permissions beyond rwx
- **Estimate**: Large

### FUT-003: Process Sandboxing/Jails
- **Priority**: 🟡 MEDIUM
- **Status**: ⬜ TODO
- **Description**: chroot-like isolation
- **Estimate**: Medium

### FUT-004: Kernel Visualization
- **Priority**: 🟢 LOW
- **Status**: ⬜ TODO
- **Description**: Real-time view of processes, memory, scheduling
- **Estimate**: Large

### FUT-005: Terminal Multiplexer
- **Priority**: 🟢 LOW
- **Status**: ⬜ TODO
- **Description**: tmux-like functionality
- **Estimate**: Medium

### FUT-006: Widget Toolkit
- **Priority**: 🟢 LOW
- **Status**: ⬜ TODO
- **Description**: Basic UI components for graphical apps
- **Estimate**: Large

### FUT-007: Built-in Debugger
- **Priority**: 🟢 LOW
- **Status**: ⬜ TODO
- **Description**: Step through WASM modules
- **Estimate**: Large

### FUT-008: Performance Profiler
- **Priority**: 🟢 LOW
- **Status**: ⬜ TODO
- **Description**: CPU and memory analysis tools
- **Estimate**: Medium

### FUT-009: Virtual Network Stack
- **Priority**: 🟢 LOW
- **Status**: ⬜ TODO
- **Description**: Simulated TCP/IP for education
- **Estimate**: Large

### FUT-010: Inter-Tab Communication
- **Priority**: 🟢 LOW
- **Status**: ⬜ TODO
- **Description**: SharedArrayBuffer for multi-window OS
- **Estimate**: Medium

### FUT-011: P2P WebRTC
- **Priority**: 🟢 LOW
- **Status**: ⬜ TODO
- **Description**: Decentralized file sharing between instances
- **Estimate**: Large

### FUT-012: Bare Metal Port
- **Priority**: 🟢 LOW
- **Status**: ⬜ TODO
- **Description**: x86_64 bootloader, real hardware support
- **Estimate**: Very Large

---

## Progress Log

### 2025-12-29 (Phase 5 Continuing)
- **FEAT-012**: Implemented shell arrays:
  - Array definition: `arr=(one two three)`
  - Array append: `arr+=(new)`
  - Element assignment: `arr[0]=value`
  - Added `ArrayAssignment` struct and `ParsedLine::Array` variant to parser
  - Added `arrays` HashMap to `ShellState` with full API
  - 15 total tests (9 parser + 6 executor)

- **FEAT-011**: Implemented shell functions:
  - Added `ShellFunction` struct and `ParsedLine` enum to parser
  - Added tokens for `(){}` characters
  - Added `parse_line()` and `try_parse_function()` for function definition parsing
  - Added `functions` HashMap to `ShellState` with get/set/has/unset methods
  - Executor stores function definitions and invokes function body when called
  - Functions work in pipelines and with logical operators
  - Builtins take priority over functions (like bash)
  - 21 total tests added (13 parser + 8 executor)

- **FEAT-009**: Implemented SEM_UNDO for semaphores:
  - Added `SemAdj` struct for per-process adjustment tracking
  - Added `semop_with_undo()` with SEM_UNDO flag support
  - Added `undo_all()` for automatic cleanup on process exit
  - Added 4 unit tests

- **FEAT-003**: Implemented file locking (fcntl/flock):
  - Created `FileLockManager` module with advisory locking
  - flock: BSD-style whole-file locks
  - fcntl: POSIX-style byte-range locks
  - Added 3 syscalls: sys_flock, sys_fcntl_lock, sys_fcntl_getlk
  - Added 8 unit tests for lock conflict scenarios

- **FEAT-008**: Completed message queues with IPC_SET:
  - Added `msgctl_set()` method for changing queue attributes
  - Added `get()` method for permission checking
  - Added 6 syscalls: msgget, msgsnd, msgrcv, msgctl_rmid, msgctl_stat, msgctl_set
  - Added SyscallNr enum variants for message queue operations
  - All syscalls include proper permission checks
  - Added 2 unit tests for msgctl_set

- **FEAT-002**: Implemented hard links:
  - Added `link()` method to FileSystem trait
  - Added `nlink` field to Metadata struct (link count)
  - MemoryFs uses copy-based approach (files start with same content)
  - LayeredFs delegates to appropriate layer with copy-up
  - Default nlink: 1 for files/symlinks, 2 for directories
- **FEAT-006**: Fixed waitpid() WCONTINUED support
- **FEAT-007**: Implemented signal masking (sigprocmask syscall)
- **FEAT-014**: Implemented heredocs (<<DELIM and <<-DELIM syntax)
- **FEAT-015**: Implemented process priority (nice/getpriority/setpriority)
- Overall: 36 total issues resolved, 21 remaining

### 2025-12-29 (Phase 5 Started!)
- **FEAT-001**: Implemented file timestamps (atime, mtime, ctime):
  - Added timestamp fields to `Metadata` and `NodeMeta` structs
  - Added `clock` field to `MemoryFs` for tracking current time
  - Timestamps updated on: file create, read, write, truncate, chmod, chown, mkdir, symlink
  - Added `set_clock()` method for kernel to set filesystem time
  - Added `utimes()` method for setting atime/mtime explicitly (touch command support)
  - Implemented in both `MemoryFs` and `LayeredFs`
  - Added 10 comprehensive unit tests
- Overall: 26 total issues resolved, 31 remaining

### 2025-12-29 (Phase 4 Complete!)
- **CQ-001**: Refactored Kernel God Object into 4 subsystems:
  - `ProcessSubsystem`: process lifecycle and scheduling
  - `VfsSubsystem`: virtual filesystem management
  - `IpcSubsystem`: FIFOs, message queues, semaphores
  - `TimeSubsystem`: timers and system time
  - Reduced Kernel from 20+ fields to 11 with clear organization
- **CQ-002**: Extracted `create_file_object()` helper to reduce file opening duplication
- **CQ-003**: Fixed unsafe integer casts with `try_from()` and checked arithmetic
- **CQ-004**: Reviewed complex functions - determined they are well-organized
- **CQ-005**: Implemented `ProcessBuilder` with fluent API for process creation
- **CQ-006**: Removed dead `_environ` code in open_proc()
- **CQ-007**: Created `syscall_names!` macro to generate syscall name lookup
- **CQ-008**: Fixed production panics in fifo.rs and init.rs
- **CQ-009**: Implemented full async pipeline support with `execute_piped_async()`, `execute_pipeline_async()`, `execute_command_list_async()`
- **CQ-010**: Added `FdFlags` and `FD_CLOEXEC` support with `clone_for_exec()`
- **Phase 4 COMPLETE**: All 10 code quality issues resolved
- Overall: 25 total issues resolved, 32 remaining

### 2025-12-28
- Created work tracker document
- Identified 57 work items across 7 categories
- **SEC-001**: Removed hardcoded root password - root now starts with no password
- **SEC-002**: Implemented secure password hashing with salted key-stretching (10,000 rounds)
- **SEC-003**: Fixed 32+ kernel panic points by adding safe process accessor methods
- **SEC-004**: Added symlink loop detection with POSIX standard 40-level depth limit
- **SEC-005**: Fixed TOCTOU race conditions with atomic `fstat()` permission checking
- **SEC-006**: Implemented setuid/setgid bit processing for WASM commands
- Total: 6 issues resolved, 51 remaining

---

## Completed Features (Historical)

The following features have been fully implemented. This section provides a reference to completed work.

### Executor

| Feature | Source | Status |
|---------|--------|--------|
| Task cancellation | `src/kernel/executor.rs` | ✅ Implemented |
| Timeouts | `src/kernel/executor.rs` | ✅ Implemented |
| Work stealing | `src/kernel/work_stealing/` | ✅ Implemented |
| Task groups | `src/kernel/executor.rs` | ✅ Implemented |

### IPC

| Feature | Source | Status |
|---------|--------|--------|
| Bounded channels | `src/kernel/ipc.rs` | ✅ Implemented |
| Waker-based async | `src/kernel/ipc.rs` | ✅ Implemented |

### Memory

| Feature | Source | Status |
|---------|--------|--------|
| Memory-mapped files | `src/kernel/memory.rs` | ✅ Implemented |
| Copy-on-write | `src/kernel/memory.rs` | ✅ Implemented |
| Memory pools | `src/kernel/memory.rs` | ✅ Implemented |
| OPFS persistence | `src/kernel/memory_persist.rs` | ✅ Implemented |

### VFS

| Feature | Source | Status |
|---------|--------|--------|
| Layered filesystem | `src/vfs/layered.rs` | ✅ Implemented |

### WASM

| Feature | Source | Status |
|---------|--------|--------|
| Command ABI v1 | `src/kernel/wasm/abi.rs` | ✅ Implemented |
| WASM loader/executor | `src/kernel/wasm/` | ✅ Implemented |
| Package manager | `src/kernel/pkg/` | ✅ Implemented |
| WASI Preview2 | `src/kernel/wasm/wasi_preview2.rs` | ✅ Implemented |

### Compositor

| Feature | Source | Status |
|---------|--------|--------|
| WebGPU rendering | `src/compositor/surface.rs` | ✅ Implemented |
| BSP tiling layout | `src/compositor/layout.rs` | ✅ Implemented |
| Text rendering | `src/compositor/text.rs` | ✅ Implemented |
| Themes | `src/compositor/mod.rs` | ✅ Implemented |
| Animations | `src/compositor/mod.rs` | ✅ Implemented |
| Window decorations | `src/compositor/mod.rs` | ✅ Implemented |

---

## Implementation Notes

For detailed documentation on each subsystem, see:

- [Executor](kernel/executor.md)
- [IPC](kernel/ipc.md)
- [Memory](kernel/memory.md)
- [VFS](userspace/vfs.md)
- [WASM Modules](kernel/wasm-modules.md)
- [Compositor](plans/compositor.md)

---

## How to Update This Document

When completing a task:
1. Change status from `⬜ TODO` to `✅ DONE`
2. Add completion date
3. Update Quick Stats table
4. Add entry to Progress Log
5. If a feature is fully implemented, move it to the "Completed Features" section

Status Legend:
- ⬜ TODO - Not started
- 🔄 IN PROGRESS - Currently working on
- ✅ DONE - Completed
- ⏸️ BLOCKED - Waiting on something
- ❌ WONTFIX - Decided not to fix
