# Examples & Tutorials

Learn axeberg by building and exploring.

## Quick Start

```bash
# Build and run axeberg
wasm-pack build --target web
cargo run --bin serve
# Open http://localhost:8080
```

## Tutorials

### 1. [Your First Commands](01-first-commands.md)
Basic shell usage and navigation.

### 2. [Working with Files](02-working-with-files.md)
Creating, reading, and manipulating files.

### 3. [Pipes and Redirects](03-pipes-and-redirects.md)
Composing commands with Unix pipes.

### 4. [Users and Permissions](04-users-and-permissions.md)
Multi-user system and access control.

### 5. [Understanding the Kernel](06-understanding-kernel.md)
Deep dive into kernel internals.

## Quick Examples

### Hello World

```bash
$ echo "Hello, axeberg!"
Hello, axeberg!
```

### File Operations

```bash
$ echo "Hello" > greeting.txt
$ cat greeting.txt
Hello
$ ls -la
total 1
-rw-r--r-- 1 root root 6 Dec 26 12:00 greeting.txt
```

### Pipes

```bash
$ cat /etc/passwd | grep root
root:x:0:0:root:/root:/bin/sh

$ ls -la | grep "^d" | wc -l
3
```

### User Management

```bash
$ useradd alice
$ passwd alice secretpass
$ su alice
$ whoami
alice
$ exit
```

### Process Management

```bash
$ sleep 100 &
[1] 42
$ jobs
[1]+  Running    sleep 100 &
$ kill %1
$ jobs
[1]+  Terminated sleep 100
```

## Code Examples

### Reading Kernel Source

The kernel is readable. Start here:

- `src/kernel/syscall.rs` - System calls
- `src/kernel/process.rs` - Process management
- `src/shell/executor.rs` - How commands run

## Architecture Exploration

### Trace a Command

Use `strace` to see what syscalls a command makes:

```bash
$ strace cat /etc/passwd
open("/etc/passwd", O_RDONLY) = 3
read(3, "root:x:0:0:root:/root:/bin/sh\n", 4096) = 31
write(1, "root:x:0:0:root:/root:/bin/sh\n", 31) = 31
close(3) = 0
```

### Explore /proc

```bash
$ cat /proc/self/status
Name: cat
Pid: 42
Uid: 0
State: Running

$ ls /proc/1/
cmdline  cwd  environ  exe  fd  status
```

### Inspect Memory

```bash
$ free
              total        used        free
Mem:       67108864     2097152    65011712
```

## Learning Paths

### Path 1: Shell User
1. Basic commands → 2. Pipes → 3. Scripting → 4. Job control

### Path 2: Kernel Hacker
1. Process model → 2. Syscalls → 3. Memory → 4. Add new syscall

### Path 3: System Builder
1. Architecture → 2. VFS design → 3. IPC → 4. Build your own component

## Further Reading

- [Kernel Documentation](../docs/kernel/overview.md)
- [Shell Guide](../docs/userspace/shell.md)
- [Contributing Guide](../docs/development/contributing.md)
- [Architecture Diagrams](../ARCHITECTURE.md)
