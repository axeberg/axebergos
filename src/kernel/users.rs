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

    /// Check if sticky bit is set
    pub fn is_sticky(&self) -> bool {
        self.0 & Self::S_ISVTX != 0
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

// ============================================================================
// POSIX Capabilities
// ============================================================================

/// Linux-style capability for fine-grained privilege control
///
/// Capabilities split the traditional "root can do everything" model into
/// distinct privileges. A process may have some capabilities but not others.
///
/// Based on POSIX 1003.1e draft and Linux capabilities(7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Capability {
    /// Override file read/search permission checks (DAC = Discretionary Access Control)
    /// Allows reading any file regardless of permission bits
    DacReadSearch = 0,

    /// Override all DAC access, including write access
    /// Allows reading/writing any file regardless of permission bits
    DacOverride = 1,

    /// Bypass file ownership checks for chown(2)
    /// Allows changing file owner to any user
    Chown = 2,

    /// Don't clear setuid/setgid bits when modifying files
    /// Allows preserving setuid/setgid on chown/write
    Fsetid = 3,

    /// Bypass permission checks for file owner operations
    /// Allows operations that require file ownership
    Fowner = 4,

    /// Allow setting file capabilities (extended attributes)
    SetFcap = 5,

    /// Allow killing any process (bypass permission checks)
    Kill = 6,

    /// Allow setgid(2), setgroups(2), and forged gids in socket credentials
    Setgid = 7,

    /// Allow setuid(2), setreuid(2), etc.
    Setuid = 8,

    /// Allow transferring/forging process capabilities
    Setpcap = 9,

    /// Allow binding to privileged ports (< 1024)
    NetBindService = 10,

    /// Allow raw socket creation
    NetRaw = 11,

    /// Allow network administration (interface config, routing, etc.)
    NetAdmin = 12,

    /// Allow system administration operations
    /// Includes: mount, sethostname, reboot, etc.
    SysAdmin = 13,

    /// Allow chroot(2)
    SysChroot = 14,

    /// Allow ptrace(2) on any process
    SysPtrace = 15,

    /// Allow nice(2), setpriority(2) to raise priority
    SysNice = 16,

    /// Allow resource limit modifications (setrlimit for other processes)
    SysResource = 17,

    /// Allow setting system time
    SysTime = 18,

    /// Allow tty configuration (vhangup, ioctl on ttys)
    SysTtyConfig = 19,

    /// Allow loading/unloading kernel modules
    SysModule = 20,

    /// Allow system boot operations
    SysBoot = 21,

    /// Allow I/O port operations
    SysRawio = 22,

    /// Allow triggering panic/reboot
    SysPanic = 23,
}

impl Capability {
    /// Total number of capabilities
    pub const COUNT: usize = 24;

