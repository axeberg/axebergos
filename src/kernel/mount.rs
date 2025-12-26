//! Mount table and filesystem mounting
//!
//! Provides mount/umount operations and tracks mounted filesystems.
//! In this WASM environment, we support virtual filesystem mounts
//! like /proc, /sys, /dev, and tmpfs.

use std::collections::HashMap;

/// Filesystem type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsType {
    /// Process information filesystem
    Proc,
    /// System filesystem for kernel objects
    Sysfs,
    /// Device filesystem
    Devfs,
    /// Temporary filesystem (RAM-backed)
    Tmpfs,
    /// Memory filesystem (our main VFS)
    MemoryFs,
    /// Unknown/custom filesystem
    Other(String),
}

impl FsType {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "proc" => FsType::Proc,
            "sysfs" => FsType::Sysfs,
            "devfs" | "devtmpfs" => FsType::Devfs,
            "tmpfs" => FsType::Tmpfs,
            "memoryfs" | "ramfs" => FsType::MemoryFs,
            other => FsType::Other(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            FsType::Proc => "proc",
            FsType::Sysfs => "sysfs",
            FsType::Devfs => "devfs",
            FsType::Tmpfs => "tmpfs",
            FsType::MemoryFs => "memoryfs",
            FsType::Other(s) => s,
        }
    }
}

/// Mount options
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MountOptions {
    /// Read-only mount
    pub read_only: bool,
    /// Don't update access times
    pub noatime: bool,
    /// Don't allow execution of binaries
    pub noexec: bool,
    /// Don't allow set-user-id or set-group-id bits
    pub nosuid: bool,
    /// Don't interpret device files
    pub nodev: bool,
    /// Size limit for tmpfs (in bytes, 0 = no limit)
    pub size_limit: usize,
}

impl MountOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse mount options from a comma-separated string
    pub fn parse(options: &str) -> Self {
        let mut opts = MountOptions::new();
        for opt in options.split(',') {
            let opt = opt.trim();
            match opt {
                "ro" | "readonly" => opts.read_only = true,
                "rw" | "readwrite" => opts.read_only = false,
                "noatime" => opts.noatime = true,
                "noexec" => opts.noexec = true,
                "nosuid" => opts.nosuid = true,
                "nodev" => opts.nodev = true,
                s if s.starts_with("size=") => {
                    if let Ok(size) = parse_size(&s[5..]) {
                        opts.size_limit = size;
                    }
                }
                _ => {} // Unknown options ignored
            }
        }
        opts
    }
}

impl std::fmt::Display for MountOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();
        if self.read_only {
            parts.push("ro");
        } else {
            parts.push("rw");
        }
        if self.noatime {
            parts.push("noatime");
        }
        if self.noexec {
            parts.push("noexec");
        }
        if self.nosuid {
            parts.push("nosuid");
        }
        if self.nodev {
            parts.push("nodev");
        }
        if self.size_limit > 0 {
            return write!(f, "{},size={}", parts.join(","), self.size_limit);
        }
        write!(f, "{}", parts.join(","))
    }
}

/// Parse size string (e.g., "1G", "512M", "1024K", "4096")
fn parse_size(s: &str) -> Result<usize, ()> {
    let s = s.trim();
    if s.is_empty() {
        return Err(());
    }

    let (num_str, multiplier) = if let Some(stripped) = s.strip_suffix('G') {
        (stripped, 1024 * 1024 * 1024)
    } else if let Some(stripped) = s.strip_suffix('M') {
        (stripped, 1024 * 1024)
    } else if let Some(stripped) = s.strip_suffix('K') {
        (stripped, 1024)
    } else if let Some(stripped) = s.strip_suffix('g') {
        (stripped, 1024 * 1024 * 1024)
    } else if let Some(stripped) = s.strip_suffix('m') {
        (stripped, 1024 * 1024)
    } else if let Some(stripped) = s.strip_suffix('k') {
        (stripped, 1024)
    } else {
        (s, 1)
    };

    num_str
        .parse::<usize>()
        .map(|n| n * multiplier)
        .map_err(|_| ())
}

/// A mounted filesystem entry
#[derive(Debug, Clone, PartialEq)]
pub struct MountEntry {
    /// Device or source (e.g., "/dev/sda1", "proc", "tmpfs")
    pub source: String,
    /// Mount point path
    pub target: String,
    /// Filesystem type
    pub fstype: FsType,
    /// Mount options
    pub options: MountOptions,
    /// Mount time (monotonic)
    pub mount_time: f64,
}

