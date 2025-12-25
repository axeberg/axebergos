//! User management programs

use super::{args_to_strs, check_help};
use crate::kernel::syscall;

/// su - switch user (simulated)
pub fn prog_su(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: su [-] [USER]\nSwitch user. Defaults to root.") {
        stdout.push_str(&help);
        return 0;
    }

    // Parse arguments
    let mut login_shell = false;
    let mut target_user = "root";

    for arg in args {
        if arg == "-" || arg == "-l" || arg == "--login" {
            login_shell = true;
        } else if !arg.starts_with('-') {
            target_user = arg;
        }
    }

    // Look up target user
    let user = match syscall::get_user_by_name(target_user) {
        Some(u) => u,
        None => {
            stderr.push_str(&format!("su: user '{}' does not exist\n", target_user));
            return 1;
        }
    };

    // Check if we have permission (root can su to anyone, others need wheel group or password)
    let euid = syscall::geteuid().unwrap_or_default();
    if euid.0 != 0 {
        // Non-root user - would need password in real system
        // For demo, check if user is in wheel group
        let groups = syscall::getgroups().unwrap_or_default();
        let in_wheel = groups.iter().any(|g| g.0 == 10); // wheel is gid 10

        if !in_wheel && target_user == "root" {
            stderr.push_str("su: authentication required (user not in wheel group)\n");
            return 1;
        }
    }

    // Set the user and group IDs
    if let Err(e) = syscall::setuid(user.uid) {
        stderr.push_str(&format!("su: failed to set uid: {}\n", e));
        return 1;
    }

    if let Err(e) = syscall::setgid(user.gid) {
        stderr.push_str(&format!("su: failed to set gid: {}\n", e));
        return 1;
    }

    // Update environment
    let _ = syscall::setenv("USER", &user.name);
    let _ = syscall::setenv("HOME", &user.home);
    if login_shell {
        let _ = syscall::setenv("SHELL", &user.shell);
    }

    stdout.push_str(&format!("Switched to user '{}'\n", user.name));
    0
}

/// sudo - run command as root (simulated)
pub fn prog_sudo(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if args.is_empty() || args.first().map(|s| s.as_ref()) == Some("--help") {
        stdout.push_str("Usage: sudo COMMAND [ARG]...\nRun command as root.\n");
        return 0;
    }

    // Check if user is in wheel group (sudoers)
    let euid = syscall::geteuid().unwrap_or_default();
    if euid.0 != 0 {
        let groups = syscall::getgroups().unwrap_or_default();
        let in_wheel = groups.iter().any(|g| g.0 == 10);

        if !in_wheel {
            stderr.push_str("sudo: user is not in sudoers (wheel group)\n");
            return 1;
        }
    }

    // Temporarily become root
    let old_euid = euid;
    let old_egid = syscall::getegid().unwrap_or_default();

    if let Err(e) = syscall::seteuid(crate::kernel::Uid::ROOT) {
        stderr.push_str(&format!("sudo: failed to elevate: {}\n", e));
        return 1;
    }
    if let Err(e) = syscall::setegid(crate::kernel::Gid::ROOT) {
        stderr.push_str(&format!("sudo: failed to elevate gid: {}\n", e));
        let _ = syscall::seteuid(old_euid);
        return 1;
    }

    // The actual command would be executed by the shell in a real implementation
    // For now, just print that we're running as root
    stdout.push_str(&format!("[sudo] Running as root: {}\n", args.join(" ")));

    // Restore original effective uid/gid
    let _ = syscall::seteuid(old_euid);
    let _ = syscall::setegid(old_egid);

    0
}

