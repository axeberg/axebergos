//! System services programs

use super::{args_to_strs, check_help};
use crate::kernel::syscall;

/// systemctl - service management
pub fn prog_systemctl(
    args: &[String],
    __stdin: &str,
    stdout: &mut String,
    stderr: &mut String,
) -> i32 {
    let args = args_to_strs(args);

    if args.is_empty() {
        stdout.push_str("Usage: systemctl COMMAND [NAME...]\n\n");
        stdout.push_str("Commands:\n");
        stdout.push_str("  list-units      List all units\n");
        stdout.push_str("  status NAME     Show unit status\n");
        stdout.push_str("  start NAME      Start a unit\n");
        stdout.push_str("  stop NAME       Stop a unit\n");
        stdout.push_str("  restart NAME    Restart a unit\n");
        stdout.push_str("  enable NAME     Enable a unit\n");
        stdout.push_str("  disable NAME    Disable a unit\n");
        stdout.push_str("  get-default     Get default target\n");
        stdout.push_str("  set-default T   Set default target\n");
        return 0;
    }

    let cmd = args[0];
    match cmd {
        "list-units" | "list" => {
            use crate::kernel::init::ServiceState;

            stdout.push_str("UNIT                    STATE      DESCRIPTION\n");
            stdout.push_str("─────────────────────────────────────────────────────\n");

            syscall::KERNEL.with(|k| {
                let kernel = k.borrow();
                let services = kernel.init().list_services();
                for svc in services {
                    let state_str = match svc.state {
                        ServiceState::Running => "\x1b[32m●\x1b[0m running ",
                        ServiceState::Stopped => "\x1b[90m○\x1b[0m stopped ",
                        ServiceState::Starting => "\x1b[33m●\x1b[0m starting",
                        ServiceState::Stopping => "\x1b[33m●\x1b[0m stopping",
                        ServiceState::Failed => "\x1b[31m✗\x1b[0m failed  ",
                    };
                    stdout.push_str(&format!(
                        "{:<23} {} {}\n",
                        &svc.config.name, state_str, &svc.config.description
                    ));
                }
            });
            0
        }
        "status" => {
            if args.len() < 2 {
                stderr.push_str("systemctl: unit name required\n");
                return 1;
            }
            let name = &args[1];
            syscall::KERNEL.with(|k| {
                let kernel = k.borrow();
                if let Some(status) = kernel.init().service_status(name) {
                    use crate::kernel::init::ServiceState;
                    let state_sym = match status.state {
                        ServiceState::Running => "\x1b[32m●\x1b[0m",
                        ServiceState::Stopped => "\x1b[90m○\x1b[0m",
                        ServiceState::Starting => "\x1b[33m●\x1b[0m",
                        ServiceState::Stopping => "\x1b[33m●\x1b[0m",
                        ServiceState::Failed => "\x1b[31m✗\x1b[0m",
                    };
                    stdout.push_str(&format!("{} {}\n", state_sym, status.name));
                    stdout.push_str(&format!("     Description: {}\n", status.description));
                    if let Some(pid) = status.pid {
                        stdout.push_str(&format!("     Main PID: {}\n", pid));
                    }
                } else {
                    stderr.push_str(&format!("Unit {} not found\n", name));
                }
            });
            0
        }
        "start" => {
            if args.len() < 2 {
                stderr.push_str("systemctl: unit name required\n");
                return 1;
            }
            let name = &args[1];
            syscall::KERNEL.with(|k| {
                let mut kernel = k.borrow_mut();
                match kernel.init_mut().start_service(name) {
                    Ok(()) => {
                        stdout.push_str(&format!("Started {}\n", name));
                    }
                    Err(e) => {
                        stderr.push_str(&format!("Failed to start {}: {}\n", name, e));
                    }
                }
            });
            0
        }
        "stop" => {
            if args.len() < 2 {
                stderr.push_str("systemctl: unit name required\n");
                return 1;
            }
            let name = &args[1];
            syscall::KERNEL.with(|k| {
                let mut kernel = k.borrow_mut();
                match kernel.init_mut().stop_service(name) {
                    Ok(()) => {
                        stdout.push_str(&format!("Stopped {}\n", name));
                    }
                    Err(e) => {
                        stderr.push_str(&format!("Failed to stop {}: {}\n", name, e));
                    }
                }
            });
            0
        }
        "restart" => {
            if args.len() < 2 {
                stderr.push_str("systemctl: unit name required\n");
                return 1;
            }
            let name = &args[1];
            syscall::KERNEL.with(|k| {
                let mut kernel = k.borrow_mut();
                match kernel.init_mut().restart_service(name) {
                    Ok(()) => {
                        stdout.push_str(&format!("Restarted {}\n", name));
                    }
                    Err(e) => {
                        stderr.push_str(&format!("Failed to restart {}: {}\n", name, e));
                    }
                }
            });
            0
        }
        "enable" => {
            if args.len() < 2 {
                stderr.push_str("systemctl: unit name required\n");
                return 1;
            }
            let name = &args[1];
            syscall::KERNEL.with(|k| {
                let mut kernel = k.borrow_mut();
                match kernel.init_mut().enable_service(name) {
                    Ok(()) => {
                        stdout.push_str(&format!("Enabled {}\n", name));
                    }
                    Err(e) => {
                        stderr.push_str(&format!("Failed to enable {}: {}\n", name, e));
                    }
                }
            });
            0
        }
        "disable" => {
            if args.len() < 2 {
                stderr.push_str("systemctl: unit name required\n");
                return 1;
            }
            let name = &args[1];
            syscall::KERNEL.with(|k| {
                let mut kernel = k.borrow_mut();
                match kernel.init_mut().disable_service(name) {
                    Ok(()) => {
                        stdout.push_str(&format!("Disabled {}\n", name));
                    }
                    Err(e) => {
                        stderr.push_str(&format!("Failed to disable {}: {}\n", name, e));
                    }
                }
            });
            0
        }
        "get-default" => {
            syscall::KERNEL.with(|k| {
                let kernel = k.borrow();
                stdout.push_str(kernel.init().get_target().as_str());
                stdout.push('\n');
            });
            0
        }
        "set-default" => {
            if args.len() < 2 {
                stderr.push_str("systemctl: target required\n");
                return 1;
            }
            use crate::kernel::init::Target;
            let target_str = &args[1];
            if let Some(target) = Target::parse(target_str) {
                syscall::KERNEL.with(|k| {
                    let mut kernel = k.borrow_mut();
                    kernel.init_mut().set_target(target);
                    stdout.push_str(&format!(
                        "Created symlink /etc/systemd/system/default.target -> {}\n",
                        target.as_str()
                    ));
                });
                0
            } else {
                stderr.push_str(&format!("Unknown target: {}\n", target_str));
                1
            }
        }
        _ => {
            stderr.push_str(&format!("systemctl: unknown command '{}'\n", cmd));
            1
        }
    }
}

