# AxebergOS Work Tracker

**Created**: 2025-12-28
**Last Updated**: 2025-12-31

This document tracks all identified issues, improvements, and feature work for AxebergOS.

---

## Implementation Notes

For detailed documentation on each subsystem, see:

- [Executor](kernel/executor.md)
- [IPC](kernel/ipc.md)
- [Memory](kernel/memory.md)
- [VFS](userspace/vfs.md)
- [WASM Modules](kernel/wasm-modules.md)
- [Compositor](userspace/compositor.md)

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

---

## Future Features (Nice to Have)

### FUT-001: Package Registry Infrastructure
- **Priority**: 🟢 LOW
- **Status**: ⬜ TODO (RFD exists)
- **Reference**: `rfd/0001-package-registry.md`
- **Description**: Server infrastructure for WASM packages
- **Estimate**: Large

### FUT-002: Capability-Based Security
- **Priority**: 🟡 MEDIUM
- **Status**: ✅ DONE (2025-12-30)
- **Files**: `src/kernel/users.rs`, `src/kernel/process.rs`, `src/kernel/syscall.rs`
- **Description**: Fine-grained permissions beyond rwx
- **Fix**: Implemented Linux-style POSIX capabilities:
  - Added `Capability` enum with 24 capabilities (DAC_OVERRIDE, SETUID, KILL, SYS_ADMIN, etc.)
  - Added `CapabilitySet` bitfield for efficient capability storage
  - Added `ProcessCapabilities` with permitted/effective/inheritable sets
  - Added `capabilities` field to Process struct, initialized based on UID
  - Syscalls: `capget`, `capset`, `cap_raise`, `cap_lower`, `cap_drop`, `cap_check`
  - Integrated into permission checks:
    - `check_permission_with_caps()` for DAC_OVERRIDE, DAC_READ_SEARCH, FOWNER
    - `sys_setuid/seteuid` check CAP_SETUID
    - `sys_setgid/setegid/setgroups` check CAP_SETGID
    - `sys_kill` checks CAP_KILL
    - `sys_setrlimit` checks CAP_SYS_RESOURCE
  - Capability inheritance: fork copies capabilities, exec transforms based on UID
  - 20+ unit tests for capability operations
- **Estimate**: Large

### FUT-003: Process Sandboxing/Jails
- **Priority**: 🟡 MEDIUM
- **Status**: ✅ DONE (2025-12-30)
- **Files**: `src/kernel/process.rs`, `src/kernel/syscall.rs`
- **Description**: chroot-like isolation
- **Fix**: Implemented chroot-based process jails:
  - Added `jail_root: Option<PathBuf>` field to Process struct
  - Added `resolve_jailed_path()` method for jail-aware path resolution
  - Added `canonicalize_path()` helper to safely handle ".." traversal
  - Added `is_jailed()`, `get_jail_root()`, `set_jail_root()` methods
  - Added `sys_chroot` syscall requiring CAP_SYS_CHROOT capability
  - Updated `resolve_path()` in syscall.rs to use jail-aware resolution
  - Jail is inherited by child processes on fork
  - ProcessBuilder supports `.jail_root()` method
  - 17 unit tests for path resolution and syscall
- **Estimate**: Medium

### FUT-004: Kernel Visualization
- **Priority**: 🟢 LOW
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/visualizer.rs`
- **Description**: Real-time view of processes, memory, scheduling
- **Fix**: Implemented comprehensive kernel visualization module:
  - `ProcessTree` with parent-child relationships and ASCII rendering
  - `SystemMemoryView` with bar charts and detailed stats
  - `ProcessMemoryLayout` with memory region types and hex dump
  - `SchedulerView` showing task queues by priority level
  - `ResourceDashboard` with CPU, memory, FD, I/O metrics
  - `SyscallMonitor` with activity log and frequency analysis
  - `KernelSnapshot` for complete system state capture
  - 8 unit tests
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
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/debugger.rs`
- **Description**: Step through WASM modules
- **Fix**: Implemented syscall-level WASM debugger:
  - `WasmDebugger` with enable/disable and debug target selection
  - Breakpoints with conditions (pid, argument values, hit count)
  - Memory watches (access, write, read, change detection)
  - Step modes: step, step-over, step-out, continue
  - Execution history with syscall arguments and results
  - Memory view with hex dump and value parsing
  - Syscall argument interpretation (fd, path, flags, pointers)
  - Status rendering and breakpoint list display
  - 13 unit tests
- **Estimate**: Large

### FUT-008: Performance Profiler
- **Priority**: 🟢 LOW
- **Status**: ✅ DONE (2025-12-29)
- **File**: `src/kernel/profiler.rs`
- **Description**: CPU and memory analysis tools
- **Fix**: Implemented comprehensive performance profiler:
  - `CpuProfile` with task sampling and CPU time tracking
  - `SyscallProfile` with timing histograms and call rates
  - `MemoryProfile` with snapshots and allocation tracking
  - `FlameGraphBuilder` for hierarchical CPU visualization
  - Profiler state machine (stopped/recording/paused)
  - Per-process and per-task CPU percentage
  - Allocation size distribution analysis
  - COW fault rate tracking
  - JSON export for external tools
  - Collapsed stack format for flame graph generation
  - 10 unit tests
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

### 2025-12-30 (Documentation Complete)
- **DOC-001**: Synced all documentation with code:
  - README.md: Complete rewrite with accurate architecture, features, stats
  - docs/index.md: Updated test counts (~1,000+), added execution flow diagram
  - kernel/overview.md: Updated with subsystem structure, capabilities, jails
  - userspace/vfs.md: Added timestamps (atime/mtime/ctime), nlink field
  - userspace/shell.md: Added functions, arrays, heredocs, process substitution
  - development/building.md: Updated project structure with all modules
  - development/testing.md: Updated test count

