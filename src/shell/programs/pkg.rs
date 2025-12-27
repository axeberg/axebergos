//! Package manager CLI
//!
//! Provides command-line interface to the axeberg package manager.
//!
//! # Commands
//!
//! - `pkg install <name>[@version]` - Install a package from registry
//! - `pkg install-local <path>` - Install from local .axepkg file
//! - `pkg remove <name>` - Remove an installed package
//! - `pkg list` - List installed packages
//! - `pkg info <name>` - Show package information
//! - `pkg search <query>` - Search for packages
//! - `pkg update` - Update registry index
//! - `pkg upgrade` - Upgrade all packages
//! - `pkg verify` - Verify installed packages
//! - `pkg clean` - Clean package cache
//! - `pkg init` - Initialize package directories

use super::{args_to_strs, check_help};
use crate::kernel::pkg::{PackageDatabase, PackageManager};
use crate::kernel::syscall;

const HELP_TEXT: &str = r#"Usage: pkg <command> [args]

WASM Package Manager for axeberg.

Commands:
  install <name>[@version]   Install a package from registry
  install-local <path>       Install from local .axepkg file
  remove <name>              Remove an installed package
  list                       List installed packages
  info <name>                Show package information
  search <query>             Search for packages (async)
  update                     Update registry index (async)
  upgrade                    Upgrade all packages (async)
  verify                     Verify installed package integrity
  clean                      Clean package cache
  init                       Initialize package directories

Options:
  -h, --help                 Show this help message
  -v, --version              Show version information

Examples:
  pkg install hello          Install latest version of 'hello'
  pkg install hello@1.0.0    Install specific version
  pkg install-local ./my.axepkg  Install from local file
  pkg remove hello           Remove 'hello' package
  pkg list                   Show all installed packages

Note: Some commands (search, update, upgrade) require network access
and are only available in WASM builds."#;

pub fn prog_pkg(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, HELP_TEXT) {
        stdout.push_str(&help);
        return 0;
    }

    if args.is_empty() {
        stderr.push_str("pkg: missing command\nTry 'pkg --help' for more information.\n");
        return 1;
    }

    // Handle version flag
    if args[0] == "-v" || args[0] == "--version" {
        stdout.push_str("pkg 1.0.0 (axeberg package manager)\n");
        return 0;
    }

    match args[0] {
        "init" => cmd_init(stdout, stderr),
        "install" => cmd_install(&args[1..], stdout, stderr),
        "install-local" => cmd_install_local(&args[1..], stdout, stderr),
        "remove" | "uninstall" | "rm" => cmd_remove(&args[1..], stdout, stderr),
        "list" | "ls" => cmd_list(stdout, stderr),
        "info" | "show" => cmd_info(&args[1..], stdout, stderr),
        "search" => cmd_search(&args[1..], stdout, stderr),
        "update" => cmd_update(stdout, stderr),
        "upgrade" => cmd_upgrade(stdout, stderr),
        "verify" => cmd_verify(stdout, stderr),
        "clean" => cmd_clean(stdout, stderr),
        cmd => {
            stderr.push_str(&format!("pkg: unknown command '{}'\n", cmd));
            stderr.push_str("Try 'pkg --help' for available commands.\n");
            1
        }
    }
}

/// Initialize package manager directories
fn cmd_init(stdout: &mut String, stderr: &mut String) -> i32 {
    let pm = PackageManager::new();
    match pm.init() {
        Ok(()) => {
            stdout.push_str("Package manager initialized.\n");
            stdout.push_str("Directories created:\n");
            stdout.push_str("  /var/lib/pkg/db/       - Package database\n");
            stdout.push_str("  /var/lib/pkg/cache/    - Download cache\n");
            stdout.push_str("  /var/lib/pkg/registry/ - Registry index\n");
            0
        }
        Err(e) => {
            stderr.push_str(&format!("pkg init: {}\n", e));
            1
        }
    }
}

