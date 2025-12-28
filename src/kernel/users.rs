//! User and Group Management
//!
//! Provides Unix-like user and group abstractions:
//! - User database (like /etc/passwd)
//! - Group database (like /etc/group)
//! - Password hashing with salted key stretching
//! - User/group lookups
//!
//! # Security
//!
//! Passwords are hashed using a salted key-stretching algorithm:
//! - 16-byte cryptographically random salt per password
//! - 10,000 rounds of hashing to slow brute-force attacks
//! - Stored as "salt_hex:hash_hex"

use std::collections::HashMap;

/// User identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Uid(pub u32);

impl Uid {
    pub const ROOT: Uid = Uid(0);
}

impl std::fmt::Display for Uid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Group identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Gid(pub u32);

impl Gid {
    pub const ROOT: Gid = Gid(0);
}

impl std::fmt::Display for Gid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// File permission bits (Unix-style)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FileMode(pub u16);

impl FileMode {
    // Permission bits
    pub const S_IRUSR: u16 = 0o400; // Owner read
    pub const S_IWUSR: u16 = 0o200; // Owner write
    pub const S_IXUSR: u16 = 0o100; // Owner execute
    pub const S_IRGRP: u16 = 0o040; // Group read
    pub const S_IWGRP: u16 = 0o020; // Group write
    pub const S_IXGRP: u16 = 0o010; // Group execute
    pub const S_IROTH: u16 = 0o004; // Other read
    pub const S_IWOTH: u16 = 0o002; // Other write
    pub const S_IXOTH: u16 = 0o001; // Other execute

    // Special bits
    pub const S_ISUID: u16 = 0o4000; // Set-user-ID
    pub const S_ISGID: u16 = 0o2000; // Set-group-ID
    pub const S_ISVTX: u16 = 0o1000; // Sticky bit

    // Common combinations
    pub const FILE_DEFAULT: FileMode = FileMode(0o644); // rw-r--r--
    pub const DIR_DEFAULT: FileMode = FileMode(0o755); // rwxr-xr-x
    pub const EXEC_DEFAULT: FileMode = FileMode(0o755); // rwxr-xr-x

    pub fn new(mode: u16) -> Self {
        FileMode(mode & 0o7777) // Mask to valid bits
    }

    /// Check if owner can read
    pub fn owner_read(&self) -> bool {
        self.0 & Self::S_IRUSR != 0
    }

    /// Check if owner can write
    pub fn owner_write(&self) -> bool {
        self.0 & Self::S_IWUSR != 0
    }

    /// Check if owner can execute
    pub fn owner_exec(&self) -> bool {
        self.0 & Self::S_IXUSR != 0
    }

    /// Check if group can read
    pub fn group_read(&self) -> bool {
        self.0 & Self::S_IRGRP != 0
    }

    /// Check if group can write
    pub fn group_write(&self) -> bool {
        self.0 & Self::S_IWGRP != 0
    }

    /// Check if group can execute
    pub fn group_exec(&self) -> bool {
        self.0 & Self::S_IXGRP != 0
    }

    /// Check if others can read
    pub fn other_read(&self) -> bool {
        self.0 & Self::S_IROTH != 0
    }

    /// Check if others can write
    pub fn other_write(&self) -> bool {
        self.0 & Self::S_IWOTH != 0
    }

    /// Check if others can execute
    pub fn other_exec(&self) -> bool {
        self.0 & Self::S_IXOTH != 0
    }

    /// Check if setuid
    pub fn is_setuid(&self) -> bool {
        self.0 & Self::S_ISUID != 0
    }

    /// Check if setgid
    pub fn is_setgid(&self) -> bool {
        self.0 & Self::S_ISGID != 0
    }