/// reboot - reboot the system
pub fn prog_reboot(
    args: &[String],
    __stdin: &str,
    stdout: &mut String,
    _stderr: &mut String,
) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: reboot\nReboot the system.") {
        stdout.push_str(&help);
        return 0;
    }

    use crate::kernel::init::Target;
    syscall::KERNEL.with(|k| {
        let mut kernel = k.borrow_mut();
        kernel.init_mut().set_target(Target::Reboot);
    });

    stdout.push_str("System is going down for reboot NOW!\n");
    0
}

/// poweroff - power off the system
pub fn prog_poweroff(
    args: &[String],
    __stdin: &str,
    stdout: &mut String,
    _stderr: &mut String,
) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: poweroff\nPower off the system.") {
        stdout.push_str(&help);
        return 0;
    }

    use crate::kernel::init::Target;
    syscall::KERNEL.with(|k| {
        let mut kernel = k.borrow_mut();
        kernel.init_mut().set_target(Target::Poweroff);
    });

    stdout.push_str("System is going down for poweroff NOW!\n");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_systemctl_no_args() {
        let args = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_systemctl(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: systemctl"));
        assert!(stdout.contains("list-units"));
    }

    #[test]
    fn test_systemctl_unknown_command() {
        let args = vec!["invalid".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_systemctl(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("unknown command"));
    }

    #[test]
    fn test_systemctl_status_no_name() {
        let args = vec!["status".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_systemctl(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("unit name required"));
    }

    #[test]
    fn test_reboot_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_reboot(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: reboot"));
    }

    #[test]
    fn test_poweroff_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_poweroff(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: poweroff"));
    }
}