    /// Get the capability name as a string
    pub fn name(self) -> &'static str {
        match self {
            Capability::DacReadSearch => "CAP_DAC_READ_SEARCH",
            Capability::DacOverride => "CAP_DAC_OVERRIDE",
            Capability::Chown => "CAP_CHOWN",
            Capability::Fsetid => "CAP_FSETID",
            Capability::Fowner => "CAP_FOWNER",
            Capability::SetFcap => "CAP_SETFCAP",
            Capability::Kill => "CAP_KILL",
            Capability::Setgid => "CAP_SETGID",
            Capability::Setuid => "CAP_SETUID",
            Capability::Setpcap => "CAP_SETPCAP",
            Capability::NetBindService => "CAP_NET_BIND_SERVICE",
            Capability::NetRaw => "CAP_NET_RAW",
            Capability::NetAdmin => "CAP_NET_ADMIN",
            Capability::SysAdmin => "CAP_SYS_ADMIN",
            Capability::SysChroot => "CAP_SYS_CHROOT",
            Capability::SysPtrace => "CAP_SYS_PTRACE",
            Capability::SysNice => "CAP_SYS_NICE",
            Capability::SysResource => "CAP_SYS_RESOURCE",
            Capability::SysTime => "CAP_SYS_TIME",
            Capability::SysTtyConfig => "CAP_SYS_TTY_CONFIG",
            Capability::SysModule => "CAP_SYS_MODULE",
            Capability::SysBoot => "CAP_SYS_BOOT",
            Capability::SysRawio => "CAP_SYS_RAWIO",
            Capability::SysPanic => "CAP_SYS_PANIC",
        }
    }

    /// Parse capability from name (case-insensitive, with or without CAP_ prefix)
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.to_uppercase();
        let name = name.strip_prefix("CAP_").unwrap_or(&name);

        match name {
            "DAC_READ_SEARCH" => Some(Capability::DacReadSearch),
            "DAC_OVERRIDE" => Some(Capability::DacOverride),
            "CHOWN" => Some(Capability::Chown),
            "FSETID" => Some(Capability::Fsetid),
            "FOWNER" => Some(Capability::Fowner),
            "SETFCAP" => Some(Capability::SetFcap),
            "KILL" => Some(Capability::Kill),
            "SETGID" => Some(Capability::Setgid),
            "SETUID" => Some(Capability::Setuid),
            "SETPCAP" => Some(Capability::Setpcap),
            "NET_BIND_SERVICE" => Some(Capability::NetBindService),
            "NET_RAW" => Some(Capability::NetRaw),
            "NET_ADMIN" => Some(Capability::NetAdmin),
            "SYS_ADMIN" => Some(Capability::SysAdmin),
            "SYS_CHROOT" => Some(Capability::SysChroot),
            "SYS_PTRACE" => Some(Capability::SysPtrace),
            "SYS_NICE" => Some(Capability::SysNice),
            "SYS_RESOURCE" => Some(Capability::SysResource),
            "SYS_TIME" => Some(Capability::SysTime),
            "SYS_TTY_CONFIG" => Some(Capability::SysTtyConfig),
            "SYS_MODULE" => Some(Capability::SysModule),
            "SYS_BOOT" => Some(Capability::SysBoot),
            "SYS_RAWIO" => Some(Capability::SysRawio),
            "SYS_PANIC" => Some(Capability::SysPanic),
            _ => None,
        }
    }

    /// Get capability from its numeric value
    pub fn from_u8(n: u8) -> Option<Self> {
        match n {
            0 => Some(Capability::DacReadSearch),
            1 => Some(Capability::DacOverride),
            2 => Some(Capability::Chown),
            3 => Some(Capability::Fsetid),
            4 => Some(Capability::Fowner),
            5 => Some(Capability::SetFcap),
            6 => Some(Capability::Kill),
            7 => Some(Capability::Setgid),
            8 => Some(Capability::Setuid),
            9 => Some(Capability::Setpcap),
            10 => Some(Capability::NetBindService),
            11 => Some(Capability::NetRaw),
            12 => Some(Capability::NetAdmin),
            13 => Some(Capability::SysAdmin),
            14 => Some(Capability::SysChroot),
            15 => Some(Capability::SysPtrace),
            16 => Some(Capability::SysNice),
            17 => Some(Capability::SysResource),
            18 => Some(Capability::SysTime),
            19 => Some(Capability::SysTtyConfig),
            20 => Some(Capability::SysModule),
            21 => Some(Capability::SysBoot),
            22 => Some(Capability::SysRawio),
            23 => Some(Capability::SysPanic),
            _ => None,
        }
    }

    /// Iterator over all capabilities
    pub fn all() -> impl Iterator<Item = Capability> {
        (0..Self::COUNT as u8).filter_map(Self::from_u8)
    }
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// A set of capabilities represented as a bitfield
///
/// Linux uses three sets per process:
/// - Permitted: The capabilities the process is allowed to use
/// - Effective: The capabilities currently active (checked for operations)
/// - Inheritable: The capabilities that can be passed to child processes
///
/// This implementation uses a u32 bitfield (supports up to 32 capabilities).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CapabilitySet(pub u32);

