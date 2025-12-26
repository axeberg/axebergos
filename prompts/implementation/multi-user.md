# Multi-User System Implementation

## The Prompt

> Add a multi-user system to axeberg. I want:
> - User database in /etc/passwd, /etc/shadow, /etc/group (like Linux)
> - login/logout commands
> - Session isolation (one user's session can't see another's)
> - useradd, passwd, groupadd commands
> - sudo support for privilege escalation
>
> Keep it realistic but don't go overboard with security hardening.

## Design Discussion

### User Database Format

Following Linux conventions:

```
# /etc/passwd - public user info
root:x:0:0:root:/root:/bin/sh
alice:x:1000:1000:Alice:/home/alice:/bin/sh

# /etc/shadow - password hashes (restricted)
root:$hash:19000:0:99999:7:::
alice:$hash:19000:0:99999:7:::

# /etc/group - group membership
root:x:0:
users:x:100:alice,bob
sudo:x:27:alice
```

### Password Hashing

> "What hashing should we use? bcrypt? argon2?"

Decision: Simple SHA-256 with salt. Not production-grade, but:
1. No external dependencies
2. Demonstrates the concept
3. This is a demo OS, not a real security system

```rust
fn hash_password(password: &str, salt: &str) -> String {
    // SHA-256(salt + password)
    // In real system: use argon2 or bcrypt
}
```

### Session Model

```rust
struct Session {
    id: SessionId,
    user: UserId,
    group: GroupId,
    controlling_tty: Option<TtyId>,
    processes: Vec<Pid>,
    environment: HashMap<String, String>,
}
```

Key properties:
- Each login creates a new session
- Processes inherit session from parent
- `logout` terminates all processes in session
- `su` creates sub-session

### Permission Checking

```rust
fn check_permission(
    file: &FileMetadata,
    user: UserId,
    group: GroupId,
    action: Action,
) -> bool {
    if user == ROOT_UID {
        return true;  // root can do anything
    }

    let mode = file.mode;
    if file.owner == user {
        check_owner_bits(mode, action)
    } else if file.group == group {
        check_group_bits(mode, action)
    } else {
        check_other_bits(mode, action)
    }
}
```

## Implementation Sequence

### Phase 1: User Database

1. Define `User` and `Group` structs
2. Implement parsing of /etc/passwd, shadow, group
3. Add write-back on changes
4. Create default root user on boot

### Phase 2: Login/Logout

1. `login` command: verify password, create session
2. `logout` command: terminate session
3. Session tracking in kernel

### Phase 3: User Management

1. `useradd`: create user in passwd, shadow, home dir
2. `passwd`: update password hash
3. `groupadd`: create group
4. `usermod`: modify user (add to groups, etc.)

### Phase 4: Permission Enforcement

1. Add permission checks to all file operations
2. Add permission checks to process operations
3. Implement `chmod`, `chown`, `chgrp`

### Phase 5: Privilege Escalation

1. `su`: switch user (requires target password)
2. `sudo`: execute as root (if user in sudo group)

## Tricky Parts

### Circular Dependency

Problem: Need to read /etc/passwd to know users, but reading files requires permission check, which requires knowing users.

Solution: Bootstrap with in-memory defaults, then load files.

```rust
impl Kernel {
    fn new() -> Self {
        let mut k = Kernel::default();
        // Create root user in memory
        k.users.add_builtin_root();
        // Now we can read files as root
        k.users.load_from_files(&k.vfs);
    }
}
```

### Session Isolation

Problem: How isolated should sessions be?

Decision:
- Separate process groups
- Separate controlling TTY
- But SHARED filesystem (this is intentional - it's a single-user browser)

### Password Storage

Problem: Can't persist to real /etc/shadow (it's in-memory VFS)

Solution: OPFS persistence saves the entire VFS including shadow.

## Testing

```rust
#[test]
fn test_login_logout() {
    let kernel = setup_kernel();

    // Login as root
    kernel.login("root", "root").unwrap();
    assert_eq!(kernel.current_user(), 0);

    // Logout
    kernel.logout().unwrap();
}

#[test]
fn test_permission_denied() {
    let kernel = setup_kernel();

    // Create file as root
    kernel.login("root", "root").unwrap();
    kernel.write_file("/secret", "data", 0o600).unwrap();
    kernel.logout().unwrap();

    // Try to read as alice
    kernel.login("alice", "alice123").unwrap();
    assert!(kernel.read_file("/secret").is_err());
}

#[test]
fn test_sudo() {
    let kernel = setup_kernel();

    // alice is in sudo group
    kernel.login("alice", "alice123").unwrap();
    kernel.sudo("cat /etc/shadow").unwrap();  // Should work
}
```

## Result

~800 lines for user management. Features:
- Complete /etc/passwd, shadow, group support
- Login sessions with isolation
- useradd, passwd, groupadd, usermod
- sudo and su
- chmod, chown, chgrp
- Permission checks on all file operations
