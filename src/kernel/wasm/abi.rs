//! WASM Command ABI types and constants
//!
//! This module defines the stable interface between the kernel and WASM commands.

/// ABI version number
pub const ABI_VERSION: u32 = 1;

/// Required export names
pub mod exports {
    /// The linear memory export name
    pub const MEMORY: &str = "memory";
    /// The main entry point
    pub const MAIN: &str = "main";
    /// Optional: heap base for allocation
    pub const HEAP_BASE: &str = "__heap_base";
}

/// Import module namespace
pub const IMPORT_NAMESPACE: &str = "env";

/// Syscall function names (imported by commands)
pub mod syscalls {
    // File operations
    pub const OPEN: &str = "open";
    pub const CLOSE: &str = "close";
    pub const READ: &str = "read";
    pub const WRITE: &str = "write";
    pub const STAT: &str = "stat";

    // Directory operations
    pub const MKDIR: &str = "mkdir";
    pub const READDIR: &str = "readdir";
    pub const RMDIR: &str = "rmdir";
    pub const UNLINK: &str = "unlink";
    pub const RENAME: &str = "rename";

    // Process control
    pub const EXIT: &str = "exit";
    pub const GETENV: &str = "getenv";
    pub const GETCWD: &str = "getcwd";
}

/// Standard file descriptors
pub mod fd {
    pub const STDIN: i32 = 0;
    pub const STDOUT: i32 = 1;
    pub const STDERR: i32 = 2;
}

/// Open flags for the `open` syscall
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenFlags(pub i32);

impl OpenFlags {
    pub const READ: OpenFlags = OpenFlags(0);
    pub const WRITE: OpenFlags = OpenFlags(1);
    pub const READ_WRITE: OpenFlags = OpenFlags(2);
    pub const CREATE: OpenFlags = OpenFlags(4);
    pub const TRUNCATE: OpenFlags = OpenFlags(8);

    pub fn is_read(&self) -> bool {
        self.0 == 0 || (self.0 & 2) != 0
    }

    pub fn is_write(&self) -> bool {
        (self.0 & 1) != 0 || (self.0 & 2) != 0
    }

    pub fn is_create(&self) -> bool {
        (self.0 & 4) != 0
    }

    pub fn is_truncate(&self) -> bool {
        (self.0 & 8) != 0
    }
}

/// Error codes returned by syscalls
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum SyscallError {
    /// Generic/unknown error
    Generic = -1,
    /// File or directory not found
    NotFound = -2,
    /// Permission denied
    PermissionDenied = -3,
    /// File or directory already exists
    AlreadyExists = -4,
    /// Expected directory, got file
    NotADirectory = -5,
    /// Expected file, got directory
    IsADirectory = -6,
    /// Invalid argument
    InvalidArgument = -7,
    /// No space left on device
    NoSpace = -8,
    /// I/O error
    IoError = -9,
    /// Invalid file descriptor
    BadFd = -10,
    /// Directory not empty
    NotEmpty = -11,
}

impl SyscallError {
    pub fn code(&self) -> i32 {
        *self as i32
    }

    pub fn from_code(code: i32) -> Option<Self> {
        match code {
            -1 => Some(Self::Generic),
            -2 => Some(Self::NotFound),
            -3 => Some(Self::PermissionDenied),
            -4 => Some(Self::AlreadyExists),
            -5 => Some(Self::NotADirectory),
            -6 => Some(Self::IsADirectory),
            -7 => Some(Self::InvalidArgument),
            -8 => Some(Self::NoSpace),
            -9 => Some(Self::IoError),
            -10 => Some(Self::BadFd),
            -11 => Some(Self::NotEmpty),
            _ => None,
        }
    }
}

/// Stat buffer layout (32 bytes)
/// Used by the `stat` syscall to return file metadata
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct StatBuf {
    /// File size in bytes
    pub size: u32,
    /// Is directory (0 = no, 1 = yes)
    pub is_dir: u32,
    /// Last modified time (unix timestamp)
    pub modified_time: u64,
    /// Creation time (unix timestamp)
    pub created_time: u64,
    /// Reserved for future use
    pub reserved: u64,
}

impl StatBuf {
    pub const SIZE: usize = 32;

    /// Serialize to bytes (little-endian)
    pub fn to_bytes(&self) -> [u8; 32] {
        let mut buf = [0u8; 32];
        buf[0..4].copy_from_slice(&self.size.to_le_bytes());
        buf[4..8].copy_from_slice(&self.is_dir.to_le_bytes());
        buf[8..16].copy_from_slice(&self.modified_time.to_le_bytes());
        buf[16..24].copy_from_slice(&self.created_time.to_le_bytes());
        buf[24..32].copy_from_slice(&self.reserved.to_le_bytes());
        buf
    }

    /// Deserialize from bytes (little-endian)
    pub fn from_bytes(buf: &[u8; 32]) -> Self {
        Self {
            size: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
            is_dir: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
            modified_time: u64::from_le_bytes([
                buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
            ]),
            created_time: u64::from_le_bytes([
                buf[16], buf[17], buf[18], buf[19], buf[20], buf[21], buf[22], buf[23],
            ]),
            reserved: u64::from_le_bytes([
                buf[24], buf[25], buf[26], buf[27], buf[28], buf[29], buf[30], buf[31],
            ]),
        }
    }
}

