# Testing

axeberg has a comprehensive test suite ensuring kernel reliability.

## Test Overview

**674 tests (646 unit tests + 28 integration tests)** across all major components:

| Component | Tests | Coverage |
|-----------|-------|----------|
| Executor | 7 | Scheduling, priorities, waking |
| IPC | 6 | Channels, send/receive |
| Memory | 17 | Regions, limits, shared memory |
| Objects | 9 | Table, refcounts |
| Process | 5 | Creation, file tables |
| Syscall | 17 | All syscall operations |
| VFS | 17 | Files, directories, paths |

## Running Tests

### All Tests

```bash
cargo test
```

### Filtered Tests

```bash
# By module
cargo test kernel::syscall

# By name pattern
cargo test test_mem

# Single test
cargo test test_shm_basic
```

### With Output

```bash
cargo test -- --nocapture
```

### Verbose

```bash
cargo test -- --nocapture --test-threads=1
```

## Test Structure

Tests are in `#[cfg(test)]` modules alongside code:

```rust
// In src/kernel/syscall.rs

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_kernel() {
        KERNEL.with(|k| {
            *k.borrow_mut() = Kernel::new();
            let pid = k.borrow_mut().spawn_process("test", None);
            k.borrow_mut().set_current(pid);
        });
    }

    #[test]
    fn test_open_console() {
        setup_test_kernel();
        let fd = open("/dev/console", OpenFlags::RDWR).unwrap();
        assert!(fd.0 >= 3);
    }
}
```

## Key Test Categories

### Executor Tests

```rust
#[test]
fn test_priority_order() {
    // Verifies Critical > Normal > Background
}

#[test]
fn test_task_runs_to_completion() {
    // Task executes and completes
}

#[test]
fn test_tick_without_wake_leaves_task_pending() {
    // Blocked tasks stay blocked
}
```

### Memory Tests

```rust
#[test]
fn test_mem_alloc_free() {
    // Allocate, verify stats, free
}

#[test]
fn test_mem_limit() {
    // Limit enforcement
}

#[test]
fn test_shm_basic() {
    // Shared memory lifecycle
}

#[test]
fn test_mem_protection() {
    // Read-only enforcement
}
```

### Syscall Tests

```rust
#[test]
fn test_file_write_read() {
    // Write and read back file
}

#[test]
fn test_dup() {
    // File descriptor duplication
}

#[test]
fn test_refcount_with_stdio() {
    // Stdio sharing
}
```

### VFS Tests

```rust
#[test]
fn test_nested_directories() {
    // Deep path handling
}

#[test]
fn test_path_normalization() {
    // Path canonicalization
}

#[test]
fn test_seek() {
    // File positioning
}
```

## Test Helpers

### Kernel Setup

Most syscall tests need a kernel:

```rust
fn setup_test_kernel() {
    KERNEL.with(|k| {
        *k.borrow_mut() = Kernel::new();
        let pid = k.borrow_mut().spawn_process("test", None);
        k.borrow_mut().set_current(pid);
    });
}
```

### Executor Setup

For executor tests:

```rust
let mut executor = Executor::new();
let results = Rc::new(RefCell::new(Vec::new()));
// Spawn tasks...
executor.run();
// Check results
```

## Testing Async Code

Use the executor's `run()` method:

```rust
#[test]
fn test_async_task() {
    let mut executor = Executor::new();
    let completed = Rc::new(Cell::new(false));

    let c = completed.clone();
    executor.spawn(async move {
        // Async work...
        c.set(true);
    });

    executor.run();
    assert!(completed.get());
}
```

## Testing Reference Counting

Verify object lifecycle:

```rust
#[test]
fn test_refcount_shared_handle() {
    let mut table = ObjectTable::new();
    let h = table.insert(KernelObject::Pipe(PipeObject::new(1024)));

    // Simulate sharing
    table.retain(h);
    assert_eq!(table.refcount(h), 2);

    // First release
    table.release(h);
    assert!(table.get(h).is_some());

    // Final release
    table.release(h);
    assert!(table.get(h).is_none());
}
```

## Common Assertions

```rust
// Result checks
assert!(result.is_ok());
assert!(result.is_err());

// Equality
assert_eq!(actual, expected);
assert_ne!(a, b);

// Collections
assert!(vec.contains(&item));
assert_eq!(vec.len(), 3);

// Error types
assert_eq!(result, Err(SyscallError::NotFound));
```

## Debugging Failed Tests

### Get more info

```bash
RUST_BACKTRACE=1 cargo test test_name -- --nocapture
```

### Run single-threaded

```bash
cargo test -- --test-threads=1
```

### Print intermediate state

```rust
#[test]
fn test_debug() {
    setup_test_kernel();
    let fd = open("/tmp/test", OpenFlags::WRITE).unwrap();
    println!("fd = {:?}", fd);  // --nocapture to see
}
```

## Adding New Tests

1. Find the appropriate module
2. Add test function with `#[test]`
3. Use setup helpers if needed
4. Make assertions
5. Run and verify

```rust
#[test]
fn test_new_feature() {
    setup_test_kernel();

    // Arrange
    let shm = shmget(1024).unwrap();

    // Act
    let region = shmat(shm, Protection::READ_WRITE).unwrap();

    // Assert
    assert!(region.0 > 0);

    // Cleanup
    shmdt(shm).unwrap();
}
```

## Test Coverage Goals

- All public APIs should have tests
- Error paths should be tested
- Edge cases (empty, max values)
- Integration between components

## Related Documentation

- [Building](building.md) - Build and run tests
- [Contributing](contributing.md) - Test requirements for PRs