    /// Format as symbolic string (e.g., "rwxr-xr-x")
    pub fn to_symbolic(&self) -> String {
        let mut s = String::with_capacity(9);
        s.push(if self.owner_read() { 'r' } else { '-' });
        s.push(if self.owner_write() { 'w' } else { '-' });
        s.push(if self.is_setuid() {
            if self.owner_exec() { 's' } else { 'S' }
        } else if self.owner_exec() {
            'x'
        } else {
            '-'
        });
        s.push(if self.group_read() { 'r' } else { '-' });
        s.push(if self.group_write() { 'w' } else { '-' });
        s.push(if self.is_setgid() {
            if self.group_exec() { 's' } else { 'S' }
        } else if self.group_exec() {
            'x'
        } else {
            '-'
        });
        s.push(if self.other_read() { 'r' } else { '-' });
        s.push(if self.other_write() { 'w' } else { '-' });
        s.push(if self.other_exec() { 'x' } else { '-' });
        s
    }

    /// Parse from octal string (e.g., "755")
    pub fn from_octal_str(s: &str) -> Option<Self> {
        u16::from_str_radix(s, 8).ok().map(FileMode::new)
    }
}

impl std::fmt::Display for FileMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04o}", self.0)
    }
}

/// User entry (like /etc/passwd line)
#[derive(Debug, Clone)]
pub struct User {
    pub name: String,
    pub uid: Uid,
    pub gid: Gid,                      // Primary group
    pub gecos: String,                 // Full name/comment
    pub home: String,                  // Home directory
    pub shell: String,                 // Login shell
    pub password_hash: Option<String>, // None = no password
}

impl User {
    pub fn new(name: &str, uid: Uid, gid: Gid) -> Self {
        Self {
            name: name.to_string(),
            uid,
            gid,
            gecos: String::new(),
            home: format!("/home/{}", name),
            shell: "/bin/sh".to_string(),
            password_hash: None,
        }
    }

    /// Check password against stored hash
    ///
    /// Returns true if:
    /// - No password is set (account allows passwordless login)
    /// - The provided password matches the stored hash
    pub fn check_password(&self, password: &str) -> bool {
        match &self.password_hash {
            None => true, // No password set = allow
            Some(hash) => verify_password(password, hash),
        }
    }

    /// Set password using secure salted hashing
    ///
    /// The password is hashed with:
    /// - A cryptographically random 16-byte salt
    /// - 10,000 rounds of key stretching
    pub fn set_password(&mut self, password: &str) {
        self.password_hash = Some(hash_password(password));
    }

    /// Lock the account (disable password login)
    pub fn lock_account(&mut self) {
        self.password_hash = Some("!".to_string());
    }

    /// Check if account is locked
    pub fn is_locked(&self) -> bool {
        matches!(&self.password_hash, Some(h) if h == "!" || h == "*")
    }
}

/// Group entry (like /etc/group line)
#[derive(Debug, Clone)]
pub struct Group {
    pub name: String,
    pub gid: Gid,
    pub members: Vec<String>, // Usernames
}

impl Group {
    pub fn new(name: &str, gid: Gid) -> Self {
        Self {
            name: name.to_string(),
            gid,
            members: Vec::new(),
        }
    }

    pub fn add_member(&mut self, username: &str) {
        if !self.members.contains(&username.to_string()) {
            self.members.push(username.to_string());
        }
    }

    pub fn remove_member(&mut self, username: &str) {
        self.members.retain(|m| m != username);
    }
}

/// User and group database
#[derive(Debug, Clone)]
pub struct UserDb {
    users: HashMap<Uid, User>,
    users_by_name: HashMap<String, Uid>,
    groups: HashMap<Gid, Group>,
    groups_by_name: HashMap<String, Gid>,
    next_uid: u32,
    next_gid: u32,
}

impl Default for UserDb {
    fn default() -> Self {
        Self::new()
    }
}

impl UserDb {
    /// Create an empty user database (no default users)
    pub fn empty() -> Self {
        Self {
            users: HashMap::new(),
            users_by_name: HashMap::new(),
            groups: HashMap::new(),
            groups_by_name: HashMap::new(),
            next_uid: 1000,
            next_gid: 1000,
        }
    }

