# Inter-Process Communication

axeberg provides multiple IPC mechanisms for processes to communicate.

## IPC Mechanisms

1. **Channels**: High-level typed message passing
2. **Pipes**: Low-level byte streams
3. **FIFOs (Named Pipes)**: Filesystem-visible pipes for unrelated processes
4. **Message Queues**: System V-style tagged message passing
5. **Shared Memory**: Zero-copy data sharing
6. **Files**: Persistent shared state

## Channels

Channels are type-safe, MPSC (multiple-producer, single-consumer) queues.

### Creating Channels

```rust
use axeberg::kernel::channel;

// Create a channel for String messages
let (tx, rx) = channel::<String>();
```

### Sending Messages

```rust
// Send a message
tx.send("Hello!".to_string()).unwrap();
tx.send("World!".to_string()).unwrap();

// Clone sender for multiple producers
let tx2 = tx.clone();
tx2.send("From tx2".to_string()).unwrap();

// Close when done sending
tx.close();
```

### Receiving Messages

```rust
// Try to receive (non-blocking)
match rx.try_recv() {
    Ok(msg) => println!("Got: {}", msg),
    Err(TryRecvError::Empty) => println!("No messages"),
    Err(TryRecvError::Closed) => println!("Channel closed"),
}

// Async receive - yields until message available
while let Some(msg) = rx.recv().await {
    handle(msg);
}
// Loop exits when channel closes
```

### Channel Semantics

- **Unbounded**: No capacity limit (for now)
- **FIFO**: Messages delivered in order
- **Reference counted**: Closes when all senders dropped
- **Type-safe**: Compile-time type checking

## Pipes

Pipes are low-level byte streams.

### Creating Pipes

```rust
let (read_fd, write_fd) = syscall::pipe()?;
```

### Using Pipes

```rust
// Writer
syscall::write(write_fd, b"Hello, pipe!")?;
syscall::close(write_fd)?;

// Reader
let mut buf = [0u8; 100];
let n = syscall::read(read_fd, &mut buf)?;
println!("Read {} bytes", n);
syscall::close(read_fd)?;
```

### Pipe Characteristics

- **Unidirectional**: One end writes, one reads
- **Buffered**: 4KB internal buffer
- **Reference counted**: Shared via `dup()`
- **Blocking**: Read blocks when empty (in async)

## FIFOs (Named Pipes)

FIFOs are filesystem-visible pipes that allow unrelated processes to communicate.

### Creating FIFOs

```rust
// From shell
$ mkfifo /tmp/myfifo

// Via syscall
syscall::mkfifo("/tmp/myfifo")?;
```

### Using FIFOs

```rust
// Writer process
let fd = syscall::open("/tmp/myfifo", OpenFlags::WRITE)?;
syscall::write(fd, b"Hello via FIFO!")?;
syscall::close(fd)?;

// Reader process
let fd = syscall::open("/tmp/myfifo", OpenFlags::READ)?;
let mut buf = [0u8; 100];
let n = syscall::read(fd, &mut buf)?;
syscall::close(fd)?;
```

### FIFO Characteristics

- **Named**: Appears in filesystem, persists until deleted
- **Bidirectional setup**: Can open for read or write
- **Buffered**: 4KB internal buffer (same as pipes)
- **Blocking**: Opens block until both reader and writer present

## Message Queues

System V-style message queues with typed messages for selective receiving.

### Creating Message Queues

```rust
// Create or get queue with key
let msqid = syscall::msgget(key, IPC_CREAT | 0o644)?;
```

### Sending Messages

```rust
// Messages have a type (> 0) and data
syscall::msgsnd(msqid, mtype: 1, b"request data")?;
syscall::msgsnd(msqid, mtype: 2, b"response data")?;
```

### Receiving Messages

```rust
// Receive any message (mtype = 0)
let (mtype, data) = syscall::msgrcv(msqid, 0)?;

// Receive specific type only
let (mtype, data) = syscall::msgrcv(msqid, 2)?; // Only type 2
```

### Message Queue Characteristics

