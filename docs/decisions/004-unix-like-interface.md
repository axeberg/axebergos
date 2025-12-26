# ADR-004: Unix-like Interface Design

## Status
Accepted

## Context

We need to decide on the user interface paradigm for the OS. Options:

1. **Unix-like CLI**: Shell, commands, pipes, files
2. **GUI-first**: Windows/macOS style desktop
3. **Novel interface**: Something new (tiles, cards, etc.)
4. **REPL-style**: Like a programming language

The target audience is developers learning OS concepts.

## Decision

We will implement a **Unix-like command-line interface** as the primary interaction model.

This means:
- Shell with familiar commands (ls, cat, grep, etc.)
- Pipes and redirects
- Hierarchical filesystem with /etc, /home, /proc, etc.
- Users, groups, permissions (rwx model)
- Processes with PIDs

We will NOT strictly follow POSIX. We'll simplify where it makes sense.

## Consequences

### Positive

1. **Familiar**: Developers know Unix concepts
2. **Educational**: Demonstrates real OS patterns
3. **Composable**: Pipes are a powerful abstraction
4. **Text-based**: Works everywhere, easy to implement
5. **Well-documented**: Decades of Unix literature
6. **Scriptable**: Commands can be combined

### Negative

1. **Learning curve**: Non-technical users may struggle
2. **No GUI**: Less visual appeal
3. **Historical baggage**: Some Unix patterns are arcane
4. **Comparison pressure**: Users expect bash-level features

## What We Keep

| Feature | Reason |
|---------|--------|
| File descriptors | Essential abstraction |
| Pipes | Composition is powerful |
| /proc, /dev, /sys | Good patterns for virtual resources |
| User/group permissions | Security model demo |
| Signals (basic) | Process control |
| Environment variables | Configuration pattern |

## What We Simplify

| Unix Feature | Our Approach | Reason |
|--------------|--------------|--------|
| fork() | spawn() | fork is complex and WASM-unfriendly |
| Complex signals | Basic set (TERM, KILL, STOP, CONT) | Full POSIX signals are over-engineered |
| errno | Rust Result types | More idiomatic |
| Symlink semantics | Simpler resolution | Full semantics are complex |
| File locking | None | Overkill for demo |
| TTY complexity | Minimal line discipline | Full TTY is arcane |

## What We Skip

- setuid/setgid bits (too complex, security risk)
- Advanced shell features (arrays, complex expansion)
- Networking stack (use WebSocket directly)
- Block devices, mounting (use VFS abstraction)
- System V IPC quirks (clean up the API)

## Alternatives Considered

### 1. Full POSIX compliance
- **Pro**: Maximum compatibility
- **Con**: Enormous complexity, not tractable

### 2. Windows-like
- **Pro**: Different perspective
- **Con**: Less common in developer education, GUI-focused

### 3. Plan 9-like
- **Pro**: Cleaner design than Unix
- **Con**: Less familiar, smaller community

### 4. Novel design
- **Pro**: No baggage, can innovate
- **Con**: Everything must be learned, no ecosystem

## Examples

```bash
# These work like you'd expect
$ ls -la /etc
$ cat file.txt | grep pattern | wc -l
$ echo "hello" > file.txt
$ chmod 600 secret.txt
$ ps aux | grep sleep
$ kill %1

# But these are simplified
$ spawn ./program    # Instead of fork+exec
$ Result::Err(...)   # Instead of errno
```

## Lessons Learned

1. Users appreciate familiar patterns
2. Simplification is acceptable when explained
3. The 80/20 rule applies: 20% of Unix covers 80% of use cases
4. Pipes are universally loved