    pub fn new() -> Self {
        let mut db = Self {
            users: HashMap::new(),
            users_by_name: HashMap::new(),
            groups: HashMap::new(),
            groups_by_name: HashMap::new(),
            next_uid: 1000, // Start regular users at 1000
            next_gid: 1000,
        };

        // Create root user and group
        db.create_system_user("root", Uid(0), Gid(0), "/root");
        db.create_system_group("root", Gid(0));

        // Create wheel group for sudo
        db.create_system_group("wheel", Gid(10));

        // Create regular user
        let user_gid = Gid(1000);
        db.create_system_group("user", user_gid);
        db.create_system_user("user", Uid(1000), user_gid, "/home/user");

        // Add user to wheel group for sudo access
        if let Some(wheel) = db.groups.get_mut(&Gid(10)) {
            wheel.add_member("user");
        }

        // Create nobody user (for unprivileged operations)
        db.create_system_user("nobody", Uid(65534), Gid(65534), "/nonexistent");
        db.create_system_group("nogroup", Gid(65534));

        db
    }

    fn create_system_user(&mut self, name: &str, uid: Uid, gid: Gid, home: &str) {
        let mut user = User::new(name, uid, gid);
        user.home = home.to_string();
        if name == "root" {
            user.shell = "/bin/sh".to_string();
            // Root account starts with no password (passwordless login allowed)
            // In a real system, you would either:
            // 1. Lock the account until first boot setup
            // 2. Require password setup during installation
            // For this educational OS, we allow passwordless root initially
            // Use `passwd root` to set a password
        }
        self.users.insert(uid, user);
        self.users_by_name.insert(name.to_string(), uid);
    }

    fn create_system_group(&mut self, name: &str, gid: Gid) {
        let group = Group::new(name, gid);
        self.groups.insert(gid, group);
        self.groups_by_name.insert(name.to_string(), gid);
    }

    // ========== /etc/passwd FORMAT ==========
    // Format: name:x:uid:gid:gecos:home:shell