/// Install a package from registry
fn cmd_install(args: &[&str], _stdout: &mut String, stderr: &mut String) -> i32 {
    if args.is_empty() {
        stderr.push_str("pkg install: missing package name\n");
        stderr.push_str("Usage: pkg install <name>[@version]\n");
        return 1;
    }

    // Parse name[@version]
    let spec = args[0];
    let (name, _version) = if let Some(at_pos) = spec.find('@') {
        (&spec[..at_pos], Some(&spec[at_pos + 1..]))
    } else {
        (spec, None)
    };

    // Validate package name
    if name.is_empty() || name.contains('/') {
        stderr.push_str("pkg install: invalid package name\n");
        return 1;
    }

    // In WASM builds, spawn async installation
    #[cfg(target_arch = "wasm32")]
    {
        let name = name.to_string();
        let version = version.map(|v| v.to_string());

        stdout.push_str(&format!("Installing {}...\n", spec));
        stdout.push_str("(Running in background - check console for results)\n");

        wasm_bindgen_futures::spawn_local(async move {
            let mut pm = PackageManager::new();
            if let Err(e) = pm.init() {
                crate::console_log!("pkg install: init failed: {}", e);
                return;
            }

            match pm.install(&name, version.as_deref()).await {
                Ok(id) => {
                    crate::console_log!("pkg: installed {} successfully", id);
                }
                Err(e) => {
                    crate::console_log!("pkg install: {}", e);
                }
            }
        });
        return 0;
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        stderr.push_str("pkg install: network installation requires WASM build\n");
        stderr.push_str("Use 'pkg install-local <path>' to install from a local file.\n");
        1
    }
}

/// Install a package from local file
fn cmd_install_local(args: &[&str], stdout: &mut String, stderr: &mut String) -> i32 {
    if args.is_empty() {
        stderr.push_str("pkg install-local: missing file path\n");
        stderr.push_str("Usage: pkg install-local <path>\n");
        return 1;
    }

    let path = args[0];

    // Check file exists
    if !syscall::exists(path).unwrap_or(false) {
        stderr.push_str(&format!("pkg install-local: file not found: {}\n", path));
        return 1;
    }

    let mut pm = PackageManager::new();
    if let Err(e) = pm.init() {
        stderr.push_str(&format!("pkg: initialization failed: {}\n", e));
        return 1;
    }

    match pm.install_local(path) {
        Ok(id) => {
            stdout.push_str(&format!("Installed {} from {}\n", id, path));
            0
        }
        Err(e) => {
            stderr.push_str(&format!("pkg install-local: {}\n", e));
            1
        }
    }
}

/// Remove an installed package
fn cmd_remove(args: &[&str], stdout: &mut String, stderr: &mut String) -> i32 {
    if args.is_empty() {
        stderr.push_str("pkg remove: missing package name\n");
        stderr.push_str("Usage: pkg remove <name>\n");
        return 1;
    }

    let name = args[0];

    let mut pm = PackageManager::new();
    match pm.remove(name) {
        Ok(()) => {
            stdout.push_str(&format!("Removed package '{}'\n", name));
            0
        }
        Err(e) => {
            stderr.push_str(&format!("pkg remove: {}\n", e));
            1
        }
    }
}

