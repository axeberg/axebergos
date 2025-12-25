# Contributing to axeberg

Guidelines for contributing to the axeberg project.

## Philosophy

axeberg follows these principles:

1. **Tractable**: Code should be understandable by one person
2. **Simple**: Prefer simple solutions over clever ones
3. **Complete**: Features should be fully implemented, not partial
4. **Tested**: All code needs tests

## Getting Started

1. Clone the repository
2. Build and run tests: `cargo test`
3. Build WASM: `wasm-pack build --target web`
4. Run dev server: `cargo run --bin serve`

## Code Style

### Formatting

Always run before committing:

```bash
cargo fmt
```

### Linting

Check for issues:

```bash
cargo clippy
```

### Documentation

- Public APIs should have doc comments
- Complex logic should have inline comments
- Update docs/ for significant changes

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

## Commit Messages

Use conventional commit format:

```
type: brief description

Longer explanation if needed.
- Bullet points for multiple changes
- Reference issues: Fixes #123
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation
- `test`: Tests
- `refactor`: Code restructuring
- `perf`: Performance improvement

Examples:
```
feat: Add shared memory syscalls

Implement shmget, shmat, shmdt for inter-process
shared memory communication.
- Add MemoryManager for shared segments
- Add ProcessMemory tracking
- 17 new tests
```

```
fix: Handle empty file reads correctly

Files with zero size were returning errors instead
of empty buffers. Now correctly returns 0 bytes read.
```

## Pull Requests

### Before Submitting

1. Run all tests: `cargo test`
2. Check formatting: `cargo fmt --check`
3. Run clippy: `cargo clippy`
4. Update documentation if needed
5. Add tests for new code

### PR Description

Include:
- What the change does
- Why it's needed
- How to test it
- Any breaking changes

### Review Process

- PRs need review before merging
- Address feedback promptly
- Keep PRs focused (one feature/fix per PR)

## Architecture Guidelines

### Kernel Code

- All resource access through syscalls
- Validate all inputs
- Use reference counting for shared objects
- Handle errors explicitly (no panics)

### New Syscalls

1. Add method to `Kernel` struct
2. Add public wrapper function
3. Update syscall documentation
4. Add comprehensive tests

```rust
// In Kernel impl
pub fn sys_new_call(&mut self, arg: Type) -> SyscallResult<Result> {
    let current = self.current.ok_or(SyscallError::NoProcess)?;
    // Implementation...
}

// Public wrapper
pub fn new_call(arg: Type) -> SyscallResult<Result> {
    KERNEL.with(|k| k.borrow_mut().sys_new_call(arg))
}
```

### Memory Safety

- Prefer owned types over references where practical
- Use `RefCell` for interior mutability (single-threaded)
- Validate buffer sizes before operations
- Handle all error cases

### Testing Requirements

New code must have tests:

```rust
#[test]
fn test_feature_basic() {
    // Happy path
}

#[test]
fn test_feature_error_case() {
    // Error handling
}

#[test]
fn test_feature_edge_case() {
    // Boundary conditions
}
```

## Areas for Contribution

### Good First Issues

- Documentation improvements
- Additional test coverage
- Code cleanup/refactoring
- Small feature additions

### Larger Projects

- OPFS backend for VFS
- WebGPU compositor
- Shell/terminal application
- Process signals
- Memory-mapped files

## Questions?

- Check existing documentation
- Look at similar code in the codebase
- Open an issue for discussion

## Code of Conduct

Be respectful and constructive. We're building something together.

## License

Contributions are licensed under MIT.
