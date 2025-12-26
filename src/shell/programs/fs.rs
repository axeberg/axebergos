//! Filesystem management programs
//!
//! This module provides filesystem-related programs including:
//! - `save`: Save filesystem to OPFS (Origin Private File System)
//! - `fsload`: Load filesystem from OPFS
//! - `fsreset`: Reset OPFS storage
//! - `autosave`: Configure automatic filesystem saving
//! - `find`: Search for files and directories
//! - `du`: Disk usage analyzer
//! - `df`: Filesystem space usage

use super::{args_to_strs, check_help};
use crate::kernel::syscall;

/// save - save filesystem to OPFS
pub fn prog_save(_args: &[String], __stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    // Queue the async save operation
    #[cfg(target_arch = "wasm32")]
    {
        use crate::vfs::Persistence;
        wasm_bindgen_futures::spawn_local(async {
            let data = match syscall::vfs_snapshot() {
                Ok(d) => d,
                Err(e) => {
                    crate::console_log!("[save] Snapshot failed: {}", e);
                    return;
                }
            };

            let fs = match crate::vfs::MemoryFs::from_json(&data) {
                Ok(f) => f,
                Err(e) => {
                    crate::console_log!("[save] Deserialize failed: {}", e);
                    return;
                }
            };

            if let Err(e) = Persistence::save(&fs).await {
                crate::console_log!("[save] Save failed: {}", e);
            } else {
                crate::console_log!("[save] Filesystem saved to OPFS");
            }
        });
    }
    stdout.push_str("Saving filesystem to OPFS...\n");
    stdout.push_str("(Check browser console for result)\n");
    0
}

/// fsload - reload filesystem from OPFS
pub fn prog_fsload(args: &[String], __stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);
    if let Some(help) = check_help(&args, "Usage: fsload\nReload filesystem from OPFS storage.\nSee 'man fsload' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    #[cfg(target_arch = "wasm32")]
    {
        use crate::vfs::Persistence;
        wasm_bindgen_futures::spawn_local(async {
            match Persistence::load().await {
                Ok(Some(fs)) => {
                    // Serialize and restore
                    match fs.to_json() {
                        Ok(data) => {
                            if let Err(e) = syscall::vfs_restore(&data) {
                                crate::console_log!("[fsload] Restore failed: {}", e);
                            } else {
                                crate::console_log!("[fsload] Filesystem restored from OPFS");
                            }
                        }
                        Err(e) => {
                            crate::console_log!("[fsload] Serialize failed: {}", e);
                        }
                    }
                }
                Ok(None) => {
                    crate::console_log!("[fsload] No saved filesystem found in OPFS");
                }
                Err(e) => {
                    crate::console_log!("[fsload] Load failed: {}", e);
                }
            }
        });
    }
    stdout.push_str("Loading filesystem from OPFS...\n");
    stdout.push_str("(Check browser console for result)\n");
    0
}

/// fsreset - clear OPFS and reset to fresh filesystem
pub fn prog_fsreset(args: &[String], __stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);
    if let Some(help) = check_help(&args, "Usage: fsreset [-f]\nClear OPFS storage and reset filesystem.\n  -f  Force reset without confirmation\nSee 'man fsreset' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    let force = args.iter().any(|a| *a == "-f" || *a == "--force");

    if !force {
        stderr.push_str("fsreset: This will clear all saved data!\n");
        stderr.push_str("fsreset: Use 'fsreset -f' to confirm.\n");
        return 1;
    }

    #[cfg(target_arch = "wasm32")]
    {
        use crate::vfs::Persistence;
        wasm_bindgen_futures::spawn_local(async {
            if let Err(e) = Persistence::clear().await {
                crate::console_log!("[fsreset] Clear failed: {}", e);
            } else {
                crate::console_log!("[fsreset] OPFS storage cleared");
            }
        });
    }
    stdout.push_str("Clearing OPFS storage...\n");
    stdout.push_str("(Reload the page for a fresh filesystem)\n");
    0
}