impl CapabilitySet {
    /// Empty capability set
    pub const EMPTY: CapabilitySet = CapabilitySet(0);

    /// All capabilities set (root-equivalent)
    pub const ALL: CapabilitySet = CapabilitySet((1 << Capability::COUNT) - 1);

    /// Create a new empty capability set
    pub fn new() -> Self {
        Self::EMPTY
    }

    /// Create a capability set with all capabilities
    pub fn all() -> Self {
        Self::ALL
    }

    /// Check if a capability is in the set
    pub fn has(&self, cap: Capability) -> bool {
        self.0 & (1 << cap as u32) != 0
    }

    /// Add a capability to the set
    pub fn add(&mut self, cap: Capability) {
        self.0 |= 1 << cap as u32;
    }

    /// Remove a capability from the set
    pub fn remove(&mut self, cap: Capability) {
        self.0 &= !(1 << cap as u32);
    }

    /// Set whether a capability is in the set
    pub fn set(&mut self, cap: Capability, value: bool) {
        if value {
            self.add(cap);
        } else {
            self.remove(cap);
        }
    }

    /// Clear all capabilities
    pub fn clear(&mut self) {
        self.0 = 0;
    }

    /// Set all capabilities
    pub fn set_all(&mut self) {
        self.0 = Self::ALL.0;
    }

    /// Union of two capability sets
    pub fn union(&self, other: &CapabilitySet) -> CapabilitySet {
        CapabilitySet(self.0 | other.0)
    }

    /// Intersection of two capability sets
    pub fn intersection(&self, other: &CapabilitySet) -> CapabilitySet {
        CapabilitySet(self.0 & other.0)
    }

    /// Difference (self - other)
    pub fn difference(&self, other: &CapabilitySet) -> CapabilitySet {
        CapabilitySet(self.0 & !other.0)
    }

    /// Check if this set is a subset of another
    pub fn is_subset_of(&self, other: &CapabilitySet) -> bool {
        (self.0 & other.0) == self.0
    }

    /// Check if the set is empty
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// Count the number of capabilities in the set
    pub fn count(&self) -> usize {
        self.0.count_ones() as usize
    }

    /// Iterate over capabilities in the set
    pub fn iter(&self) -> impl Iterator<Item = Capability> + '_ {
        Capability::all().filter(|cap| self.has(*cap))
    }

    /// Create from raw bits
    pub fn from_bits(bits: u32) -> Self {
        CapabilitySet(bits & Self::ALL.0)
    }

    /// Get raw bits
    pub fn bits(&self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for CapabilitySet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_empty() {
            write!(f, "(none)")
        } else if *self == Self::ALL {
            write!(f, "(all)")
        } else {
            let caps: Vec<_> = self.iter().map(|c| c.name()).collect();
            write!(f, "{}", caps.join(","))
        }
    }
}

impl From<Capability> for CapabilitySet {
    fn from(cap: Capability) -> Self {
        let mut set = CapabilitySet::new();
        set.add(cap);
        set
    }
}

impl FromIterator<Capability> for CapabilitySet {
    fn from_iter<I: IntoIterator<Item = Capability>>(iter: I) -> Self {
        let mut set = CapabilitySet::new();
        for cap in iter {
            set.add(cap);
        }
        set
    }
}

/// Capability state for a process
///
/// Each process has three capability sets:
/// - `permitted`: Maximum capabilities the process can use
/// - `effective`: Currently active capabilities (actually checked)
/// - `inheritable`: Capabilities preserved across execve()
///
/// A capability must be in `permitted` to be added to `effective`.
/// The `inheritable` set controls what capabilities child processes can inherit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessCapabilities {
    /// Maximum capabilities the process is allowed to have
    pub permitted: CapabilitySet,

    /// Currently active capabilities (used for permission checks)
    pub effective: CapabilitySet,

    /// Capabilities that can be inherited by child processes
    pub inheritable: CapabilitySet,
}

