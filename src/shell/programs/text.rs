//! Text processing programs
//!
//! Programs for text manipulation: head, tail, wc, grep, sort, uniq, tee,
//! rev, cut, tr, nl, fold, paste, comm, strings, diff

use super::{args_to_strs, check_help, read_file_content};
use crate::kernel::syscall;

/// head - output first lines
pub fn prog_head(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: head [-n N] [FILE]\nOutput first N lines (default 10). See 'man head' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    let mut n = 10;
    let mut files = Vec::new();

    let mut i = 0;
    while i < args.len() {
        if args[i] == "-n" && i + 1 < args.len() {
            n = args[i + 1].parse().unwrap_or(10);
            i += 2;
        } else if args[i].starts_with("-n") {
            n = args[i][2..].parse().unwrap_or(10);
            i += 1;
        } else {
            files.push(args[i]);
            i += 1;
        }
    }

    let input = if files.is_empty() {
        stdin.to_string()
    } else {
        // Read first file
        match syscall::read_file(files[0]) {
            Ok(content) => content,
            Err(_) => return 1,
        }
    };

    for (i, line) in input.lines().enumerate() {
        if i >= n {
            break;
        }
        stdout.push_str(line);
        stdout.push('\n');
    }

    if stdout.ends_with('\n') {
        stdout.pop();
    }

    0
}

/// tail - output last lines
pub fn prog_tail(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: tail [-n N] [FILE]\nOutput last N lines (default 10). See 'man tail' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    let mut n = 10;

    for i in 0..args.len() {
        if args[i] == "-n" && i + 1 < args.len() {
            n = args[i + 1].parse().unwrap_or(10);
        } else if args[i].starts_with("-n") {
            n = args[i][2..].parse().unwrap_or(10);
        }
    }

    let input = stdin.to_string();
    let lines: Vec<&str> = input.lines().collect();
    let start = lines.len().saturating_sub(n);

    for line in &lines[start..] {
        stdout.push_str(line);
        stdout.push('\n');
    }

    if stdout.ends_with('\n') {
        stdout.pop();
    }

    0
}

/// wc - word, line, character count
pub fn prog_wc(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: wc [-lwc] [FILE]\nCount lines, words, and characters. See 'man wc' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    let show_lines = args.contains(&"-l");
    let show_words = args.contains(&"-w");
    let show_chars = args.contains(&"-c") || args.contains(&"-m");
    let show_all = !show_lines && !show_words && !show_chars;

    let input = stdin.to_string();
    let lines = input.lines().count();
    let words = input.split_whitespace().count();
    let chars = input.len();

    if show_all {
        stdout.push_str(&format!("{} {} {}", lines, words, chars));
    } else {
        let mut parts = Vec::new();
        if show_lines {
            parts.push(lines.to_string());
        }
        if show_words {
            parts.push(words.to_string());
        }
        if show_chars {
            parts.push(chars.to_string());
        }
        stdout.push_str(&parts.join(" "));
    }

    0
}

/// grep - search for patterns
pub fn prog_grep(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: grep [-inv] PATTERN [FILE]...\nSearch for patterns in files. See 'man grep' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    if args.is_empty() {
        stderr.push_str("grep: missing pattern\n");
        return 1;
    }

    // ANSI color codes
    const RED: &str = "\x1b[31m";
    const RESET: &str = "\x1b[0m";

    let pattern = args[0];
    let input = stdin.to_string();
    let mut found = false;

    for line in input.lines() {
        if line.contains(pattern) {
            // Highlight all matches in red
            let highlighted = line.replace(pattern, &format!("{}{}{}", RED, pattern, RESET));
            stdout.push_str(&highlighted);
            stdout.push('\n');
            found = true;
        }
    }

    if stdout.ends_with('\n') {
        stdout.pop();
    }

    if found { 0 } else { 1 }
}

