//! Shell utility programs

use super::{args_to_strs, check_help};
use crate::kernel::syscall;
use crate::shell::builtins;
use crate::shell::executor::ProgramRegistry;

/// clear - clear the terminal screen
pub fn prog_clear(_args: &[String], _stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    stdout.push_str("\x1b[2J\x1b[H");
    0
}

/// history - display command history
pub fn prog_history(args: &[String], _stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    // Get history from terminal module
    #[cfg(target_arch = "wasm32")]
    let history = crate::terminal::get_history();

    #[cfg(not(target_arch = "wasm32"))]
    let history: Vec<String> = Vec::new();

    // Check for -c (clear) flag
    if args.iter().any(|a| *a == "-c") {
        // Can't clear history from here - would need terminal module support
        stdout.push_str("history: clearing not supported\n");
        return 0;
    }

    // Check for count argument
    let count: Option<usize> = args.first().and_then(|a| a.parse().ok());

    let start = match count {
        Some(n) => history.len().saturating_sub(n),
        None => 0,
    };

    for (i, cmd) in history.iter().enumerate().skip(start) {
        stdout.push_str(&format!("{:5}  {}\n", i + 1, cmd));
    }

    if stdout.ends_with('\n') {
        stdout.pop();
    }

    0
}

/// Text editor - opens a file for editing
#[allow(unused_variables)]
pub fn prog_edit(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: edit [FILE]\nOpen text editor. Ctrl+Q to quit, Ctrl+S to save. See 'man edit' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    let filename = args.first().copied();

    #[cfg(target_arch = "wasm32")]
    {
        match crate::editor::start(filename) {
            Ok(()) => {
                // Editor started - control transfers to event loop
                // Don't output anything - editor takes over screen
                0
            }
            Err(e) => {
                stderr.push_str(&format!("edit: {}\n", e));
                1
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        stderr.push_str("edit: not available in this environment\n");
        1
    }
}

/// man - display manual pages
pub fn prog_man(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: man COMMAND\nDisplay manual page for a command. See 'man man' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    if args.is_empty() {
        stderr.push_str("What manual page do you want?\n");
        return 1;
    }

    let page = args[0];

    // Embedded man pages (pre-rendered from scdoc)
    let content = match page {
        "basename" => include_str!("../../../man/formatted/basename.txt"),
        "base64" => include_str!("../../../man/formatted/base64.txt"),
        "bg" => include_str!("../../../man/formatted/bg.txt"),
        "cal" => include_str!("../../../man/formatted/cal.txt"),
        "cat" => include_str!("../../../man/formatted/cat.txt"),
        "cd" => include_str!("../../../man/formatted/cd.txt"),
        "comm" => include_str!("../../../man/formatted/comm.txt"),
        "cp" => include_str!("../../../man/formatted/cp.txt"),
        "cut" => include_str!("../../../man/formatted/cut.txt"),
        "date" => include_str!("../../../man/formatted/date.txt"),
        "df" => include_str!("../../../man/formatted/df.txt"),
        "diff" => include_str!("../../../man/formatted/diff.txt"),
        "dirname" => include_str!("../../../man/formatted/dirname.txt"),
        "du" => include_str!("../../../man/formatted/du.txt"),
        "echo" => include_str!("../../../man/formatted/echo.txt"),
        "edit" => include_str!("../../../man/formatted/edit.txt"),
        "expr" => include_str!("../../../man/formatted/expr.txt"),
        "fg" => include_str!("../../../man/formatted/fg.txt"),
        "find" => include_str!("../../../man/formatted/find.txt"),
        "fold" => include_str!("../../../man/formatted/fold.txt"),
        "free" => include_str!("../../../man/formatted/free.txt"),
        "grep" => include_str!("../../../man/formatted/grep.txt"),
        "head" => include_str!("../../../man/formatted/head.txt"),
        "hostname" => include_str!("../../../man/formatted/hostname.txt"),
        "id" => include_str!("../../../man/formatted/id.txt"),
        "jobs" => include_str!("../../../man/formatted/jobs.txt"),
        "kill" => include_str!("../../../man/formatted/kill.txt"),
        "ln" => include_str!("../../../man/formatted/ln.txt"),
        "ls" => include_str!("../../../man/formatted/ls.txt"),
        "man" => include_str!("../../../man/formatted/man.txt"),
        "mkdir" => include_str!("../../../man/formatted/mkdir.txt"),
        "mv" => include_str!("../../../man/formatted/mv.txt"),
        "nl" => include_str!("../../../man/formatted/nl.txt"),
        "paste" => include_str!("../../../man/formatted/paste.txt"),
        "printenv" => include_str!("../../../man/formatted/printenv.txt"),
        "printf" => include_str!("../../../man/formatted/printf.txt"),
        "ps" => include_str!("../../../man/formatted/ps.txt"),
        "pwd" => include_str!("../../../man/formatted/pwd.txt"),
        "rev" => include_str!("../../../man/formatted/rev.txt"),
        "rm" => include_str!("../../../man/formatted/rm.txt"),
        "seq" => include_str!("../../../man/formatted/seq.txt"),
        "sort" => include_str!("../../../man/formatted/sort.txt"),
        "strace" => include_str!("../../../man/formatted/strace.txt"),
        "strings" => include_str!("../../../man/formatted/strings.txt"),
        "tail" => include_str!("../../../man/formatted/tail.txt"),
        "tee" => include_str!("../../../man/formatted/tee.txt"),
        "test" => include_str!("../../../man/formatted/test.txt"),
        "[" => include_str!("../../../man/formatted/test.txt"),
        "time" => include_str!("../../../man/formatted/time.txt"),
        "touch" => include_str!("../../../man/formatted/touch.txt"),
        "tr" => include_str!("../../../man/formatted/tr.txt"),
        "tree" => include_str!("../../../man/formatted/tree.txt"),
        "type" => include_str!("../../../man/formatted/type.txt"),
        "uname" => include_str!("../../../man/formatted/uname.txt"),
        "uniq" => include_str!("../../../man/formatted/uniq.txt"),
        "uptime" => include_str!("../../../man/formatted/uptime.txt"),
        "wc" => include_str!("../../../man/formatted/wc.txt"),
        "which" => include_str!("../../../man/formatted/which.txt"),
        "whoami" => include_str!("../../../man/formatted/whoami.txt"),
        "xargs" => include_str!("../../../man/formatted/xargs.txt"),
        "xxd" => include_str!("../../../man/formatted/xxd.txt"),
        "yes" => include_str!("../../../man/formatted/yes.txt"),
        _ => {
            stderr.push_str(&format!("No manual entry for {}\n", page));
            return 1;
        }
    };

    stdout.push_str(content.trim());
    0
}

/// printenv - print environment variables (uses kernel syscalls)
pub fn prog_printenv(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: printenv [NAME...]\nPrint environment variables from the kernel process.") {
        stdout.push_str(&help);
        return 0;
    }

    // Get environment from kernel
    match syscall::environ() {
        Ok(env) => {
            if args.is_empty() {
                // Print all environment variables
                let mut vars: Vec<_> = env.iter().collect();
                vars.sort_by(|a, b| a.0.cmp(&b.0));
                for (name, value) in vars {
                    stdout.push_str(&format!("{}={}\n", name, value));
                }
            } else {
                // Print specific variables
                let env_map: std::collections::HashMap<String, String> = env.into_iter().collect();
                for name in args {
                    if let Some(value) = env_map.get(&name.to_string()) {
                        stdout.push_str(&format!("{}\n", value));
                    }
                }
            }
            0
        }
        Err(e) => {
            stderr.push_str(&format!("printenv: {}\n", e));
            1
        }
    }
}

/// seq - print sequence of numbers
pub fn prog_seq(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: seq [FIRST] [INCREMENT] LAST\nPrint sequence of numbers.") {
        stdout.push_str(&help);
        return 0;
    }

    if args.is_empty() {
        stderr.push_str("seq: missing operand\n");
        return 1;
    }

    // Parse arguments
    let (first, increment, last) = match args.len() {
        1 => (1i64, 1i64, args[0].parse::<i64>().unwrap_or(1)),
        2 => (args[0].parse::<i64>().unwrap_or(1), 1i64, args[1].parse::<i64>().unwrap_or(1)),
        _ => (
            args[0].parse::<i64>().unwrap_or(1),
            args[1].parse::<i64>().unwrap_or(1),
            args[2].parse::<i64>().unwrap_or(1),
        ),
    };

    if increment == 0 {
        stderr.push_str("seq: increment cannot be zero\n");
        return 1;
    }

    let mut n = first;
    if increment > 0 {
        while n <= last {
            stdout.push_str(&format!("{}\n", n));
            n += increment;
        }
    } else {
        while n >= last {
            stdout.push_str(&format!("{}\n", n));
            n += increment;
        }
    }

    0
}

