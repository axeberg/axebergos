//! Shell programs
//!
//! This module contains all the built-in programs available in the shell.
//! Programs are organized by category for maintainability.

use crate::kernel::syscall;

// Program modules by category
pub mod cron;
pub mod encoding;
pub mod file;
pub mod fs;
pub mod ipc;
pub mod mount;
pub mod net;
pub mod perms;
pub mod pkg;
pub mod process;
pub mod services;
pub mod shell;
pub mod system;
pub mod text;
pub mod tty;
pub mod user;

// Re-export all program functions for the registry
pub use cron::*;
pub use encoding::*;
pub use file::*;
pub use fs::*;
pub use ipc::*;
pub use mount::*;
pub use net::*;
pub use perms::*;
pub use pkg::*;
pub use process::*;
pub use services::*;
pub use shell::*;
pub use system::*;
pub use text::*;
pub use tty::*;
pub use user::*;

// ============ Shared Utilities ============

/// Check if args contain -h or --help and return usage message if so
pub fn check_help(args: &[&str], usage: &str) -> Option<String> {
    if args.iter().any(|a| *a == "-h" || *a == "--help") {
        Some(usage.to_string())
    } else {
        None
    }
}

/// Helper to read file content as string
pub fn read_file_content(path: &str) -> Result<String, String> {
    match syscall::open(path, syscall::OpenFlags::READ) {
        Ok(fd) => {
            let mut content = String::new();
            let mut buf = [0u8; 4096];
            loop {
                match syscall::read(fd, &mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Ok(s) = std::str::from_utf8(&buf[..n]) {
                            content.push_str(s);
                        }
                    }
                    Err(e) => {
                        let _ = syscall::close(fd);
                        return Err(e.to_string());
                    }
                }
            }
            let _ = syscall::close(fd);
            Ok(content)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Convert String slice to &str slice for easier handling
pub fn args_to_strs(args: &[String]) -> Vec<&str> {
    args.iter().map(|s| s.as_str()).collect()
}
