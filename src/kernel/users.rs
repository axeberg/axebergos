//! User and Group Management
//!
//! Provides Unix-like user and group abstractions:
//! - User database (like /etc/passwd)
//! - Group database (like /etc/group)
//! - Password hashing (simplified)
//! - User/group lookups

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
    pub const FILE_DEFAULT: FileMode = FileMode(0o644);   // rw-r--r--
    pub const DIR_DEFAULT: FileMode = FileMode(0o755);    // rwxr-xr-x
    pub const EXEC_DEFAULT: FileMode = FileMode(0o755);   // rwxr-xr-x

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
        } else {
            if self.owner_exec() { 'x' } else { '-' }
        });
        s.push(if self.group_read() { 'r' } else { '-' });
        s.push(if self.group_write() { 'w' } else { '-' });
        s.push(if self.is_setgid() {
            if self.group_exec() { 's' } else { 'S' }
        } else {
            if self.group_exec() { 'x' } else { '-' }
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
    pub gid: Gid,           // Primary group
    pub gecos: String,      // Full name/comment
    pub home: String,       // Home directory
    pub shell: String,      // Login shell
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

    /// Check password (simplified hash)
    pub fn check_password(&self, password: &str) -> bool {
        match &self.password_hash {
            None => true, // No password set = allow
            Some(hash) => simple_hash(password) == *hash,
        }
    }

    /// Set password
    pub fn set_password(&mut self, password: &str) {
        self.password_hash = Some(simple_hash(password));
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
        }
        self.users.insert(uid, user);
        self.users_by_name.insert(name.to_string(), uid);
    }

    fn create_system_group(&mut self, name: &str, gid: Gid) {
        let group = Group::new(name, gid);
        self.groups.insert(gid, group);
        self.groups_by_name.insert(name.to_string(), gid);
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
        self.users_by_name.get(name).and_then(|uid| self.users.get(uid))
    }

    /// Look up user mutably by UID
    pub fn get_user_mut(&mut self, uid: Uid) -> Option<&mut User> {
        self.users.get_mut(&uid)
    }

    /// Look up group by GID
    pub fn get_group(&self, gid: Gid) -> Option<&Group> {
        self.groups.get(&gid)
    }

    /// Look up group by name
    pub fn get_group_by_name(&self, name: &str) -> Option<&Group> {
        self.groups_by_name.get(name).and_then(|gid| self.groups.get(gid))
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

/// Simple password hashing (NOT cryptographically secure, just for demo)
fn simple_hash(password: &str) -> String {
    let mut hash: u64 = 5381;
    for byte in password.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    format!("{:016x}", hash)
}

/// Check if a user can access a file with given permissions
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
        (file_mode.owner_read(), file_mode.owner_write(), file_mode.owner_exec())
    } else if user_gid == file_gid || user_groups.contains(&file_gid) {
        // Group permissions
        (file_mode.group_read(), file_mode.group_write(), file_mode.group_exec())
    } else {
        // Other permissions
        (file_mode.other_read(), file_mode.other_write(), file_mode.other_exec())
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

        // Set password
        user.set_password("secret");
        assert!(user.check_password("secret"));
        assert!(!user.check_password("wrong"));
    }

    #[test]
    fn test_permission_check() {
        // Owner can read/write, group can read, others nothing
        let mode = FileMode(0o640);
        let file_uid = Uid(1000);
        let file_gid = Gid(1000);

        // Owner
        assert!(check_permission(file_uid, file_gid, mode, Uid(1000), Gid(1000), &[], true, false, false));
        assert!(check_permission(file_uid, file_gid, mode, Uid(1000), Gid(1000), &[], true, true, false));

        // Group member
        assert!(check_permission(file_uid, file_gid, mode, Uid(1001), Gid(1000), &[], true, false, false));
        assert!(!check_permission(file_uid, file_gid, mode, Uid(1001), Gid(1000), &[], true, true, false));

        // Other
        assert!(!check_permission(file_uid, file_gid, mode, Uid(1001), Gid(1001), &[], true, false, false));

        // Root can do anything
        assert!(check_permission(file_uid, file_gid, mode, Uid::ROOT, Gid::ROOT, &[], true, true, true));
    }
}