impl MountEntry {
    pub fn new(
        source: &str,
        target: &str,
        fstype: FsType,
        options: MountOptions,
        now: f64,
    ) -> Self {
        Self {
            source: source.to_string(),
            target: target.to_string(),
            fstype,
            options,
            mount_time: now,
        }
    }
}

/// fstab entry for automatic mounting
#[derive(Debug, Clone)]
pub struct FstabEntry {
    /// Device or source
    pub source: String,
    /// Mount point
    pub target: String,
    /// Filesystem type
    pub fstype: FsType,
    /// Mount options
    pub options: String,
    /// Dump frequency (0 = don't dump)
    pub dump: u8,
    /// Pass number for fsck (0 = skip)
    pub pass: u8,
}

impl FstabEntry {
    /// Parse a line from fstab format
    pub fn parse(line: &str) -> Option<Self> {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            return None;
        }

        Some(FstabEntry {
            source: parts[0].to_string(),
            target: parts[1].to_string(),
            fstype: FsType::parse(parts[2]),
            options: parts[3].to_string(),
            dump: parts.get(4).and_then(|s| s.parse().ok()).unwrap_or(0),
            pass: parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0),
        })
    }
}

/// Mount error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MountError {
    /// Mount point doesn't exist
    MountPointNotFound,
    /// Already mounted
    AlreadyMounted,
    /// Not mounted
    NotMounted,
    /// Filesystem type not supported
    UnsupportedFilesystem,
    /// Permission denied
    PermissionDenied,
    /// Device busy (has open files)
    Busy,
    /// Invalid mount options
    InvalidOptions,
}

/// Mount table managing all mounted filesystems
pub struct MountTable {
    /// Active mounts (target path -> entry)
    mounts: HashMap<String, MountEntry>,
}

impl MountTable {
    pub fn new() -> Self {
        Self {
            mounts: HashMap::new(),
        }
    }

    /// Initialize with default mounts
    pub fn with_defaults(now: f64) -> Self {
        let mut table = Self::new();

        // Root filesystem
        let _ = table.mount("rootfs", "/", FsType::MemoryFs, MountOptions::new(), now);

        // Virtual filesystems
        let _ = table.mount(
            "proc",
            "/proc",
            FsType::Proc,
            MountOptions {
                read_only: true,
                ..Default::default()
            },
            now,
        );

        let _ = table.mount(
            "sysfs",
            "/sys",
            FsType::Sysfs,
            MountOptions {
                read_only: true,
                ..Default::default()
            },
            now,
        );

        let _ = table.mount("devfs", "/dev", FsType::Devfs, MountOptions::new(), now);

        let _ = table.mount("tmpfs", "/tmp", FsType::Tmpfs, MountOptions::new(), now);

        table
    }

    /// Mount a filesystem
    pub fn mount(
        &mut self,
        source: &str,
        target: &str,
        fstype: FsType,
        options: MountOptions,
        now: f64,
    ) -> Result<(), MountError> {
        // Normalize target path
        let target = normalize_path(target);

        // Check if already mounted
        if self.mounts.contains_key(&target) {
            return Err(MountError::AlreadyMounted);
        }

        let entry = MountEntry::new(source, &target, fstype, options, now);
        self.mounts.insert(target, entry);
        Ok(())
    }

    /// Unmount a filesystem
    pub fn umount(&mut self, target: &str) -> Result<MountEntry, MountError> {
        let target = normalize_path(target);

        // Can't unmount root
        if target == "/" {
            return Err(MountError::Busy);
        }

        self.mounts.remove(&target).ok_or(MountError::NotMounted)
    }

    /// Check if a path is a mount point
    pub fn is_mount_point(&self, path: &str) -> bool {
        let path = normalize_path(path);
        self.mounts.contains_key(&path)
    }

    /// Get mount entry for a path
    pub fn get_mount(&self, path: &str) -> Option<&MountEntry> {
        let path = normalize_path(path);
        self.mounts.get(&path)
    }

    /// Get the mount entry that contains a given path
    pub fn get_containing_mount(&self, path: &str) -> Option<&MountEntry> {
        let path = normalize_path(path);

        // Find the longest matching mount point
        let mut best_match: Option<&MountEntry> = None;
        let mut best_len = 0;

        for (mount_point, entry) in &self.mounts {
            let matches = if *mount_point == "/" {
                // Root matches everything
                true
            } else {
                // Non-root: exact match or path starts with mount_point/
                path == *mount_point || path.starts_with(&format!("{}/", mount_point))
            };

            if matches {
                let len = mount_point.len();
                // Prefer longer (more specific) mount points
                if len > best_len {
                    best_match = Some(entry);
                    best_len = len;
                }
            }
        }

        best_match
    }

