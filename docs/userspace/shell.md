# Shell

The axeberg shell is a command-line interpreter providing a Unix-like interface for interacting with the system.

## Features

- **Pipes**: `cat file.txt | grep pattern | wc -l`
- **Redirections**: `ls > files.txt`, `sort < input.txt`
- **Logical operators**: `cmd1 && cmd2`, `cmd1 || cmd2`
- **Background jobs**: `sleep 100 &`, `jobs`, `fg`, `bg`
- **Functions**: `greet() { echo "Hello $1"; }`
- **Arrays**: `arr=(one two three)`, `arr[0]=value`
- **Heredocs**: `cat <<EOF ... EOF`
- **Process substitution**: `diff <(cmd1) <(cmd2)`
- **Variable expansion**: `$VAR`, `${VAR}`
- **Job control**: Ctrl+C, Ctrl+Z, fg, bg

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
| `login <user> [pass]` | Log in as user (spawns new session) |
| `logout` | End current session |
| `id [user]` | Print user/group IDs |
| `whoami` | Print current username |
| `who` | Show logged in users |
| `w` | Show who is logged in and what they're doing |
| `groups [user]` | Print group memberships |
| `useradd <name>` | Create new user |
| `groupadd <name>` | Create new group |
| `passwd [user] [pass]` | Change password |
| `su [user]` | Switch user (spawns new shell) |
| `sudo <cmd>` | Run as root |
| `chmod <mode> <file>` | Change permissions |
| `chown <user> <file>` | Change owner |
| `chgrp <group> <file>` | Change group |

#### Session Management

The `login` command creates a proper Linux-like session:

```bash
$ login alice password
Login successful: alice
  PID: 5, SID: 5, PGID: 5
  UID: 1001, GID: 1001
  Home: /home/alice
  Shell: /bin/sh
  TTY: tty1
```

Use `logout` to end the session and return to the parent process.

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

### Logical Operators

Chain commands based on exit status:

```bash
make && ./test           # Run test only if make succeeds
grep pattern file || echo "Not found"  # Echo if grep fails
```

### Functions

Define reusable command sequences:

```bash
greet() {
    echo "Hello, $1!"
}

greet "World"            # Prints: Hello, World!
```

### Arrays

Bash-like array syntax:

```bash
arr=(one two three)      # Define array
arr+=(four)              # Append element
arr[0]=zero              # Set by index
echo ${arr[1]}           # Access element (expansion not yet implemented)
```

### Heredocs

Multi-line input:

```bash
cat <<EOF
This is a
multi-line
document
EOF

cat <<-INDENTED
	Tabs at start are stripped
	with the - variant
INDENTED
```

### Process Substitution

Use command output as file:

```bash
diff <(ls dir1) <(ls dir2)   # Compare directory listings
grep pattern <(cat file | sort)
```

### Background Execution

Run commands in background:

```bash
long-running-task &          # Run in background
jobs                         # List background jobs
fg %1                        # Bring job 1 to foreground
bg %1                        # Continue job 1 in background
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
    pub stdin: Option<Redirect>,   // Input redirection: < file
    pub stdout: Option<Redirect>,  // Output redirection: > file or >> file
    pub stderr: Option<Redirect>,  // Error redirection: 2> file or 2>> file
}

pub struct Redirect {
    pub path: String,    // Target file path
    pub append: bool,    // Append mode (>> vs >)
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
type ProgramFn = fn(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32;
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
(lists available commands - see man pages for details)
```

## Related Documentation

- [WASM Modules](../kernel/wasm-modules.md) - Command executable format
- [VFS](vfs.md) - Filesystem commands operate on
- [Standard I/O](stdio.md) - How stdin/stdout/stderr work