/// yes - output string repeatedly (limited iterations for safety)
pub fn prog_yes(args: &[String], _stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: yes [STRING]\nRepeatedly output STRING (limited to 100 lines).") {
        stdout.push_str(&help);
        return 0;
    }

    let text = if args.is_empty() { "y" } else { args[0] };

    // Limit to 100 iterations for safety in this environment
    for _ in 0..100 {
        stdout.push_str(text);
        stdout.push('\n');
    }

    0
}

/// basename - strip directory and suffix from filename
pub fn prog_basename(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: basename PATH [SUFFIX]\nStrip directory and suffix from PATH.") {
        stdout.push_str(&help);
        return 0;
    }

    if args.is_empty() {
        stderr.push_str("basename: missing operand\n");
        return 1;
    }

    let path = args[0];
    let suffix = args.get(1).map(|s| *s);

    // Get the last component
    let base = path.rsplit('/').next().unwrap_or(path);

    // Strip suffix if provided
    let result = if let Some(suf) = suffix {
        base.strip_suffix(suf).unwrap_or(base)
    } else {
        base
    };

    stdout.push_str(result);
    stdout.push('\n');
    0
}

/// dirname - strip last component from filename
pub fn prog_dirname(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: dirname PATH\nStrip last component from PATH.") {
        stdout.push_str(&help);
        return 0;
    }

    if args.is_empty() {
        stderr.push_str("dirname: missing operand\n");
        return 1;
    }

    let path = args[0];

    // Find the last slash
    let result = if let Some(pos) = path.rfind('/') {
        if pos == 0 {
            "/" // Root case
        } else {
            &path[..pos]
        }
    } else {
        "." // No directory component
    };

    stdout.push_str(result);
    stdout.push('\n');
    0
}

