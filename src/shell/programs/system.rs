//! System information programs

use super::{args_to_strs, check_help};
use crate::kernel::syscall;

/// id - print process and user IDs (uses kernel syscalls)
pub fn prog_id(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: id [USER]\nPrint user and group IDs.") {
        stdout.push_str(&help);
        return 0;
    }

    // Get user info - either for specified user or current process
    if let Some(username) = args.first() {
        // Show info for specified user
        if let Some(user) = syscall::get_user_by_name(username) {
            let group_name = syscall::get_group_by_gid(user.gid)
                .map(|g| g.name.clone())
                .unwrap_or_else(|| user.gid.0.to_string());

            stdout.push_str(&format!(
                "uid={}({}) gid={}({})\n",
                user.uid.0, user.name, user.gid.0, group_name
            ));
            return 0;
        } else {
            stderr.push_str(&format!("id: '{}': no such user\n", username));
            return 1;
        }
    }

    // Show info for current process
    let uid = match syscall::getuid() {
        Ok(u) => u,
        Err(e) => {
            stderr.push_str(&format!("id: {}\n", e));
            return 1;
        }
    };

    let gid = syscall::getgid().unwrap_or_default();
    let euid = syscall::geteuid().unwrap_or(uid);
    let egid = syscall::getegid().unwrap_or(gid);
    let groups = syscall::getgroups().unwrap_or_default();

    // Get names from user database
    let uid_name = syscall::get_user_by_uid(uid)
        .map(|u| u.name.clone())
        .unwrap_or_else(|| uid.0.to_string());
    let gid_name = syscall::get_group_by_gid(gid)
        .map(|g| g.name.clone())
        .unwrap_or_else(|| gid.0.to_string());

    // Format uid and gid
    stdout.push_str(&format!("uid={}({}) gid={}({})", uid.0, uid_name, gid.0, gid_name));

    // Show effective uid if different
    if euid != uid {
        let euid_name = syscall::get_user_by_uid(euid)
            .map(|u| u.name.clone())
            .unwrap_or_else(|| euid.0.to_string());
        stdout.push_str(&format!(" euid={}({})", euid.0, euid_name));
    }

    // Show effective gid if different
    if egid != gid {
        let egid_name = syscall::get_group_by_gid(egid)
            .map(|g| g.name.clone())
            .unwrap_or_else(|| egid.0.to_string());
        stdout.push_str(&format!(" egid={}({})", egid.0, egid_name));
    }

    // Show groups
    if !groups.is_empty() {
        stdout.push_str(" groups=");
        let group_strs: Vec<String> = groups
            .iter()
            .map(|g| {
                let name = syscall::get_group_by_gid(*g)
                    .map(|gr| gr.name.clone())
                    .unwrap_or_else(|| g.0.to_string());
                format!("{}({})", g.0, name)
            })
            .collect();
        stdout.push_str(&group_strs.join(","));
    }

    stdout.push('\n');
    0
}

/// whoami - print effective username
pub fn prog_whoami(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: whoami\nPrint effective username.") {
        stdout.push_str(&help);
        return 0;
    }

    // Get effective user ID and look up the username
    match syscall::geteuid() {
        Ok(euid) => {
            if let Some(user) = syscall::get_user_by_uid(euid) {
                stdout.push_str(&user.name);
                stdout.push('\n');
                0
            } else {
                // Fallback to environment or uid
                if let Ok(Some(user)) = syscall::getenv("USER") {
                    stdout.push_str(&user);
                    stdout.push('\n');
                    0
                } else {
                    stdout.push_str(&format!("{}\n", euid.0));
                    0
                }
            }
        }
        Err(e) => {
            stderr.push_str(&format!("whoami: {}\n", e));
            1
        }
    }
}

/// groups - print group memberships
pub fn prog_groups(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: groups [USER]\nPrint group memberships.") {
        stdout.push_str(&help);
        return 0;
    }

    // Get groups for specified user or current process
    if let Some(username) = args.first() {
        // Look up user's groups
        if let Some(user) = syscall::get_user_by_name(username) {
            stdout.push_str(username);
            stdout.push_str(" : ");

            // Primary group
            let primary = syscall::get_group_by_gid(user.gid)
                .map(|g| g.name.clone())
                .unwrap_or_else(|| user.gid.0.to_string());
            stdout.push_str(&primary);

            // Get supplementary groups (groups where user is a member)
            for group in syscall::list_groups() {
                if group.gid != user.gid && group.members.iter().any(|m| m == username) {
                    stdout.push(' ');
                    stdout.push_str(&group.name);
                }
            }
            stdout.push('\n');
            return 0;
        } else {
            stderr.push_str(&format!("groups: '{}': no such user\n", username));
            return 1;
        }
    }

    // Current user's groups
    let groups = match syscall::getgroups() {
        Ok(g) => g,
        Err(e) => {
            stderr.push_str(&format!("groups: {}\n", e));
            return 1;
        }
    };

    let names: Vec<String> = groups
        .iter()
        .map(|g| {
            syscall::get_group_by_gid(*g)
                .map(|gr| gr.name.clone())
                .unwrap_or_else(|| g.0.to_string())
        })
        .collect();

    stdout.push_str(&names.join(" "));
    stdout.push('\n');
    0
}