/// useradd - create a new user
pub fn prog_useradd(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if args.is_empty() || args.first().map(|s| s.as_ref()) == Some("--help") {
        stdout.push_str("Usage: useradd [-g GID] USERNAME\nCreate a new user.\n");
        return 0;
    }

    // Check if caller is root
    let euid = syscall::geteuid().unwrap_or_default();
    if euid.0 != 0 {
        stderr.push_str("useradd: permission denied (must be root)\n");
        return 1;
    }

    // Parse arguments
    let mut gid: Option<crate::kernel::Gid> = None;
    let mut username = None;
    let mut iter = args.iter();

    while let Some(arg) = iter.next() {
        if *arg == "-g" {
            if let Some(gid_str) = iter.next() {
                if let Ok(n) = gid_str.parse::<u32>() {
                    gid = Some(crate::kernel::Gid(n));
                } else if let Some(group) = syscall::get_group_by_name(gid_str) {
                    gid = Some(group.gid);
                } else {
                    stderr.push_str(&format!("useradd: group '{}' does not exist\n", gid_str));
                    return 1;
                }
            }
        } else if !arg.starts_with('-') {
            username = Some(*arg);
        }
    }

    let username = match username {
        Some(u) => u,
        None => {
            stderr.push_str("useradd: missing username\n");
            return 1;
        }
    };

    // Check if user already exists
    if syscall::get_user_by_name(username).is_some() {
        stderr.push_str(&format!("useradd: user '{}' already exists\n", username));
        return 1;
    }

    // Create the user
    match syscall::add_user(username, gid) {
        Ok(uid) => {
            // Create home directory
            let home = format!("/home/{}", username);
            let _ = syscall::mkdir(&home);

            // Save updated user database to /etc/passwd, /etc/shadow, /etc/group
            syscall::save_user_db();

            stdout.push_str(&format!("Created user '{}' with uid={}\n", username, uid.0));
            0
        }
        Err(e) => {
            stderr.push_str(&format!("useradd: {}\n", e));
            1
        }
    }
}

/// groupadd - create a new group
pub fn prog_groupadd(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if args.is_empty() || args.first().map(|s| s.as_ref()) == Some("--help") {
        stdout.push_str("Usage: groupadd GROUPNAME\nCreate a new group.\n");
        return 0;
    }

    // Check if caller is root
    let euid = syscall::geteuid().unwrap_or_default();
    if euid.0 != 0 {
        stderr.push_str("groupadd: permission denied (must be root)\n");
        return 1;
    }

    let groupname = &args[0];

    // Check if group already exists
    if syscall::get_group_by_name(groupname).is_some() {
        stderr.push_str(&format!("groupadd: group '{}' already exists\n", groupname));
        return 1;
    }

    // Create the group
    match syscall::add_group(groupname) {
        Ok(gid) => {
            // Save updated user database to /etc/group
            syscall::save_user_db();
            stdout.push_str(&format!("Created group '{}' with gid={}\n", groupname, gid.0));
            0
        }
        Err(e) => {
            stderr.push_str(&format!("groupadd: {}\n", e));
            1
        }
    }
}

/// passwd - change password
pub fn prog_passwd(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: passwd [USER] [PASSWORD]\n\nChange user password.\n\nExamples:\n  passwd mypassword          Set your own password\n  passwd root newpass        Set root's password (requires root)\n  passwd user                Clear user's password (requires root)") {
        stdout.push_str(&help);
        return 0;
    }

    // Determine target user and new password
    let euid = syscall::geteuid().unwrap_or_default();

    let (target, new_password) = if args.is_empty() {
        stderr.push_str("passwd: usage: passwd [USER] <PASSWORD>\n");
        return 1;
    } else if args.len() == 1 {
        // Single arg: could be password for self, or username to clear password (if root)
        let current_user = syscall::get_user_by_uid(euid)
            .map(|u| u.name.clone())
            .unwrap_or_else(|| "user".to_string());

        // If argument looks like a username that exists, treat it as clearing password
        if euid.0 == 0 && syscall::get_user_by_name(&args[0]).is_some() {
            (args[0].to_string(), None)
        } else {
            // Treat as password for current user
            (current_user, Some(args[0].to_string()))
        }
    } else {
        // Two or more args: first is username, rest is password
        let username = args[0].to_string();
        let password = args[1..].join(" ");

        // Check permission
        if euid.0 != 0 {
            let current_user = syscall::get_user_by_uid(euid)
                .map(|u| u.name.clone())
                .unwrap_or_else(|| "".to_string());
            if username != current_user {
                stderr.push_str("passwd: permission denied (must be root to change other users' passwords)\n");
                return 1;
            }
        }
        (username, if password.is_empty() { None } else { Some(password) })
    };

    if syscall::get_user_by_name(&target).is_none() {
        stderr.push_str(&format!("passwd: user '{}' does not exist\n", target));
        return 1;
    }

    // Set the password
    let result = syscall::KERNEL.with(|k| {
        let mut kernel = k.borrow_mut();
        if let Some(user) = kernel.users_mut().get_user_by_name_mut(&target) {
            match new_password {
                Some(pwd) => {
                    user.set_password(&pwd);
                    Ok(format!("Password set for '{}'\n", target))
                }
                None => {
                    user.password_hash = None;
                    Ok(format!("Password cleared for '{}'\n", target))
                }
            }
        } else {
            Err(format!("User '{}' not found\n", target))
        }
    });

    match result {
        Ok(msg) => {
            // Save updated user database to /etc/passwd, /etc/shadow
            syscall::save_user_db();
            stdout.push_str(&msg);
            0
        }
        Err(msg) => {
            stderr.push_str(&msg);
            1
        }
    }
}

