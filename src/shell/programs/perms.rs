//! Permission management programs

use super::{args_to_strs, check_help};
use crate::kernel::syscall;

/// chmod - change file permissions
pub fn prog_chmod(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if args.len() < 2 || args.first().map(|s| s.as_ref()) == Some("--help") {
        stdout.push_str("Usage: chmod MODE FILE...\nChange file permissions.\n\n");
        stdout.push_str("MODE can be:\n");
        stdout.push_str("  Octal: 755, 644, etc.\n");
        stdout.push_str("  Symbolic: u+x, go-w, a=r, etc.\n");
        return if args.is_empty() { 0 } else { 1 };
    }

    let mode_str = &args[0];
    let mode = if let Ok(octal) = u16::from_str_radix(mode_str, 8) {
        octal
    } else {
        // Parse symbolic mode (simplified)
        stderr.push_str(&format!("chmod: invalid mode: '{}' (use octal for now)\n", mode_str));
        return 1;
    };

    let mut errors = 0;
    for path in &args[1..] {
        match syscall::chmod(path, mode) {
            Ok(()) => {}
            Err(e) => {
                stderr.push_str(&format!("chmod: {}: {}\n", path, e));
                errors += 1;
            }
        }
    }

    if errors > 0 { 1 } else { 0 }
}

/// chown - change file owner
pub fn prog_chown(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if args.len() < 2 || args.first().map(|s| s.as_ref()) == Some("--help") {
        stdout.push_str("Usage: chown [OWNER][:GROUP] FILE...\nChange file owner and group.\n");
        return if args.is_empty() { 0 } else { 1 };
    }

    let owner_str = &args[0];

    // Parse owner:group or owner.group or just owner
    let (uid, gid) = if owner_str.contains(':') || owner_str.contains('.') {
        let sep = if owner_str.contains(':') { ':' } else { '.' };
        let parts: Vec<&str> = owner_str.splitn(2, sep).collect();
        let uid = if parts[0].is_empty() {
            None
        } else if let Ok(n) = parts[0].parse::<u32>() {
            Some(n)
        } else if let Some(user) = syscall::get_user_by_name(parts[0]) {
            Some(user.uid.0)
        } else {
            stderr.push_str(&format!("chown: invalid user: '{}'\n", parts[0]));
            return 1;
        };

        let gid = if parts.len() > 1 && !parts[1].is_empty() {
            if let Ok(n) = parts[1].parse::<u32>() {
                Some(n)
            } else if let Some(group) = syscall::get_group_by_name(parts[1]) {
                Some(group.gid.0)
            } else {
                stderr.push_str(&format!("chown: invalid group: '{}'\n", parts[1]));
                return 1;
            }
        } else {
            None
        };

        (uid, gid)
    } else {
        // Just owner
        let uid = if let Ok(n) = owner_str.parse::<u32>() {
            Some(n)
        } else if let Some(user) = syscall::get_user_by_name(owner_str) {
            Some(user.uid.0)
        } else {
            stderr.push_str(&format!("chown: invalid user: '{}'\n", owner_str));
            return 1;
        };
        (uid, None)
    };

    let mut errors = 0;
    for path in &args[1..] {
        match syscall::chown(path, uid, gid) {
            Ok(()) => {}
            Err(e) => {
                stderr.push_str(&format!("chown: {}: {}\n", path, e));
                errors += 1;
            }
        }
    }

    if errors > 0 { 1 } else { 0 }
}

/// chgrp - change file group
pub fn prog_chgrp(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if args.len() < 2 || args.first().map(|s| s.as_ref()) == Some("--help") {
        stdout.push_str("Usage: chgrp GROUP FILE...\nChange file group.\n");
        return if args.is_empty() { 0 } else { 1 };
    }

    let group_str = &args[0];
    let gid = if let Ok(n) = group_str.parse::<u32>() {
        n
    } else if let Some(group) = syscall::get_group_by_name(group_str) {
        group.gid.0
    } else {
        stderr.push_str(&format!("chgrp: invalid group: '{}'\n", group_str));
        return 1;
    };

    let mut errors = 0;
    for path in &args[1..] {
        match syscall::chown(path, None, Some(gid)) {
            Ok(()) => {}
            Err(e) => {
                stderr.push_str(&format!("chgrp: {}: {}\n", path, e));
                errors += 1;
            }
        }
    }

    if errors > 0 { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chmod_help() {
        let args = vec![String::from("--help")];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_chmod(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stdout.contains("Usage: chmod"));
        assert!(stdout.contains("MODE"));
    }

    #[test]
    fn test_chmod_no_args() {
        let args = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_chmod(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: chmod"));
    }

    #[test]
    fn test_chmod_invalid_mode() {
        let args = vec![String::from("invalid"), String::from("file.txt")];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_chmod(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("invalid mode"));
    }

    #[test]
    fn test_chown_help() {
        let args = vec![String::from("--help")];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_chown(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stdout.contains("Usage: chown"));
        assert!(stdout.contains("OWNER"));
    }

    #[test]
    fn test_chown_no_args() {
        let args = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_chown(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: chown"));
    }

    #[test]
    fn test_chgrp_help() {
        let args = vec![String::from("--help")];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_chgrp(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stdout.contains("Usage: chgrp"));
        assert!(stdout.contains("GROUP"));
    }

    #[test]
    fn test_chgrp_no_args() {
        let args = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_chgrp(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: chgrp"));
    }
}