/// xargs - build command lines from stdin
pub fn prog_xargs(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: xargs [COMMAND] [ARGS]\nBuild command lines from stdin.") {
        stdout.push_str(&help);
        return 0;
    }

    // Get the command to run (default: echo)
    let cmd = if args.is_empty() { "echo" } else { args[0] };
    let cmd_args: Vec<&str> = if args.len() > 1 { args[1..].to_vec() } else { vec![] };

    // Read items from stdin
    let items: Vec<&str> = stdin.split_whitespace().collect();

    if items.is_empty() {
        return 0;
    }

    // For now, just show what would be executed
    // (In a full implementation we'd actually run the command)
    let full_cmd = format!("{} {} {}", cmd, cmd_args.join(" "), items.join(" "));
    stdout.push_str(&format!("xargs: would execute: {}\n", full_cmd.trim()));

    0
}

/// cal - display a calendar
pub fn prog_cal(args: &[String], _stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: cal [MONTH] [YEAR]\nDisplay a calendar.") {
        stdout.push_str(&help);
        return 0;
    }
    let args: Vec<String> = args.into_iter().map(|s| s.to_string()).collect();

    // Get current date from system (or use defaults)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();

    // Calculate year/month from timestamp
    let secs = now.as_secs() as i64;
    let days_since_epoch = secs / 86400;

    // Approximate year calculation (leap years make this imprecise, but good enough)
    let mut year = 1970i32;
    let mut remaining_days = days_since_epoch;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    // Calculate month
    let mut month = 1u32;
    loop {
        let days_in_month = days_in_month(month, year);
        if remaining_days < days_in_month as i64 {
            break;
        }
        remaining_days -= days_in_month as i64;
        month += 1;
    }

    let current_day = (remaining_days + 1) as u32;

    // Parse arguments
    let (show_month, show_year) = if args.len() >= 2 {
        (args[0].parse().unwrap_or(month), args[1].parse().unwrap_or(year))
    } else if args.len() == 1 {
        (month, args[0].parse().unwrap_or(year))
    } else {
        (month, year)
    };

    let month_names = [
        "January", "February", "March", "April", "May", "June",
        "July", "August", "September", "October", "November", "December"
    ];

    let month_name = month_names.get((show_month - 1) as usize).unwrap_or(&"???");

    // Header
    let header = format!("{} {}", month_name, show_year);
    let padding = (20 - header.len()) / 2;
    stdout.push_str(&" ".repeat(padding));
    stdout.push_str(&header);
    stdout.push('\n');
    stdout.push_str("Su Mo Tu We Th Fr Sa\n");

    // First day of month (Zeller's congruence simplified)
    let first_day = day_of_week(1, show_month, show_year);

    // Print leading spaces
    for _ in 0..first_day {
        stdout.push_str("   ");
    }

    let days = days_in_month(show_month, show_year);
    let mut col = first_day;

    for day in 1..=days {
        let is_today = show_month == month && show_year == year && day == current_day;
        if is_today {
            stdout.push_str(&format!("{:>2}*", day));
        } else {
            stdout.push_str(&format!("{:>2} ", day));
        }
        col += 1;
        if col == 7 {
            stdout.push('\n');
            col = 0;
        }
    }

    if col != 0 {
        stdout.push('\n');
    }

    0
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month(month: u32, year: i32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if is_leap_year(year) { 29 } else { 28 },
        _ => 30,
    }
}