/// List installed packages
fn cmd_list(stdout: &mut String, stderr: &mut String) -> i32 {
    let pm = PackageManager::new();
    match pm.list_installed() {
        Ok(packages) => {
            if packages.is_empty() {
                stdout.push_str("No packages installed.\n");
                stdout.push_str("Use 'pkg install <name>' to install a package.\n");
            } else {
                stdout.push_str("Installed packages:\n");
                stdout.push_str(&format!(
                    "{:<20} {:<12} {}\n",
                    "NAME", "VERSION", "BINARIES"
                ));
                stdout.push_str(&format!(
                    "{:<20} {:<12} {}\n",
                    "----", "-------", "--------"
                ));
                for pkg in packages {
                    let bins = if pkg.binaries.is_empty() {
                        "(none)".to_string()
                    } else {
                        pkg.binaries
                            .iter()
                            .map(|b| b.rsplit('/').next().unwrap_or(b).trim_end_matches(".wasm"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    stdout.push_str(&format!("{:<20} {:<12} {}\n", pkg.name, pkg.version, bins));
                }
            }
            0
        }
        Err(e) => {
            stderr.push_str(&format!("pkg list: {}\n", e));
            1
        }
    }
}

/// Show package information
fn cmd_info(args: &[&str], stdout: &mut String, stderr: &mut String) -> i32 {
    if args.is_empty() {
        stderr.push_str("pkg info: missing package name\n");
        stderr.push_str("Usage: pkg info <name>\n");
        return 1;
    }

    let name = args[0];

    let mut db = PackageDatabase::new();
    match db.get_installed(name) {
        Ok(Some(pkg)) => {
            stdout.push_str(&format!("Package: {}\n", pkg.name));
            stdout.push_str(&format!("Version: {}\n", pkg.version));
            stdout.push_str(&format!(
                "Installed: {}\n",
                format_timestamp(pkg.installed_at)
            ));

            if !pkg.binaries.is_empty() {
                stdout.push_str("Binaries:\n");
                for bin in &pkg.binaries {
                    stdout.push_str(&format!("  {}\n", bin));
                }
            }

            if !pkg.dependencies.is_empty() {
                stdout.push_str("Dependencies:\n");
                for dep in &pkg.dependencies {
                    stdout.push_str(&format!("  {}\n", dep));
                }
            }

            // Try to show manifest info
            if let Ok(Some(manifest)) = db.get_manifest(name) {
                if let Some(desc) = manifest.description {
                    stdout.push_str(&format!("\nDescription: {}\n", desc));
                }
                if let Some(license) = manifest.license {
                    stdout.push_str(&format!("License: {}\n", license));
                }
                if !manifest.authors.is_empty() {
                    stdout.push_str(&format!("Authors: {}\n", manifest.authors.join(", ")));
                }
            }

            0
        }
        Ok(None) => {
            stderr.push_str(&format!("pkg info: package '{}' not installed\n", name));
            1
        }
        Err(e) => {
            stderr.push_str(&format!("pkg info: {}\n", e));
            1
        }
    }
}

/// Search for packages (async)
fn cmd_search(args: &[&str], _stdout: &mut String, stderr: &mut String) -> i32 {
    if args.is_empty() {
        stderr.push_str("pkg search: missing search query\n");
        stderr.push_str("Usage: pkg search <query>\n");
        return 1;
    }

    let _query = args.join(" ");

    #[cfg(target_arch = "wasm32")]
    {
        _stdout.push_str(&format!("Searching for '{}'...\n", _query));
        _stdout.push_str("(Running in background - check console for results)\n");

        wasm_bindgen_futures::spawn_local(async move {
            let pm = PackageManager::new();
            match pm.search(&_query).await {
                Ok(results) => {
                    if results.is_empty() {
                        crate::console_log!("No packages found matching '{}'", _query);
                    } else {
                        crate::console_log!("Found {} package(s):", results.len());
                        for pkg in results {
                            let desc = pkg.description.as_deref().unwrap_or("No description");
                            crate::console_log!("  {} ({}) - {}", pkg.name, pkg.latest, desc);
                        }
                    }
                }
                Err(e) => {
                    crate::console_log!("pkg search: {}", e);
                }
            }
        });
        return 0;
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        stderr.push_str("pkg search: requires WASM build for network access\n");
        1
    }
}

/// Update registry index (async)
fn cmd_update(_stdout: &mut String, stderr: &mut String) -> i32 {
    #[cfg(target_arch = "wasm32")]
    {
        _stdout.push_str("Updating package registry...\n");
        _stdout.push_str("(Running in background - check console for results)\n");

        wasm_bindgen_futures::spawn_local(async move {
            let mut pm = PackageManager::new();
            match pm.update_index().await {
                Ok(()) => {
                    crate::console_log!("pkg: registry index updated successfully");
                }
                Err(e) => {
                    crate::console_log!("pkg update: {}", e);
                }
            }
        });
        return 0;
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        stderr.push_str("pkg update: requires WASM build for network access\n");
        1
    }
}

/// Upgrade all packages (async)
fn cmd_upgrade(_stdout: &mut String, stderr: &mut String) -> i32 {
    #[cfg(target_arch = "wasm32")]
    {
        _stdout.push_str("Checking for upgrades...\n");
        _stdout.push_str("(Running in background - check console for results)\n");

        wasm_bindgen_futures::spawn_local(async move {
            let mut pm = PackageManager::new();
            match pm.upgrade_all().await {
                Ok(upgraded) => {
                    if upgraded.is_empty() {
                        crate::console_log!("pkg: all packages are up to date");
                    } else {
                        crate::console_log!("pkg: upgraded {} package(s):", upgraded.len());
                        for id in upgraded {
                            crate::console_log!("  {} -> {}", id.name, id.version);
                        }
                    }
                }
                Err(e) => {
                    crate::console_log!("pkg upgrade: {}", e);
                }
            }
        });
        return 0;
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        stderr.push_str("pkg upgrade: requires WASM build for network access\n");
        1
    }
}

/// Verify installed packages
fn cmd_verify(stdout: &mut String, stderr: &mut String) -> i32 {
    let pm = PackageManager::new();
    match pm.verify() {
        Ok(results) => {
            let mut all_valid = true;
            for (name, valid) in results {
                let status = if valid { "OK" } else { "INVALID" };
                stdout.push_str(&format!("{}: {}\n", name, status));
                if !valid {
                    all_valid = false;
                }
            }
            if all_valid {
                stdout.push_str("\nAll packages verified successfully.\n");
                0
            } else {
                stderr.push_str("\nSome packages failed verification.\n");
                1
            }
        }
        Err(e) => {
            stderr.push_str(&format!("pkg verify: {}\n", e));
            1
        }
    }
}

/// Clean package cache
fn cmd_clean(stdout: &mut String, stderr: &mut String) -> i32 {
    let pm = PackageManager::new();
    match pm.clean_cache() {
        Ok(()) => {
            stdout.push_str("Package cache cleaned.\n");
            0
        }
        Err(e) => {
            stderr.push_str(&format!("pkg clean: {}\n", e));
            1
        }
    }
}

/// Format a Unix timestamp for display
fn format_timestamp(ts: u64) -> String {
    if ts == 0 {
        return "unknown".to_string();
    }

    // Simple date formatting (no external dependencies)
    // This is approximate - doesn't handle leap seconds, etc.
    let secs_per_day = 86400u64;
    let secs_per_year = 31536000u64; // 365 days
    let secs_per_leap_year = 31622400u64; // 366 days

    let mut remaining = ts;
    let mut year = 1970u64;

    // Count years
    loop {
        let year_secs = if is_leap_year(year) {
            secs_per_leap_year
        } else {
            secs_per_year
        };
        if remaining < year_secs {
            break;
        }
        remaining -= year_secs;
        year += 1;
    }

    // Count months
    let days_in_months: [u64; 12] = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 0usize;
    for (i, &days) in days_in_months.iter().enumerate() {
        let month_secs = days * secs_per_day;
        if remaining < month_secs {
            month = i;
            break;
        }
        remaining -= month_secs;
    }

    let day = remaining / secs_per_day + 1;
    remaining %= secs_per_day;
    let hour = remaining / 3600;
    remaining %= 3600;
    let min = remaining / 60;

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        year,
        month + 1,
        day,
        hour,
        min
    )
}

fn is_leap_year(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
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
        assert!(stdout.contains("WASM Package Manager"));
        assert!(stderr.is_empty());
    }

    #[test]
    fn test_pkg_version() {
        let args = vec!["-v".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_pkg(&args, "", &mut stdout, &mut stderr);

        assert_eq!(result, 0);
        assert!(stdout.contains("pkg 1.0.0"));
    }

    #[test]
    fn test_pkg_no_args() {
        let args = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_pkg(&args, "", &mut stdout, &mut stderr);

        assert_eq!(result, 1);
        assert!(stderr.contains("pkg: missing command"));
    }

    #[test]
    fn test_pkg_unknown_command() {
        let args = vec!["unknown".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_pkg(&args, "", &mut stdout, &mut stderr);

        assert_eq!(result, 1);
        assert!(stderr.contains("pkg: unknown command 'unknown'"));
    }

    #[test]
    fn test_pkg_install_missing_args() {
        let args = vec!["install".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_pkg(&args, "", &mut stdout, &mut stderr);

        assert_eq!(result, 1);
        assert!(stderr.contains("pkg install: missing package name"));
    }

    #[test]
    fn test_pkg_remove_missing_args() {
        let args = vec!["remove".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_pkg(&args, "", &mut stdout, &mut stderr);

        assert_eq!(result, 1);
        assert!(stderr.contains("pkg remove: missing package name"));
    }

    #[test]
    fn test_pkg_info_missing_args() {
        let args = vec!["info".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_pkg(&args, "", &mut stdout, &mut stderr);

        assert_eq!(result, 1);
        assert!(stderr.contains("pkg info: missing package name"));
    }

    #[test]
    fn test_pkg_search_missing_args() {
        let args = vec!["search".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_pkg(&args, "", &mut stdout, &mut stderr);

        assert_eq!(result, 1);
        assert!(stderr.contains("pkg search: missing search query"));
    }

    #[test]
    fn test_format_timestamp() {
        // January 1, 2024 00:00 UTC
        let ts = 1704067200;
        let formatted = format_timestamp(ts);
        assert!(formatted.contains("2024"));
    }

    #[test]
    fn test_format_timestamp_zero() {
        assert_eq!(format_timestamp(0), "unknown");
    }

    #[test]
    fn test_is_leap_year() {
        assert!(!is_leap_year(2023));
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(2100));
        assert!(is_leap_year(2000));
    }
}
