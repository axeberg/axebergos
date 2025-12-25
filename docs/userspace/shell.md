# Shell

The axeberg shell is a command-line interpreter providing a Unix-like interface for interacting with the system.

## Features

- **Command parsing** with pipes, redirections, and quotes
- **Built-in commands** that run in the shell process
- **External programs** that run via the executor
- **Pipes** for chaining commands: `cat file.txt | grep pattern | wc -l`
- **Redirections** for file I/O: `ls > files.txt`, `sort < input.txt`
- **Terminal** with scrollback and line editing

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Terminal                              │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ user input: cat file.txt | grep hello > out.txt     │   │
│  └─────────────────────────┬───────────────────────────┘   │
│                            │                                 │
│                            ▼                                 │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ Parser                                                │   │
│  │ - Tokenize input                                     │   │
│  │ - Handle quotes and escapes                          │   │
│  │ - Build Pipeline of SimpleCommands                   │   │
│  │ - Parse redirections                                 │   │
│  └─────────────────────────┬───────────────────────────┘   │
│                            │                                 │
│                            ▼                                 │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ Pipeline { commands: [...], background: false }      │   │
│  └─────────────────────────┬───────────────────────────┘   │
│                            │                                 │
│              ┌─────────────┴─────────────┐                  │
│              ▼                           ▼                  │
│  ┌─────────────────────┐    ┌─────────────────────────┐   │
│  │ Builtins            │    │ Executor                 │   │
│  │ (cd, pwd, echo...)  │    │ (cat, ls, grep...)       │   │
│  │ Direct execution    │    │ Via ProgramRegistry      │   │
│  └─────────────────────┘    └─────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## Built-in Commands

Builtins run in the shell process itself (cannot be piped):

| Command | Description |
|---------|-------------|
| `cd <path>` | Change working directory |
| `pwd` | Print working directory |
| `exit [code]` | Exit the shell |
| `echo [args...]` | Print arguments to stdout |
| `export VAR=value` | Set environment variable |
| `unset VAR` | Remove environment variable |
| `env` | List all environment variables |
| `true` | Return exit code 0 |
| `false` | Return exit code 1 |
| `help` | Show available commands |

## External Programs

Programs run via the executor with full pipe/redirect support:

### File Operations

| Command | Description |
|---------|-------------|
| `cat <files...>` | Concatenate and print files |
| `ls [-l] [-a] [path]` | List directory contents |
| `mkdir [-p] <path>` | Create directory |
| `touch <file>` | Create empty file or update timestamp |
| `rm [-r] [-f] <paths...>` | Remove files/directories |
| `cp <src> <dst>` | Copy file |
| `mv <src> <dst>` | Move/rename file |
| `ln [-s] <target> <link>` | Create hard/symbolic link |
| `readlink <link>` | Print symlink target |
| `stat <path>` | Display file status |
| `file <path>` | Determine file type |

### Text Processing

| Command | Description |
|---------|-------------|
| `head [-n N] [file]` | Show first N lines (default 10) |
| `tail [-n N] [file]` | Show last N lines (default 10) |
| `wc [-l] [-w] [-c] [file]` | Count lines, words, bytes |
| `grep [-i] [-v] [-n] <pattern> [file]` | Search for pattern |
| `sort [-r] [-n] [file]` | Sort lines |
| `uniq [-c] [-d] [file]` | Remove duplicate adjacent lines |
| `cut -d<delim> -f<fields> [file]` | Extract fields |
| `tr <set1> <set2>` | Translate characters |
| `sed <script> [file]` | Stream editor |
| `awk <program> [file]` | Pattern scanning |
| `diff <file1> <file2>` | Compare files |
| `comm <file1> <file2>` | Compare sorted files |

### Utilities

| Command | Description |
|---------|-------------|
| `tee [-a] <file>` | Copy stdin to file and stdout |
| `clear` | Clear terminal screen |
| `date` | Display current date/time |
| `sleep <seconds>` | Sleep for specified time |
| `basename <path>` | Extract filename |
| `dirname <path>` | Extract directory |
| `which <cmd>` | Locate command |
| `xargs <cmd>` | Build command from stdin |
| `seq <start> <end>` | Print number sequence |
| `yes [string]` | Print string repeatedly |
| `printf <format> [args]` | Formatted output |

### Process Management

| Command | Description |
|---------|-------------|
| `ps [-e] [-f]` | List processes |
| `kill [-signal] <pid>` | Send signal to process |
| `jobs` | List background jobs |
| `fg [job]` | Bring job to foreground |
| `bg [job]` | Continue job in background |

### System Information

| Command | Description |
|---------|-------------|
| `uname [-a]` | Print system information |
| `uptime` | Show system uptime |
| `free` | Display memory usage |
| `df [-h]` | Show disk space usage |
| `du [-s] [-h] [path]` | Estimate file space |

### User & Permission Management

| Command | Description |
|---------|-------------|
| `id [user]` | Print user/group IDs |
| `whoami` | Print current username |
| `groups [user]` | Print group memberships |
| `useradd <name>` | Create new user |
| `groupadd <name>` | Create new group |
| `passwd [user]` | Change password |
| `su [user]` | Switch user |
| `sudo <cmd>` | Run as root |
| `chmod <mode> <file>` | Change permissions |
| `chown <user> <file>` | Change owner |
| `chgrp <group> <file>` | Change group |

### Service Management

| Command | Description |
|---------|-------------|
| `systemctl <cmd> [service]` | Manage services |
| `reboot` | Reboot system |
| `poweroff` | Power off system |

