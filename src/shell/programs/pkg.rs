//! Package manager program

use super::{args_to_strs, check_help};
use crate::kernel::syscall;

pub fn prog_pkg(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: pkg <command> [args]\n\nPackage manager for axeberg.\n\nCommands:\n  install <name> <script>   Install a package (script content)\n  remove <name>             Remove a package\n  list                      List installed packages\n  run <name> [args]         Run an installed package\n  info <name>               Show package info\n\nPackages are stored in /var/packages/") {
        stdout.push_str(&help);
        return 0;
    }

    if args.is_empty() {
        stderr.push_str("pkg: missing command\nTry 'pkg --help' for more information.\n");
        return 1;
    }

    // Ensure package directories exist
    let _ = syscall::mkdir("/var");
    let _ = syscall::mkdir("/var/packages");

    match &args[0][..] {
        "install" => {
            if args.len() < 3 {
                stderr.push_str("pkg install: usage: pkg install <name> <script>\n");
                return 1;
            }
            let name = args[1];
            // Join remaining args as the script content (or use quoted string)
            let script = args[2..].join(" ");

            // Validate package name
            if name.contains('/') || name.contains('\0') || name.is_empty() {
                stderr.push_str("pkg install: invalid package name\n");
                return 1;
            }

            let pkg_path = format!("/var/packages/{}", name);

            // Write the package script
            match syscall::open(&pkg_path, syscall::OpenFlags::WRITE) {
                Ok(fd) => {
                    let _ = syscall::write(fd, script.as_bytes());
                    let _ = syscall::close(fd);

                    // Make it executable (mode 755)
                    let _ = syscall::chmod(&pkg_path, 0o755);

                    stdout.push_str(&format!("Installed package '{}'\n", name));
                    0
                }
                Err(e) => {
                    stderr.push_str(&format!("pkg install: failed to install '{}': {:?}\n", name, e));
                    1
                }
            }
        }
        "remove" | "uninstall" => {
            if args.len() < 2 {
                stderr.push_str("pkg remove: usage: pkg remove <name>\n");
                return 1;
            }
            let name = args[1];
            let pkg_path = format!("/var/packages/{}", name);

            match syscall::remove_file(&pkg_path) {
                Ok(()) => {
                    stdout.push_str(&format!("Removed package '{}'\n", name));
                    0
                }
                Err(e) => {
                    stderr.push_str(&format!("pkg remove: '{}': {:?}\n", name, e));
                    1
                }
            }
        }
        "list" | "ls" => {
            match syscall::readdir("/var/packages") {
                Ok(entries) => {
                    if entries.is_empty() {
                        stdout.push_str("No packages installed.\n");
                    } else {
                        stdout.push_str("Installed packages:\n");
                        for entry in entries {
                            stdout.push_str(&format!("  {}\n", entry));
                        }
                    }
                    0
                }
                Err(_) => {
                    stdout.push_str("No packages installed.\n");
                    0
                }
            }
        }
        "run" | "exec" => {
            if args.len() < 2 {
                stderr.push_str("pkg run: usage: pkg run <name> [args]\n");
                return 1;
            }
            let name = args[1];
            let pkg_path = format!("/var/packages/{}", name);

            // Read the package script
            match syscall::open(&pkg_path, syscall::OpenFlags::READ) {
                Ok(fd) => {
                    let mut buf = vec![0u8; 65536];
                    match syscall::read(fd, &mut buf) {
                        Ok(n) => {
                            let _ = syscall::close(fd);
                            let script = String::from_utf8_lossy(&buf[..n]).to_string();

                            // Execute each line of the script
                            for line in script.lines() {
                                let line = line.trim();
                                if line.is_empty() || line.starts_with('#') {
                                    continue;
                                }
                                // Note: In a full implementation, we'd parse and execute properly
                                stdout.push_str(&format!("> {}\n", line));
                            }
                            0
                        }
                        Err(e) => {
                            let _ = syscall::close(fd);
                            stderr.push_str(&format!("pkg run: failed to read '{}': {:?}\n", name, e));
                            1
                        }
                    }
                }
                Err(_) => {
                    stderr.push_str(&format!("pkg run: package '{}' not found\n", name));
                    1
                }
            }
        }
        "info" | "show" => {
            if args.len() < 2 {
                stderr.push_str("pkg info: usage: pkg info <name>\n");
                return 1;
            }
            let name = args[1];
            let pkg_path = format!("/var/packages/{}", name);

            match syscall::metadata(&pkg_path) {
                Ok(meta) => {
                    stdout.push_str(&format!("Package: {}\n", name));
                    stdout.push_str(&format!("Path: {}\n", pkg_path));
                    stdout.push_str(&format!("Size: {} bytes\n", meta.size));

                    // Show first few lines of script
                    if let Ok(fd) = syscall::open(&pkg_path, syscall::OpenFlags::READ) {
                        let mut buf = vec![0u8; 512];
                        if let Ok(n) = syscall::read(fd, &mut buf) {
                            let preview = String::from_utf8_lossy(&buf[..n]);
                            stdout.push_str("\nScript preview:\n");
                            for (i, line) in preview.lines().take(5).enumerate() {
                                stdout.push_str(&format!("  {}: {}\n", i + 1, line));
                            }
                        }
                        let _ = syscall::close(fd);
                    }
                    0
                }
                Err(_) => {
                    stderr.push_str(&format!("pkg info: package '{}' not found\n", name));
                    1
                }
            }
        }
        cmd => {
            stderr.push_str(&format!("pkg: unknown command '{}'\n", cmd));
            stderr.push_str("Try 'pkg --help' for available commands.\n");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkg_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_pkg(&args, "", &mut stdout, &mut stderr);

        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: pkg <command> [args]"));
        assert!(stdout.contains("Package manager for axeberg"));
        assert!(stderr.is_empty());
    }

    #[test]
    fn test_pkg_no_args() {
        let args = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_pkg(&args, "", &mut stdout, &mut stderr);

        assert_eq!(result, 1);
        assert!(stderr.contains("pkg: missing command"));
        assert!(stdout.is_empty());
    }

    #[test]
    fn test_pkg_unknown_command() {
        let args = vec!["unknown".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_pkg(&args, "", &mut stdout, &mut stderr);

        assert_eq!(result, 1);
        assert!(stderr.contains("pkg: unknown command 'unknown'"));
        assert!(stderr.contains("Try 'pkg --help' for available commands"));
    }

    #[test]
    fn test_pkg_install_missing_args() {
        let args = vec!["install".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_pkg(&args, "", &mut stdout, &mut stderr);

        assert_eq!(result, 1);
        assert!(stderr.contains("pkg install: usage: pkg install <name> <script>"));
    }

    #[test]
    fn test_pkg_install_invalid_name() {
        let args = vec!["install".to_string(), "bad/name".to_string(), "echo test".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_pkg(&args, "", &mut stdout, &mut stderr);

        assert_eq!(result, 1);
        assert!(stderr.contains("pkg install: invalid package name"));
    }

    #[test]
    fn test_pkg_remove_missing_args() {
        let args = vec!["remove".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_pkg(&args, "", &mut stdout, &mut stderr);

        assert_eq!(result, 1);
        assert!(stderr.contains("pkg remove: usage: pkg remove <name>"));
    }

    #[test]
    fn test_pkg_run_missing_args() {
        let args = vec!["run".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_pkg(&args, "", &mut stdout, &mut stderr);

        assert_eq!(result, 1);
        assert!(stderr.contains("pkg run: usage: pkg run <name> [args]"));
    }

    #[test]
    fn test_pkg_info_missing_args() {
        let args = vec!["info".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_pkg(&args, "", &mut stdout, &mut stderr);

        assert_eq!(result, 1);
        assert!(stderr.contains("pkg info: usage: pkg info <name>"));
    }
}