    /// Generate /etc/passwd content
    pub fn to_passwd(&self) -> String {
        let mut lines: Vec<_> = self.users.values().collect();
        lines.sort_by_key(|u| u.uid.0);

        lines
            .iter()
            .map(|u| {
                format!(
                    "{}:x:{}:{}:{}:{}:{}",
                    u.name, u.uid.0, u.gid.0, u.gecos, u.home, u.shell
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    /// Parse /etc/passwd content
    pub fn parse_passwd(&mut self, content: &str) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 7 {
                let name = parts[0];
                let uid = parts[2].parse::<u32>().unwrap_or(65534);
                let gid = parts[3].parse::<u32>().unwrap_or(65534);
                let gecos = parts[4];
                let home = parts[5];
                let shell = parts[6];

                let uid = Uid(uid);
                let gid = Gid(gid);

                // Update next_uid if needed
                if uid.0 >= 1000 && uid.0 < 65534 && uid.0 >= self.next_uid {
                    self.next_uid = uid.0 + 1;
                }

                let mut user = User::new(name, uid, gid);
                user.gecos = gecos.to_string();
                user.home = home.to_string();
                user.shell = shell.to_string();

                self.users.insert(uid, user);
                self.users_by_name.insert(name.to_string(), uid);
            }
        }
    }

    // ========== /etc/shadow FORMAT ==========
    // Format: name:password_hash:lastchange:min:max:warn:inactive:expire:

    /// Generate /etc/shadow content
    pub fn to_shadow(&self) -> String {
        let mut lines: Vec<_> = self.users.values().collect();
        lines.sort_by_key(|u| u.uid.0);

        lines
            .iter()
            .map(|u| {
                let hash = u.password_hash.as_deref().unwrap_or("!");
                format!("{}:{}:19000:0:99999:7:::", u.name, hash)
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    /// Parse /etc/shadow content
    pub fn parse_shadow(&mut self, content: &str) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 2 {
                let name = parts[0];
                let hash = parts[1];

                if let Some(user) = self.get_user_by_name_mut(name) {
                    if hash == "!" || hash == "*" || hash.is_empty() {
                        user.password_hash = None;
                    } else {
                        user.password_hash = Some(hash.to_string());
                    }
                }
            }
        }
    }

    // ========== /etc/group FORMAT ==========
    // Format: name:x:gid:member1,member2,...

    /// Generate /etc/group content
    pub fn to_group(&self) -> String {
        let mut lines: Vec<_> = self.groups.values().collect();
        lines.sort_by_key(|g| g.gid.0);

        lines
            .iter()
            .map(|g| format!("{}:x:{}:{}", g.name, g.gid.0, g.members.join(",")))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    /// Parse /etc/group content
    pub fn parse_group(&mut self, content: &str) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 4 {
                let name = parts[0];
                let gid = parts[2].parse::<u32>().unwrap_or(65534);
                let members_str = parts[3];

                let gid = Gid(gid);

                // Update next_gid if needed
                if gid.0 >= 1000 && gid.0 < 65534 && gid.0 >= self.next_gid {
                    self.next_gid = gid.0 + 1;
                }

                let mut group = Group::new(name, gid);
                if !members_str.is_empty() {
                    group.members = members_str.split(',').map(|s| s.to_string()).collect();
                }

                self.groups.insert(gid, group);
                self.groups_by_name.insert(name.to_string(), gid);
            }
        }
    }

    /// Add a new user
    pub fn add_user(&mut self, name: &str, gid: Option<Gid>) -> Result<Uid, &'static str> {
        if self.users_by_name.contains_key(name) {
            return Err("User already exists");
        }

        let uid = Uid(self.next_uid);
        self.next_uid += 1;

        // Use provided gid or create a new group with same name
        let gid = gid.unwrap_or_else(|| {
            let gid = Gid(self.next_gid);
            self.next_gid += 1;
            let group = Group::new(name, gid);
            self.groups.insert(gid, group.clone());
            self.groups_by_name.insert(name.to_string(), gid);
            gid
        });

        let user = User::new(name, uid, gid);
        self.users.insert(uid, user);
        self.users_by_name.insert(name.to_string(), uid);

        Ok(uid)
    }

    /// Add a new group
    pub fn add_group(&mut self, name: &str) -> Result<Gid, &'static str> {
        if self.groups_by_name.contains_key(name) {
            return Err("Group already exists");
        }

        let gid = Gid(self.next_gid);
        self.next_gid += 1;

        let group = Group::new(name, gid);
        self.groups.insert(gid, group);
        self.groups_by_name.insert(name.to_string(), gid);

        Ok(gid)
    }

    /// Look up user by UID
    pub fn get_user(&self, uid: Uid) -> Option<&User> {
        self.users.get(&uid)
    }

    /// Look up user by name
    pub fn get_user_by_name(&self, name: &str) -> Option<&User> {
        self.users_by_name
            .get(name)
            .and_then(|uid| self.users.get(uid))
    }

    /// Look up user mutably by UID
    pub fn get_user_mut(&mut self, uid: Uid) -> Option<&mut User> {
        self.users.get_mut(&uid)
    }

    /// Look up user mutably by name
    pub fn get_user_by_name_mut(&mut self, name: &str) -> Option<&mut User> {
        let uid = self.users_by_name.get(name).copied()?;
        self.users.get_mut(&uid)
    }

    /// Look up group by GID
    pub fn get_group(&self, gid: Gid) -> Option<&Group> {
        self.groups.get(&gid)
    }

    /// Look up group by name
    pub fn get_group_by_name(&self, name: &str) -> Option<&Group> {
        self.groups_by_name
            .get(name)
            .and_then(|gid| self.groups.get(gid))
    }