/// sort - sort lines
pub fn prog_sort(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: sort [-ru] [FILE]\nSort lines of text. See 'man sort' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    let reverse = args.contains(&"-r");
    let unique = args.contains(&"-u");

    let input = stdin.to_string();
    let mut lines: Vec<&str> = input.lines().collect();

    lines.sort();
    if reverse {
        lines.reverse();
    }
    if unique {
        lines.dedup();
    }

    stdout.push_str(&lines.join("\n"));
    0
}

/// uniq - filter adjacent duplicate lines
pub fn prog_uniq(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: uniq [-c] [FILE]\nFilter adjacent duplicate lines. See 'man uniq' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    let count = args.contains(&"-c");

    let input = stdin.to_string();
    let mut prev: Option<&str> = None;
    let mut cnt = 0;

    for line in input.lines() {
        if Some(line) == prev {
            cnt += 1;
        } else {
            if let Some(p) = prev {
                if count {
                    stdout.push_str(&format!("{:>4} {}\n", cnt, p));
                } else {
                    stdout.push_str(p);
                    stdout.push('\n');
                }
            }
            prev = Some(line);
            cnt = 1;
        }
    }

    // Output last line
    if let Some(p) = prev {
        if count {
            stdout.push_str(&format!("{:>4} {}", cnt, p));
        } else {
            stdout.push_str(p);
        }
    }

    0
}

/// tee - read stdin and write to files
pub fn prog_tee(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let files = args_to_strs(args);

    if let Some(help) = check_help(&files, "Usage: tee [-a] FILE\nCopy stdin to file and stdout. See 'man tee' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    let input = stdin.to_string();

    // Write to stdout
    stdout.push_str(&input);

    // Write to files
    let append = files.contains(&"-a");
    let files: Vec<&str> = files.into_iter().filter(|f| *f != "-a").collect();

    for file in files {
        let flags = if append {
            syscall::OpenFlags::APPEND
        } else {
            syscall::OpenFlags::WRITE
        };

        match syscall::open(file, flags) {
            Ok(fd) => {
                let _ = syscall::write(fd, input.as_bytes());
                let _ = syscall::close(fd);
            }
            Err(e) => {
                stderr.push_str(&format!("tee: {}: {}\n", file, e));
            }
        }
    }

    0
}

/// rev - reverse lines
pub fn prog_rev(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: rev [FILE]\nReverse characters in each line.") {
        stdout.push_str(&help);
        return 0;
    }

    let content = if !stdin.is_empty() {
        stdin.to_string()
    } else if !args.is_empty() {
        match read_file_content(args[0]) {
            Ok(c) => c,
            Err(e) => {
                stderr.push_str(&format!("rev: {}: {}\n", args[0], e));
                return 1;
            }
        }
    } else {
        String::new()
    };

    for line in content.lines() {
        let reversed: String = line.chars().rev().collect();
        stdout.push_str(&reversed);
        stdout.push('\n');
    }

    0
}

/// cut - remove sections from each line
pub fn prog_cut(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: cut -d DELIM -f FIELDS [FILE]\nRemove sections from each line.") {
        stdout.push_str(&help);
        return 0;
    }

    // Parse options
    let mut delimiter = '\t';
    let mut fields: Option<Vec<usize>> = None;
    let mut file: Option<&str> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "-d" if i + 1 < args.len() => {
                delimiter = args[i + 1].chars().next().unwrap_or('\t');
                i += 2;
            }
            "-f" if i + 1 < args.len() => {
                // Parse field list (e.g., "1,2,3" or "1-3")
                let field_spec = args[i + 1];
                let mut field_list = Vec::new();
                for part in field_spec.split(',') {
                    if let Some(dash_pos) = part.find('-') {
                        let start: usize = part[..dash_pos].parse().unwrap_or(1);
                        let end: usize = part[dash_pos + 1..].parse().unwrap_or(start);
                        for f in start..=end {
                            field_list.push(f);
                        }
                    } else if let Ok(f) = part.parse::<usize>() {
                        field_list.push(f);
                    }
                }
                fields = Some(field_list);
                i += 2;
            }
            s if !s.starts_with('-') => {
                file = Some(s);
                i += 1;
            }
            _ => i += 1,
        }
    }

    let fields = match fields {
        Some(f) => f,
        None => {
            stderr.push_str("cut: you must specify a list of fields\n");
            return 1;
        }
    };

    let content = if !stdin.is_empty() {
        stdin.to_string()
    } else if let Some(path) = file {
        match read_file_content(path) {
            Ok(c) => c,
            Err(e) => {
                stderr.push_str(&format!("cut: {}: {}\n", path, e));
                return 1;
            }
        }
    } else {
        String::new()
    };

    for line in content.lines() {
        let parts: Vec<&str> = line.split(delimiter).collect();
        let selected: Vec<&str> = fields.iter()
            .filter_map(|&f| parts.get(f.saturating_sub(1)))
            .copied()
            .collect();
        stdout.push_str(&selected.join(&delimiter.to_string()));
        stdout.push('\n');
    }

    0
}

