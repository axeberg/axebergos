# Tutorial 2: Working with Files

Learn file operations and text processing in axeberg.

## Creating Files

### Using echo

```bash
$ echo "Hello, World!" > hello.txt
$ cat hello.txt
Hello, World!
```

### Using touch

```bash
$ touch empty.txt
$ ls -la empty.txt
-rw-r--r-- 1 root root 0 Dec 26 12:00 empty.txt
```

### Using cat with heredoc-style input

```bash
$ cat > notes.txt << EOF
Line 1
Line 2
Line 3
EOF

$ cat notes.txt
Line 1
Line 2
Line 3
```

## Reading Files

### Full file

```bash
$ cat /etc/passwd
root:x:0:0:root:/root:/bin/sh
```

### First/last lines

```bash
$ head -3 bigfile.txt    # First 3 lines
$ tail -5 bigfile.txt    # Last 5 lines
```

### Specific lines

```bash
$ head -10 file.txt | tail -5    # Lines 6-10
```

### With line numbers

```bash
$ cat -n file.txt
     1  Line one
     2  Line two
     3  Line three
```

## File Information

### Detailed listing

```bash
$ ls -la
drwxr-xr-x 2 root root 4096 Dec 26 12:00 .
drwxr-xr-x 3 root root 4096 Dec 26 12:00 ..
-rw-r--r-- 1 root root   13 Dec 26 12:00 hello.txt
```

Fields: permissions, links, owner, group, size, date, name.

## Searching Files

### Find by name

```bash
$ find /etc -name "*.conf"
/etc/system.conf

$ find . -name "*.txt"
./hello.txt
./notes.txt
```

### Find by type

```bash
$ find / -type d -name "etc"    # Directories only
$ find /home -type f -size +1M  # Files > 1MB
```

### Search content

```bash
$ grep "root" /etc/passwd
root:x:0:0:root:/root:/bin/sh

$ grep -r "TODO" src/    # Recursive search
$ grep -i "error" log    # Case insensitive
$ grep -n "pattern" file # Show line numbers
$ grep -c "word" file    # Count matches
```

## Text Processing

### Sort

```bash
$ cat names.txt
charlie
alice
bob

$ sort names.txt
alice
bob
charlie

$ sort -r names.txt    # Reverse
charlie
bob
alice
```

### Unique values

```bash
$ cat data.txt
apple
banana
apple
cherry
banana

$ sort data.txt | uniq
apple
banana
cherry

$ sort data.txt | uniq -c    # Count occurrences
      2 apple
      2 banana
      1 cherry
```

### Cut columns

```bash
$ cat /etc/passwd | cut -d: -f1
root

$ cat data.csv | cut -d, -f1,3    # Fields 1 and 3
```

### Word/line/char count

```bash
$ wc hello.txt
 1  2 13 hello.txt
 │  │  │
 │  │  └── characters
 │  └───── words
 └──────── lines

$ wc -l file.txt    # Lines only
$ wc -w file.txt    # Words only
```

### Transform text

```bash
$ echo "hello" | tr 'a-z' 'A-Z'
HELLO

$ cat file.txt | tr -d '\r'    # Remove carriage returns
$ cat file.txt | tr -s ' '      # Squeeze repeated spaces
```

## Comparing Files

### Diff

```bash
$ diff file1.txt file2.txt
2c2
< old line
---
> new line
```

### Common lines

```bash
$ comm file1.txt file2.txt
        line in both
line only in 1
        another common
```

## Encoding

### Base64

```bash
$ base64 file.bin > file.b64
$ base64 -d file.b64 > file.bin
```

### Hex dump (xxd)

```bash
$ xxd file.bin | head
00000000: 4865 6c6c 6f20 576f 726c 640a            Hello World.
```

## Exercises

### Exercise 1: Log File Analysis

Create a mock log file and analyze it:

```bash
$ echo -e "INFO: started\nERROR: failed\nINFO: done\nERROR: timeout" > app.log
```

1. Count total lines
2. Count ERROR lines
3. Extract just the message part (after the colon)

### Exercise 2: Data Processing

Create a CSV and process it:

```bash
$ echo -e "name,age,city\nalice,30,NYC\nbob,25,LA\nalice,30,NYC" > data.csv
```

1. Extract unique names
2. Count occurrences of each name
3. Sort by age

### Exercise 3: File Organization

1. Create a directory structure for a project
2. Create some files in different directories
3. Use `find` to locate all `.txt` files
4. Use `grep` to search for content across files

## What's Next?

Continue to [Pipes and Redirects](03-pipes-and-redirects.md) to learn about composing commands.
