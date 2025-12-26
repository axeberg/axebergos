//! Process control programs

use super::{args_to_strs, check_help};
use crate::kernel::syscall;

/// sleep - pause for specified seconds
pub fn prog_sleep(args: &[String], __stdin: &str, _stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if args.is_empty() {
        stderr.push_str("sleep: missing operand\n");
        return 1;
    }

    let seconds: f64 = match args[0].parse() {
        Ok(n) => n,
        Err(_) => {
            stderr.push_str(&format!("sleep: invalid time interval '{}'\n", args[0]));
            return 1;
        }
    };

    // In WASM we can't actually block, but we can note the intent
    // For now, just return immediately with a message
    // A proper implementation would use setTimeout via JS interop
    #[cfg(target_arch = "wasm32")]
    {
        // Can't block in WASM - would need async support
        crate::console_log!("[sleep] Would sleep for {} seconds (non-blocking in WASM)", seconds);
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        std::thread::sleep(std::time::Duration::from_secs_f64(seconds));
    }

    0
}

/// jobs - list background jobs
pub fn prog_jobs(args: &[String], __stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: jobs [-l]\nList background jobs.") {
        stdout.push_str(&help);
        return 0;
    }

    let long_format = args.contains(&"-l");

    // Get list of processes from kernel
    let processes = syscall::list_processes();

    // Filter to show only background/stopped jobs (not the shell itself)
    let mut job_num = 0;
    for (pid, name, state) in processes {
        // Skip the shell process (typically pid 1)
        if pid.0 == 1 {
            continue;
        }

        let state_str = match &state {
            syscall::ProcessState::Running => "Running",
            syscall::ProcessState::Stopped => "Stopped",
            syscall::ProcessState::Sleeping => "Sleeping",
            syscall::ProcessState::Blocked(_) => "Blocked",
            syscall::ProcessState::Zombie(code) => {
                stdout.push_str(&format!("[{}]  Done({})\t\t{}\n", job_num + 1, code, name));
                job_num += 1;
                continue;
            }
        };

        job_num += 1;
        if long_format {
            stdout.push_str(&format!("[{}]  {} {}\t\t{}\n", job_num, pid.0, state_str, name));
        } else {
            stdout.push_str(&format!("[{}]  {}\t\t{}\n", job_num, state_str, name));
        }
    }

    if job_num == 0 {
        // No jobs - that's fine, just return success
    }

    0
}

/// fg - bring job to foreground
pub fn prog_fg(args: &[String], __stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: fg [%JOB]\nBring job to foreground.") {
        stdout.push_str(&help);
        return 0;
    }

    // Parse job specification
    let job_spec = if args.is_empty() {
        None // Use current job
    } else {
        let spec = args[0];
        if spec.starts_with('%') {
            spec.trim_start_matches('%').parse::<u32>().ok()
        } else {
            spec.parse::<u32>().ok()
        }
    };

    // Get processes and find the matching job
    let processes = syscall::list_processes();
    let jobs: Vec<_> = processes.into_iter()
        .filter(|(pid, _, _)| pid.0 != 1) // Skip shell
        .collect();

    if jobs.is_empty() {
        stderr.push_str("fg: no current job\n");
        return 1;
    }

    let target = match job_spec {
        Some(n) if n > 0 && (n as usize) <= jobs.len() => {
            jobs.get((n - 1) as usize)
        }
        None => jobs.last(), // Default to most recent
        _ => {
            stderr.push_str("fg: no such job\n");
            return 1;
        }
    };

    if let Some((pid, name, state)) = target {
        // If stopped, send SIGCONT
        if matches!(state, syscall::ProcessState::Stopped)
            && let Err(e) = syscall::kill(*pid, crate::kernel::signal::Signal::SIGCONT) {
                stderr.push_str(&format!("fg: {}\n", e));
                return 1;
            }
        stdout.push_str(&format!("{}\n", name));
        0
    } else {
        stderr.push_str("fg: no such job\n");
        1
    }
}

/// bg - continue job in background
pub fn prog_bg(args: &[String], __stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: bg [%JOB]\nContinue job in background.") {
        stdout.push_str(&help);
        return 0;
    }

    // Parse job specification (same as fg)
    let job_spec = if args.is_empty() {
        None
    } else {
        let spec = args[0];
        if spec.starts_with('%') {
            spec.trim_start_matches('%').parse::<u32>().ok()
        } else {
            spec.parse::<u32>().ok()
        }
    };

    let processes = syscall::list_processes();
    let stopped_jobs: Vec<_> = processes.into_iter()
        .filter(|(pid, _, state)| {
            pid.0 != 1 && matches!(state, syscall::ProcessState::Stopped)
        })
        .collect();

    if stopped_jobs.is_empty() {
        stderr.push_str("bg: no stopped jobs\n");
        return 1;
    }

    let target = match job_spec {
        Some(n) if n > 0 && (n as usize) <= stopped_jobs.len() => {
            stopped_jobs.get((n - 1) as usize)
        }
        None => stopped_jobs.last(),
        _ => {
            stderr.push_str("bg: no such job\n");
            return 1;
        }
    };

    if let Some((pid, name, _)) = target {
        if let Err(e) = syscall::kill(*pid, crate::kernel::signal::Signal::SIGCONT) {
            stderr.push_str(&format!("bg: {}\n", e));
            return 1;
        }
        stdout.push_str(&format!("[1] {} &\n", name));
        0
    } else {
        stderr.push_str("bg: no such job\n");
        1
    }
}