fn day_of_week(day: u32, month: u32, year: i32) -> u32 {
    // Zeller's congruence (for Gregorian calendar)
    let mut m = month as i32;
    let mut y = year;

    if m < 3 {
        m += 12;
        y -= 1;
    }

    let k = y % 100;
    let j = y / 100;

    let h = (day as i32 + (13 * (m + 1)) / 5 + k + k / 4 + j / 4 - 2 * j) % 7;

    // Convert from Zeller (0=Sat) to Sunday-first (0=Sun)
    ((h + 6) % 7) as u32
}

/// printf - format and print data
pub fn prog_printf(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if args.is_empty() {
        stderr.push_str("printf: usage: printf FORMAT [ARG]...\n");
        return 1;
    }

    if let Some(help) = check_help(&args, "Usage: printf FORMAT [ARG]...\nFormat and print data.") {
        stdout.push_str(&help);
        return 0;
    }

    let format = args[0];
    let args = &args[1..];
    let mut arg_idx = 0;

    let mut chars = format.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => stdout.push('\n'),
                Some('t') => stdout.push('\t'),
                Some('r') => stdout.push('\r'),
                Some('\\') => stdout.push('\\'),
                Some('"') => stdout.push('"'),
                Some('0') => stdout.push('\0'),
                Some(other) => {
                    stdout.push('\\');
                    stdout.push(other);
                }
                None => stdout.push('\\'),
            }
        } else if c == '%' {
            match chars.next() {
                Some('s') => {
                    if arg_idx < args.len() {
                        stdout.push_str(&args[arg_idx]);
                        arg_idx += 1;
                    }
                }
                Some('d') | Some('i') => {
                    if arg_idx < args.len() {
                        let val: i64 = args[arg_idx].parse().unwrap_or(0);
                        stdout.push_str(&val.to_string());
                        arg_idx += 1;
                    }
                }
                Some('x') => {
                    if arg_idx < args.len() {
                        let val: i64 = args[arg_idx].parse().unwrap_or(0);
                        stdout.push_str(&format!("{:x}", val));
                        arg_idx += 1;
                    }
                }
                Some('X') => {
                    if arg_idx < args.len() {
                        let val: i64 = args[arg_idx].parse().unwrap_or(0);
                        stdout.push_str(&format!("{:X}", val));
                        arg_idx += 1;
                    }
                }
                Some('o') => {
                    if arg_idx < args.len() {
                        let val: i64 = args[arg_idx].parse().unwrap_or(0);
                        stdout.push_str(&format!("{:o}", val));
                        arg_idx += 1;
                    }
                }
                Some('c') => {
                    if arg_idx < args.len() {
                        if let Some(ch) = args[arg_idx].chars().next() {
                            stdout.push(ch);
                        }
                        arg_idx += 1;
                    }
                }
                Some('%') => stdout.push('%'),
                Some(other) => {
                    stdout.push('%');
                    stdout.push(other);
                }
                None => stdout.push('%'),
            }
        } else {
            stdout.push(c);
        }
    }

    0
}