/// tr - translate characters
pub fn prog_tr(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: tr SET1 SET2\nTranslate characters from SET1 to SET2.") {
        stdout.push_str(&help);
        return 0;
    }

    if args.len() < 2 {
        stderr.push_str("tr: missing operand\n");
        return 1;
    }

    let set1: Vec<char> = args[0].chars().collect();
    let set2: Vec<char> = args[1].chars().collect();

    let content = stdin.to_string();

    for ch in content.chars() {
        let translated = if let Some(pos) = set1.iter().position(|&c| c == ch) {
            set2.get(pos).copied().unwrap_or(*set2.last().unwrap_or(&ch))
        } else {
            ch
        };
        stdout.push(translated);
    }

    0
}

/// nl - number lines
pub fn prog_nl(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: nl [FILE]\nNumber lines of a file.") {
        stdout.push_str(&help);
        return 0;
    }

    let input = if let Some(file) = args.first().filter(|f| !f.starts_with('-')) {
        match read_file_content(file) {
            Ok(c) => c,
            Err(e) => {
                stderr.push_str(&format!("nl: {}: {}\n", file, e));
                return 1;
            }
        }
    } else {
        stdin.to_string()
    };

    for (i, line) in input.lines().enumerate() {
        stdout.push_str(&format!("{:6}\t{}\n", i + 1, line));
    }

    0
}

/// fold - wrap lines
pub fn prog_fold(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: fold [-w WIDTH] [FILE]\nWrap lines at specified width.\n  -w WIDTH  Width (default 80)") {
        stdout.push_str(&help);
        return 0;
    }

    let mut width: usize = 80;
    let mut i = 0;
    let mut file = None;

    while i < args.len() {
        if args[i] == "-w" && i + 1 < args.len() {
            width = args[i + 1].parse().unwrap_or(80);
            i += 2;
        } else if !args[i].starts_with('-') {
            file = Some(args[i].to_string());
            i += 1;
        } else {
            i += 1;
        }
    }

    let input = if let Some(ref f) = file {
        match read_file_content(f) {
            Ok(c) => c,
            Err(e) => {
                stderr.push_str(&format!("fold: {}: {}\n", f, e));
                return 1;
            }
        }
    } else {
        stdin.to_string()
    };

    for line in input.lines() {
        let chars: Vec<char> = line.chars().collect();
        for chunk in chars.chunks(width) {
            let s: String = chunk.iter().collect();
            stdout.push_str(&s);
            stdout.push('\n');
        }
    }

    0
}

/// paste - merge lines of files
pub fn prog_paste(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: paste FILE1 FILE2...\nMerge lines of files.") {
        stdout.push_str(&help);
        return 0;
    }

    if args.is_empty() {
        stderr.push_str("paste: requires at least one file\n");
        return 1;
    }

    let mut file_lines: Vec<Vec<String>> = Vec::new();
    let mut max_lines = 0;

    for file in &args {
        match read_file_content(file) {
            Ok(content) => {
                let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                max_lines = max_lines.max(lines.len());
                file_lines.push(lines);
            }
            Err(e) => {
                stderr.push_str(&format!("paste: {}: {}\n", file, e));
                return 1;
            }
        }
    }

    for i in 0..max_lines {
        for (j, lines) in file_lines.iter().enumerate() {
            if j > 0 { stdout.push('\t'); }
            if let Some(line) = lines.get(i) {
                stdout.push_str(line);
            }
        }
        stdout.push('\n');
    }

    0
}

