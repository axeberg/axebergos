# Users and Groups

axeberg implements Linux-like multi-user support with file-based persistence.

## User Database

Users are stored in `/etc/passwd` with standard format:

```
root:x:0:0::/root:/bin/sh
user:x:1000:1000::/home/user:/bin/sh
nobody:x:65534:65534::/nonexistent:/bin/sh
```

Format: `name:x:uid:gid:gecos:home:shell`

## Password Storage

Passwords are stored in `/etc/shadow`:

```
root:!:19000:0:99999:7:::
user:!:19000:0:99999:7:::
```

- `!` or `*` (or an empty hash field) means no password is set
- When a password is set, the hash field has the form `salt_hex$hash_hex`

Hashing (see `src/kernel/users.rs`) is a homebrew, salted key-stretching
construction — **not** a standard KDF, and not intended to be
cryptographically strong:

- A 16-byte random salt is generated per password
- The password and salt are mixed through 10,000 rounds of key stretching
- The result is stored as `salt_hex$hash_hex` (the `$` separator keeps the
  value free of the `:` field delimiter used in `/etc/shadow`)
- Verification recomputes the hash from the stored salt and compares in
  constant time

## Groups

Groups are stored in `/etc/group`:

```
root:x:0:
wheel:x:10:user
user:x:1000:
```

Format: `name:x:gid:member1,member2,...`

## Default Users

| User | UID | Home | Notes |
|------|-----|------|-------|
| root | 0 | /root | No password (passwordless login; set one with `passwd root`) |
| user | 1000 | /home/user | No password |
| nobody | 65534 | /nonexistent | Unprivileged |

## Default Groups

| Group | GID | Members |
|-------|-----|---------|
| root | 0 | |
| wheel | 10 | user |
| user | 1000 | |

## User Management Commands

### login

```bash
$ login alice password
Login successful: alice
  PID: 5, SID: 5, PGID: 5
  UID: 1001, GID: 1001
```

Creates a new session as the specified user.

### logout

```bash
$ logout
Session 5 ended for user 'alice'
```

Ends the current session.

### su

```bash
$ su              # Switch to root (defaults to root)
$ su alice        # Switch to alice
$ su - alice      # Also update HOME/SHELL environment
```

`su` switches the user **in place** in the current shell (via `setuid`/
`setgid` plus updating `USER`/`HOME`); it does not spawn a new shell. A
non-root caller must be in the `wheel` group to `su` to root.

### sudo

```bash
$ sudo whoami     # Intended to run as root
```

Requires membership in the `wheel` group. Note: `sudo` currently only
performs the authorization check — actually re-dispatching the target
command under root requires support in the shell executor that is not yet
implemented, so `sudo` reports that running commands is unsupported rather
than silently failing to elevate.

### useradd

```bash
$ useradd alice           # Create user with new group
$ useradd -g users bob    # Create with existing group
```

### groupadd

```bash
$ groupadd developers
```

### passwd

```bash
$ passwd secret           # Set own password
$ passwd alice newpass    # Set alice's password (root only)
$ passwd alice            # Clear password (root only)
```

### id

```bash
$ id
uid=1000(user) gid=1000(user) groups=1000(user),10(wheel)
```

### whoami

```bash
$ whoami
user
```

### who / w

```bash
$ who
USER     TTY        LOGIN@
user     tty1       12:34
```

## Permission Model

File permissions follow Unix conventions:

```
-rwxr-xr-x  1 root   root    4096 Jan  1 00:00 /bin/ls
drwxr-xr-x  2 alice  users   4096 Jan  1 00:00 /home/alice
```

### Permission Bits

| Bit | Meaning |
|-----|---------|
| r (4) | Read |
| w (2) | Write |
| x (1) | Execute/search |

### Categories

1. **Owner** (u): Matches file's uid
2. **Group** (g): Matches file's gid or supplementary groups
3. **Other** (o): Everyone else

### Root Bypass

UID 0 (root) bypasses all permission checks.

## Kernel API

### UserDb Structure

```rust
pub struct UserDb {
    users: HashMap<Uid, User>,
    groups: HashMap<Gid, Group>,
}

pub struct User {
    pub name: String,
    pub uid: Uid,
    pub gid: Gid,
    pub home: String,
    pub shell: String,
    pub password_hash: Option<String>,
}
```

### Persistence Functions

```rust
// Save to /etc/passwd, /etc/shadow, /etc/group
kernel.save_user_db();

// Load from files
kernel.load_user_db();
```

### Syscalls

| Syscall | Description |
|---------|-------------|
| `getuid()` | Get real user ID |
| `geteuid()` | Get effective user ID |
| `getgid()` | Get real group ID |
| `getegid()` | Get effective group ID |
| `setuid(uid)` | Set user ID |
| `setgid(gid)` | Set group ID |
| `seteuid(uid)` | Set effective UID |
| `setegid(gid)` | Set effective GID |
| `getgroups()` | Get supplementary groups |

## Related Documentation

- [Process Model](processes.md) - Process credentials
- [Syscall Interface](syscalls.md) - User syscalls
- [Shell](../userspace/shell.md) - User commands