    /// Look up group mutably by GID
    pub fn get_group_mut(&mut self, gid: Gid) -> Option<&mut Group> {
        self.groups.get_mut(&gid)
    }

    /// Get all groups a user belongs to
    pub fn get_user_groups(&self, username: &str) -> Vec<Gid> {
        let mut groups = Vec::new();

        // Primary group
        if let Some(user) = self.get_user_by_name(username) {
            groups.push(user.gid);
        }

        // Supplementary groups
        for (gid, group) in &self.groups {
            if group.members.contains(&username.to_string()) && !groups.contains(gid) {
                groups.push(*gid);
            }
        }

        groups
    }

    /// List all users
    pub fn list_users(&self) -> Vec<&User> {
        let mut users: Vec<_> = self.users.values().collect();
        users.sort_by_key(|u| u.uid.0);
        users
    }

    /// List all groups
    pub fn list_groups(&self) -> Vec<&Group> {
        let mut groups: Vec<_> = self.groups.values().collect();
        groups.sort_by_key(|g| g.gid.0);
        groups
    }

    /// Check if user is in wheel group (can sudo)
    pub fn can_sudo(&self, username: &str) -> bool {
        if let Some(wheel) = self.get_group_by_name("wheel") {
            wheel.members.contains(&username.to_string())
        } else {
            false
        }
    }
}

/// Password hashing configuration
const HASH_ROUNDS: u32 = 10_000;
const SALT_LENGTH: usize = 16;

/// Generate cryptographically random bytes for salt
fn generate_salt() -> [u8; SALT_LENGTH] {
    let mut salt = [0u8; SALT_LENGTH];
    // Use getrandom which works in both native and WASM environments
    if getrandom::fill(&mut salt).is_err() {
        // Fallback: use a timestamp-based seed (less secure, but better than nothing)
        // This should rarely happen as getrandom supports WASM with wasm_js feature
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for (i, byte) in salt.iter_mut().enumerate() {
            *byte = ((now >> (i * 8)) & 0xff) as u8;
        }
    }
    salt
}

/// Hash a password with a given salt using key stretching
///
/// Uses a simple but effective key-stretching approach:
/// - Combines password and salt
/// - Applies multiple rounds of hashing to slow brute-force attacks
fn hash_with_salt(password: &str, salt: &[u8]) -> [u8; 32] {
    // Initial state: combine password bytes and salt
    let mut state = [0u8; 32];

    // Mix in password
    for (i, byte) in password.bytes().enumerate() {
        state[i % 32] ^= byte;
        state[(i + 17) % 32] = state[(i + 17) % 32].wrapping_add(byte);
    }

    // Mix in salt
    for (i, byte) in salt.iter().enumerate() {
        state[(i + 7) % 32] ^= byte;
        state[(i + 23) % 32] = state[(i + 23) % 32].wrapping_add(*byte);
    }

    // Key stretching: multiple rounds of mixing
    for round in 0..HASH_ROUNDS {
        let round_byte = (round & 0xff) as u8;

        // Forward pass
        for i in 0..32 {
            let prev = state[(i + 31) % 32];
            let next = state[(i + 1) % 32];
            state[i] = state[i]
                .wrapping_add(prev)
                .wrapping_mul(33)
                .wrapping_add(next)
                .wrapping_add(round_byte);
        }

        // Backward pass for better diffusion
        for i in (0..32).rev() {
            let prev = state[(i + 1) % 32];
            let salt_byte = salt[i % salt.len()];
            state[i] = state[i]
                .wrapping_mul(17)
                .wrapping_add(prev)
                .wrapping_add(salt_byte);
        }
    }

    state
}