/// comm - compare sorted files
pub fn prog_comm(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: comm [-123] FILE1 FILE2\nCompare sorted files line by line.\n  -1  Suppress column 1 (lines unique to FILE1)\n  -2  Suppress column 2 (lines unique to FILE2)\n  -3  Suppress column 3 (common lines)") {
        stdout.push_str(&help);
        return 0;
    }

    let suppress1 = args.iter().any(|a| a.contains('1'));
    let suppress2 = args.iter().any(|a| a.contains('2'));
    let suppress3 = args.iter().any(|a| a.contains('3'));

    let files: Vec<&str> = args.iter().filter(|a| !a.starts_with('-')).map(|s| s.as_ref()).collect();
    if files.len() < 2 {
        stderr.push_str("comm: requires two files\n");
        return 1;
    }

    let content1 = match read_file_content(files[0]) {
        Ok(c) => c,
        Err(e) => {
            stderr.push_str(&format!("comm: {}: {}\n", files[0], e));
            return 1;
        }
    };

    let content2 = match read_file_content(files[1]) {
        Ok(c) => c,
        Err(e) => {
            stderr.push_str(&format!("comm: {}: {}\n", files[1], e));
            return 1;
        }
    };

    let lines1: Vec<&str> = content1.lines().collect();
    let lines2: Vec<&str> = content2.lines().collect();

    let mut i = 0;
    let mut j = 0;

    while i < lines1.len() || j < lines2.len() {
        match (lines1.get(i), lines2.get(j)) {
            (Some(a), Some(b)) if a == b => {
                if !suppress3 {
                    let prefix = if suppress1 || suppress2 { "" } else { "\t\t" };
                    stdout.push_str(&format!("{}{}\n", prefix, a));
                }
                i += 1;
                j += 1;
            }
            (Some(a), Some(b)) if a < b => {
                if !suppress1 {
                    stdout.push_str(&format!("{}\n", a));
                }
                i += 1;
            }
            (Some(_), Some(b)) => {
                if !suppress2 {
                    let prefix = if suppress1 { "" } else { "\t" };
                    stdout.push_str(&format!("{}{}\n", prefix, b));
                }
                j += 1;
            }
            (Some(a), None) => {
                if !suppress1 {
                    stdout.push_str(&format!("{}\n", a));
                }
                i += 1;
            }
            (None, Some(b)) => {
                if !suppress2 {
                    let prefix = if suppress1 { "" } else { "\t" };
                    stdout.push_str(&format!("{}{}\n", prefix, b));
                }
                j += 1;
            }
            (None, None) => break,
        }
    }

    0
}

/// strings - print strings from binary
pub fn prog_strings(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: strings [-n MIN] [FILE]\nPrint printable strings from file.\n  -n MIN  Minimum string length (default 4)") {
        stdout.push_str(&help);
        return 0;
    }

    let mut min_len: usize = 4;
    let mut i = 0;
    let mut file = None;

    while i < args.len() {
        if args[i] == "-n" && i + 1 < args.len() {
            min_len = args[i + 1].parse().unwrap_or(4);
            i += 2;
        } else if !args[i].starts_with('-') {
            file = Some(args[i].to_string());
            i += 1;
        } else {
            i += 1;
        }
    }

    let input = if let Some(ref f) = file {
        match read_file_content(f) {
            Ok(c) => c,
            Err(e) => {
                stderr.push_str(&format!("strings: {}: {}\n", f, e));
                return 1;
            }
        }
    } else {
        stdin.to_string()
    };

    let bytes = input.as_bytes();
    let mut current = String::new();

    for byte in bytes {
        if *byte >= 0x20 && *byte < 0x7f {
            current.push(*byte as char);
        } else {
            if current.len() >= min_len {
                stdout.push_str(&current);
                stdout.push('\n');
            }
            current.clear();
        }
    }

    if current.len() >= min_len {
        stdout.push_str(&current);
        stdout.push('\n');
    }

    0
}

