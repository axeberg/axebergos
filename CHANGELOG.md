# Changelog

All notable changes to axeberg are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `ARCHITECTURE.md` with visual system diagrams
- `examples/` directory with learning tutorials
- `docs/decisions/` with Architecture Decision Records (ADRs)
- This CHANGELOG

### Changed
- Upgraded `getrandom` from 0.2 to 0.3 (breaking: `js` feature renamed to `wasm_js`)

## [0.1.0] - 2024-12-26

Initial release of axeberg - a mini-OS in Rust running in WebAssembly.

### Core Kernel
- **Process Management**: Process creation, lifecycle, file descriptor tables
- **Memory Management**: Per-process allocation limits with peak tracking
- **Async Executor**: Priority-based scheduler (Critical > Normal > Background)
- **Object Table**: Reference-counted kernel object handles
- **Timers**: Async-aware timer queue with priority scheduling

### Multi-User System
- **User Database**: `/etc/passwd`, `/etc/shadow`, `/etc/group` persistence
- **Sessions**: Login/logout with per-session process isolation
- **Permissions**: Unix-style rwx model with user/group enforcement
- **Commands**: `login`, `logout`, `useradd`, `passwd`, `groupadd`, `sudo`, `su`

### Virtual Filesystem
- **MemoryFS**: In-memory filesystem with full directory operations
- **procfs**: `/proc` with process information
- **devfs**: `/dev` with null, zero, random, urandom, tty
- **sysfs**: `/sys` with kernel information
- **Symlinks**: Full symbolic and hard link support
- **OPFS Persistence**: Optional save/restore to Origin Private File System

### IPC (Inter-Process Communication)
- **Pipes**: Anonymous and named pipes (`mkfifo`)
- **Message Queues**: POSIX-style message passing
- **Shared Memory**: Protected memory regions (read/write/execute)
- **Semaphores**: Counting semaphores for synchronization

### Shell
- **Command Execution**: Built-in and external commands
- **Pipes**: `cmd1 | cmd2 | cmd3`
- **Redirects**: `>`, `>>`, `<`, `2>`
- **Background Jobs**: `command &` with job control (`fg`, `bg`, `jobs`)
- **Quoting**: Single and double quote support

### Commands (96 programs + 12 builtins)
- **File Operations**: `cat`, `ls`, `mkdir`, `touch`, `rm`, `cp`, `mv`, `ln`, `find`, `tree`
- **Text Processing**: `grep`, `sort`, `uniq`, `cut`, `tr`, `diff`, `head`, `tail`, `wc`
- **User Management**: `login`, `logout`, `useradd`, `passwd`, `id`, `whoami`, `sudo`, `su`
- **System**: `ps`, `kill`, `uptime`, `free`, `df`, `du`, `uname`, `date`
- **Networking**: `curl`, `wget`
- **Package Manager**: `pkg install`, `pkg list`, `pkg remove`

### Signals
- POSIX-like signal delivery with coalescing
- Supported: SIGTERM, SIGKILL, SIGINT, SIGSTOP, SIGCONT, SIGHUP, SIGUSR1/2

### Documentation
- Full kernel documentation in `docs/kernel/`
- Shell guide in `docs/userspace/shell.md`
- Building and testing guides in `docs/development/`
- 61 Unix-style man pages

### Testing
- 646 unit tests across all kernel modules
- 28 integration tests
- CI/CD with format, lint, type-check, test, and WASM build

### Formal Specifications (TLA+)
- Process state machine
- Signal delivery semantics
- Timer queue correctness
- History buffer behavior
- Path validation rules

---

## Version History Summary

| Version | Date | Highlights |
|---------|------|------------|
| 0.1.0 | 2024-12-26 | Initial release with full kernel, shell, multi-user |

## Migration Notes

### From pre-0.1.0
This is the initial release. No migration needed.

### getrandom 0.2 → 0.3
If you have code depending on axeberg's random functions:
```rust
// Old (0.2)
getrandom::getrandom(&mut buf)
// New (0.3)
getrandom::fill(&mut buf)
```