/// login - log in as a user with password authentication
/// This behaves like real Linux login(1): it spawns a NEW shell process
/// as the target user with proper session management.
pub fn prog_login(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: login <username> [password]\n\nLog in as a user with password authentication.\n\nThis command spawns a new login shell as the specified user,\ncreating a proper session like Linux login(1).\n\nIf no password is provided, allows login for users without passwords.\nUse 'logout' to end the current session.\nUse 'passwd' to change your password.\n\nDefault users:\n  root     - password: root (uid 0)\n  user     - no password (uid 1000)\n  nobody   - no password (uid 65534)") {
        stdout.push_str(&help);
        return 0;
    }

    if args.is_empty() {
        stderr.push_str("login: usage: login <username> [password]\n");
        return 1;
    }

    // Ensure session directory exists
    let _ = syscall::mkdir("/var");
    let _ = syscall::mkdir("/var/run");

    let username = args[0].to_string();
    let password = if args.len() > 1 { Some(args[1..].join(" ")) } else { None };

    // Verify user exists and check password
    let auth_result = syscall::KERNEL.with(|k| {
        let kernel = k.borrow();
        if let Some(user) = kernel.users().get_user_by_name(&username) {
            // Check password
            match (&user.password_hash, &password) {
                (None, _) => {
                    // No password set - allow login
                    Ok((user.uid.0, user.gid.0, user.home.clone(), user.shell.clone()))
                }
                (Some(_), None) => {
                    // Password required but not provided
                    Err("Password required".to_string())
                }
                (Some(_), Some(pwd)) => {
                    // Verify password
                    if user.check_password(pwd) {
                        Ok((user.uid.0, user.gid.0, user.home.clone(), user.shell.clone()))
                    } else {
                        Err("Authentication failed".to_string())
                    }
                }
            }
        } else {
            Err(format!("Unknown user '{}'", username))
        }
    });

    let (uid, gid, home, shell) = match auth_result {
        Ok(info) => info,
        Err(msg) => {
            stderr.push_str(&format!("login: {}\n", msg));
            return 1;
        }
    };

    // Spawn a NEW login shell process with proper credentials
    // This is how real Linux login(1) works - it forks and execs a shell
    let new_pid = syscall::spawn_login_shell(&username, uid, gid, &home, &shell);

    // Switch to the new process (make it the current process)
    syscall::set_current_process(new_pid);

    // Change to user's home directory
    let _ = syscall::chdir(&home);

    // Record login session in utmp
    let session_file = "/var/run/utmp";
    let now = syscall::now();
    let session_data = format!("{}:{}:{}:{}:{}\n", username, uid, new_pid.0, now as u64, "tty1");

    // Write session file as root (temporarily)
    syscall::KERNEL.with(|k| {
        let mut kernel = k.borrow_mut();
        if let Some(proc) = kernel.current_process_mut() {
            let saved_euid = proc.euid;
            proc.euid = crate::kernel::users::Uid(0); // Temporarily become root
            drop(kernel); // Release borrow

            if let Ok(fd) = syscall::open(session_file, syscall::OpenFlags::WRITE) {
                let _ = syscall::write(fd, session_data.as_bytes());
                let _ = syscall::close(fd);
            }

            // Restore euid
            syscall::KERNEL.with(|k2| {
                if let Some(p) = k2.borrow_mut().current_process_mut() {
                    p.euid = saved_euid;
                }
            });
        }
    });

    // Get session info for display
    let (pid, sid, pgid, _, ctty) = syscall::get_session_info().unwrap_or((0, 0, 0, String::new(), String::new()));

    stdout.push_str(&format!("\nLogin successful: {}\n", username));
    stdout.push_str(&format!("  PID: {}, SID: {}, PGID: {}\n", pid, sid, pgid));
    stdout.push_str(&format!("  UID: {}, GID: {}\n", uid, gid));
    stdout.push_str(&format!("  Home: {}\n", home));
    stdout.push_str(&format!("  Shell: {}\n", shell));
    stdout.push_str(&format!("  TTY: {}\n", if ctty.is_empty() { "none" } else { &ctty }));
    stdout.push_str("\nType 'logout' to end this session.\n");

    0
}