/// strace - trace system calls
pub fn prog_strace(args: &[String], __stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: strace [-c] COMMAND [ARGS...]\nTrace system calls.") {
        stdout.push_str(&help);
        return 0;
    }

    if args.is_empty() {
        stderr.push_str("strace: must have COMMAND to run\n");
        return 1;
    }

    let count_mode = args.contains(&"-c");
    let cmd_args: Vec<_> = args.iter()
        .filter(|a| !a.starts_with('-')).copied()
        .collect();

    if cmd_args.is_empty() {
        stderr.push_str("strace: must have COMMAND to run\n");
        return 1;
    }

    // Enable tracing
    syscall::trace_enable();
    syscall::trace_reset();

    // Run the command (we'd need to actually execute it here)
    // For now, just show the trace summary
    stdout.push_str(&format!("strace: would trace '{}'\n", cmd_args.join(" ")));

    // Get trace summary
    let summary = syscall::trace_summary();

    if count_mode {
        stdout.push_str(&format!(
            "% time     seconds  usecs/call     calls  syscall\n\
             ------ ----------- ----------- --------- --------\n\
             100.00    {:>8.6}           0  {:>8}  total\n",
            summary.uptime / 1000.0,
            summary.syscall_count
        ));
    } else {
        stdout.push_str(&format!(
            "--- tracing enabled for {:.3}ms ---\n\
             syscalls: {}\n\
             events: {}\n",
            summary.uptime,
            summary.syscall_count,
            summary.event_count
        ));
    }

    // Disable tracing
    syscall::trace_disable();

    0
}

/// kill - send signal to process
pub fn prog_kill(args: &[String], __stdin: &str, _stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: kill [-s SIGNAL] PID...\nSend signal to processes.") {
        stderr.push_str(&help);
        return 0;
    }

    // Parse signal
    let mut signal = crate::kernel::signal::Signal::SIGTERM;
    let mut pids: Vec<u32> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let arg = args[i];
        if arg == "-s" && i + 1 < args.len() {
            signal = match args[i + 1].to_uppercase().as_str() {
                "TERM" | "SIGTERM" | "15" => crate::kernel::signal::Signal::SIGTERM,
                "KILL" | "SIGKILL" | "9" => crate::kernel::signal::Signal::SIGKILL,
                "STOP" | "SIGSTOP" | "19" => crate::kernel::signal::Signal::SIGSTOP,
                "CONT" | "SIGCONT" | "18" => crate::kernel::signal::Signal::SIGCONT,
                "INT" | "SIGINT" | "2" => crate::kernel::signal::Signal::SIGINT,
                "HUP" | "SIGHUP" | "1" => crate::kernel::signal::Signal::SIGHUP,
                "USR1" | "SIGUSR1" | "10" => crate::kernel::signal::Signal::SIGUSR1,
                "USR2" | "SIGUSR2" | "12" => crate::kernel::signal::Signal::SIGUSR2,
                s => {
                    stderr.push_str(&format!("kill: invalid signal: {}\n", s));
                    return 1;
                }
            };
            i += 2;
        } else if arg.starts_with('-') && arg.len() > 1 {
            // -9, -KILL, etc.
            let sig_str = &arg[1..];
            signal = match sig_str.to_uppercase().as_str() {
                "TERM" | "SIGTERM" | "15" => crate::kernel::signal::Signal::SIGTERM,
                "KILL" | "SIGKILL" | "9" => crate::kernel::signal::Signal::SIGKILL,
                "STOP" | "SIGSTOP" | "19" => crate::kernel::signal::Signal::SIGSTOP,
                "CONT" | "SIGCONT" | "18" => crate::kernel::signal::Signal::SIGCONT,
                "INT" | "SIGINT" | "2" => crate::kernel::signal::Signal::SIGINT,
                "HUP" | "SIGHUP" | "1" => crate::kernel::signal::Signal::SIGHUP,
                s => {
                    stderr.push_str(&format!("kill: invalid signal: {}\n", s));
                    return 1;
                }
            };
            i += 1;
        } else if let Ok(pid) = arg.parse::<u32>() {
            pids.push(pid);
            i += 1;
        } else {
            stderr.push_str(&format!("kill: invalid pid: {}\n", arg));
            return 1;
        }
    }

    if pids.is_empty() {
        stderr.push_str("kill: missing pid\n");
        return 1;
    }

    let mut exit_code = 0;
    for pid in pids {
        if let Err(e) = syscall::kill(syscall::Pid(pid), signal) {
            stderr.push_str(&format!("kill: ({}) - {}\n", pid, e));
            exit_code = 1;
        }
    }

    exit_code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sleep_missing_operand() {
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_sleep(&[], "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("missing operand"));
    }

    #[test]
    fn test_sleep_invalid_time() {
        let args = vec!["abc".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_sleep(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("invalid time interval"));
    }

    #[test]
    fn test_jobs_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_jobs(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: jobs"));
    }

    #[test]
    fn test_fg_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_fg(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: fg"));
    }

    #[test]
    fn test_bg_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_bg(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: bg"));
    }

    #[test]
    fn test_strace_missing_command() {
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_strace(&[], "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("must have COMMAND"));
    }

    #[test]
    fn test_strace_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_strace(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: strace"));
    }

    #[test]
    fn test_kill_missing_pid() {
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_kill(&[], "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("missing pid"));
    }

    #[test]
    fn test_kill_invalid_pid() {
        let args = vec!["abc".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_kill(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("invalid pid"));
    }

    #[test]
    fn test_kill_invalid_signal() {
        let args = vec!["-s".to_string(), "INVALID".to_string(), "1".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_kill(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("invalid signal"));
    }

    #[test]
    fn test_kill_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_kill(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stderr.contains("Usage: kill"));
    }
}
