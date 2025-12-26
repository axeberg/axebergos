//! Mount and filesystem programs

use super::{args_to_strs, check_help};
use crate::kernel::syscall;

pub fn prog_mount(args: &[String], __stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: mount [-t TYPE] [-o OPTIONS] SOURCE TARGET\n       mount (show all mounts)\n\nMount a filesystem.\n\nOptions:\n  -t TYPE   Filesystem type (proc, sysfs, devfs, tmpfs)\n  -o OPTS   Mount options (ro, noexec, noatime, etc.)") {
        stdout.push_str(&help);
        return 0;
    }

    // No arguments: list all mounts
    if args.is_empty() {
        syscall::KERNEL.with(|k| {
            let kernel = k.borrow();
            for entry in kernel.mounts().list() {
                stdout.push_str(&format!(
                    "{} on {} type {} ({})\n",
                    entry.source,
                    entry.target,
                    entry.fstype.as_str(),
                    entry.options
                ));
            }
        });
        return 0;
    }

    // Parse arguments
    let mut fstype = "tmpfs".to_string();
    let mut options = "rw".to_string();
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;

    while i < args.len() {
        let arg = args[i];
        match arg {
            "-t" => {
                if i + 1 < args.len() {
                    i += 1;
                    fstype = args[i].to_string();
                } else {
                    stderr.push_str("mount: option requires an argument -- 't'\n");
                    return 1;
                }
            }
            "-o" => {
                if i + 1 < args.len() {
                    i += 1;
                    options = args[i].to_string();
                } else {
                    stderr.push_str("mount: option requires an argument -- 'o'\n");
                    return 1;
                }
            }
            _ if !arg.starts_with('-') => {
                positional.push(args[i].to_string());
            }
            _ => {
                // Unknown option
            }
        }
        i += 1;
    }

    if positional.len() < 2 {
        stderr.push_str("mount: usage: mount [-t type] [-o options] source target\n");
        return 1;
    }

    let source = &positional[0];
    let target = &positional[1];

    use crate::kernel::mount::{FsType, MountOptions};

    let fs = FsType::parse(&fstype);
    let opts = MountOptions::parse(&options);
    let now = syscall::KERNEL.with(|k| k.borrow().now());

    let result = syscall::KERNEL.with(|k| {
        let mut kernel = k.borrow_mut();
        kernel.mounts_mut().mount(source, target, fs, opts, now)
    });

    match result {
        Ok(()) => 0,
        Err(e) => {
            stderr.push_str(&format!("mount: {:?}\n", e));
            1
        }
    }
}

pub fn prog_umount(args: &[String], __stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: umount TARGET\nUnmount a filesystem.") {
        stdout.push_str(&help);
        return 0;
    }

    if args.is_empty() {
        stderr.push_str("umount: usage: umount target\n");
        return 1;
    }

    let target = &args[0];

    let result = syscall::KERNEL.with(|k| {
        k.borrow_mut().mounts_mut().umount(target)
    });

    match result {
        Ok(_) => 0,
        Err(e) => {
            stderr.push_str(&format!("umount: {}: {:?}\n", target, e));
            1
        }
    }
}

pub fn prog_findmnt(args: &[String], __stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: findmnt [TARGET]\nFind a filesystem mount point.\n\nWith no arguments, lists all mounts in a tree-like format.") {
        stdout.push_str(&help);
        return 0;
    }

    syscall::KERNEL.with(|k| {
        let kernel = k.borrow();

        if args.is_empty() {
            // List all mounts
            stdout.push_str("TARGET                  SOURCE     FSTYPE   OPTIONS\n");
            let mut mounts: Vec<_> = kernel.mounts().list();
            mounts.sort_by(|a, b| a.target.cmp(&b.target));

            for entry in mounts {
                stdout.push_str(&format!(
                    "{:<23} {:<10} {:<8} {}\n",
                    entry.target,
                    if entry.source.len() > 10 { &entry.source[..10] } else { &entry.source },
                    entry.fstype.as_str(),
                    entry.options
                ));
            }
        } else {
            // Find specific mount point
            let target = &args[0];
            if let Some(entry) = kernel.mounts().get_mount(target) {
                stdout.push_str(&format!(
                    "TARGET: {}\nSOURCE: {}\nFSTYPE: {}\nOPTIONS: {}\n",
                    entry.target,
                    entry.source,
                    entry.fstype.as_str(),
                    entry.options
                ));
            } else if let Some(entry) = kernel.mounts().get_containing_mount(target) {
                stdout.push_str(&format!(
                    "{} is under mount point:\nTARGET: {}\nSOURCE: {}\nFSTYPE: {}\n",
                    target,
                    entry.target,
                    entry.source,
                    entry.fstype.as_str()
                ));
            } else {
                stdout.push_str(&format!("{}: not a mount point\n", target));
            }
        }
    });

    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mount_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_mount(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: mount"));
        assert!(stderr.is_empty());
    }

    #[test]
    fn test_mount_missing_args() {
        let args = vec!["source".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_mount(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("usage"));
    }

    #[test]
    fn test_mount_missing_option_arg() {
        let args = vec!["-t".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_mount(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("option requires an argument"));
    }

    #[test]
    fn test_umount_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_umount(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: umount"));
        assert!(stderr.is_empty());
    }

    #[test]
    fn test_umount_missing_args() {
        let args = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_umount(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("usage"));
    }

    #[test]
    fn test_findmnt_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_findmnt(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: findmnt"));
        assert!(stderr.is_empty());
    }
}
