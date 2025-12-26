//! TTY (terminal) programs

use super::{args_to_strs, check_help};
use crate::kernel::syscall;

pub fn prog_stty(args: &[String], __stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(
        &args,
        "Usage: stty [SETTING]...\n       stty -a\n       stty sane\n       stty raw\n\nChange and print terminal line settings.\n\nSettings:\n  -echo/-icanon/-isig  Toggle flags\n  sane                 Reset to sane defaults\n  raw                  Set raw mode\n  -a                   Print all settings",
    ) {
        stdout.push_str(&help);
        return 0;
    }

    use crate::kernel::tty::{Termios, format_stty_settings, parse_stty_setting};

    syscall::KERNEL.with(|k| {
        let mut kernel = k.borrow_mut();

        // If no args or -a, print current settings
        if args.is_empty() || args.contains(&"-a") {
            if let Some(tty) = kernel.ttys().current_tty() {
                stdout.push_str(&format_stty_settings(&tty.termios));
            } else {
                stderr.push_str("stty: no controlling terminal\n");
                return 1;
            }
            return 0;
        }

        // Get current termios
        let mut termios = if let Some(tty) = kernel.ttys().current_tty() {
            tty.termios.clone()
        } else {
            Termios::default()
        };

        // Apply settings
        for setting in &args {
            if let Err(e) = parse_stty_setting(&mut termios, setting) {
                stderr.push_str(&format!("stty: {}\n", e));
                return 1;
            }
        }

        // Update the terminal
        if let Some(tty) = kernel.ttys_mut().current_tty_mut() {
            tty.termios = termios;
        }

        0
    })
}

pub fn prog_tty(args: &[String], __stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(
        &args,
        "Usage: tty\nPrint the file name of the terminal connected to standard input.",
    ) {
        stdout.push_str(&help);
        return 0;
    }

    let silent = args.contains(&"-s");

    syscall::KERNEL.with(|k| {
        let kernel = k.borrow();
        if let Some(tty) = kernel.ttys().current_tty() {
            if !silent {
                stdout.push_str(&format!("/dev/{}\n", tty.name));
            }
            0
        } else {
            if !silent {
                stdout.push_str("not a tty\n");
            }
            1
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stty_help() {
        let args = vec![String::from("--help")];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let result = prog_stty(&args, "", &mut stdout, &mut stderr);

        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: stty"));
        assert!(stdout.contains("Change and print terminal line settings"));
        assert!(stderr.is_empty());
    }

    #[test]
    fn test_tty_help() {
        let args = vec![String::from("--help")];
        let mut stdout = String::new();
        let mut stderr = String::new();

        let result = prog_tty(&args, "", &mut stdout, &mut stderr);

        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: tty"));
        assert!(stdout.contains("Print the file name of the terminal"));
        assert!(stderr.is_empty());
    }

    #[test]
    fn test_tty_silent_flag() {
        let args = vec![String::from("-s")];
        let mut stdout = String::new();
        let mut stderr = String::new();

        // Result depends on whether we have a tty, but output should be empty with -s
        let _result = prog_tty(&args, "", &mut stdout, &mut stderr);

        // With -s flag, stdout should be empty regardless of result
        assert!(stdout.is_empty());
        assert!(stderr.is_empty());
    }

    #[test]
    fn test_stty_no_args() {
        let args: Vec<String> = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();

        // This will either print settings or error about no tty
        let _result = prog_stty(&args, "", &mut stdout, &mut stderr);

        // Should produce some output (either settings or error)
        assert!(!stdout.is_empty() || !stderr.is_empty());
    }

    #[test]
    fn test_tty_no_args() {
        let args: Vec<String> = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();

        // This will either print tty name or "not a tty"
        let _result = prog_tty(&args, "", &mut stdout, &mut stderr);

        // Should produce output in stdout
        assert!(!stdout.is_empty());
        assert!(stdout.contains("/dev/") || stdout.contains("not a tty"));
    }
}