impl ProcessCapabilities {
    /// Create empty capabilities (unprivileged)
    pub fn new() -> Self {
        Self {
            permitted: CapabilitySet::EMPTY,
            effective: CapabilitySet::EMPTY,
            inheritable: CapabilitySet::EMPTY,
        }
    }

    /// Create full capabilities (root-equivalent)
    pub fn root() -> Self {
        Self {
            permitted: CapabilitySet::ALL,
            effective: CapabilitySet::ALL,
            inheritable: CapabilitySet::ALL,
        }
    }

    /// Create capabilities for a specific UID
    /// Root (UID 0) gets all capabilities, others get none
    pub fn for_uid(uid: Uid) -> Self {
        if uid == Uid::ROOT {
            Self::root()
        } else {
            Self::new()
        }
    }

    /// Check if a capability is effective (active)
    pub fn has_effective(&self, cap: Capability) -> bool {
        self.effective.has(cap)
    }

    /// Check if a capability is permitted
    pub fn has_permitted(&self, cap: Capability) -> bool {
        self.permitted.has(cap)
    }

    /// Check if a capability is inheritable
    pub fn has_inheritable(&self, cap: Capability) -> bool {
        self.inheritable.has(cap)
    }

    /// Raise a capability (add to effective if permitted)
    /// Returns false if the capability is not in permitted set
    pub fn raise(&mut self, cap: Capability) -> bool {
        if self.permitted.has(cap) {
            self.effective.add(cap);
            true
        } else {
            false
        }
    }

    /// Lower a capability (remove from effective)
    pub fn lower(&mut self, cap: Capability) {
        self.effective.remove(cap);
    }

    /// Drop a capability permanently (remove from all sets)
    pub fn drop_cap(&mut self, cap: Capability) {
        self.permitted.remove(cap);
        self.effective.remove(cap);
        self.inheritable.remove(cap);
    }

    /// Clear all capabilities
    pub fn clear(&mut self) {
        self.permitted.clear();
        self.effective.clear();
        self.inheritable.clear();
    }

    /// Calculate capabilities for a child process after fork()
    /// Child inherits parent's capabilities unchanged
    pub fn for_fork(&self) -> Self {
        *self
    }

    /// Calculate capabilities after exec() with no file capabilities
    ///
    /// By default, exec clears effective and permitted capabilities unless
    /// the process has CAP_SETPCAP or is running as root.
    ///
    /// For root processes: P'(effective) = P'(permitted) = P(inheritable) | F(permitted)
    /// For non-root: P'(effective) = P'(permitted) = P(inheritable) & F(permitted)
    ///
    /// Since we're not implementing file capabilities yet, F(permitted) = 0,
    /// so non-root processes lose their capabilities on exec.
    pub fn for_exec(&self, is_root: bool) -> Self {
        if is_root {
            // Root preserves capabilities across exec
            Self {
                permitted: self.inheritable,
                effective: self.inheritable,
                inheritable: self.inheritable,
            }
        } else {
            // Non-root loses capabilities unless the file has capabilities
            // Since we don't have file capabilities, clear everything
            Self::new()
        }
    }

    /// Calculate capabilities after exec() with file capabilities
    ///
    /// Uses the Linux capability transformation formula:
    /// P'(permitted) = (P(inheritable) & F(inheritable)) | (F(permitted) & cap_bset)
    /// P'(effective) = F(effective) ? P'(permitted) : 0
    /// P'(inheritable) = P(inheritable)
    pub fn for_exec_with_file_caps(
        &self,
        file_permitted: CapabilitySet,
        file_inheritable: CapabilitySet,
        file_effective: bool,
    ) -> Self {
        // Calculate new permitted: (P(inh) & F(inh)) | F(perm)
        let new_permitted = self
            .inheritable
            .intersection(&file_inheritable)
            .union(&file_permitted);

        // New effective is either all of permitted (if file has effective bit) or empty
        let new_effective = if file_effective {
            new_permitted
        } else {
            CapabilitySet::EMPTY
        };

        Self {
            permitted: new_permitted,
            effective: new_effective,
            inheritable: self.inheritable,
        }
    }
}