/// diff - compare files line by line
pub fn prog_diff(args: &[String], _stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: diff FILE1 FILE2\nCompare files line by line.") {
        stdout.push_str(&help);
        return 0;
    }

    if args.len() < 2 {
        stderr.push_str("diff: requires two files\n");
        return 1;
    }

    let file1 = args[0];
    let file2 = args[1];

    let content1 = match read_file_content(file1) {
        Ok(c) => c,
        Err(e) => {
            stderr.push_str(&format!("diff: {}: {}\n", file1, e));
            return 1;
        }
    };

    let content2 = match read_file_content(file2) {
        Ok(c) => c,
        Err(e) => {
            stderr.push_str(&format!("diff: {}: {}\n", file2, e));
            return 1;
        }
    };

    let lines1: Vec<&str> = content1.lines().collect();
    let lines2: Vec<&str> = content2.lines().collect();

    let mut has_diff = false;
    let max_len = lines1.len().max(lines2.len());

    for i in 0..max_len {
        let l1 = lines1.get(i).copied();
        let l2 = lines2.get(i).copied();

        match (l1, l2) {
            (Some(a), Some(b)) if a != b => {
                stdout.push_str(&format!("{}c{}\n", i + 1, i + 1));
                stdout.push_str(&format!("< {}\n", a));
                stdout.push_str("---\n");
                stdout.push_str(&format!("> {}\n", b));
                has_diff = true;
            }
            (Some(a), None) => {
                stdout.push_str(&format!("{}d{}\n", i + 1, lines2.len()));
                stdout.push_str(&format!("< {}\n", a));
                has_diff = true;
            }
            (None, Some(b)) => {
                stdout.push_str(&format!("{}a{}\n", lines1.len(), i + 1));
                stdout.push_str(&format!("> {}\n", b));
                has_diff = true;
            }
            _ => {}
        }
    }

    if has_diff { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prog_wc() {
        let args: Vec<String> = vec![];
        let stdin = "hello world\nfoo bar baz";
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_wc(&args, stdin, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stdout.contains("2")); // 2 lines
        assert!(stdout.contains("5")); // 5 words
    }

    #[test]
    fn test_prog_grep() {
        let args = vec!["ap".to_string()];
        let stdin = "apple\nbanana\napricot\ncherry";
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_grep(&args, stdin, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        // Strip ANSI codes for checking
        let plain: String = stdout.chars()
            .filter(|&c| c != '\x1b')
            .collect::<String>()
            .replace("[31m", "")
            .replace("[0m", "");
        assert!(plain.contains("apple"));
        assert!(plain.contains("apricot"));
    }

    #[test]
    fn test_prog_sort() {
        let args: Vec<String> = vec![];
        let stdin = "banana\napple\ncherry";
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_sort(&args, stdin, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout, "apple\nbanana\ncherry");
    }

    #[test]
    fn test_prog_uniq() {
        let args: Vec<String> = vec![];
        let stdin = "a\na\nb\nb\nb\nc";
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_uniq(&args, stdin, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout, "a\nb\nc");
    }

    #[test]
    fn test_prog_head() {
        let args = vec!["-n".to_string(), "3".to_string()];
        let stdin = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12";
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_head(&args, stdin, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout, "1\n2\n3");
    }

    #[test]
    fn test_prog_tail() {
        let args = vec!["-n".to_string(), "2".to_string()];
        let stdin = "1\n2\n3\n4\n5";
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_tail(&args, stdin, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout, "4\n5");
    }

    #[test]
    fn test_tr() {
        let args = vec!["abc".to_string(), "xyz".to_string()];
        let stdin = "abcdef";
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_tr(&args, stdin, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout, "xyzdef");
    }

    #[test]
    fn test_rev() {
        let args: Vec<String> = vec![];
        let stdin = "hello\nworld";
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_rev(&args, stdin, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stdout.contains("olleh"));
        assert!(stdout.contains("dlrow"));
    }
}
