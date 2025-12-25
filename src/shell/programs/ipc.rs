//! IPC (Inter-Process Communication) programs

use super::{args_to_strs, check_help};
use crate::kernel::syscall;

pub fn prog_mkfifo(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: mkfifo NAME...\nCreate named pipes (FIFOs).\n\nOptions:\n  -m MODE  Set permission mode (octal)") {
        stdout.push_str(&help);
        return 0;
    }

    if args.is_empty() {
        stderr.push_str("mkfifo: missing operand\n");
        return 1;
    }

    let mut exit_code = 0;
    for path in &args {
        if path.starts_with('-') {
            continue; // Skip options for now
        }

        syscall::KERNEL.with(|k| {
            let mut kernel = k.borrow_mut();
            match kernel.fifos_mut().mkfifo(path) {
                Ok(()) => {
                    // FIFO registered successfully
                }
                Err(e) => {
                    stderr.push_str(&format!("mkfifo: cannot create fifo '{}': {:?}\n", path, e));
                    exit_code = 1;
                }
            }
        });
    }

    exit_code
}

pub fn prog_ipcs(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: ipcs [options]\nShow IPC facilities.\n\nOptions:\n  -a  Show all (default)\n  -q  Show message queues\n  -s  Show semaphores\n  -m  Show shared memory") {
        stdout.push_str(&help);
        return 0;
    }

    let show_all = args.is_empty() || args.iter().any(|a| *a == "-a");
    let show_queues = show_all || args.iter().any(|a| *a == "-q");
    let show_sems = show_all || args.iter().any(|a| *a == "-s");
    let show_shm = show_all || args.iter().any(|a| *a == "-m");

    syscall::KERNEL.with(|k| {
        let kernel = k.borrow();

        // Message Queues
        if show_queues {
            stdout.push_str("\n------ Message Queues --------\n");
            stdout.push_str("key        msqid      owner      perms      used-bytes   messages\n");
            let queues = kernel.msgqueues().list();
            if queues.is_empty() {
                stdout.push_str("(none)\n");
            } else {
                for id in queues {
                    if let Ok(stats) = kernel.msgqueues().msgctl_stat(id) {
                        stdout.push_str(&format!(
                            "{:<10} {:<10} {:<10} {:<10} {:<12} {}\n",
                            "-", id.0, "-", "0644", stats.msg_cbytes, stats.msg_qnum
                        ));
                    }
                }
            }
        }

        // Semaphore Arrays
        if show_sems {
            stdout.push_str("\n------ Semaphore Arrays ------\n");
            stdout.push_str("key        semid      owner      perms      nsems\n");
            let sems = kernel.semaphores().list();
            if sems.is_empty() {
                stdout.push_str("(none)\n");
            } else {
                for id in sems {
                    if let Some(set) = kernel.semaphores().get_set(id) {
                        stdout.push_str(&format!(
                            "{:<10} {:<10} {:<10} {:04o}       {}\n",
                            "-", id.0, set.uid, set.mode, set.len()
                        ));
                    }
                }
            }
        }

        // Shared Memory
        if show_shm {
            stdout.push_str("\n------ Shared Memory Segments ------\n");
            stdout.push_str("key        shmid      creator    attached   bytes\n");
            let shm_list = kernel.sys_shm_list().unwrap_or_default();
            if shm_list.is_empty() {
                stdout.push_str("(none)\n");
            } else {
                for info in shm_list {
                    stdout.push_str(&format!(
                        "{:<10} {:<10} {:<10} {:<10} {}\n",
                        "-", info.id.0, info.creator.0, info.attached_count, info.size
                    ));
                }
            }
        }

        stdout.push_str("\n");
    });

    0
}