/// Hash a password with a new random salt
/// Returns the hash in format "salt_hex:hash_hex"
fn hash_password(password: &str) -> String {
    let salt = generate_salt();
    let hash = hash_with_salt(password, &salt);

    // Format: salt_hex:hash_hex
    let salt_hex: String = salt.iter().map(|b| format!("{:02x}", b)).collect();
    let hash_hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();

    format!("{}:{}", salt_hex, hash_hex)
}

/// Verify a password against a stored hash
/// Expects hash in format "salt_hex:hash_hex"
fn verify_password(password: &str, stored_hash: &str) -> bool {
    let parts: Vec<&str> = stored_hash.split(':').collect();

    if parts.len() != 2 {
        // Invalid format - check if this is a legacy DJB2 hash (16 hex chars)
        // For backwards compatibility during migration
        if stored_hash.len() == 16 && stored_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return legacy_hash(password) == stored_hash;
        }
        return false;
    }

    let salt_hex = parts[0];
    let expected_hash_hex = parts[1];

    // Parse salt from hex
    if salt_hex.len() != SALT_LENGTH * 2 {
        return false;
    }

    let mut salt = [0u8; SALT_LENGTH];
    for (i, chunk) in salt_hex.as_bytes().chunks(2).enumerate() {
        if i >= SALT_LENGTH {
            break;
        }
        let hex_str = std::str::from_utf8(chunk).unwrap_or("00");
        salt[i] = u8::from_str_radix(hex_str, 16).unwrap_or(0);
    }

    // Compute hash with extracted salt
    let computed_hash = hash_with_salt(password, &salt);
    let computed_hex: String = computed_hash.iter().map(|b| format!("{:02x}", b)).collect();

    // Constant-time comparison to prevent timing attacks
    constant_time_compare(&computed_hex, expected_hash_hex)
}

/// Constant-time string comparison to prevent timing attacks
fn constant_time_compare(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (byte_a, byte_b) in a.bytes().zip(b.bytes()) {
        result |= byte_a ^ byte_b;
    }

    result == 0
}

/// Legacy DJB2 hash for backwards compatibility
/// Only used to verify old passwords - new passwords use the secure hash
fn legacy_hash(password: &str) -> String {
    let mut hash: u64 = 5381;
    for byte in password.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    format!("{:016x}", hash)
}