- **DOC-002**: Created `docs/kernel/work-stealing.md`:
  - Architecture diagram with injector and worker deques
  - Chase-Lev algorithm explanation with LIFO/FIFO semantics
  - Formal properties (TLA+ verified): no lost tasks, no double execution
  - Lock-free guarantees and memory ordering details

- **DOC-003**: Created `docs/userspace/layered-fs.md`:
  - Union mount semantics with ASCII diagrams
  - Read/write/delete operation flow
  - Whiteout markers (.wh.) and opaque directories
  - API reference and use cases (containers, sandboxing)

- **DOC-004**: Created `docs/guides/` directory with three guides:
  - `custom-commands.md`: Builtins, programs, WASM modules
  - `vfs-backends.md`: FileSystem trait implementation
  - `adding-syscalls.md`: Kernel extension guide

- **DOC-005**: Verified man pages already fixed (unimplemented options marked)

- Overall: 50 total issues resolved, 7 remaining (all future features)

### 2025-12-30 (FUT-002 and FUT-003 Complete)
- **FUT-002**: Implemented Capability-Based Security:
  - Added `Capability` enum with 24 Linux-style capabilities (CAP_DAC_OVERRIDE, CAP_SETUID, CAP_KILL, CAP_SYS_ADMIN, etc.)
  - Added `CapabilitySet` as u32 bitfield with set operations (union, intersection, difference, subset)
  - Added `ProcessCapabilities` with permitted/effective/inheritable sets per process
  - Added `capabilities` field to Process struct, auto-initialized based on UID (root gets all, others get none)
  - Added syscalls: `capget` (get process caps), `capset` (set caps), `cap_raise`, `cap_lower`, `cap_drop`, `cap_check`
  - Integrated into existing syscalls:
    - `sys_setuid/seteuid` now check CAP_SETUID instead of just root
    - `sys_setgid/setegid/setgroups` check CAP_SETGID
    - `sys_kill` checks CAP_KILL for signaling other processes
    - `sys_setrlimit` checks CAP_SYS_RESOURCE for raising hard limits
  - Added `check_permission_with_caps()` for capability-aware file permission checks
  - Capability inheritance: fork copies capabilities unchanged, exec transforms based on UID
  - 20+ comprehensive unit tests

- **FUT-003**: Implemented Process Sandboxing/Jails (chroot):
  - Added `jail_root: Option<PathBuf>` field to Process struct for jail containment
  - Added `resolve_jailed_path()` method with jail-aware path resolution
  - Added `canonicalize_path()` helper to safely remove ".." traversal attempts
  - Added `is_jailed()`, `get_jail_root()`, `set_jail_root()` process methods
  - Added `sys_chroot` syscall requiring CAP_SYS_CHROOT capability
  - Added `Chroot` to SyscallNr enum (syscall 314)
  - Updated `resolve_path()` in syscall.rs to use jail-aware resolution
  - All VFS operations (open, mkdir, stat, etc.) now automatically respect jail boundaries
  - Jail is inherited by child processes on fork
  - ProcessBuilder supports `.jail_root()` method for programmatic jail setup
  - 17 unit tests (12 for path resolution, 5 for syscall)

- Overall: 45 total issues resolved, 12 remaining (5 docs, 7 future features)

### 2025-12-29 (Phase 7 - Future Features)
- **FUT-004**: Implemented Kernel Visualization (`src/kernel/visualizer.rs`):
  - `ProcessTree` with parent-child relationships and ASCII tree rendering
  - `SystemMemoryView` with bar charts and detailed memory stats
  - `ProcessMemoryLayout` with memory region types and hex dump view
  - `SchedulerView` showing task queues organized by priority level
  - `ResourceDashboard` with CPU, memory, FD, I/O metrics in ASCII UI
  - `SyscallMonitor` with activity log and frequency table
  - `KernelSnapshot` combining all views for complete system state
  - 8 unit tests

- **FUT-007**: Implemented Built-in WASM Debugger (`src/kernel/debugger.rs`):
  - `WasmDebugger` with syscall-level debugging (practical without bytecode manipulation)
  - Breakpoints with conditions: pid filter, argument value checks, hit counts
  - Memory watches: access, write, read, and change detection types
  - Step modes: step (next syscall), step-over, step-out, continue
  - Execution history tracking with syscall arguments and results
  - `MemoryView` with hex dump rendering and value parsing (u8/u16/u32/cstring)
  - Syscall argument interpretation (fd, path, flags, pointers, sizes)
  - Status and breakpoint list rendering for UI integration
  - 13 unit tests

- **FUT-008**: Implemented Performance Profiler (`src/kernel/profiler.rs`):
  - `CpuProfile` with task sampling and per-process/task CPU time tracking
  - `SyscallProfile` with timing histograms (0-1ms, 1-10ms, 10-100ms, 100ms+)
  - `MemoryProfile` with periodic snapshots and allocation event tracking
  - `FlameGraphBuilder` for hierarchical CPU visualization with collapsed stack export
  - Profiler state machine: stopped, recording, paused with duration tracking
  - Allocation size distribution analysis (histogram buckets)
  - COW fault rate tracking and memory pressure indicators
  - JSON export format for external tools
  - 10 unit tests

- Overall: 43 total issues resolved, 14 remaining (5 docs, 9 future features)

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
