//! File operations programs
//!
//! Programs for basic file manipulation: cat, ls, mkdir, touch, rm, cp, mv, ln, readlink, tree

use super::{args_to_strs, check_help};
use crate::kernel::syscall;

/// cat - concatenate files or stdin
pub fn prog_cat(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let files = args_to_strs(args);

    if let Some(help) = check_help(&files, "Usage: cat [FILE]...\nConcatenate files and print to stdout. See 'man cat' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    if files.is_empty() {
        // Read from stdin
        if !stdin.is_empty() {
            stdout.push_str(stdin);
        }
        return 0;
    }

    let mut code = 0;
    for file in files {
        match syscall::open(file, syscall::OpenFlags::READ) {
            Ok(fd) => {
                let mut buf = [0u8; 1024];
                loop {
                    match syscall::read(fd, &mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Ok(s) = std::str::from_utf8(&buf[..n]) {
                                stdout.push_str(s);
                            }
                        }
                        Err(e) => {
                            stderr.push_str(&format!("cat: {}: {}\n", file, e));
                            code = 1;
                            break;
                        }
                    }
                }
                let _ = syscall::close(fd);
            }
            Err(e) => {
                stderr.push_str(&format!("cat: {}: {}\n", file, e));
                code = 1;
            }
        }
    }
    code
}