/// test - evaluate conditional expression
pub fn prog_test(args: &[String], _stdin: &str, _stdout: &mut String, stderr: &mut String) -> i32 {
    if args.is_empty() {
        return 1; // No arguments = false
    }

    // Handle [ ... ] form: strip trailing ]
    let args: Vec<&str> = if !args.is_empty() && args[args.len() - 1] == "]" {
        args[..args.len() - 1].iter().map(|s| s.as_str()).collect()
    } else {
        args.iter().map(|s| s.as_str()).collect()
    };

    if args.is_empty() {
        return 1;
    }

    if args.len() == 1 {
        // Single argument: true if non-empty string
        return if args[0].is_empty() { 1 } else { 0 };
    }

    if args[0] == "!" {
        // Negation
        let rest: Vec<String> = args[1..].iter().map(|s| s.to_string()).collect();
        let result = prog_test(&rest, "", &mut String::new(), stderr);
        return if result == 0 { 1 } else { 0 };
    }

    if args.len() == 2 {
        // Unary operators
        let op = args[0];
        let arg = args[1];

        return match op {
            "-n" => if arg.is_empty() { 1 } else { 0 },
            "-z" => if arg.is_empty() { 0 } else { 1 },
            "-e" | "-a" => if syscall::exists(arg).unwrap_or(false) { 0 } else { 1 },
            "-f" => {
                if syscall::exists(arg).unwrap_or(false) {
                    if let Ok(meta) = syscall::stat(arg) {
                        if !meta.is_dir { 0 } else { 1 }
                    } else {
                        1
                    }
                } else {
                    1
                }
            }
            "-d" => {
                if syscall::exists(arg).unwrap_or(false) {
                    if let Ok(meta) = syscall::stat(arg) {
                        if meta.is_dir { 0 } else { 1 }
                    } else {
                        1
                    }
                } else {
                    1
                }
            }
            "-r" | "-w" | "-x" => {
                // Assume readable/writable/executable if exists
                if syscall::exists(arg).unwrap_or(false) { 0 } else { 1 }
            }
            "-s" => {
                // True if file exists and has size > 0
                if let Ok(meta) = syscall::stat(arg) {
                    if meta.size > 0 { 0 } else { 1 }
                } else {
                    1
                }
            }
            "-L" | "-h" => {
                // True if symbolic link (check via read_link)
                if syscall::read_link(arg).is_ok() { 0 } else { 1 }
            }
            _ => 1, // Unknown unary operator
        };
    }

    if args.len() >= 3 {
        let left = args[0];
        let op = args[1];
        let right = args[2];

        // String comparisons
        match op {
            "=" | "==" => return if left == right { 0 } else { 1 },
            "!=" => return if left != right { 0 } else { 1 },
            _ => {}
        }

        // Numeric comparisons
        let left_num: i64 = left.parse().unwrap_or(0);
        let right_num: i64 = right.parse().unwrap_or(0);

        match op {
            "-eq" => return if left_num == right_num { 0 } else { 1 },
            "-ne" => return if left_num != right_num { 0 } else { 1 },
            "-lt" => return if left_num < right_num { 0 } else { 1 },
            "-le" => return if left_num <= right_num { 0 } else { 1 },
            "-gt" => return if left_num > right_num { 0 } else { 1 },
            "-ge" => return if left_num >= right_num { 0 } else { 1 },
            _ => {}
        }
    }

    stderr.push_str("test: unknown condition\n");
    1
}

