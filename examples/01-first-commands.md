# Tutorial 1: Your First Commands

Learn the basics of using the axeberg shell.

## Getting Started

When you first load axeberg in your browser, you'll see a terminal prompt:

```
Welcome to axeberg v0.1.0

root@axeberg:~$
```

You're logged in as `root` (the superuser) and ready to explore.

## Basic Commands

### See Where You Are

```bash
$ pwd
/root
```

`pwd` (print working directory) shows your current location.

### Look Around

```bash
$ ls
```

Lists files in the current directory. Try with options:

```bash
$ ls -l      # Long format with details
$ ls -a      # Show hidden files (starting with .)
$ ls -la     # Both
$ ls /       # List root directory
```

### Move Around

```bash
$ cd /etc    # Change to /etc directory
$ pwd
/etc

$ cd ..      # Go up one level
$ pwd
/

$ cd ~       # Go home (shortcut for /root)
$ cd         # Also goes home
```

### Read Files

```bash
$ cat /etc/passwd
root:x:0:0:root:/root:/bin/sh
```

For longer files:

```bash
$ head -5 /etc/passwd    # First 5 lines
$ tail -5 /etc/passwd    # Last 5 lines
```

### Create Files

```bash
$ echo "Hello, World!" > hello.txt
$ cat hello.txt
Hello, World!

$ echo "More text" >> hello.txt    # Append (>>)
$ cat hello.txt
Hello, World!
More text
```

### Create Directories

```bash
$ mkdir projects
$ mkdir -p projects/rust/src    # Create nested dirs
$ ls projects/rust/
src
```

### Copy, Move, Remove

```bash
$ cp hello.txt hello_backup.txt
$ mv hello_backup.txt backup.txt
$ rm backup.txt
$ rmdir projects/rust/src    # Remove empty directory
$ rm -r projects             # Remove directory and contents
```

## Getting Help

### List All Commands

```bash
$ help
Available commands:
  cat, cd, chmod, chown, cp, date, df, du, echo, env, exit,
  find, grep, head, help, id, kill, ln, login, logout, ls,
  mkdir, mv, passwd, ps, pwd, rm, rmdir, sleep, sort, su,
  sudo, tail, touch, uname, uniq, useradd, wc, which, whoami
  ...
```

### Command-Specific Help

```bash
$ ls --help
Usage: ls [OPTIONS] [PATH]

Options:
  -l    Long format
  -a    Show hidden files
  -R    Recursive
  -h    Human-readable sizes
```

### Manual Pages

```bash
$ man ls
LS(1)                    User Commands                    LS(1)

NAME
       ls - list directory contents
...
```

## System Information

```bash
$ uname -a
axeberg 0.1.0 wasm32 browser

$ date
Thu Dec 26 2024 12:00:00

$ uptime
12:00:00 up 0 min, 1 user, load average: 0.00, 0.00, 0.00

$ whoami
root

$ id
uid=0(root) gid=0(root) groups=0(root)
```

## Exercises

### Exercise 1: Navigate
1. Go to the `/etc` directory
2. List all files with details
3. Read the `passwd` file
4. Return to your home directory

### Exercise 2: Create Files
1. Create a directory called `practice`
2. Inside it, create a file `notes.txt` with some text
3. Copy it to `notes_backup.txt`
4. Verify both files exist

### Exercise 3: Explore
1. Find all directories in `/` using `ls -la / | grep "^d"`
2. Check what's in `/proc`
3. Read `/proc/self/status`

## What's Next?

Move on to [Working with Files](02-working-with-files.md) to learn more about file operations and text processing.
