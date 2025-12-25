# Kernel Objects

Everything in the kernel is an object. Files, pipes, the console, windows - all are kernel objects accessed through handles.

## Object Types

```rust
pub enum KernelObject {
    File(FileObject),       // Regular file
    Pipe(PipeObject),       // IPC pipe
    Console(ConsoleObject), // System console
    Window(WindowObject),   // GUI window
    Directory(DirectoryObject), // Directory
}
```

### FileObject

Represents an open file:

```rust
pub struct FileObject {
    pub path: PathBuf,
    pub data: Vec<u8>,
    pub position: usize,
    pub readable: bool,
    pub writable: bool,
}
```

Operations:
- `read()` - Read bytes from current position
- `write()` - Write bytes at current position
- `seek()` - Move position within file

### PipeObject

Unidirectional data channel:

```rust
pub struct PipeObject {
    buffer: VecDeque<u8>,
    capacity: usize,
    closed: bool,
}
```

Operations:
- `read()` - Read available data (may block)
- `write()` - Write data (respects capacity)
- `close()` - Close the pipe

### ConsoleObject

System console for I/O:

```rust
pub struct ConsoleObject {
    input_buffer: VecDeque<u8>,
    output_buffer: Vec<u8>,
}
```

Operations:
- `read()` - Read from input buffer
- `write()` - Write to output buffer
- `push_input()` - Push keyboard input
- `take_output()` - Take output for display

### WindowObject

GUI window handle:

```rust
pub struct WindowObject {
    window_id: WindowId,
}
```

Used to associate processes with compositor windows.

### DirectoryObject

Directory listing handle:

```rust
pub struct DirectoryObject {
    entries: Vec<DirEntry>,
    position: usize,
}
```

## Handles

A handle is a reference to an object:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handle(pub u64);

impl Handle {
    pub const NULL: Handle = Handle(0);
}
```

Handles are:
- Unique across the system
- Never reused
- Opaque to user code

## Object Table

The kernel maintains a global object table:

```rust
pub struct ObjectTable {
    next_id: u64,
    objects: HashMap<Handle, ObjectEntry>,
}

struct ObjectEntry {
    object: KernelObject,
    refcount: usize,
}
```

## Reference Counting

Objects use reference counting for lifetime management:

### Creating Objects

```rust
// Insert with refcount = 1
let handle = table.insert(KernelObject::File(file));
assert_eq!(table.refcount(handle), 1);
```

### Sharing Objects

```rust
// Retain increments refcount
table.retain(handle);
assert_eq!(table.refcount(handle), 2);
```

### Releasing Objects

```rust
// Release decrements refcount
table.release(handle);
assert_eq!(table.refcount(handle), 1);

// When refcount hits 0, object is removed
table.release(handle);
assert_eq!(table.refcount(handle), 0);
assert!(table.get(handle).is_none());
```

## Object Lifecycle

1. **Creation**: Object inserted into table with refcount 1
2. **Sharing**: `retain()` increments refcount for each new reference
3. **Access**: Object accessed via handle through syscalls
4. **Cleanup**: `release()` on close, freed when refcount = 0

### Example: Stdio Sharing

When a process is spawned, stdio is shared:

```rust
pub fn spawn_process(&mut self, name: &str, parent: Option<Pid>) -> Pid {
    let mut process = Process::new(pid, name.to_string(), parent);

    // Retain console handle 3 times (stdin, stdout, stderr)
    self.objects.retain(self.console_handle); // stdin
    self.objects.retain(self.console_handle); // stdout
    self.objects.retain(self.console_handle); // stderr

    process.files.insert(Fd::STDIN, self.console_handle);
    process.files.insert(Fd::STDOUT, self.console_handle);
    process.files.insert(Fd::STDERR, self.console_handle);

    // Console now has refcount = initial + 3
}
```

## Unified Interface

All objects implement common operations:

```rust
impl KernelObject {
    pub fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    pub fn write(&mut self, buf: &[u8]) -> io::Result<usize>;
    pub fn seek(&mut self, pos: SeekFrom) -> io::Result<u64>;
}
```

This allows polymorphic I/O through file descriptors.

## File Descriptors vs Handles

- **Handle**: Kernel-internal reference to an object
- **Fd**: Process-local index into file table

```
Process A          Kernel Object Table
┌─────────┐        ┌─────────────────┐
│ Fd(0) ──┼────────►│ Handle(1) → Console
│ Fd(1) ──┼────────►│ Handle(1) → Console
│ Fd(3) ──┼───┐    │ Handle(2) → File
└─────────┘   │    │ Handle(3) → Pipe
              │    └─────────────────┘
              │            ▲
              └────────────┘

Process B
┌─────────┐
│ Fd(0) ──┼────────► Handle(1) → Console
│ Fd(3) ──┼────────► Handle(3) → Pipe (shared!)
└─────────┘
```

## Best Practices

1. **Always close fds**: Release references properly
2. **Don't leak handles**: Track what you open
3. **Understand sharing**: Multiple fds can reference same object
4. **Check refcounts**: Useful for debugging resource leaks

## Related Documentation

- [Syscall Interface](syscalls.md) - Object operations
- [Process Model](processes.md) - File descriptor tables
- [IPC](ipc.md) - Pipes and shared objects