### IPC Commands

| Command | Description |
|---------|-------------|
| `mkfifo <name>` | Create named pipe |
| `ipcs [-q] [-s] [-m]` | Show IPC facilities |
| `ipcrm -q\|-s\|-m <id>` | Remove IPC resource |

### Mount Commands

| Command | Description |
|---------|-------------|
| `mount [-t type] [-o opts] <src> <tgt>` | Mount filesystem |
| `umount <target>` | Unmount filesystem |
| `findmnt [target]` | Find mount point |

### TTY Commands

| Command | Description |
|---------|-------------|
| `tty` | Print terminal name |
| `stty [-a] [setting]` | Get/set terminal settings |

### Persistence

| Command | Description |
|---------|-------------|
| `save` | Save filesystem to OPFS |
| `fsload` | Reload filesystem from OPFS |
| `fsreset [-f]` | Clear OPFS and reset filesystem |
| `autosave [on\|off\|status\|interval N]` | Configure auto-save |

### Networking

| Command | Description |
|---------|-------------|
| `curl [options] URL` | HTTP client (fetch API) |
| `wget [-O file] URL` | Download file from URL |

## Syntax

### Pipes

Connect stdout of one command to stdin of another:

```bash
cat file.txt | grep error | wc -l
```

### Redirections

| Syntax | Meaning |
|--------|---------|
| `> file` | Redirect stdout to file (overwrite) |
| `>> file` | Redirect stdout to file (append) |
| `< file` | Redirect stdin from file |
| `2> file` | Redirect stderr to file |

Examples:
```bash
ls > listing.txt           # Save directory listing
sort < unsorted.txt        # Sort from file
cat file.txt 2> errors.txt # Capture errors
```

### Quoting

| Syntax | Behavior |
|--------|----------|
| `"double quotes"` | Preserves spaces, expands variables |
| `'single quotes'` | Preserves everything literally |
| `\` | Escape next character |

Examples:
```bash
echo "hello world"         # Prints: hello world
echo 'hello $USER'         # Prints: hello $USER
echo "path: \"$PWD\""      # Prints: path: "/current/dir"
```

### Background Execution

Append `&` to run command in background (not yet fully implemented):

```bash
long-running-task &
```

## Parser Details

The parser handles complex command lines:

```rust
pub struct Pipeline {
    pub commands: Vec<SimpleCommand>,
    pub background: bool,
}

pub struct SimpleCommand {
    pub program: String,
    pub args: Vec<String>,
    pub redirects: Vec<Redirect>,
}

pub struct Redirect {
    pub kind: RedirectKind,  // In, Out, Append, Err
    pub target: String,      // File path
}
```

Parsing stages:
1. **Tokenization**: Split by whitespace, respecting quotes
2. **Pipeline split**: Divide at `|` tokens
3. **Redirect extraction**: Find `<`, `>`, `>>`, `2>` and their targets
4. **Command building**: First token is program, rest are args

## Executor Details

The executor runs pipelines:

```rust
pub struct Executor {
    registry: ProgramRegistry,
}

impl Executor {
    pub fn run(&self, pipeline: &Pipeline, state: &ShellState) -> ExecResult {
        // 1. For each command in pipeline
        // 2. Connect stdin from previous stdout (pipes)
        // 3. Apply redirections
        // 4. Execute via registry lookup
        // 5. Return combined result
    }
}
```

Programs are simple functions:
```rust
type Program = fn(args: &[String], stdout: &mut String, stderr: &mut String) -> i32;
```

## Terminal

The terminal provides:

- **Line editing**: Backspace, cursor movement
- **History**: Up/down arrow to navigate previous commands
- **Scrollback**: View output that scrolled off screen
- **Unicode support**: Full UTF-8 text handling

```rust
pub struct Terminal {
    lines: Vec<String>,      // Output buffer
    input: String,           // Current input line
    cursor: usize,           // Cursor position
    scroll_offset: usize,    // For scrollback
    history: Vec<String>,    // Command history
    history_index: Option<usize>,
}
```

## Future: WASM Command Modules

The current executor uses hardcoded Rust functions. The future architecture uses WASM modules:

```
Current:  shell → executor → prog_cat()
Future:   shell → loader → /bin/cat.wasm → main()
```

See [WASM Modules](../kernel/wasm-modules.md) for the ABI specification.

## Example Session

```
axeberg v0.1.0
Type 'help' for available commands.

$ pwd
/home

$ mkdir projects
$ cd projects

$ echo "Hello, axeberg!" > greeting.txt
$ cat greeting.txt
Hello, axeberg!

$ ls
greeting.txt

$ cat greeting.txt | wc
      1       2      16

$ help
Built-in commands:
  cd <path>     - Change directory
  pwd           - Print working directory
  exit [code]   - Exit shell
  export VAR=val - Set environment variable
  ...

Programs:
  File: cat, ls, mkdir, touch, rm, cp, mv, ln, stat
  Text: head, tail, wc, grep, sort, uniq, cut, tr, sed, awk
  System: ps, kill, mount, df, free, uname, uptime
  User: id, whoami, groups, su, sudo, chmod, chown
  Service: systemctl, reboot, poweroff
  IPC: mkfifo, ipcs, ipcrm
  TTY: tty, stty
```

## Related Documentation

- [WASM Modules](../kernel/wasm-modules.md) - Command executable format
- [VFS](vfs.md) - Filesystem commands operate on
- [Standard I/O](stdio.md) - How stdin/stdout/stderr work