- **Tagged**: Messages have types for selective receiving
- **Persistent**: Queue persists until explicitly removed
- **Bounded**: 16KB default capacity
- **Priority**: Lower message types received first with negative mtype

## Shared Memory

For zero-copy data sharing.

### Creating Shared Memory

```rust
// Process 1: Create segment
let shm_id = syscall::shmget(4096)?;

// Attach and write
let region = syscall::shmat(shm_id, Protection::READ_WRITE)?;
syscall::mem_write(region, 0, b"shared data")?;
syscall::shm_sync(shm_id)?;
```

### Accessing Shared Memory

```rust
// Process 2: Attach to existing segment
let region = syscall::shmat(shm_id, Protection::READ)?;
syscall::shm_refresh(shm_id)?;

let mut buf = [0u8; 100];
syscall::mem_read(region, 0, &mut buf)?;
```

### Shared Memory Synchronization

Unlike channels, shared memory requires explicit synchronization:

```rust
// Writer
mem_write(region, 0, &new_data)?;
shm_sync(shm_id)?;  // Push to shared

// Reader
shm_refresh(shm_id)?;  // Pull from shared
mem_read(region, 0, &mut buf)?;
```

## File-Based IPC

Processes can communicate through the filesystem.

```rust
// Process 1: Write
let fd = syscall::open("/tmp/ipc", OpenFlags::WRITE)?;
syscall::write(fd, b"message")?;
syscall::close(fd)?;

// Process 2: Read
let fd = syscall::open("/tmp/ipc", OpenFlags::READ)?;
let mut buf = [0u8; 100];
syscall::read(fd, &mut buf)?;
syscall::close(fd)?;
```

This is simple but not efficient for high-frequency communication.

## Choosing an IPC Mechanism

| Mechanism | Best For | Overhead | Type Safety |
|-----------|----------|----------|-------------|
| Channels | Messages, commands | Medium | Yes |
| Pipes | Byte streams, shell pipelines | Low | No |
| FIFOs | Unrelated processes, shell | Low | No |
| Message Queues | Tagged messages, priority | Medium | No |
| Shared Memory | Large data, zero-copy | Very Low | No |
| Files | Persistent state | High | No |

## Example: Producer-Consumer

Using channels:

```rust
let (tx, rx) = channel::<WorkItem>();

// Producer
kernel::spawn(async move {
    for i in 0..100 {
        tx.send(WorkItem { id: i, data: vec![0; 1024] }).unwrap();
    }
    tx.close();
});

// Consumer
kernel::spawn(async move {
    loop {
        match rx.try_recv() {
            Ok(item) => process(item),
            Err(TryRecvError::Closed) => break,
            Err(TryRecvError::Empty) => futures::pending!(),
        }
    }
});
```

## Example: Shared Buffer

Using shared memory:

```rust
const BUFFER_SIZE: usize = 64 * 1024;

// Create shared buffer
let shm_id = shmget(BUFFER_SIZE)?;

// Writer process
kernel::spawn(async move {
    let region = shmat(shm_id, Protection::READ_WRITE)?;

    loop {
        let data = generate_frame();
        mem_write(region, 0, &data)?;
        shm_sync(shm_id)?;

        futures::pending!();
    }
});

// Reader process (e.g., renderer)
kernel::spawn(async move {
    let region = shmat(shm_id, Protection::READ)?;

    loop {
        shm_refresh(shm_id)?;

        let mut frame = [0u8; BUFFER_SIZE];
        mem_read(region, 0, &mut frame)?;
        render(&frame);

        futures::pending!();
    }
});
```

## Thread Safety Notes

All IPC mechanisms are designed for single-threaded WASM:
- Channels use `RefCell` internally
- No mutex/lock overhead
- Safe for cooperative multitasking

If multi-threading is added, these would need synchronization primitives.

## Related Documentation

- [Process Model](processes.md) - Process isolation
- [Memory Management](memory.md) - Shared memory details
- [Executor](executor.md) - Async task scheduling
- [Future Work](../future-work.md) - Planned enhancements