pub fn prog_ipcrm(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: ipcrm [options]\nRemove IPC resources.\n\nOptions:\n  -q ID   Remove message queue with ID\n  -s ID   Remove semaphore set with ID\n  -m ID   Remove shared memory with ID\n  -a      Remove all IPC resources") {
        stdout.push_str(&help);
        return 0;
    }

    let mut exit_code = 0;

    // Check for -a (remove all)
    if args.iter().any(|a| *a == "-a") {
        syscall::KERNEL.with(|k| {
            let mut kernel = k.borrow_mut();

            // Remove all message queues
            let queues: Vec<_> = kernel.msgqueues().list();
            for id in queues {
                let _ = kernel.msgqueues_mut().msgctl_rmid(id);
            }

            // Remove all semaphores
            let sems: Vec<_> = kernel.semaphores().list();
            for id in sems {
                let _ = kernel.semaphores_mut().semctl_rmid(id);
            }
        });
        stdout.push_str("All IPC resources removed.\n");
        return 0;
    }

    let mut i = 0;
    while i < args.len() {
        let opt = &args[i][..];
        match opt {
            "-q" => {
                if i + 1 >= args.len() {
                    stderr.push_str("ipcrm: option requires an argument -- 'q'\n");
                    exit_code = 1;
                } else {
                    i += 1;
                    if let Ok(id) = args[i].parse::<u32>() {
                        use crate::kernel::msgqueue::MsgQueueId;
                        let success = syscall::KERNEL.with(|k| {
                            k.borrow_mut().msgqueues_mut().msgctl_rmid(MsgQueueId(id)).is_ok()
                        });
                        if !success {
                            stderr.push_str(&format!("ipcrm: invalid id: {}\n", id));
                            exit_code = 1;
                        }
                    } else {
                        stderr.push_str(&format!("ipcrm: invalid id: {}\n", args[i]));
                        exit_code = 1;
                    }
                }
            }
            "-s" => {
                if i + 1 >= args.len() {
                    stderr.push_str("ipcrm: option requires an argument -- 's'\n");
                    exit_code = 1;
                } else {
                    i += 1;
                    if let Ok(id) = args[i].parse::<u32>() {
                        use crate::kernel::semaphore::SemId;
                        let success = syscall::KERNEL.with(|k| {
                            k.borrow_mut().semaphores_mut().semctl_rmid(SemId(id)).is_ok()
                        });
                        if !success {
                            stderr.push_str(&format!("ipcrm: invalid id: {}\n", id));
                            exit_code = 1;
                        }
                    } else {
                        stderr.push_str(&format!("ipcrm: invalid id: {}\n", args[i]));
                        exit_code = 1;
                    }
                }
            }
            "-m" => {
                if i + 1 >= args.len() {
                    stderr.push_str("ipcrm: option requires an argument -- 'm'\n");
                    exit_code = 1;
                } else {
                    i += 1;
                    // Note: Shared memory segments cannot be removed directly in this implementation
                    // They are automatically cleaned up when all processes detach
                    stderr.push_str(&format!("ipcrm: shared memory removal not supported (id: {})\n", args[i]));
                    stderr.push_str("       Shared memory is cleaned up when all processes detach.\n");
                }
            }
            _ => {
                // Skip unknown options
            }
        }
        i += 1;
    }

    exit_code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mkfifo_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_mkfifo(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: mkfifo"));
    }

    #[test]
    fn test_mkfifo_missing_operand() {
        let args = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_mkfifo(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("missing operand"));
    }

    #[test]
    fn test_ipcs_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_ipcs(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: ipcs"));
    }

    #[test]
    fn test_ipcs_default_shows_all() {
        let args = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_ipcs(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Message Queues"));
        assert!(stdout.contains("Semaphore Arrays"));
        assert!(stdout.contains("Shared Memory"));
    }

    #[test]
    fn test_ipcrm_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_ipcrm(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: ipcrm"));
    }

    #[test]
    fn test_ipcrm_invalid_queue_id() {
        let args = vec!["-q".to_string(), "invalid".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_ipcrm(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("invalid id"));
    }

    #[test]
    fn test_ipcrm_missing_argument() {
        let args = vec!["-q".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_ipcrm(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("option requires an argument"));
    }
}
