# Standard I/O

Every process has standard input, output, and error streams.

## Standard File Descriptors

```rust
impl Fd {
    pub const STDIN: Fd = Fd(0);   // Standard input
    pub const STDOUT: Fd = Fd(1);  // Standard output
    pub const STDERR: Fd = Fd(2);  // Standard error
}
```

These are automatically set up when a process is created.

## Console

By default, all three stdio streams point to the system console:

```
Process                    Kernel
┌─────────────────┐       ┌──────────────────┐
│ Fd(0) STDIN  ───┼──────►│                  │
│ Fd(1) STDOUT ───┼──────►│  ConsoleObject   │
│ Fd(2) STDERR ───┼──────►│                  │
└─────────────────┘       └──────────────────┘
```

### Console Structure

```rust
pub struct ConsoleObject {
    input: VecDeque<u8>,
    output: Vec<u8>,
}

impl ConsoleObject {
    pub fn push_input(&mut self, data: &[u8]);
    pub fn take_output(&mut self) -> Vec<u8>;
    pub fn peek_output(&self) -> &[u8];
    pub fn clear_input(&mut self);
    pub fn clear_output(&mut self);
}
```

### Writing to Console

```rust
// Write to stdout
syscall::write(Fd::STDOUT, b"Hello, world!")?;

// Write to stderr
syscall::write(Fd::STDERR, b"Error: something went wrong")?;
```

Output goes to the console's output buffer, which is rendered by the compositor.

### Reading from Console

```rust
let mut buf = [0u8; 256];
let n = syscall::read(Fd::STDIN, &mut buf)?;
let input = String::from_utf8_lossy(&buf[..n]);
```

Reads from the console's input buffer, which is populated by keyboard events.

## Console API

### Pushing Input

The runtime pushes keyboard input to the console:

```rust
// Called from keyboard event handler
syscall::console_push_input(b"hello");
```

### Taking Output

The compositor takes output for display:

```rust
let output = syscall::console_take_output();
// Render output to terminal window
```

## Pipes

Processes can create pipes for IPC:

```rust
let (read_fd, write_fd) = syscall::pipe()?;

// Writer
syscall::write(write_fd, b"data")?;

// Reader
let mut buf = [0u8; 100];
let n = syscall::read(read_fd, &mut buf)?;
```

## Redirecting I/O

### To a File

```rust
// Close stdout
syscall::close(Fd::STDOUT)?;

// Open file as fd 1 (stdout)
let fd = syscall::open("/tmp/output.log", OpenFlags::WRITE)?;
assert_eq!(fd, Fd::STDOUT); // First available fd

// Now writes go to file
syscall::write(Fd::STDOUT, b"This goes to file")?;
```

### To a Pipe

```rust
let (read_fd, write_fd) = syscall::pipe()?;

// Child process writes to pipe
// (once fork is implemented)
syscall::write(write_fd, b"from child")?;

// Parent reads from pipe
let mut buf = [0u8; 100];
syscall::read(read_fd, &mut buf)?;
```

## File Descriptor Duplication

```rust
// Dup stdout
let backup = syscall::dup(Fd::STDOUT)?;

// Redirect stdout to file
syscall::close(Fd::STDOUT)?;
let fd = syscall::open("/tmp/log", OpenFlags::WRITE)?;

// Do some work...
syscall::write(Fd::STDOUT, b"to file")?;

// Restore stdout
syscall::close(Fd::STDOUT)?;
// (Would need dup2 to restore to exact fd)
```

## Shared Stdio

When processes share the console:

```
Process A                  Kernel
┌─────────────────┐       ┌───────────────────┐
│ Fd(1) STDOUT ───┼──────►│                   │
└─────────────────┘       │   ConsoleObject   │
                          │   refcount = 6    │
Process B                 │                   │
┌─────────────────┐       │  (A: 3 fds)       │
│ Fd(1) STDOUT ───┼──────►│  (B: 3 fds)       │
└─────────────────┘       └───────────────────┘
```

Output from both processes goes to the same console.

## Best Practices

1. **Check errors**: I/O can fail
2. **Buffer appropriately**: Don't read one byte at a time
3. **Close when done**: Release file descriptors
4. **Use stderr for errors**: Keep stdout clean for data

## Example: Simple Echo

```rust
kernel::spawn(async {
    let mut buf = [0u8; 256];

    loop {
        match syscall::read(Fd::STDIN, &mut buf) {
            Ok(0) => break, // EOF
            Ok(n) => {
                syscall::write(Fd::STDOUT, &buf[..n])?;
            }
            Err(SyscallError::WouldBlock) => {
                futures::pending!();
            }
            Err(e) => {
                let msg = format!("Error: {}\n", e);
                syscall::write(Fd::STDERR, msg.as_bytes())?;
                break;
            }
        }
    }
});
```

## Related Documentation

- [Syscall Interface](../kernel/syscalls.md) - I/O syscalls
- [Kernel Objects](../kernel/objects.md) - Console object
- [IPC](../kernel/ipc.md) - Pipes
