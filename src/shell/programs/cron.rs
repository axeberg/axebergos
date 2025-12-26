//! Cron and scheduling programs

use super::{args_to_strs, check_help};
use crate::kernel::syscall;

/// crontab - maintain cron tables for scheduled jobs
pub fn prog_crontab(
    args: &[String],
    __stdin: &str,
    stdout: &mut String,
    stderr: &mut String,
) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(
        &args,
        "Usage: crontab [-l | -e | -r] [file]\n\nMaintain cron tables for scheduled jobs.\n\nOptions:\n  -l        List current crontab\n  -e        Edit crontab (prints current, use crontab file to set)\n  -r        Remove crontab\n  file      Install crontab from file\n\nCrontab format:\n  minute hour day month weekday command\n  @reboot  Run at startup\n  @hourly  Run every hour (0 * * * *)\n  @daily   Run daily (0 0 * * *)\n\nExamples:\n  */5 * * * * echo 'every 5 min'    Run every 5 minutes\n  0 * * * * date                    Run at the top of every hour\n  @reboot /var/packages/startup     Run at boot",
    ) {
        stdout.push_str(&help);
        return 0;
    }

    // Ensure cron directories exist
    let _ = syscall::mkdir("/var");
    let _ = syscall::mkdir("/var/spool");
    let _ = syscall::mkdir("/var/spool/cron");

    // Get current username
    let username = syscall::KERNEL.with(|k| {
        let kernel = k.borrow();
        let uid = kernel.current_process().map(|p| p.uid.0).unwrap_or(1000);
        kernel
            .users()
            .get_user(crate::kernel::users::Uid(uid))
            .map(|u| u.name.clone())
            .unwrap_or_else(|| "user".to_string())
    });

    let crontab_path = format!("/var/spool/cron/{}", username);

    if args.is_empty() || args[0] == "-l" {
        // List crontab
        match syscall::open(&crontab_path, syscall::OpenFlags::READ) {
            Ok(fd) => {
                let mut buf = vec![0u8; 65536];
                match syscall::read(fd, &mut buf) {
                    Ok(n) => {
                        let _ = syscall::close(fd);
                        let content = String::from_utf8_lossy(&buf[..n]);
                        if content.trim().is_empty() {
                            stdout.push_str("no crontab for ");
                            stdout.push_str(&username);
                            stdout.push('\n');
                        } else {
                            stdout.push_str(&content);
                        }
                    }
                    Err(_) => {
                        let _ = syscall::close(fd);
                        stdout.push_str("no crontab for ");
                        stdout.push_str(&username);
                        stdout.push('\n');
                    }
                }
            }
            Err(_) => {
                stdout.push_str("no crontab for ");
                stdout.push_str(&username);
                stdout.push('\n');
            }
        }
        return 0;
    }

    match args[0] {
        "-e" => {
            // Print current crontab for manual editing
            stdout.push_str("# Edit your crontab below, then save with:\n");
            stdout.push_str("#   echo 'your crontab' | crontab -\n");
            stdout.push_str("# or: crontab /path/to/crontab/file\n");
            stdout.push_str("#\n");
            stdout.push_str("# Format: minute hour day month weekday command\n");
            stdout.push_str("#\n");

            // Show existing entries
            if let Ok(fd) = syscall::open(&crontab_path, syscall::OpenFlags::READ) {
                let mut buf = vec![0u8; 65536];
                if let Ok(n) = syscall::read(fd, &mut buf) {
                    let content = String::from_utf8_lossy(&buf[..n]);
                    stdout.push_str(&content);
                }
                let _ = syscall::close(fd);
            }
            0
        }
        "-r" => {
            // Remove crontab
            match syscall::remove_file(&crontab_path) {
                Ok(()) => {
                    stdout.push_str(&format!("crontab removed for {}\n", username));
                    0
                }
                Err(_) => {
                    stderr.push_str(&format!("no crontab for {}\n", username));
                    1
                }
            }
        }
        "-" => {
            // Read from stdin (the rest of the args after -)
            stderr.push_str(
                "crontab: use 'crontab <file>' or 'echo ... > /var/spool/cron/username'\n",
            );
            1
        }
        file => {
            // Install crontab from file
            match syscall::open(file, syscall::OpenFlags::READ) {
                Ok(fd) => {
                    let mut buf = vec![0u8; 65536];
                    match syscall::read(fd, &mut buf) {
                        Ok(n) => {
                            let _ = syscall::close(fd);
                            let content = &buf[..n];

                            // Write to crontab
                            match syscall::open(&crontab_path, syscall::OpenFlags::WRITE) {
                                Ok(out_fd) => {
                                    let _ = syscall::write(out_fd, content);
                                    let _ = syscall::close(out_fd);

                                    // Parse and validate entries
                                    let text = String::from_utf8_lossy(content);
                                    let mut entry_count = 0;
                                    for line in text.lines() {
                                        let line = line.trim();
                                        if line.is_empty() || line.starts_with('#') {
                                            continue;
                                        }
                                        entry_count += 1;
                                    }

                                    stdout.push_str(&format!(
                                        "crontab: installed {} entries for {}\n",
                                        entry_count, username
                                    ));
                                    0
                                }
                                Err(e) => {
                                    stderr.push_str(&format!("crontab: cannot install: {:?}\n", e));
                                    1
                                }
                            }
                        }
                        Err(e) => {
                            let _ = syscall::close(fd);
                            stderr.push_str(&format!("crontab: cannot read '{}': {:?}\n", file, e));
                            1
                        }
                    }
                }
                Err(_) => {
                    // Maybe it's inline content
                    let content = args.join(" ");

                    match syscall::open(&crontab_path, syscall::OpenFlags::WRITE) {
                        Ok(out_fd) => {
                            let _ = syscall::write(out_fd, content.as_bytes());
                            let _ = syscall::close(out_fd);
                            stdout.push_str(&format!("crontab: installed for {}\n", username));
                            0
                        }
                        Err(e) => {
                            stderr.push_str(&format!("crontab: cannot install: {:?}\n", e));
                            1
                        }
                    }
                }
            }
        }
    }
}