/// expr - evaluate expressions
pub fn prog_expr(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args_ref = args_to_strs(args);

    if args_ref.is_empty() {
        stderr.push_str("expr: missing operand\n");
        return 2;
    }

    if let Some(help) = check_help(&args_ref, "Usage: expr EXPRESSION\nEvaluate expressions.") {
        stdout.push_str(&help);
        return 0;
    }

    let args: Vec<String> = args_ref.into_iter().map(|s| s.to_string()).collect();

    // Simple expression evaluation
    if args.len() == 1 {
        stdout.push_str(&args[0]);
        stdout.push('\n');
        return if args[0] == "0" || args[0].is_empty() { 1 } else { 0 };
    }

    if args.len() == 3 {
        let left = &args[0];
        let op = &args[1];
        let right = &args[2];

        // String operations
        match op.as_str() {
            ":" | "match" => {
                // Pattern match - simplified: returns length of match
                // In real expr, this would use regex
                let result = if left.starts_with(right) {
                    right.len()
                } else {
                    0
                };
                stdout.push_str(&result.to_string());
                stdout.push('\n');
                return if result == 0 { 1 } else { 0 };
            }
            _ => {}
        }

        // Try numeric operations
        let left_num: Result<i64, _> = left.parse();
        let right_num: Result<i64, _> = right.parse();

        if let (Ok(l), Ok(r)) = (left_num, right_num) {
            let result = match op.as_str() {
                "+" => Some(l + r),
                "-" => Some(l - r),
                "*" => Some(l * r),
                "/" => {
                    if r == 0 {
                        stderr.push_str("expr: division by zero\n");
                        return 2;
                    }
                    Some(l / r)
                }
                "%" => {
                    if r == 0 {
                        stderr.push_str("expr: division by zero\n");
                        return 2;
                    }
                    Some(l % r)
                }
                "<" => Some(if l < r { 1 } else { 0 }),
                "<=" => Some(if l <= r { 1 } else { 0 }),
                ">" => Some(if l > r { 1 } else { 0 }),
                ">=" => Some(if l >= r { 1 } else { 0 }),
                "=" => Some(if l == r { 1 } else { 0 }),
                "!=" => Some(if l != r { 1 } else { 0 }),
                "&" => Some(if l != 0 && r != 0 { l } else { 0 }),
                "|" => Some(if l != 0 { l } else { r }),
                _ => None,
            };

            if let Some(val) = result {
                stdout.push_str(&val.to_string());
                stdout.push('\n');
                return if val == 0 { 1 } else { 0 };
            }
        }

        // String comparison
        match op.as_str() {
            "=" => {
                let result = if left == right { 1 } else { 0 };
                stdout.push_str(&result.to_string());
                stdout.push('\n');
                return if left == right { 0 } else { 1 };
            }
            "!=" => {
                let result = if left != right { 1 } else { 0 };
                stdout.push_str(&result.to_string());
                stdout.push('\n');
                return if left != right { 0 } else { 1 };
            }
            _ => {}
        }
    }

    // Handle length operation
    if args.len() == 2 && args[0] == "length" {
        stdout.push_str(&args[1].len().to_string());
        stdout.push('\n');
        return 0;
    }

    // Handle substr
    if args.len() == 4 && args[0] == "substr" {
        let string = &args[1];
        let pos: usize = args[2].parse().unwrap_or(1);
        let len: usize = args[3].parse().unwrap_or(0);
        let start = pos.saturating_sub(1); // expr uses 1-based indexing
        let substr: String = string.chars().skip(start).take(len).collect();
        stdout.push_str(&substr);
        stdout.push('\n');
        return 0;
    }

    stderr.push_str("expr: syntax error\n");
    2
}

/// which - locate a command
pub fn prog_which(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if args.is_empty() {
        stderr.push_str("which: missing argument\n");
        return 1;
    }

    if let Some(help) = check_help(&args, "Usage: which COMMAND\nLocate a command.") {
        stdout.push_str(&help);
        return 0;
    }

    let reg = ProgramRegistry::new();
    let mut exit_code = 0;

    for cmd in &args {
        if builtins::is_builtin(cmd) {
            stdout.push_str(&format!("{}: shell built-in command\n", cmd));
        } else if reg.contains(cmd) {
            stdout.push_str(&format!("/bin/{}\n", cmd));
        } else {
            stderr.push_str(&format!("{} not found\n", cmd));
            exit_code = 1;
        }
    }

    exit_code
}