/// Argument layout helper
///
/// When passing arguments to a WASM command, we need to:
/// 1. Write the argument strings to memory
/// 2. Write an array of pointers to those strings
/// 3. Pass argc and argv pointer to main()
#[derive(Debug)]
pub struct ArgLayout {
    /// Total bytes needed for strings (including null terminators)
    pub strings_size: usize,
    /// Total bytes needed for argv array (including null terminator)
    pub argv_size: usize,
    /// Offsets of each string from the base address
    pub string_offsets: Vec<usize>,
}

impl ArgLayout {
    /// Calculate layout for given arguments
    pub fn new(args: &[&str]) -> Self {
        let mut strings_size = 0;
        let mut string_offsets = Vec::with_capacity(args.len());

        for arg in args {
            string_offsets.push(strings_size);
            strings_size += arg.len() + 1; // +1 for null terminator
        }

        // argv array: one i32 pointer per arg, plus null terminator
        let argv_size = (args.len() + 1) * 4;

        Self {
            strings_size,
            argv_size,
            string_offsets,
        }
    }

    /// Total memory needed (strings + argv array)
    pub fn total_size(&self) -> usize {
        self.strings_size + self.argv_size
    }

    /// Write arguments to a memory buffer
    ///
    /// Returns the argv pointer (offset from base)
    pub fn write_to(&self, args: &[&str], base_addr: u32, buf: &mut [u8]) -> u32 {
        assert!(buf.len() >= self.total_size());

        // Write strings
        for (i, arg) in args.iter().enumerate() {
            let offset = self.string_offsets[i];
            buf[offset..offset + arg.len()].copy_from_slice(arg.as_bytes());
            buf[offset + arg.len()] = 0; // null terminator
        }

        // Write argv array after strings
        let argv_offset = self.strings_size;
        for (i, _) in args.iter().enumerate() {
            let ptr = base_addr + self.string_offsets[i] as u32;
            let arr_offset = argv_offset + i * 4;
            buf[arr_offset..arr_offset + 4].copy_from_slice(&ptr.to_le_bytes());
        }

        // Null terminator for argv
        let null_offset = argv_offset + args.len() * 4;
        buf[null_offset..null_offset + 4].copy_from_slice(&0u32.to_le_bytes());

        // Return argv pointer
        base_addr + argv_offset as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_flags() {
        assert!(OpenFlags::READ.is_read());
        assert!(!OpenFlags::READ.is_write());

        assert!(OpenFlags::WRITE.is_write());
        assert!(!OpenFlags::WRITE.is_read());

        assert!(OpenFlags::READ_WRITE.is_read());
        assert!(OpenFlags::READ_WRITE.is_write());

        let create_write = OpenFlags(OpenFlags::WRITE.0 | OpenFlags::CREATE.0);
        assert!(create_write.is_write());
        assert!(create_write.is_create());
    }

    #[test]
    fn test_syscall_error_codes() {
        assert_eq!(SyscallError::NotFound.code(), -2);
        assert_eq!(SyscallError::from_code(-2), Some(SyscallError::NotFound));
        assert_eq!(SyscallError::from_code(-999), None);
    }

    #[test]
    fn test_stat_buf_roundtrip() {
        let stat = StatBuf {
            size: 1234,
            is_dir: 1,
            modified_time: 1700000000,
            created_time: 1600000000,
            reserved: 0,
        };

        let bytes = stat.to_bytes();
        let recovered = StatBuf::from_bytes(&bytes);

        assert_eq!(recovered.size, stat.size);
        assert_eq!(recovered.is_dir, stat.is_dir);
        assert_eq!(recovered.modified_time, stat.modified_time);
        assert_eq!(recovered.created_time, stat.created_time);
    }

    #[test]
    fn test_arg_layout_simple() {
        let args = &["cat", "file.txt"];
        let layout = ArgLayout::new(args);

        // "cat\0" = 4 bytes, "file.txt\0" = 9 bytes
        assert_eq!(layout.strings_size, 13);
        // 3 pointers (2 args + null) * 4 bytes
        assert_eq!(layout.argv_size, 12);
        assert_eq!(layout.total_size(), 25);

        // Check offsets
        assert_eq!(layout.string_offsets[0], 0); // "cat" at offset 0
        assert_eq!(layout.string_offsets[1], 4); // "file.txt" at offset 4
    }

    #[test]
    fn test_arg_layout_write() {
        let args = &["echo", "hello"];
        let layout = ArgLayout::new(args);
        let base_addr = 1024u32;

        let mut buf = vec![0u8; layout.total_size()];
        let argv_ptr = layout.write_to(args, base_addr, &mut buf);

        // Check strings were written
        assert_eq!(&buf[0..4], b"echo");
        assert_eq!(buf[4], 0); // null terminator
        assert_eq!(&buf[5..10], b"hello");
        assert_eq!(buf[10], 0); // null terminator

        // Check argv array
        let strings_end = layout.strings_size;
        let ptr0 = u32::from_le_bytes([
            buf[strings_end],
            buf[strings_end + 1],
            buf[strings_end + 2],
            buf[strings_end + 3],
        ]);
        assert_eq!(ptr0, base_addr); // points to "echo"

        let ptr1 = u32::from_le_bytes([
            buf[strings_end + 4],
            buf[strings_end + 5],
            buf[strings_end + 6],
            buf[strings_end + 7],
        ]);
        assert_eq!(ptr1, base_addr + 5); // points to "hello"

        // Check null terminator
        let null_ptr = u32::from_le_bytes([
            buf[strings_end + 8],
            buf[strings_end + 9],
            buf[strings_end + 10],
            buf[strings_end + 11],
        ]);
        assert_eq!(null_ptr, 0);

        // Check argv pointer
        assert_eq!(argv_ptr, base_addr + strings_end as u32);
    }
}