/// at - schedule a one-time job
pub fn prog_at(args: &[String], __stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(
        &args,
        "Usage: at <time> <command>\n       at -l         List pending jobs\n       at -r <id>    Remove a job\n\nSchedule a command to run at a specific time.\n\nTime formats:\n  +5m    5 minutes from now\n  +1h    1 hour from now\n  +30s   30 seconds from now\n\nExamples:\n  at +5m echo 'Hello'     Run in 5 minutes\n  at +1h date             Run in 1 hour",
    ) {
        stdout.push_str(&help);
        return 0;
    }

    // Ensure at spool directory exists
    let _ = syscall::mkdir("/var");
    let _ = syscall::mkdir("/var/spool");
    let _ = syscall::mkdir("/var/spool/at");

    if args.is_empty() {
        stderr.push_str("at: missing time specification\nTry 'at --help' for usage.\n");
        return 1;
    }

    match args[0] {
        "-l" | "list" => {
            // List pending jobs
            match syscall::readdir("/var/spool/at") {
                Ok(entries) => {
                    if entries.is_empty() {
                        stdout.push_str("No pending jobs.\n");
                    } else {
                        stdout.push_str("ID       SCHEDULED           COMMAND\n");
                        for entry in entries {
                            let job_path = format!("/var/spool/at/{}", entry);
                            if let Ok(fd) = syscall::open(&job_path, syscall::OpenFlags::READ) {
                                let mut buf = vec![0u8; 1024];
                                if let Ok(n) = syscall::read(fd, &mut buf) {
                                    let content = String::from_utf8_lossy(&buf[..n]);
                                    let lines: Vec<&str> = content.lines().collect();
                                    if lines.len() >= 2 {
                                        let time_str = lines[0];
                                        let command = lines[1];
                                        stdout.push_str(&format!(
                                            "{:<8} {:<19} {}\n",
                                            entry,
                                            time_str,
                                            command.chars().take(40).collect::<String>()
                                        ));
                                    }
                                }
                                let _ = syscall::close(fd);
                            }
                        }
                    }
                    0
                }
                Err(_) => {
                    stdout.push_str("No pending jobs.\n");
                    0
                }
            }
        }
        "-r" | "-d" | "remove" => {
            if args.len() < 2 {
                stderr.push_str("at: missing job ID\n");
                return 1;
            }
            let job_id = args[1];
            let job_path = format!("/var/spool/at/{}", job_id);

            match syscall::remove_file(&job_path) {
                Ok(()) => {
                    stdout.push_str(&format!("Job {} removed.\n", job_id));
                    0
                }
                Err(_) => {
                    stderr.push_str(&format!("at: job '{}' not found\n", job_id));
                    1
                }
            }
        }
        time_spec => {
            if args.len() < 2 {
                stderr.push_str("at: missing command\n");
                return 1;
            }

            // Parse time specification
            let delay_ms: u64 = if let Some(spec) = time_spec.strip_prefix('+') {
                if let Some(stripped) = spec.strip_suffix('s') {
                    stripped.parse::<u64>().unwrap_or(0) * 1000
                } else if let Some(stripped) = spec.strip_suffix('m') {
                    stripped.parse::<u64>().unwrap_or(0) * 60 * 1000
                } else if let Some(stripped) = spec.strip_suffix('h') {
                    stripped.parse::<u64>().unwrap_or(0) * 60 * 60 * 1000
                } else {
                    spec.parse::<u64>().unwrap_or(0) * 1000 // default to seconds
                }
            } else {
                stderr.push_str("at: invalid time format (use +5m, +1h, +30s)\n");
                return 1;
            };

            if delay_ms == 0 {
                stderr.push_str("at: invalid time specification\n");
                return 1;
            }

            let command = args[1..].join(" ");

            // Generate job ID
            let now = syscall::now() as u64;
            let scheduled = now + delay_ms;
            let job_id = format!("{}", now % 100000);

            // Create job file
            let job_path = format!("/var/spool/at/{}", job_id);
            let job_content = format!("{}\n{}\n", scheduled, command);

            match syscall::open(&job_path, syscall::OpenFlags::WRITE) {
                Ok(fd) => {
                    let _ = syscall::write(fd, job_content.as_bytes());
                    let _ = syscall::close(fd);

                    stdout.push_str(&format!(
                        "Job {} scheduled to run in {}\n",
                        job_id, time_spec
                    ));
                    stdout.push_str(&format!("Command: {}\n", command));
                    0
                }
                Err(e) => {
                    stderr.push_str(&format!("at: failed to schedule job: {:?}\n", e));
                    1
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crontab_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_crontab(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: crontab"));
        assert!(stdout.contains("Maintain cron tables"));
    }

    #[test]
    fn test_crontab_list_no_crontab() {
        let args = vec!["-l".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_crontab(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("no crontab for"));
    }

    #[test]
    fn test_crontab_edit_format() {
        let args = vec!["-e".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_crontab(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("# Edit your crontab"));
        assert!(stdout.contains("# Format: minute hour day month weekday command"));
    }

    #[test]
    fn test_at_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_at(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: at"));
        assert!(stdout.contains("Schedule a command"));
    }

    #[test]
    fn test_at_missing_time() {
        let args: Vec<String> = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_at(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("missing time specification"));
    }

    #[test]
    fn test_at_missing_command() {
        let args = vec!["+5m".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_at(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("missing command"));
    }

    #[test]
    fn test_at_invalid_time_format() {
        let args = vec!["5m".to_string(), "echo".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_at(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("invalid time format"));
    }

    #[test]
    fn test_at_list_no_jobs() {
        let args = vec!["-l".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_at(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("No pending jobs"));
    }

    #[test]
    fn test_at_remove_missing_id() {
        let args = vec!["-r".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_at(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("missing job ID"));
    }
}