impl Default for ProcessCapabilities {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ProcessCapabilities {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "eff={} perm={} inh={}",
            self.effective, self.permitted, self.inheritable
        )
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

/// Check if a user can access a file with given permissions, considering capabilities
///
/// This is an enhanced version of `check_permission` that also checks relevant
/// capabilities like CAP_DAC_OVERRIDE and CAP_DAC_READ_SEARCH.
///
/// Capability effects:
/// - CAP_DAC_OVERRIDE: Bypass all DAC permission checks (read, write, execute)
/// - CAP_DAC_READ_SEARCH: Bypass read permission and directory search/execute checks
/// - CAP_FOWNER: Bypass permission checks for operations that require file ownership
#[allow(clippy::too_many_arguments)]
pub fn check_permission_with_caps(
    file_uid: Uid,
    file_gid: Gid,
    file_mode: FileMode,
    user_uid: Uid,
    user_gid: Gid,
    user_groups: &[Gid],
    caps: &ProcessCapabilities,
    want_read: bool,
    want_write: bool,
    want_exec: bool,
) -> bool {
    // Root can do anything (traditional behavior)
    if user_uid == Uid::ROOT {
        return true;
    }

    // CAP_DAC_OVERRIDE bypasses all file permission checks
    if caps.has_effective(Capability::DacOverride) {
        return true;
    }

    // CAP_DAC_READ_SEARCH bypasses read and directory search (execute) checks
    // Note: It does NOT bypass write permission checks
    if caps.has_effective(Capability::DacReadSearch) && !want_write {
        return true;
    }

    // CAP_FOWNER bypasses permission checks requiring file ownership
    // (for owner permission bits, process is treated as file owner)
    let is_owner = user_uid == file_uid || caps.has_effective(Capability::Fowner);

    // Determine which permission bits to check
    let (r, w, x) = if is_owner {
        // Owner permissions (or CAP_FOWNER holder)
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

    // ========== CAPABILITY TESTS ==========

    #[test]
    fn test_capability_basic() {
        // Test single capability operations
        let mut set = CapabilitySet::new();
        assert!(set.is_empty());
        assert!(!set.has(Capability::DacOverride));

        set.add(Capability::DacOverride);
        assert!(!set.is_empty());
        assert!(set.has(Capability::DacOverride));
        assert_eq!(set.count(), 1);

        set.remove(Capability::DacOverride);
        assert!(set.is_empty());
        assert!(!set.has(Capability::DacOverride));
    }

    #[test]
    fn test_capability_set_operations() {
        let mut set1 = CapabilitySet::new();
        set1.add(Capability::DacOverride);
        set1.add(Capability::Setuid);

        let mut set2 = CapabilitySet::new();
        set2.add(Capability::Setuid);
        set2.add(Capability::Kill);

        // Union
        let union = set1.union(&set2);
        assert!(union.has(Capability::DacOverride));
        assert!(union.has(Capability::Setuid));
        assert!(union.has(Capability::Kill));
        assert_eq!(union.count(), 3);

        // Intersection
        let inter = set1.intersection(&set2);
        assert!(!inter.has(Capability::DacOverride));
        assert!(inter.has(Capability::Setuid));
        assert!(!inter.has(Capability::Kill));
        assert_eq!(inter.count(), 1);

        // Difference
        let diff = set1.difference(&set2);
        assert!(diff.has(Capability::DacOverride));
        assert!(!diff.has(Capability::Setuid));
        assert!(!diff.has(Capability::Kill));
    }

    #[test]
    fn test_capability_all() {
        let all = CapabilitySet::ALL;
        assert_eq!(all.count(), Capability::COUNT);
        assert!(all.has(Capability::DacOverride));
        assert!(all.has(Capability::SysAdmin));
        assert!(all.has(Capability::Setuid));
    }

    #[test]
    fn test_capability_subset() {
        let mut small = CapabilitySet::new();
        small.add(Capability::DacOverride);

        let mut large = CapabilitySet::new();
        large.add(Capability::DacOverride);
        large.add(Capability::Setuid);

        assert!(small.is_subset_of(&large));
        assert!(!large.is_subset_of(&small));
        assert!(small.is_subset_of(&CapabilitySet::ALL));
        assert!(CapabilitySet::EMPTY.is_subset_of(&small));
    }

    #[test]
    fn test_capability_from_name() {
        assert_eq!(
            Capability::from_name("CAP_DAC_OVERRIDE"),
            Some(Capability::DacOverride)
        );
        assert_eq!(
            Capability::from_name("dac_override"),
            Some(Capability::DacOverride)
        );
        assert_eq!(Capability::from_name("SETUID"), Some(Capability::Setuid));
        assert_eq!(Capability::from_name("invalid"), None);
    }

    #[test]
    fn test_process_capabilities_root() {
        let caps = ProcessCapabilities::root();
        assert!(caps.has_effective(Capability::DacOverride));
        assert!(caps.has_effective(Capability::SysAdmin));
        assert!(caps.has_permitted(Capability::Setuid));
        assert!(caps.has_inheritable(Capability::Kill));
    }

    #[test]
    fn test_process_capabilities_for_uid() {
        // Root gets all caps
        let root_caps = ProcessCapabilities::for_uid(Uid::ROOT);
        assert!(root_caps.has_effective(Capability::DacOverride));

        // Regular users get no caps
        let user_caps = ProcessCapabilities::for_uid(Uid(1000));
        assert!(!user_caps.has_effective(Capability::DacOverride));
        assert!(user_caps.effective.is_empty());
    }

    #[test]
    fn test_process_capabilities_raise_lower() {
        let mut caps = ProcessCapabilities::root();

        // Lower a capability
        caps.lower(Capability::DacOverride);
        assert!(!caps.has_effective(Capability::DacOverride));
        assert!(caps.has_permitted(Capability::DacOverride)); // Still permitted

        // Raise it back
        assert!(caps.raise(Capability::DacOverride));
        assert!(caps.has_effective(Capability::DacOverride));
    }

    #[test]
    fn test_process_capabilities_raise_not_permitted() {
        let mut caps = ProcessCapabilities::new(); // No capabilities
        caps.permitted.add(Capability::Setuid); // Add only to permitted

        // Can raise because it's in permitted
        assert!(caps.raise(Capability::Setuid));

        // Cannot raise DacOverride because it's not in permitted
        assert!(!caps.raise(Capability::DacOverride));
    }

    #[test]
    fn test_process_capabilities_drop() {
        let mut caps = ProcessCapabilities::root();

        // Drop a capability permanently
        caps.drop_cap(Capability::SysAdmin);
        assert!(!caps.has_effective(Capability::SysAdmin));
        assert!(!caps.has_permitted(Capability::SysAdmin));
        assert!(!caps.has_inheritable(Capability::SysAdmin));

        // Can't raise it back
        assert!(!caps.raise(Capability::SysAdmin));
    }

    #[test]
    fn test_process_capabilities_fork() {
        let parent = ProcessCapabilities::root();
        let child = parent.for_fork();

        // Child inherits same capabilities
        assert_eq!(child.effective, parent.effective);
        assert_eq!(child.permitted, parent.permitted);
        assert_eq!(child.inheritable, parent.inheritable);
    }

    #[test]
    fn test_process_capabilities_exec_root() {
        let caps = ProcessCapabilities::root();
        let after_exec = caps.for_exec(true);

        // Root preserves inheritable capabilities on exec
        assert_eq!(after_exec.effective, caps.inheritable);
        assert_eq!(after_exec.permitted, caps.inheritable);
    }

    #[test]
    fn test_process_capabilities_exec_non_root() {
        let mut caps = ProcessCapabilities::new();
        caps.permitted.add(Capability::Setuid);
        caps.effective.add(Capability::Setuid);
        caps.inheritable.add(Capability::Setuid);

        let after_exec = caps.for_exec(false);

        // Non-root loses capabilities on exec (without file caps)
        assert!(after_exec.effective.is_empty());
        assert!(after_exec.permitted.is_empty());
    }

    #[test]
    fn test_permission_with_caps_dac_override() {
        let mode = FileMode(0o000); // No permissions
        let file_uid = Uid(1000);
        let file_gid = Gid(1000);
        let user_uid = Uid(2000); // Different user

        // Without caps - should fail
        let no_caps = ProcessCapabilities::new();
        assert!(!check_permission_with_caps(
            file_uid,
            file_gid,
            mode,
            user_uid,
            Gid(2000),
            &[],
            &no_caps,
            true,
            false,
            false
        ));

        // With CAP_DAC_OVERRIDE - should succeed
        let mut with_dac = ProcessCapabilities::new();
        with_dac.permitted.add(Capability::DacOverride);
        with_dac.effective.add(Capability::DacOverride);
        assert!(check_permission_with_caps(
            file_uid,
            file_gid,
            mode,
            user_uid,
            Gid(2000),
            &[],
            &with_dac,
            true,
            true,
            true
        ));
    }

    #[test]
    fn test_permission_with_caps_dac_read_search() {
        let mode = FileMode(0o000); // No permissions
        let file_uid = Uid(1000);
        let file_gid = Gid(1000);
        let user_uid = Uid(2000);

        let mut caps = ProcessCapabilities::new();
        caps.permitted.add(Capability::DacReadSearch);
        caps.effective.add(Capability::DacReadSearch);

        // CAP_DAC_READ_SEARCH allows read but not write
        assert!(check_permission_with_caps(
            file_uid,
            file_gid,
            mode,
            user_uid,
            Gid(2000),
            &[],
            &caps,
            true,
            false,
            false
        ));
        assert!(!check_permission_with_caps(
            file_uid,
            file_gid,
            mode,
            user_uid,
            Gid(2000),
            &[],
            &caps,
            false,
            true,
            false
        ));
    }

    #[test]
    fn test_permission_with_caps_fowner() {
        let mode = FileMode(0o700); // Owner only
        let file_uid = Uid(1000);
        let file_gid = Gid(1000);
        let user_uid = Uid(2000); // Different user

        // Without FOWNER - should fail (not owner, no 'other' perms)
        let no_caps = ProcessCapabilities::new();
        assert!(!check_permission_with_caps(
            file_uid,
            file_gid,
            mode,
            user_uid,
            Gid(2000),
            &[],
            &no_caps,
            true,
            true,
            true
        ));

        // With CAP_FOWNER - treated as owner
        let mut with_fowner = ProcessCapabilities::new();
        with_fowner.permitted.add(Capability::Fowner);
        with_fowner.effective.add(Capability::Fowner);
        assert!(check_permission_with_caps(
            file_uid,
            file_gid,
            mode,
            user_uid,
            Gid(2000),
            &[],
            &with_fowner,
            true,
            true,
            true
        ));
    }

    #[test]
    fn test_capability_iterator() {
        let mut set = CapabilitySet::new();
        set.add(Capability::DacOverride);
        set.add(Capability::Kill);
        set.add(Capability::Setuid);

        let caps: Vec<Capability> = set.iter().collect();
        assert_eq!(caps.len(), 3);
        assert!(caps.contains(&Capability::DacOverride));
        assert!(caps.contains(&Capability::Kill));
        assert!(caps.contains(&Capability::Setuid));
    }

    #[test]
    fn test_capability_display() {
        assert_eq!(Capability::DacOverride.to_string(), "CAP_DAC_OVERRIDE");
        assert_eq!(Capability::SysAdmin.to_string(), "CAP_SYS_ADMIN");

        let empty = CapabilitySet::EMPTY;
        assert_eq!(empty.to_string(), "(none)");

        let all = CapabilitySet::ALL;
        assert_eq!(all.to_string(), "(all)");
    }
}