/// hostname - show or set system hostname
pub fn prog_hostname(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: hostname [NAME]\nShow or set system hostname.") {
        stdout.push_str(&help);
        return 0;
    }

    if args.is_empty() {
        // Show hostname
        match syscall::getenv("HOSTNAME") {
            Ok(Some(hostname)) => {
                stdout.push_str(&hostname);
                stdout.push('\n');
                0
            }
            Ok(None) => {
                // Default hostname
                stdout.push_str("axeberg\n");
                0
            }
            Err(e) => {
                stderr.push_str(&format!("hostname: {}\n", e));
                1
            }
        }
    } else {
        // Set hostname
        let new_hostname = args[0];
        match syscall::setenv("HOSTNAME", new_hostname) {
            Ok(()) => 0,
            Err(e) => {
                stderr.push_str(&format!("hostname: {}\n", e));
                1
            }
        }
    }
}

/// uname - print system information
pub fn prog_uname(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: uname [-amnrsv]\nPrint system information.") {
        stdout.push_str(&help);
        return 0;
    }

    // System info
    let kernel_name = "axeberg";
    let hostname = syscall::getenv("HOSTNAME")
        .ok()
        .flatten()
        .unwrap_or_else(|| "axeberg".to_string());
    let kernel_release = "0.1.0";
    let kernel_version = "axebergOS";
    let machine = "wasm32";

    let show_all = args.iter().any(|a| *a == "-a");
    let show_kernel = args.is_empty() || args.iter().any(|a| *a == "-s") || show_all;
    let show_hostname = args.iter().any(|a| *a == "-n") || show_all;
    let show_release = args.iter().any(|a| *a == "-r") || show_all;
    let show_version = args.iter().any(|a| *a == "-v") || show_all;
    let show_machine = args.iter().any(|a| *a == "-m") || show_all;

    let mut parts = Vec::new();
    if show_kernel {
        parts.push(kernel_name);
    }
    if show_hostname {
        parts.push(&hostname);
    }
    if show_release {
        parts.push(kernel_release);
    }
    if show_version {
        parts.push(kernel_version);
    }
    if show_machine {
        parts.push(machine);
    }

    if parts.is_empty() {
        parts.push(kernel_name);
    }

    stdout.push_str(&parts.join(" "));
    stdout.push('\n');
    0
}

/// ps - process status
pub fn prog_ps(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: ps [-a] [-l]\nReport process status.") {
        stdout.push_str(&help);
        return 0;
    }

    let long_format = args.iter().any(|a| *a == "-l");

    let processes = syscall::list_processes();

    if long_format {
        stdout.push_str("  PID  PPID  PGID STATE    COMMAND\n");
    } else {
        stdout.push_str("  PID STATE    COMMAND\n");
    }

    for (pid, name, state) in processes {
        let state_str = match &state {
            syscall::ProcessState::Running => "R",
            syscall::ProcessState::Sleeping => "S",
            syscall::ProcessState::Stopped => "T",
            syscall::ProcessState::Blocked(_) => "D",
            syscall::ProcessState::Zombie(_) => "Z",
        };

        if long_format {
            let ppid = syscall::getppid().ok().flatten().map(|p| p.0).unwrap_or(0);
            let pgid = syscall::getpgid(pid).ok().map(|p| p.0).unwrap_or(pid.0);
            stdout.push_str(&format!(
                "{:>5} {:>5} {:>5} {:8} {}\n",
                pid.0, ppid, pgid, state_str, name
            ));
        } else {
            stdout.push_str(&format!("{:>5} {:8} {}\n", pid.0, state_str, name));
        }
    }

    0
}

/// time - time command execution
pub fn prog_time(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: time COMMAND [ARGS...]\nTime command execution.") {
        stdout.push_str(&help);
        return 0;
    }

    if args.is_empty() {
        stderr.push_str("time: missing command\n");
        return 1;
    }

    let start = syscall::now();

    // We can't actually execute the command here since we're just a program
    // But we can show what we would time
    stdout.push_str(&format!("time: would execute '{}'\n", args.join(" ")));

    let elapsed = syscall::now() - start;

    // Format like Unix time command
    stdout.push_str(&format!(
        "\nreal    {:.3}s\nuser    {:.3}s\nsys     {:.3}s\n",
        elapsed / 1000.0,
        0.0, // In a real OS we'd track user time
        0.0  // In a real OS we'd track system time
    ));

    0
}

/// date - print current date and time
pub fn prog_date(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: date [+FORMAT]\nPrint current date and time.") {
        stdout.push_str(&help);
        return 0;
    }

    // Get current time from syscall
    let now_ms = syscall::now();

    // Convert to readable format (simplified - just show ms since start)
    // In a real OS we'd have proper time syscalls
    let secs = (now_ms / 1000.0) as u64;
    let hours = (secs / 3600) % 24;
    let mins = (secs / 60) % 60;
    let secs = secs % 60;

    // Simple format: show uptime as time
    stdout.push_str(&format!("{:02}:{:02}:{:02} UTC\n", hours, mins, secs));
    0
}

