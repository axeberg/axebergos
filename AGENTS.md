# Agent Guidelines for AxebergOS

This document provides instructions for LLM agents (Claude, GPT, etc.) working on this codebase.

## Pre-Commit Checklist

**Before every commit, run these commands in order:**

```bash
cargo fmt
cargo clippy --lib -- -D warnings
cargo test --lib
```

Only proceed with `git add` and `git commit` if all three pass.

## Code Standards

### Rust
- All code must pass `cargo fmt` formatting
- All code must pass `cargo clippy` with warnings as errors
- All tests must pass
- No `.unwrap()` in production code (tests are acceptable)
- Use `?` operator for error propagation

### Commits
- Run the pre-commit checklist before every commit
- Write clear commit messages describing what changed and why
- Don't batch unrelated changes into single commits

### Pull Requests
- Ensure CI will pass before pushing (run the pre-commit checklist)
- Update relevant documentation if behavior changes
- Update `docs/WORK_TRACKER.md` if completing tracked items

## Project Structure

See `docs/PROJECT_ANALYSIS.md` for full codebase analysis.

Key directories:
- `src/kernel/` - Kernel subsystems (syscalls, processes, memory, IPC)
- `src/shell/` - Shell and built-in commands
- `src/vfs/` - Virtual filesystem
- `src/compositor/` - WebGPU compositor
- `docs/` - Documentation and tracking

## Current Work

See `docs/WORK_TRACKER.md` for:
- Known issues and their status
- Priority order for fixes
- Progress log

## Testing

```bash
# Run all tests
cargo test --lib

# Run specific module tests
cargo test --lib syscall::
cargo test --lib vfs::memory::

# Run with output
cargo test --lib -- --nocapture
```

## Build Targets

```bash
# Native (for testing)
cargo build

# WASM (production)
wasm-pack build --target web --release

# Type check only
cargo check --lib --target wasm32-unknown-unknown
```