/// logout - log out current user
/// In a real Linux system, this would exit the login shell and return to getty.
/// Here we terminate the current session and switch back to the init/parent process.
pub fn prog_logout(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: logout\n\nEnd the current login session and return to the parent process.\nThis terminates the login shell that was spawned by 'login'.") {
        stdout.push_str(&help);
        return 0;
    }

    // Get current session info before logging out
    let (current_pid, current_sid, username) = syscall::KERNEL.with(|k| {
        let kernel = k.borrow();
        let proc = kernel.current_process();
        let pid = proc.map(|p| p.pid.0).unwrap_or(0);
        let sid = proc.map(|p| p.sid.0).unwrap_or(0);
        let uid = proc.map(|p| p.uid.0).unwrap_or(1000);
        let user = kernel.users().get_user(crate::kernel::users::Uid(uid))
            .map(|u| u.name.clone())
            .unwrap_or_else(|| "unknown".to_string());
        (pid, sid, user)
    });

    // Clear the session file
    let _ = syscall::remove_file("/var/run/utmp");

    // Mark current process as a zombie and switch to parent or spawn new init
    let parent_pid = syscall::KERNEL.with(|k| {
        let mut kernel = k.borrow_mut();

        // Get parent PID before we modify anything
        let parent = kernel.current_process().and_then(|p| p.parent);

        // Mark current session process as zombie
        if let Some(proc) = kernel.current_process_mut() {
            proc.state = crate::kernel::process::ProcessState::Zombie(0);
        }

        parent
    });

    // If there's a parent process, switch to it; otherwise spawn new init
    if let Some(parent) = parent_pid {
        syscall::set_current_process(parent);
        stdout.push_str(&format!("Session {} ended for user '{}' (PID {})\n", current_sid, username, current_pid));
        stdout.push_str("Returned to parent process.\n");
    } else {
        // No parent - spawn a new shell as default user
        let new_pid = syscall::spawn_login_shell("user", 1000, 1000, "/home/user", "/bin/sh");
        syscall::set_current_process(new_pid);
        stdout.push_str(&format!("Session {} ended for user '{}'\n", current_sid, username));
        stdout.push_str("Started new session as 'user'.\n");
    }

    0
}