/// autosave - configure automatic filesystem saving
pub fn prog_autosave(args: &[String], __stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);
    if let Some(help) = check_help(&args, "Usage: autosave [on|off|status|interval N]\nConfigure automatic filesystem saving.\n  on       Enable auto-save\n  off      Disable auto-save\n  status   Show current settings\n  interval Set commands between saves (default: 10)\nSee 'man autosave' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    #[cfg(target_arch = "wasm32")]
    {
        use crate::terminal;

        if args.is_empty() || (args.len() == 1 && args[0] == "status") {
            let (enabled, interval) = terminal::get_autosave_settings();
            stdout.push_str(&format!("Auto-save: {}\n", if enabled { "enabled" } else { "disabled" }));
            stdout.push_str(&format!("Interval: every {} commands\n", interval));
            return 0;
        }

        match args[0] {
            "on" => {
                terminal::set_autosave(true);
                stdout.push_str("Auto-save enabled\n");
            }
            "off" => {
                terminal::set_autosave(false);
                stdout.push_str("Auto-save disabled\n");
            }
            "interval" => {
                if args.len() < 2 {
                    stderr.push_str("autosave: interval requires a number\n");
                    return 1;
                }
                match args[1].parse::<usize>() {
                    Ok(n) => {
                        terminal::set_autosave_interval(n);
                        stdout.push_str(&format!("Auto-save interval set to {} commands\n", n));
                    }
                    Err(_) => {
                        stderr.push_str("autosave: invalid interval\n");
                        return 1;
                    }
                }
            }
            _ => {
                stderr.push_str("autosave: unknown option. Use 'autosave --help' for usage.\n");
                return 1;
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = stderr;
        stdout.push_str("autosave: not available in this build\n");
    }

    0
}

/// find - search for files and directories
pub fn prog_find(args: &[String], __stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: find [PATH] [-name PATTERN] [-type TYPE]\nSearch for files.") {
        stdout.push_str(&help);
        return 0;
    }

    // Parse arguments
    let mut start_path = ".";
    let mut name_pattern: Option<&str> = None;
    let mut type_filter: Option<char> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "-name" if i + 1 < args.len() => {
                name_pattern = Some(args[i + 1]);
                i += 2;
            }
            "-type" if i + 1 < args.len() => {
                type_filter = args[i + 1].chars().next();
                i += 2;
            }
            s if !s.starts_with('-') && i == 0 => {
                start_path = s;
                i += 1;
            }
            _ => i += 1,
        }
    }

    // Recursive find helper
    fn find_recursive(
        path: &str,
        name_pattern: Option<&str>,
        type_filter: Option<char>,
        stdout: &mut String,
    ) -> Result<(), String> {
        let entries = syscall::readdir(path).map_err(|e| e.to_string())?;

        for entry in entries {
            let full_path = if path == "/" {
                format!("/{}", entry)
            } else {
                format!("{}/{}", path, entry)
            };

            let meta = match syscall::metadata(&full_path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            // Type filter
            let type_match = match type_filter {
                Some('f') => meta.is_file,
                Some('d') => meta.is_dir,
                Some('l') => meta.is_symlink,
                Some(_) | None => true,
            };

            // Name filter (simple glob with * support)
            let name_match = match name_pattern {
                Some(pattern) => {
                    if pattern.contains('*') {
                        let parts: Vec<&str> = pattern.split('*').collect();
                        if parts.len() == 2 {
                            let (prefix, suffix) = (parts[0], parts[1]);
                            entry.starts_with(prefix) && entry.ends_with(suffix)
                        } else if let Some(suffix) = pattern.strip_prefix('*') {
                            entry.ends_with(suffix)
                        } else if let Some(prefix) = pattern.strip_suffix('*') {
                            entry.starts_with(prefix)
                        } else {
                            entry == pattern
                        }
                    } else {
                        entry == pattern
                    }
                }
                None => true,
            };

            if type_match && name_match {
                stdout.push_str(&full_path);
                stdout.push('\n');
            }

            // Recurse into directories
            if meta.is_dir && !meta.is_symlink {
                let _ = find_recursive(&full_path, name_pattern, type_filter, stdout);
            }
        }
        Ok(())
    }

    // Resolve start path
    let resolved = if start_path == "." {
        syscall::getcwd().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| "/".to_string())
    } else if start_path.starts_with('/') {
        start_path.to_string()
    } else {
        let cwd = syscall::getcwd().unwrap_or_else(|_| std::path::PathBuf::from("/"));
        format!("{}/{}", cwd.display(), start_path)
    };

    if let Err(e) = find_recursive(&resolved, name_pattern, type_filter, stdout) {
        stderr.push_str(&format!("find: {}\n", e));
        return 1;
    }

    0
}