    /// List all mounts
    pub fn list(&self) -> Vec<&MountEntry> {
        self.mounts.values().collect()
    }

    /// Get mounts in /proc/mounts format
    pub fn to_proc_mounts(&self) -> String {
        let mut lines = Vec::new();
        for entry in self.mounts.values() {
            lines.push(format!(
                "{} {} {} {} 0 0",
                entry.source,
                entry.target,
                entry.fstype.as_str(),
                entry.options
            ));
        }
        lines.join("\n")
    }
}

impl Default for MountTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalize a path (remove trailing slash, handle . and ..)
fn normalize_path(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() {
        return "/".to_string();
    }

    // Remove trailing slashes (except for root)
    let path = path.trim_end_matches('/');
    if path.is_empty() {
        return "/".to_string();
    }

    // Ensure leading slash
    if !path.starts_with('/') {
        format!("/{}", path)
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mount_table_defaults() {
        let table = MountTable::with_defaults(0.0);

        assert!(table.is_mount_point("/"));
        assert!(table.is_mount_point("/proc"));
        assert!(table.is_mount_point("/sys"));
        assert!(table.is_mount_point("/dev"));
        assert!(table.is_mount_point("/tmp"));
    }

    #[test]
    fn test_mount_umount() {
        let mut table = MountTable::new();

        // Mount
        table
            .mount(
                "tmpfs",
                "/mnt/test",
                FsType::Tmpfs,
                MountOptions::new(),
                1.0,
            )
            .unwrap();
        assert!(table.is_mount_point("/mnt/test"));

        // Can't mount again
        let err = table.mount(
            "tmpfs",
            "/mnt/test",
            FsType::Tmpfs,
            MountOptions::new(),
            2.0,
        );
        assert_eq!(err, Err(MountError::AlreadyMounted));

        // Unmount
        table.umount("/mnt/test").unwrap();
        assert!(!table.is_mount_point("/mnt/test"));

        // Can't unmount again
        let err = table.umount("/mnt/test");
        assert_eq!(err, Err(MountError::NotMounted));
    }

    #[test]
    fn test_containing_mount() {
        let table = MountTable::with_defaults(0.0);

        let mount = table.get_containing_mount("/proc/1/status").unwrap();
        assert_eq!(mount.target, "/proc");

        let mount = table.get_containing_mount("/home/user").unwrap();
        assert_eq!(mount.target, "/");
    }

    #[test]
    fn test_mount_options_parse() {
        let opts = MountOptions::parse("ro,noexec,noatime,size=1G");
        assert!(opts.read_only);
        assert!(opts.noexec);
        assert!(opts.noatime);
        assert_eq!(opts.size_limit, 1024 * 1024 * 1024);
    }

    #[test]
    fn test_fstype_parse() {
        assert_eq!(FsType::parse("proc"), FsType::Proc);
        assert_eq!(FsType::parse("SYSFS"), FsType::Sysfs);
        assert_eq!(FsType::parse("tmpfs"), FsType::Tmpfs);
        assert_eq!(FsType::parse("ext4"), FsType::Other("ext4".to_string()));
    }

    #[test]
    fn test_fstab_parse() {
        let entry = FstabEntry::parse("proc /proc proc defaults 0 0").unwrap();
        assert_eq!(entry.source, "proc");
        assert_eq!(entry.target, "/proc");
        assert_eq!(entry.fstype, FsType::Proc);

        // Comment line
        assert!(FstabEntry::parse("# This is a comment").is_none());

        // Empty line
        assert!(FstabEntry::parse("").is_none());
    }

    #[test]
    fn test_proc_mounts_format() {
        let mut table = MountTable::new();
        table
            .mount("tmpfs", "/tmp", FsType::Tmpfs, MountOptions::new(), 1.0)
            .unwrap();

        let output = table.to_proc_mounts();
        assert!(output.contains("tmpfs /tmp tmpfs"));
    }

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("1024"), Ok(1024));
        assert_eq!(parse_size("1K"), Ok(1024));
        assert_eq!(parse_size("1M"), Ok(1024 * 1024));
        assert_eq!(parse_size("1G"), Ok(1024 * 1024 * 1024));
    }

    #[test]
    fn test_cant_umount_root() {
        let mut table = MountTable::with_defaults(0.0);
        let err = table.umount("/");
        assert_eq!(err, Err(MountError::Busy));
    }
}