/// who - show who is logged in
pub fn prog_who(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: who\n\nShow who is logged in.") {
        stdout.push_str(&help);
        return 0;
    }

    // Read session file
    match syscall::open("/var/run/utmp", syscall::OpenFlags::READ) {
        Ok(fd) => {
            let mut buf = vec![0u8; 4096];
            match syscall::read(fd, &mut buf) {
                Ok(n) => {
                    let _ = syscall::close(fd);
                    let content = String::from_utf8_lossy(&buf[..n]);

                    stdout.push_str("USER     TTY        LOGIN@\n");
                    for line in content.lines() {
                        let parts: Vec<&str> = line.split(':').collect();
                        if parts.len() >= 3 {
                            let username = parts[0];
                            let login_time = parts[2].parse::<u64>().unwrap_or(0);
                            let secs = (login_time / 1000) as u64;
                            let hours = (secs / 3600) % 24;
                            let mins = (secs / 60) % 60;
                            stdout.push_str(&format!(
                                "{:<8} tty1       {:02}:{:02}\n",
                                username, hours, mins
                            ));
                        }
                    }
                }
                Err(_) => {
                    let _ = syscall::close(fd);
                    stdout.push_str("No users logged in.\n");
                }
            }
        }
        Err(_) => {
            // No session file, show current user from process
            let username = syscall::KERNEL.with(|k| {
                let kernel = k.borrow();
                let uid = kernel.current_process()
                    .map(|p| p.uid.0)
                    .unwrap_or(1000);
                kernel.users().get_user(crate::kernel::users::Uid(uid))
                    .map(|u| u.name.clone())
                    .unwrap_or_else(|| "user".to_string())
            });

            stdout.push_str("USER     TTY        LOGIN@\n");
            stdout.push_str(&format!("{:<8} tty1       00:00\n", username));
        }
    }

    0
}

/// w - show who is logged in and what they are doing
pub fn prog_w(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: w\n\nShow who is logged in and what they are doing.") {
        stdout.push_str(&help);
        return 0;
    }

    // Show current time
    let now_ms = syscall::now();
    let secs = (now_ms / 1000.0) as u64;
    let hours = (secs / 3600) % 24;
    let mins = (secs / 60) % 60;
    let secs_display = secs % 60;

    stdout.push_str(&format!(" {:02}:{:02}:{:02} up ", hours, mins, secs_display));

    // Uptime
    let uptime_hours = secs / 3600;
    let uptime_mins = (secs / 60) % 60;
    if uptime_hours > 0 {
        stdout.push_str(&format!("{}:{:02}", uptime_hours, uptime_mins));
    } else {
        stdout.push_str(&format!("{} min", uptime_mins));
    }

    stdout.push_str(",  1 user\n");
    stdout.push_str("USER     TTY      FROM             LOGIN@   IDLE   WHAT\n");

    // Get current user
    let username = syscall::KERNEL.with(|k| {
        let kernel = k.borrow();
        let uid = kernel.current_process()
            .map(|p| p.uid.0)
            .unwrap_or(1000);
        kernel.users().get_user(crate::kernel::users::Uid(uid))
            .map(|u| u.name.clone())
            .unwrap_or_else(|| "user".to_string())
    });

    stdout.push_str(&format!(
        "{:<8} tty1     -                {:02}:{:02}    0.00s  -sh\n",
        username, hours, mins
    ));

    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_su_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_su(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stdout.contains("Usage:"));
    }

    #[test]
    fn test_sudo_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_sudo(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stdout.contains("Usage:"));
    }

    #[test]
    fn test_sudo_no_args() {
        let args: Vec<String> = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_sudo(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stdout.contains("Usage:"));
    }

    #[test]
    fn test_useradd_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_useradd(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stdout.contains("Usage:"));
    }

    #[test]
    fn test_useradd_no_args() {
        let args: Vec<String> = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_useradd(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stdout.contains("Usage:"));
    }

    #[test]
    fn test_groupadd_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_groupadd(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stdout.contains("Usage:"));
    }

    #[test]
    fn test_groupadd_no_args() {
        let args: Vec<String> = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_groupadd(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stdout.contains("Usage:"));
    }

    #[test]
    fn test_passwd_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_passwd(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stdout.contains("Usage:"));
    }

    #[test]
    fn test_passwd_no_args() {
        let args: Vec<String> = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_passwd(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        assert!(stderr.contains("usage:"));
    }

    #[test]
    fn test_login_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_login(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stdout.contains("Usage:"));
    }

    #[test]
    fn test_login_no_args() {
        let args: Vec<String> = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_login(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        assert!(stderr.contains("usage:"));
    }

    #[test]
    fn test_logout_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_logout(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stdout.contains("Usage:"));
    }

    #[test]
    fn test_who_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_who(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stdout.contains("Usage:"));
    }

    #[test]
    fn test_w_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_w(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stdout.contains("Usage:"));
    }
}