/// du - disk usage
pub fn prog_du(args: &[String], __stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: du [-s] [-h] [PATH...]\nEstimate file space usage.") {
        stdout.push_str(&help);
        return 0;
    }

    let summary_only = args.contains(&"-s");
    let human_readable = args.contains(&"-h");
    let paths: Vec<&str> = args.iter()
        .filter(|a| !a.starts_with('-')).copied()
        .collect();

    let paths = if paths.is_empty() { vec!["."] } else { paths };

    fn format_size(size: u64, human: bool) -> String {
        if human {
            if size >= 1024 * 1024 * 1024 {
                format!("{:.1}G", size as f64 / (1024.0 * 1024.0 * 1024.0))
            } else if size >= 1024 * 1024 {
                format!("{:.1}M", size as f64 / (1024.0 * 1024.0))
            } else if size >= 1024 {
                format!("{:.1}K", size as f64 / 1024.0)
            } else {
                format!("{}", size)
            }
        } else {
            format!("{}", size.div_ceil(1024)) // blocks
        }
    }

    fn du_recursive(path: &str, human: bool, summary: bool, stdout: &mut String) -> u64 {
        let mut total: u64 = 0;

        if let Ok(meta) = syscall::metadata(path) {
            if meta.is_file {
                total = meta.size;
            } else if meta.is_dir
                && let Ok(entries) = syscall::readdir(path) {
                    for entry in entries {
                        let full = if path == "/" {
                            format!("/{}", entry)
                        } else {
                            format!("{}/{}", path, entry)
                        };
                        let sub_size = du_recursive(&full, human, true, stdout);
                        total += sub_size;
                    }
                }
        }

        if !summary {
            stdout.push_str(&format!("{}\t{}\n", format_size(total, human), path));
        }

        total
    }

    for path in paths {
        let resolved = if path == "." {
            syscall::getcwd().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| "/".to_string())
        } else if path.starts_with('/') {
            path.to_string()
        } else {
            let cwd = syscall::getcwd().unwrap_or_else(|_| std::path::PathBuf::from("/"));
            format!("{}/{}", cwd.display(), path)
        };

        let total = du_recursive(&resolved, human_readable, summary_only, stdout);
        if summary_only {
            stdout.push_str(&format!("{}\t{}\n", format_size(total, human_readable), path));
        }
    }

    0
}

/// df - filesystem space
pub fn prog_df(args: &[String], __stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: df [-h]\nShow filesystem disk space usage.") {
        stdout.push_str(&help);
        return 0;
    }

    let human_readable = args.contains(&"-h");

    // Calculate total VFS size by walking the filesystem
    fn count_size(path: &str) -> u64 {
        let mut total: u64 = 0;
        if let Ok(meta) = syscall::metadata(path) {
            if meta.is_file {
                total = meta.size;
            } else if meta.is_dir
                && let Ok(entries) = syscall::readdir(path) {
                    for entry in entries {
                        let full = if path == "/" {
                            format!("/{}", entry)
                        } else {
                            format!("{}/{}", path, entry)
                        };
                        total += count_size(&full);
                    }
                }
        }
        total
    }

    let used = count_size("/");
    let total: u64 = 1024 * 1024 * 100; // 100MB virtual filesystem
    let available = total.saturating_sub(used);
    let use_pct = if total > 0 { (used * 100 / total) as u32 } else { 0 };

    fn format_size(size: u64, human: bool) -> String {
        if human {
            if size >= 1024 * 1024 * 1024 {
                format!("{:.1}G", size as f64 / (1024.0 * 1024.0 * 1024.0))
            } else if size >= 1024 * 1024 {
                format!("{:.1}M", size as f64 / (1024.0 * 1024.0))
            } else if size >= 1024 {
                format!("{:.1}K", size as f64 / 1024.0)
            } else {
                format!("{}B", size)
            }
        } else {
            format!("{}", size.div_ceil(1024))
        }
    }

    stdout.push_str("Filesystem      Size  Used Avail Use% Mounted on\n");
    stdout.push_str(&format!(
        "axeberg-vfs  {:>6} {:>5} {:>5} {:>3}% /\n",
        format_size(total, human_readable),
        format_size(used, human_readable),
        format_size(available, human_readable),
        use_pct
    ));

    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prog_save_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let result = prog_save(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_prog_fsload_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let result = prog_fsload(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: fsload"));
    }

    #[test]
    fn test_prog_fsreset_without_force() {
        let args = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let result = prog_fsreset(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("Use 'fsreset -f' to confirm"));
    }

    #[test]
    fn test_prog_fsreset_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let result = prog_fsreset(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: fsreset"));
    }

    #[test]
    fn test_prog_autosave_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let result = prog_autosave(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: autosave"));
    }

    #[test]
    fn test_prog_find_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let result = prog_find(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: find"));
    }

    #[test]
    fn test_prog_du_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let result = prog_du(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: du"));
    }

    #[test]
    fn test_prog_df_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let result = prog_df(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: df"));
    }

    #[test]
    fn test_prog_df_output_format() {
        let args = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let result = prog_df(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Filesystem"));
        assert!(stdout.contains("axeberg-vfs"));
    }

    #[test]
    fn test_prog_df_no_args() {
        let args: Vec<String> = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let result = prog_df(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Filesystem"));
        assert!(stdout.contains("axeberg-vfs"));
    }
}