/// Check if a user can access a file with given permissions
#[allow(clippy::too_many_arguments)]
pub fn check_permission(
    file_uid: Uid,
    file_gid: Gid,
    file_mode: FileMode,
    user_uid: Uid,
    user_gid: Gid,
    user_groups: &[Gid],
    want_read: bool,
    want_write: bool,
    want_exec: bool,
) -> bool {
    // Root can do anything
    if user_uid == Uid::ROOT {
        return true;
    }

    // Determine which permission bits to check
    let (r, w, x) = if user_uid == file_uid {
        // Owner permissions
        (
            file_mode.owner_read(),
            file_mode.owner_write(),
            file_mode.owner_exec(),
        )
    } else if user_gid == file_gid || user_groups.contains(&file_gid) {
        // Group permissions
        (
            file_mode.group_read(),
            file_mode.group_write(),
            file_mode.group_exec(),
        )
    } else {
        // Other permissions
        (
            file_mode.other_read(),
            file_mode.other_write(),
            file_mode.other_exec(),
        )
    };

    // Check requested permissions
    (!want_read || r) && (!want_write || w) && (!want_exec || x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_mode_symbolic() {
        assert_eq!(FileMode(0o755).to_symbolic(), "rwxr-xr-x");
        assert_eq!(FileMode(0o644).to_symbolic(), "rw-r--r--");
        assert_eq!(FileMode(0o000).to_symbolic(), "---------");
        assert_eq!(FileMode(0o777).to_symbolic(), "rwxrwxrwx");
    }

    #[test]
    fn test_file_mode_setuid() {
        assert_eq!(FileMode(0o4755).to_symbolic(), "rwsr-xr-x");
        assert_eq!(FileMode(0o4655).to_symbolic(), "rwSr-xr-x"); // setuid without exec
    }

    #[test]
    fn test_user_db_default_users() {
        let db = UserDb::new();

        assert!(db.get_user_by_name("root").is_some());
        assert!(db.get_user_by_name("user").is_some());

        let root = db.get_user_by_name("root").unwrap();
        assert_eq!(root.uid, Uid::ROOT);
    }

    #[test]
    fn test_add_user() {
        let mut db = UserDb::new();
        let uid = db.add_user("testuser", None).unwrap();

        let user = db.get_user(uid).unwrap();
        assert_eq!(user.name, "testuser");
    }

    #[test]
    fn test_password() {
        let mut user = User::new("test", Uid(1), Gid(1));

        // No password set
        assert!(user.check_password("anything"));

        // Set password with secure hashing
        user.set_password("secret");
        assert!(user.check_password("secret"));
        assert!(!user.check_password("wrong"));
        assert!(!user.check_password("")); // Empty password should fail
        assert!(!user.check_password("Secret")); // Case sensitive
    }

    #[test]
    fn test_password_hash_format() {
        // Test that password hashes have correct format (salt:hash)
        let hash = hash_password("testpassword");
        let parts: Vec<&str> = hash.split(':').collect();
        assert_eq!(parts.len(), 2, "Hash should be in salt:hash format");
        assert_eq!(parts[0].len(), 32, "Salt should be 32 hex chars (16 bytes)");
        assert_eq!(parts[1].len(), 64, "Hash should be 64 hex chars (32 bytes)");
    }

    #[test]
    fn test_password_uniqueness() {
        // Same password should produce different hashes (due to random salt)
        let hash1 = hash_password("samepassword");
        let hash2 = hash_password("samepassword");
        assert_ne!(
            hash1, hash2,
            "Same password should produce different hashes"
        );

        // But both should verify correctly
        assert!(verify_password("samepassword", &hash1));
        assert!(verify_password("samepassword", &hash2));
    }

    #[test]
    fn test_account_locking() {
        let mut user = User::new("test", Uid(1), Gid(1));

        // Set password first
        user.set_password("secret");
        assert!(user.check_password("secret"));
        assert!(!user.is_locked());

        // Lock account
        user.lock_account();
        assert!(user.is_locked());
        assert!(!user.check_password("secret")); // Can't login to locked account
    }

    #[test]
    fn test_legacy_hash_compatibility() {
        // Test that legacy DJB2 hashes still work for verification
        let legacy = legacy_hash("oldpassword");
        assert!(verify_password("oldpassword", &legacy));
        assert!(!verify_password("wrongpassword", &legacy));
    }

    #[test]
    fn test_permission_check() {
        // Owner can read/write, group can read, others nothing
        let mode = FileMode(0o640);
        let file_uid = Uid(1000);
        let file_gid = Gid(1000);

        // Owner
        assert!(check_permission(
            file_uid,
            file_gid,
            mode,
            Uid(1000),
            Gid(1000),
            &[],
            true,
            false,
            false
        ));
        assert!(check_permission(
            file_uid,
            file_gid,
            mode,
            Uid(1000),
            Gid(1000),
            &[],
            true,
            true,
            false
        ));

        // Group member
        assert!(check_permission(
            file_uid,
            file_gid,
            mode,
            Uid(1001),
            Gid(1000),
            &[],
            true,
            false,
            false
        ));
        assert!(!check_permission(
            file_uid,
            file_gid,
            mode,
            Uid(1001),
            Gid(1000),
            &[],
            true,
            true,
            false
        ));

        // Other
        assert!(!check_permission(
            file_uid,
            file_gid,
            mode,
            Uid(1001),
            Gid(1001),
            &[],
            true,
            false,
            false
        ));

        // Root can do anything
        assert!(check_permission(
            file_uid,
            file_gid,
            mode,
            Uid::ROOT,
            Gid::ROOT,
            &[],
            true,
            true,
            true
        ));
    }
}