/// uptime - show how long system has been running
pub fn prog_uptime(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: uptime\nShow how long the system has been running.") {
        stdout.push_str(&help);
        return 0;
    }

    // Get trace summary for uptime info
    let summary = syscall::trace_summary();
    let uptime_ms = summary.uptime;

    let seconds = (uptime_ms / 1000.0) as u64;
    let minutes = seconds / 60;
    let hours = minutes / 60;
    let days = hours / 24;

    let secs = seconds % 60;
    let mins = minutes % 60;
    let hrs = hours % 24;

    stdout.push_str("up ");
    if days > 0 {
        stdout.push_str(&format!("{} day{}, ", days, if days > 1 { "s" } else { "" }));
    }
    if hours > 0 || days > 0 {
        stdout.push_str(&format!("{}:{:02}, ", hrs, mins));
    } else {
        stdout.push_str(&format!("{} min, ", mins));
    }
    stdout.push_str(&format!("{} sec\n", secs));

    // Show system stats
    stdout.push_str(&format!("syscalls: {}, ", summary.syscall_count));
    stdout.push_str(&format!("processes: {}/{}\n", summary.processes_spawned, summary.processes_exited));

    0
}

/// free - display amount of free and used memory
pub fn prog_free(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);
    let human = args.iter().any(|a| *a == "-h" || *a == "--human");

    if let Some(help) = check_help(&args, "Usage: free [-h]\nDisplay memory usage.\n  -h  Human readable output") {
        stdout.push_str(&help);
        return 0;
    }

    let stats = syscall::system_memstats().unwrap_or_default();

    fn format_size(bytes: usize, human: bool) -> String {
        if !human {
            return format!("{:>12}", bytes);
        }
        if bytes >= 1024 * 1024 * 1024 {
            format!("{:>8.1}G", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        } else if bytes >= 1024 * 1024 {
            format!("{:>8.1}M", bytes as f64 / (1024.0 * 1024.0))
        } else if bytes >= 1024 {
            format!("{:>8.1}K", bytes as f64 / 1024.0)
        } else {
            format!("{:>8}B", bytes)
        }
    }

    let total = stats.system_limit;
    let used = stats.total_allocated;
    let free = total.saturating_sub(used);
    let shared = stats.shm_total_size;

    stdout.push_str("              total        used        free      shared\n");

    stdout.push_str(&format!(
        "Mem:    {} {} {} {}\n",
        format_size(total, human),
        format_size(used, human),
        format_size(free, human),
        format_size(shared, human)
    ));

    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_whoami_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let exit_code = prog_whoami(&args, "", &mut stdout, &mut stderr);

        assert_eq!(exit_code, 0);
        assert!(stdout.contains("whoami"));
        assert!(stdout.contains("username"));
    }

    #[test]
    fn test_hostname_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let exit_code = prog_hostname(&args, "", &mut stdout, &mut stderr);

        assert_eq!(exit_code, 0);
        assert!(stdout.contains("hostname"));
    }

    #[test]
    fn test_uname_default() {
        let args = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let exit_code = prog_uname(&args, "", &mut stdout, &mut stderr);

        assert_eq!(exit_code, 0);
        assert!(stdout.contains("axeberg"));
    }

    #[test]
    fn test_uname_all() {
        let args = vec!["-a".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let exit_code = prog_uname(&args, "", &mut stdout, &mut stderr);

        assert_eq!(exit_code, 0);
        assert!(stdout.contains("axeberg"));
        assert!(stdout.contains("wasm32"));
    }

    #[test]
    fn test_ps_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let exit_code = prog_ps(&args, "", &mut stdout, &mut stderr);

        assert_eq!(exit_code, 0);
        assert!(stdout.contains("ps"));
        assert!(stdout.contains("process"));
    }

    #[test]
    fn test_date_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let exit_code = prog_date(&args, "", &mut stdout, &mut stderr);

        assert_eq!(exit_code, 0);
        assert!(stdout.contains("date"));
    }

    #[test]
    fn test_time_no_args() {
        let args = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let exit_code = prog_time(&args, "", &mut stdout, &mut stderr);

        assert_eq!(exit_code, 1);
        assert!(stderr.contains("missing command"));
    }

    #[test]
    fn test_uptime_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let exit_code = prog_uptime(&args, "", &mut stdout, &mut stderr);

        assert_eq!(exit_code, 0);
        assert!(stdout.contains("uptime"));
    }

    #[test]
    fn test_free_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let exit_code = prog_free(&args, "", &mut stdout, &mut stderr);

        assert_eq!(exit_code, 0);
        assert!(stdout.contains("free"));
        assert!(stdout.contains("memory"));
    }

    #[test]
    fn test_id_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let exit_code = prog_id(&args, "", &mut stdout, &mut stderr);

        assert_eq!(exit_code, 0);
        assert!(stdout.contains("id"));
    }

    #[test]
    fn test_groups_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let exit_code = prog_groups(&args, "", &mut stdout, &mut stderr);

        assert_eq!(exit_code, 0);
        assert!(stdout.contains("groups"));
    }
}
