# Tutorial 3: Pipes and Redirects

Learn to compose commands using Unix pipes and redirects.

## The Unix Philosophy

> Do one thing well, and compose with others.

axeberg follows this principle. Each command is simple, but you can combine them into powerful pipelines.

## Output Redirection

### Write to File (`>`)

```bash
$ echo "Hello" > greeting.txt
$ cat greeting.txt
Hello
```

The `>` redirects stdout to a file, **overwriting** if it exists.

### Append to File (`>>`)

```bash
$ echo "Line 1" > file.txt
$ echo "Line 2" >> file.txt
$ echo "Line 3" >> file.txt
$ cat file.txt
Line 1
Line 2
Line 3
```

The `>>` appends instead of overwriting.

### Redirect Errors (`2>`)

```bash
$ cat nonexistent 2> errors.txt
$ cat errors.txt
cat: nonexistent: No such file or directory
```

The `2>` redirects stderr (error messages).

### Redirect Both (`&>` or `2>&1`)

```bash
$ cat file.txt nonexistent &> output.txt
# Or equivalently:
$ cat file.txt nonexistent > output.txt 2>&1
```

## Input Redirection

### Read from File (`<`)

```bash
$ wc -l < /etc/passwd
5

$ sort < unsorted.txt > sorted.txt
```

The `<` redirects stdin from a file.

## Pipes

### Basic Pipes (`|`)

A pipe connects the stdout of one command to the stdin of another.

```bash
$ cat /etc/passwd | grep root
root:x:0:0:root:/root:/bin/sh
```

This is equivalent to:
```bash
$ cat /etc/passwd > temp.txt
$ grep root < temp.txt
$ rm temp.txt
```

But pipes are:
- Faster (no disk I/O)
- Cleaner (no temp files)
- Streamlined (processes run in parallel)

### Pipeline Chains

You can chain multiple pipes:

```bash
$ cat /etc/passwd | grep -v "^#" | cut -d: -f1 | sort
alice
bob
root
```

What this does:
1. `cat /etc/passwd` - Read the file
2. `grep -v "^#"` - Remove comment lines
3. `cut -d: -f1` - Extract first field (username)
4. `sort` - Sort alphabetically

### Counting Patterns

```bash
$ ls -la | wc -l
15

$ cat access.log | grep "ERROR" | wc -l
42
```

### Filtering and Transforming

```bash
# Find large files
$ ls -la | sort -k5 -n -r | head -5

# Extract unique values
$ cat data.txt | cut -d',' -f2 | sort | uniq

# Count occurrences
$ cat data.txt | cut -d',' -f2 | sort | uniq -c | sort -rn
```

## Practical Examples

### Log Analysis

```bash
# Find error count by type
$ cat app.log | grep ERROR | cut -d: -f2 | sort | uniq -c

# Most frequent IP addresses
$ cat access.log | cut -d' ' -f1 | sort | uniq -c | sort -rn | head -10
```

### File Management

```bash
# Find all .txt files and count lines
$ find . -name "*.txt" | xargs wc -l

# List files modified today
$ ls -la | grep "Dec 26"
```

### Text Processing

```bash
# Convert to uppercase
$ echo "hello world" | tr 'a-z' 'A-Z'
HELLO WORLD

# Replace tabs with commas
$ cat data.txt | tr '\t' ','

# Remove blank lines
$ cat file.txt | grep -v "^$"
```

## Command Substitution

Use output of one command as argument to another:

```bash
$ echo "Today is $(date)"
Today is Thu Dec 26 2024 12:00:00

$ echo "You are $(whoami)"
You are root
```

## Background Jobs

### Run in Background (`&`)

```bash
$ sleep 100 &
[1] 42
```

The command runs in the background. You get a job number `[1]` and PID `42`.

### Job Control

```bash
$ jobs
[1]+  Running    sleep 100 &

$ fg %1        # Bring to foreground
sleep 100
^C             # Ctrl+C to stop

# Or to stop a background job:
$ kill %1
```

## Exercises

### Exercise 1: Pipeline Practice
Create a file with 10 lines of random words, then:
1. Sort them alphabetically
2. Remove duplicates
3. Count the remaining lines

### Exercise 2: Log Processing
```bash
$ echo -e "ERROR: disk full\nINFO: started\nERROR: timeout" > app.log
```
Use pipes to count how many ERROR lines exist.

### Exercise 3: File Discovery
Use a pipeline to find the 5 largest files in `/etc`:
```bash
$ ls -la /etc | sort -k5 -n -r | head -5
```

## Common Patterns

| Pattern | Purpose |
|---------|---------|
| `cmd \| head -n` | First n lines |
| `cmd \| tail -n` | Last n lines |
| `cmd \| wc -l` | Count lines |
| `cmd \| sort \| uniq` | Unique values |
| `cmd \| sort \| uniq -c` | Count occurrences |
| `cmd \| grep pattern` | Filter lines |
| `cmd \| xargs other` | Use output as args |

## What's Next?

Continue to [Users and Permissions](04-users-and-permissions.md) to learn about the multi-user system.