/// ls - list directory contents
pub fn prog_ls(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let paths = args_to_strs(args);

    if let Some(help) = check_help(&paths, "Usage: ls [-la] [PATH]...\nList directory contents. See 'man ls' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    let paths: Vec<&str> = paths.into_iter().filter(|p| !p.starts_with('-') || *p == "-" ).collect();
    let paths = if paths.is_empty() {
        vec!["."]
    } else {
        paths
    };

    // ANSI color codes
    const BLUE: &str = "\x1b[34m";   // directories
    const CYAN: &str = "\x1b[36m";   // symlinks
    const RESET: &str = "\x1b[0m";

    let mut code = 0;
    for path in paths {
        match syscall::readdir(path) {
            Ok(entries) => {
                for entry in entries {
                    // Check if it's a directory
                    let full_path = if path == "." {
                        entry.clone()
                    } else if path == "/" {
                        format!("/{}", entry)
                    } else {
                        format!("{}/{}", path, entry)
                    };

                    let meta = syscall::metadata(&full_path);
                    let is_dir = meta.as_ref().map(|m| m.is_dir).unwrap_or(false);
                    let is_symlink = meta.as_ref().map(|m| m.is_symlink).unwrap_or(false);
                    let symlink_target = meta.as_ref().ok().and_then(|m| m.symlink_target.clone());

                    if is_symlink {
                        stdout.push_str(CYAN);
                        stdout.push_str(&entry);
                        stdout.push_str(RESET);
                        if let Some(target) = symlink_target {
                            stdout.push_str(" -> ");
                            stdout.push_str(&target);
                        }
                    } else if is_dir {
                        stdout.push_str(BLUE);
                        stdout.push_str(&entry);
                        stdout.push_str(RESET);
                    } else {
                        stdout.push_str(&entry);
                    }
                    stdout.push('\n');
                }
            }
            Err(e) => {
                stderr.push_str(&format!("ls: {}: {}\n", path, e));
                code = 1;
            }
        }
    }

    // Remove trailing newline
    if stdout.ends_with('\n') {
        stdout.pop();
    }

    code
}

/// mkdir - create directories
pub fn prog_mkdir(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let paths = args_to_strs(args);

    if let Some(help) = check_help(&paths, "Usage: mkdir DIRECTORY...\nCreate directories. See 'man mkdir' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    if paths.is_empty() {
        stderr.push_str("mkdir: missing operand\n");
        return 1;
    }

    let mut code = 0;
    for path in paths {
        if let Err(e) = syscall::mkdir(path) {
            stderr.push_str(&format!("mkdir: {}: {}\n", path, e));
            code = 1;
        }
    }
    code
}

/// touch - create empty files
pub fn prog_touch(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let paths = args_to_strs(args);

    if let Some(help) = check_help(&paths, "Usage: touch FILE...\nCreate empty files or update timestamps. See 'man touch' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    if paths.is_empty() {
        stderr.push_str("touch: missing operand\n");
        return 1;
    }

    let mut code = 0;
    for path in paths {
        // OpenFlags::WRITE includes create and truncate
        match syscall::open(path, syscall::OpenFlags::WRITE) {
            Ok(fd) => {
                let _ = syscall::close(fd);
            }
            Err(e) => {
                stderr.push_str(&format!("touch: {}: {}\n", path, e));
                code = 1;
            }
        }
    }
    code
}

/// rm - remove files
pub fn prog_rm(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: rm [-r] FILE...\nRemove files or directories. See 'man rm' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    if args.is_empty() {
        stderr.push_str("rm: missing operand\n");
        return 1;
    }

    let recursive = args.iter().any(|&a| a == "-r" || a == "-rf" || a == "-fr");
    let paths: Vec<&str> = args.iter()
        .copied()
        .filter(|a| !a.starts_with('-'))
        .collect();

    if paths.is_empty() {
        stderr.push_str("rm: missing operand\n");
        return 1;
    }

    let mut failed = false;
    for path in paths {
        // Check if it's a directory
        match syscall::metadata(path) {
            Ok(meta) if meta.is_dir => {
                if recursive {
                    if let Err(e) = syscall::remove_dir(path) {
                        stderr.push_str(&format!("rm: cannot remove '{}': {}\n", path, e));
                        failed = true;
                    }
                } else {
                    stderr.push_str(&format!("rm: cannot remove '{}': Is a directory\n", path));
                    failed = true;
                }
            }
            Ok(_) => {
                if let Err(e) = syscall::remove_file(path) {
                    stderr.push_str(&format!("rm: cannot remove '{}': {}\n", path, e));
                    failed = true;
                }
            }
            Err(e) => {
                stderr.push_str(&format!("rm: cannot remove '{}': {}\n", path, e));
                failed = true;
            }
        }
    }

    if failed { 1 } else { 0 }
}

/// cp - copy files
pub fn prog_cp(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: cp SOURCE DEST\nCopy files. See 'man cp' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    if args.len() < 2 {
        stderr.push_str("cp: missing operand\n");
        return 1;
    }

    let src = &args[0];
    let dst = &args[1];

    match syscall::copy_file(src, dst) {
        Ok(_) => 0,
        Err(e) => {
            stderr.push_str(&format!("cp: cannot copy '{}' to '{}': {}\n", src, dst, e));
            1
        }
    }
}

/// mv - move/rename files
pub fn prog_mv(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: mv SOURCE DEST\nMove or rename files. See 'man mv' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    if args.len() < 2 {
        stderr.push_str("mv: missing operand\n");
        return 1;
    }

    let src = &args[0];
    let dst = &args[1];

    match syscall::rename(src, dst) {
        Ok(()) => 0,
        Err(e) => {
            stderr.push_str(&format!("mv: cannot move '{}' to '{}': {}\n", src, dst, e));
            1
        }
    }
}

/// ln - create symbolic links
pub fn prog_ln(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: ln -s TARGET LINK_NAME\nCreate symbolic links. See 'man ln' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    // Parse flags
    let mut symbolic = false;
    let mut force = false;
    let mut targets: Vec<&str> = Vec::new();

    for arg in &args {
        if *arg == "-s" || *arg == "--symbolic" {
            symbolic = true;
        } else if *arg == "-f" || *arg == "--force" {
            force = true;
        } else if arg.starts_with('-') {
            // Handle combined flags like -sf
            for c in arg[1..].chars() {
                match c {
                    's' => symbolic = true,
                    'f' => force = true,
                    _ => {
                        stderr.push_str(&format!("ln: unknown option: -{}\n", c));
                        return 1;
                    }
                }
            }
        } else {
            targets.push(arg);
        }
    }

    if targets.len() < 2 {
        stderr.push_str("ln: missing file operand\n");
        stderr.push_str("Usage: ln [-sf] TARGET LINK_NAME\n");
        return 1;
    }

    if !symbolic {
        stderr.push_str("ln: hard links not supported, use -s for symbolic links\n");
        return 1;
    }

    let target = targets[0];
    let link_name = targets[1];

    // If force, try to remove existing link
    if force {
        let _ = syscall::remove_file(link_name);
    }

    match syscall::symlink(target, link_name) {
        Ok(_) => 0,
        Err(e) => {
            stderr.push_str(&format!("ln: {}: {}\n", link_name, e));
            1
        }
    }
}

/// readlink - print value of a symbolic link
pub fn prog_readlink(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if args.is_empty() {
        stderr.push_str("readlink: missing file operand\n");
        return 1;
    }

    let path = &args[0];

    match syscall::read_link(path) {
        Ok(target) => {
            stdout.push_str(&target);
            0
        }
        Err(e) => {
            stderr.push_str(&format!("readlink: {}: {}\n", path, e));
            1
        }
    }
}

/// tree - display directory tree
pub fn prog_tree(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let paths = args_to_strs(args);

    if let Some(help) = check_help(&paths, "Usage: tree [DIRECTORY]\nDisplay directory tree. See 'man tree' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    let path = if paths.is_empty() { "." } else { paths[0] };

    // ANSI colors
    const BLUE: &str = "\x1b[34m";
    const RESET: &str = "\x1b[0m";

    fn print_tree(
        path: &str,
        prefix: &str,
        stdout: &mut String,
        _is_last: bool,
        dir_count: &mut usize,
        file_count: &mut usize,
    ) -> Result<(), String> {
        let entries = syscall::readdir(path).map_err(|e| e.to_string())?;
        let mut entries: Vec<_> = entries.into_iter().collect();
        entries.sort();

        for (i, entry) in entries.iter().enumerate() {
            let is_last_entry = i == entries.len() - 1;
            let connector = if is_last_entry { "└── " } else { "├── " };
            let child_prefix = if is_last_entry { "    " } else { "│   " };

            let full_path = if path == "/" {
                format!("/{}", entry)
            } else if path == "." {
                entry.clone()
            } else {
                format!("{}/{}", path, entry)
            };

            let meta = syscall::metadata(&full_path);
            let is_dir = meta.as_ref().map(|m| m.is_dir).unwrap_or(false);
            let is_symlink = meta.as_ref().map(|m| m.is_symlink).unwrap_or(false);
            let symlink_target = meta.as_ref().ok().and_then(|m| m.symlink_target.clone());

            if is_symlink {
                *file_count += 1;
                let target_str = symlink_target.map(|t| format!(" -> {}", t)).unwrap_or_default();
                stdout.push_str(&format!("{}{}\x1b[36m{}\x1b[0m{}\n", prefix, connector, entry, target_str));
            } else if is_dir {
                *dir_count += 1;
                stdout.push_str(&format!("{}{}{}{}{}\n", prefix, connector, BLUE, entry, RESET));
                let new_prefix = format!("{}{}", prefix, child_prefix);
                let _ = print_tree(&full_path, &new_prefix, stdout, is_last_entry, dir_count, file_count);
            } else {
                *file_count += 1;
                stdout.push_str(&format!("{}{}{}\n", prefix, connector, entry));
            }
        }
        Ok(())
    }

    // Print root
    let is_dir = syscall::metadata(path).map(|m| m.is_dir).unwrap_or(false);
    if !is_dir {
        stderr.push_str(&format!("tree: {}: Not a directory\n", path));
        return 1;
    }

    stdout.push_str(&format!("{}{}{}\n", BLUE, path, RESET));

    let mut dir_count = 0usize;
    let mut file_count = 0usize;

    if let Err(e) = print_tree(path, "", stdout, false, &mut dir_count, &mut file_count) {
        stderr.push_str(&format!("tree: {}\n", e));
        return 1;
    }

    stdout.push_str(&format!("\n{} directories, {} files", dir_count, file_count));
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cat_stdin() {
        let args: Vec<String> = vec![];
        let stdin = "hello world";
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_cat(&args, stdin, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout, "hello world");
    }

    #[test]
    fn test_ls_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_ls(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stdout.contains("Usage:"));
    }

    #[test]
    fn test_mkdir_missing_operand() {
        let args: Vec<String> = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_mkdir(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        assert!(stderr.contains("missing operand"));
    }

    #[test]
    fn test_touch_missing_operand() {
        let args: Vec<String> = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_touch(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        assert!(stderr.contains("missing operand"));
    }

    #[test]
    fn test_rm_missing_operand() {
        let args: Vec<String> = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_rm(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        assert!(stderr.contains("missing operand"));
    }

    #[test]
    fn test_cp_missing_operand() {
        let args: Vec<String> = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_cp(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        assert!(stderr.contains("missing operand"));
    }

    #[test]
    fn test_mv_missing_operand() {
        let args: Vec<String> = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_mv(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        assert!(stderr.contains("missing operand"));
    }

    #[test]
    fn test_ln_requires_symbolic() {
        let args = vec!["target".to_string(), "link".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_ln(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        assert!(stderr.contains("hard links not supported"));
    }

    #[test]
    fn test_readlink_missing_operand() {
        let args: Vec<String> = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_readlink(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        assert!(stderr.contains("missing file operand"));
    }
}
