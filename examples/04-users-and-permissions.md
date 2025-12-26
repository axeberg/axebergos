# Tutorial 4: Users and Permissions

Learn about axeberg's multi-user system and access control.

## The User Model

axeberg implements a Unix-like multi-user system:
- **Users**: Individual accounts with unique IDs
- **Groups**: Collections of users for shared permissions
- **Sessions**: Isolated login contexts

## User Information

### Who am I?

```bash
$ whoami
root

$ id
uid=0(root) gid=0(root) groups=0(root)
```

### Who else is on the system?

```bash
$ cat /etc/passwd
root:x:0:0:root:/root:/bin/sh
```

Format: `username:x:uid:gid:comment:home:shell`

### Active sessions

```bash
$ who
root     tty0     Dec 26 12:00

$ w
USER     TTY      FROM              LOGIN@   IDLE   WHAT
root     tty0     -                 12:00    0      bash
```

## Creating Users

### Add a user

```bash
$ useradd alice
$ cat /etc/passwd | grep alice
alice:x:1000:1000::/home/alice:/bin/sh
```

### Set password

```bash
$ passwd alice newpassword
Password updated for alice

# Password hashes stored in /etc/shadow (restricted)
```

### Create a group

```bash
$ groupadd developers

$ cat /etc/group | grep developers
developers:x:1001:
```

## Sessions and Login

### Login as another user

```bash
$ login alice newpassword
Welcome alice!
alice@axeberg:~$

$ whoami
alice

$ pwd
/home/alice
```

### Logout

```bash
$ logout
Goodbye alice!
root@axeberg:~$
```

### Switch user (su)

Stay in current session but act as another user:

```bash
$ su alice
Password:
alice@axeberg:~$

$ exit    # Return to previous user
root@axeberg:~$

$ su -    # Switch to root (if allowed)
```

## File Permissions

### Understanding permission bits

```bash
$ ls -la file.txt
-rw-r--r-- 1 root root 100 Dec 26 12:00 file.txt
```

Breaking down `-rw-r--r--`:
```
-    rw-    r--    r--
│    │      │      │
│    │      │      └── Others: read only
│    │      └── Group: read only
│    └── Owner: read + write
└── File type (- = regular file, d = directory)
```

Permission bits:
- `r` (4): Read
- `w` (2): Write
- `x` (1): Execute

### Change permissions (chmod)

```bash
# Symbolic mode
$ chmod u+x script.sh     # Add execute for owner
$ chmod g-w file.txt      # Remove write for group
$ chmod o=r file.txt      # Set others to read only
$ chmod a+r file.txt      # Add read for all

# Numeric mode
$ chmod 755 script.sh     # rwxr-xr-x
$ chmod 644 file.txt      # rw-r--r--
$ chmod 600 secret.txt    # rw-------
```

Common patterns:
- `755`: Executables (owner can modify, others can run)
- `644`: Regular files (owner can modify, others can read)
- `600`: Private files (owner only)
- `700`: Private directories

### Change ownership (chown)

```bash
$ chown alice file.txt
$ ls -la file.txt
-rw-r--r-- 1 alice root 100 Dec 26 12:00 file.txt

$ chown alice:developers file.txt
-rw-r--r-- 1 alice developers 100 Dec 26 12:00 file.txt
```

### Change group (chgrp)

```bash
$ chgrp developers project/
```

## Permission Enforcement

### Access denied

```bash
$ echo "secret" > /root/secret.txt
$ chmod 600 /root/secret.txt

$ su alice
$ cat /root/secret.txt
cat: /root/secret.txt: Permission denied
```

### Directory permissions

- `r`: List contents (ls)
- `w`: Create/delete files
- `x`: Enter directory (cd), access files inside

```bash
$ mkdir restricted
$ chmod 700 restricted
$ touch restricted/data.txt

$ su alice
$ ls restricted/
ls: cannot open directory 'restricted/': Permission denied
$ cat restricted/data.txt
cat: restricted/data.txt: Permission denied
```

## Sudo

### Running as root

Use `sudo` to run commands as root:

```bash
$ su alice
$ sudo cat /etc/shadow
[sudo] password for alice:
root:$hash:...
```

### Sudo without password (for scripts)

Edit `/etc/sudoers`:
```
alice ALL=(ALL) NOPASSWD: ALL
```

## Practical Examples

### Shared project directory

```bash
# As root
$ groupadd project
$ mkdir /shared/project
$ chown root:project /shared/project
$ chmod 775 /shared/project

# Users in the project group can create files there
```

### Private home directory

```bash
$ chmod 700 /home/alice    # Only alice can access
```

### Executable script

```bash
$ cat > script.sh << 'EOF'
#!/bin/sh
echo "Hello from script!"
EOF

$ chmod +x script.sh
$ ./script.sh
Hello from script!
```

## Exercises

### Exercise 1: User Setup
1. Create users `alice` and `bob` with `useradd`
2. Set passwords with `passwd`
3. Create a group `team` with `groupadd`
4. View the user and group databases

### Exercise 2: Permission Practice
1. Create a file readable only by owner
2. Create a file writable by group
3. Create a directory only accessible by owner

### Exercise 3: Collaboration
1. Create `/shared/docs` with `mkdir`
2. Make it world-writable with `chmod 777`
3. Login as `alice`, create a file
4. Login as `bob`, verify you can see and edit it

## Security Notes

In the real axeberg:
- Passwords use SHA-256 (demo only - real systems use bcrypt/argon2)
- /etc/shadow is mode 600
- Root (uid=0) bypasses all permission checks
- This is a demo OS - don't use for actual security

## What's Next?

Continue to [Understanding the Kernel](06-understanding-kernel.md) for a deep dive into axeberg's internals.