/// type - describe a command
pub fn prog_type(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if args.is_empty() {
        stderr.push_str("type: missing argument\n");
        return 1;
    }

    if let Some(help) = check_help(&args, "Usage: type COMMAND\nDescribe how a command would be interpreted.") {
        stdout.push_str(&help);
        return 0;
    }

    let reg = ProgramRegistry::new();
    let mut exit_code = 0;

    for cmd in &args {
        if builtins::is_builtin(cmd) {
            stdout.push_str(&format!("{} is a shell builtin\n", cmd));
        } else if reg.contains(cmd) {
            stdout.push_str(&format!("{} is /bin/{}\n", cmd, cmd));
        } else {
            stderr.push_str(&format!("{}: not found\n", cmd));
            exit_code = 1;
        }
    }

    exit_code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basename() {
        let mut stdout = String::new();
        let mut stderr = String::new();
        let args = vec!["/usr/bin/test".to_string()];

        let code = prog_basename(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout.trim(), "test");
    }

    #[test]
    fn test_basename_with_suffix() {
        let mut stdout = String::new();
        let mut stderr = String::new();
        let args = vec!["/usr/bin/test.txt".to_string(), ".txt".to_string()];

        let code = prog_basename(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout.trim(), "test");
    }

    #[test]
    fn test_dirname() {
        let mut stdout = String::new();
        let mut stderr = String::new();
        let args = vec!["/usr/bin/test".to_string()];

        let code = prog_dirname(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout.trim(), "/usr/bin");
    }

    #[test]
    fn test_dirname_root() {
        let mut stdout = String::new();
        let mut stderr = String::new();
        let args = vec!["/test".to_string()];

        let code = prog_dirname(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout.trim(), "/");
    }

    #[test]
    fn test_seq() {
        let mut stdout = String::new();
        let mut stderr = String::new();
        let args = vec!["3".to_string()];

        let code = prog_seq(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout, "1\n2\n3\n");
    }

    #[test]
    fn test_seq_range() {
        let mut stdout = String::new();
        let mut stderr = String::new();
        let args = vec!["2".to_string(), "5".to_string()];

        let code = prog_seq(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout, "2\n3\n4\n5\n");
    }

    #[test]
    fn test_yes() {
        let mut stdout = String::new();
        let mut stderr = String::new();
        let args: Vec<String> = vec![];

        let code = prog_yes(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        // Should output "y\n" 100 times
        assert_eq!(stdout.lines().count(), 100);
        assert!(stdout.lines().all(|line| line == "y"));
    }

    #[test]
    fn test_printf_basic() {
        let mut stdout = String::new();
        let mut stderr = String::new();
        let args = vec!["Hello %s\\n".to_string(), "World".to_string()];

        let code = prog_printf(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout, "Hello World\n");
    }

    #[test]
    fn test_printf_number() {
        let mut stdout = String::new();
        let mut stderr = String::new();
        let args = vec!["%d".to_string(), "42".to_string()];

        let code = prog_printf(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout, "42");
    }

    #[test]
    fn test_expr_addition() {
        let mut stdout = String::new();
        let mut stderr = String::new();
        let args = vec!["5".to_string(), "+".to_string(), "3".to_string()];

        let code = prog_expr(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout.trim(), "8");
    }

    #[test]
    fn test_expr_length() {
        let mut stdout = String::new();
        let mut stderr = String::new();
        let args = vec!["length".to_string(), "hello".to_string()];

        let code = prog_expr(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout.trim(), "5");
    }

    #[test]
    fn test_test_string_empty() {
        let mut stdout = String::new();
        let mut stderr = String::new();
        let args = vec!["-z".to_string(), "".to_string()];

        let code = prog_test(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0); // -z "" is true
    }

    #[test]
    fn test_test_numeric_comparison() {
        let mut stdout = String::new();
        let mut stderr = String::new();
        let args = vec!["5".to_string(), "-lt".to_string(), "10".to_string()];

        let code = prog_test(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0); // 5 < 10 is true
    }

    #[test]
    fn test_clear() {
        let mut stdout = String::new();
        let mut stderr = String::new();
        let args: Vec<String> = vec![];

        let code = prog_clear(&args, "", &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout, "\x1b[2J\x1b[H");
    }

    #[test]
    fn test_is_leap_year() {
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2020));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2019));
    }

    #[test]
    fn test_days_in_month() {
        assert_eq!(days_in_month(1, 2020), 31);
        assert_eq!(days_in_month(2, 2020), 29); // Leap year
        assert_eq!(days_in_month(2, 2019), 28); // Not leap year
        assert_eq!(days_in_month(4, 2020), 30);
    }
}
