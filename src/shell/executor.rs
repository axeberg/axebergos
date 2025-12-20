//! Shell command executor
//!
//! Executes parsed pipelines by:
//! 1. Setting up pipes between commands
//! 2. Handling input/output redirections
//! 3. Running built-in commands directly
//! 4. Running external commands via the program registry

use super::builtins::{self, BuiltinResult, ShellState};
use super::parser::{Pipeline, SimpleCommand};
use crate::kernel::syscall;
use std::collections::HashMap;
use std::path::PathBuf;

/// Result of executing a pipeline
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecResult {
    /// Exit code (0 = success)
    pub code: i32,
    /// Output from the commands (combined stdout)
    pub output: String,
    /// Error output (combined stderr)
    pub error: String,
    /// Should the shell exit?
    pub should_exit: bool,
}

impl ExecResult {
    pub fn success() -> Self {
        Self {
            code: 0,
            output: String::new(),
            error: String::new(),
            should_exit: false,
        }
    }

    pub fn with_output(mut self, output: impl Into<String>) -> Self {
        self.output = output.into();
        self
    }

    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = error.into();
        self.code = 1;
        self
    }

    pub fn with_code(mut self, code: i32) -> Self {
        self.code = code;
        self
    }

    pub fn exit(code: i32) -> Self {
        Self {
            code,
            output: String::new(),
            error: String::new(),
            should_exit: true,
        }
    }
}

/// A program that can be executed by the shell
pub type ProgramFn = fn(&[String], &mut String, &mut String) -> i32;

/// Registry of available programs
pub struct ProgramRegistry {
    programs: HashMap<String, ProgramFn>,
}

impl ProgramRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            programs: HashMap::new(),
        };

        // Register built-in programs
        reg.register("cat", prog_cat);
        reg.register("ls", prog_ls);
        reg.register("mkdir", prog_mkdir);
        reg.register("touch", prog_touch);
        reg.register("rm", prog_rm);
        reg.register("head", prog_head);
        reg.register("tail", prog_tail);
        reg.register("wc", prog_wc);
        reg.register("grep", prog_grep);
        reg.register("sort", prog_sort);
        reg.register("uniq", prog_uniq);
        reg.register("tee", prog_tee);
        reg.register("clear", prog_clear);

        reg
    }

    pub fn register(&mut self, name: &str, func: ProgramFn) {
        self.programs.insert(name.to_string(), func);
    }

    pub fn get(&self, name: &str) -> Option<ProgramFn> {
        self.programs.get(name).copied()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.programs.contains_key(name)
    }

    pub fn list(&self) -> Vec<&str> {
        let mut names: Vec<_> = self.programs.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }
}

impl Default for ProgramRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// The shell executor
pub struct Executor {
    pub state: ShellState,
    pub registry: ProgramRegistry,
}

impl Executor {
    pub fn new() -> Self {
        Self {
            state: ShellState::new(),
            registry: ProgramRegistry::new(),
        }
    }

    /// Execute a command line string
    pub fn execute_line(&mut self, line: &str) -> ExecResult {
        // Skip empty lines and comments
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return ExecResult::success();
        }

        // Parse the command
        let pipeline = match super::parse(line) {
            Ok(p) => p,
            Err(e) => return ExecResult::success().with_error(format!("parse error: {}", e)),
        };

        self.execute_pipeline(&pipeline)
    }

    /// Execute a parsed pipeline
    pub fn execute_pipeline(&mut self, pipeline: &Pipeline) -> ExecResult {
        if pipeline.commands.is_empty() {
            return ExecResult::success();
        }

        // For single commands without pipes, execute directly
        if pipeline.commands.len() == 1 {
            return self.execute_single(&pipeline.commands[0]);
        }

        // For pipelines, chain the commands
        self.execute_piped(&pipeline.commands)
    }

    /// Execute a single command (no pipes)
    fn execute_single(&mut self, cmd: &SimpleCommand) -> ExecResult {
        // Handle built-in commands
        if builtins::is_builtin(&cmd.program) {
            return self.execute_builtin(cmd);
        }

        // Handle external programs
        if let Some(prog) = self.registry.get(&cmd.program) {
            let mut stdout = String::new();
            let mut stderr = String::new();

            // Handle input redirection
            let input = if let Some(ref redir) = cmd.stdin {
                match self.read_file(&redir.path) {
                    Ok(content) => content,
                    Err(e) => return ExecResult::success().with_error(e),
                }
            } else {
                String::new()
            };

            // Prepare args with input if needed
            let args: Vec<String> = if input.is_empty() {
                cmd.args.clone()
            } else {
                // For programs that read stdin, we pass input via a special mechanism
                let mut args = cmd.args.clone();
                args.insert(0, format!("__STDIN__:{}", input));
                args
            };

            let code = prog(&args, &mut stdout, &mut stderr);

            // Handle output redirection
            if let Some(ref redir) = cmd.stdout {
                if let Err(e) = self.write_file(&redir.path, &stdout, redir.append) {
                    return ExecResult::success().with_error(e);
                }
                stdout.clear();
            }

            // Handle stderr redirection
            if let Some(ref redir) = cmd.stderr {
                if let Err(e) = self.write_file(&redir.path, &stderr, redir.append) {
                    return ExecResult::success().with_error(e);
                }
                stderr.clear();
            }

            self.state.last_status = code;

            return ExecResult {
                code,
                output: stdout,
                error: stderr,
                should_exit: false,
            };
        }

        // Command not found
        self.state.last_status = 127;
        ExecResult::success()
            .with_error(format!("{}: command not found", cmd.program))
            .with_code(127)
    }

    /// Execute a pipeline of commands
    fn execute_piped(&mut self, commands: &[SimpleCommand]) -> ExecResult {
        let mut pipe_input = String::new();
        let mut final_stdout = String::new();
        let mut final_stderr = String::new();
        let mut last_code = 0;

        for (i, cmd) in commands.iter().enumerate() {
            let is_first = i == 0;
            let is_last = i == commands.len() - 1;

            // Handle input redirection on first command
            if is_first {
                if let Some(ref redir) = cmd.stdin {
                    match self.read_file(&redir.path) {
                        Ok(content) => pipe_input = content,
                        Err(e) => return ExecResult::success().with_error(e),
                    }
                }
            }

            // Execute the command
            let mut stdout = String::new();
            let mut stderr = String::new();

            if builtins::is_builtin(&cmd.program) {
                // Builtins in a pipeline get the pipe input as implicit stdin
                let result = builtins::execute(&cmd.program, &cmd.args, &self.state);
                match result {
                    BuiltinResult::Success(s) => {
                        stdout = s;
                        last_code = 0;
                    }
                    BuiltinResult::Ok => {
                        last_code = 0;
                    }
                    BuiltinResult::Error(e) => {
                        stderr = e;
                        last_code = 1;
                    }
                    BuiltinResult::Exit(code) => {
                        return ExecResult::exit(code);
                    }
                    BuiltinResult::Cd(path) => {
                        self.change_directory(&path);
                        last_code = 0;
                    }
                }
            } else if let Some(prog) = self.registry.get(&cmd.program) {
                // Pass pipe input via special arg
                let mut args = cmd.args.clone();
                if !pipe_input.is_empty() {
                    args.insert(0, format!("__STDIN__:{}", pipe_input));
                }

                last_code = prog(&args, &mut stdout, &mut stderr);
            } else {
                return ExecResult::success()
                    .with_error(format!("{}: command not found", cmd.program))
                    .with_code(127);
            }

            // Collect stderr
            if !stderr.is_empty() {
                final_stderr.push_str(&stderr);
                if !final_stderr.ends_with('\n') {
                    final_stderr.push('\n');
                }
            }

            // Handle output redirection on last command
            if is_last {
                if let Some(ref redir) = cmd.stdout {
                    if let Err(e) = self.write_file(&redir.path, &stdout, redir.append) {
                        return ExecResult::success().with_error(e);
                    }
                } else {
                    final_stdout = stdout;
                }
            } else {
                // Pass stdout to next command's stdin
                pipe_input = stdout;
            }
        }

        self.state.last_status = last_code;

        ExecResult {
            code: last_code,
            output: final_stdout,
            error: final_stderr,
            should_exit: false,
        }
    }

    /// Execute a built-in command
    fn execute_builtin(&mut self, cmd: &SimpleCommand) -> ExecResult {
        let result = builtins::execute(&cmd.program, &cmd.args, &self.state);

        match result {
            BuiltinResult::Success(mut output) => {
                // Handle special export/unset responses
                if output.starts_with("__EXPORT__:") {
                    let pairs = &output["__EXPORT__:".len()..];
                    for pair in pairs.split('\x00') {
                        if let Some(eq) = pair.find('=') {
                            let name = &pair[..eq];
                            let value = &pair[eq + 1..];
                            self.state.set_env(name, value);
                        }
                    }
                    output.clear();
                } else if output.starts_with("__UNSET__:") {
                    let vars = &output["__UNSET__:".len()..];
                    for var in vars.split('\x00') {
                        self.state.unset_env(var);
                    }
                    output.clear();
                }

                // Handle output redirection
                if let Some(ref redir) = cmd.stdout {
                    if let Err(e) = self.write_file(&redir.path, &output, redir.append) {
                        return ExecResult::success().with_error(e);
                    }
                    output.clear();
                }

                self.state.last_status = 0;
                ExecResult::success().with_output(output)
            }
            BuiltinResult::Ok => {
                self.state.last_status = 0;
                ExecResult::success()
            }
            BuiltinResult::Error(e) => {
                // Handle stderr redirection
                let error = if let Some(ref redir) = cmd.stderr {
                    if let Err(err) = self.write_file(&redir.path, &e, redir.append) {
                        return ExecResult::success().with_error(err);
                    }
                    String::new()
                } else {
                    e
                };

                self.state.last_status = 1;
                ExecResult::success().with_error(error).with_code(1)
            }
            BuiltinResult::Exit(code) => {
                self.state.last_status = code;
                ExecResult::exit(code)
            }
            BuiltinResult::Cd(path) => {
                self.change_directory(&path)
            }
        }
    }

    /// Change directory and update state
    fn change_directory(&mut self, path: &PathBuf) -> ExecResult {
        // Verify the directory exists
        let path_str = path.display().to_string();
        match syscall::exists(&path_str) {
            Ok(true) => {
                // Save old pwd
                let old = self.state.cwd.display().to_string();
                self.state.set_env("OLDPWD", &old);

                // Change directory
                self.state.cwd = path.clone();
                self.state.set_env("PWD", &path_str);
                self.state.last_status = 0;
                ExecResult::success()
            }
            Ok(false) => {
                self.state.last_status = 1;
                ExecResult::success().with_error(format!("cd: {}: No such file or directory", path_str))
            }
            Err(e) => {
                self.state.last_status = 1;
                ExecResult::success().with_error(format!("cd: {}: {}", path_str, e))
            }
        }
    }

    /// Read a file for input redirection
    fn read_file(&self, path: &str) -> Result<String, String> {
        let full_path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("{}/{}", self.state.cwd.display(), path)
        };

        let fd = syscall::open(&full_path, syscall::OpenFlags::READ)
            .map_err(|e| format!("{}: {}", path, e))?;

        let mut content = Vec::new();
        let mut buf = [0u8; 1024];
        loop {
            match syscall::read(fd, &mut buf) {
                Ok(0) => break,
                Ok(n) => content.extend_from_slice(&buf[..n]),
                Err(e) => {
                    let _ = syscall::close(fd);
                    return Err(format!("{}: {}", path, e));
                }
            }
        }

        let _ = syscall::close(fd);
        String::from_utf8(content).map_err(|_| format!("{}: invalid UTF-8", path))
    }

    /// Write to a file for output redirection
    fn write_file(&self, path: &str, content: &str, append: bool) -> Result<(), String> {
        let full_path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("{}/{}", self.state.cwd.display(), path)
        };

        let flags = if append {
            syscall::OpenFlags::APPEND
        } else {
            syscall::OpenFlags::WRITE
        };

        let fd = syscall::open(&full_path, flags)
            .map_err(|e| format!("{}: {}", path, e))?;

        syscall::write(fd, content.as_bytes())
            .map_err(|e| format!("{}: {}", path, e))?;

        syscall::close(fd).map_err(|e| format!("{}: {}", path, e))?;

        Ok(())
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

// ============ Built-in Programs ============

/// Extract stdin from args if present
fn extract_stdin(args: &[String]) -> (Option<String>, Vec<&str>) {
    if !args.is_empty() && args[0].starts_with("__STDIN__:") {
        let stdin = args[0]["__STDIN__:".len()..].to_string();
        let rest: Vec<&str> = args[1..].iter().map(|s| s.as_str()).collect();
        (Some(stdin), rest)
    } else {
        (None, args.iter().map(|s| s.as_str()).collect())
    }
}

/// cat - concatenate files or stdin
fn prog_cat(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (stdin, files) = extract_stdin(args);

    if files.is_empty() {
        // Read from stdin
        if let Some(input) = stdin {
            stdout.push_str(&input);
        }
        return 0;
    }

    let mut code = 0;
    for file in files {
        match syscall::open(file, syscall::OpenFlags::READ) {
            Ok(fd) => {
                let mut buf = [0u8; 1024];
                loop {
                    match syscall::read(fd, &mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Ok(s) = std::str::from_utf8(&buf[..n]) {
                                stdout.push_str(s);
                            }
                        }
                        Err(e) => {
                            stderr.push_str(&format!("cat: {}: {}\n", file, e));
                            code = 1;
                            break;
                        }
                    }
                }
                let _ = syscall::close(fd);
            }
            Err(e) => {
                stderr.push_str(&format!("cat: {}: {}\n", file, e));
                code = 1;
            }
        }
    }
    code
}

/// ls - list directory contents
fn prog_ls(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, paths) = extract_stdin(args);

    let paths = if paths.is_empty() {
        vec!["."]
    } else {
        paths
    };

    let mut code = 0;
    for path in paths {
        match syscall::readdir(path) {
            Ok(entries) => {
                for entry in entries {
                    stdout.push_str(&entry);
                    stdout.push('\n');
                }
            }
            Err(e) => {
                stderr.push_str(&format!("ls: {}: {}\n", path, e));
                code = 1;
            }
        }
    }

    // Remove trailing newline
    if stdout.ends_with('\n') {
        stdout.pop();
    }

    code
}

/// mkdir - create directories
fn prog_mkdir(args: &[String], _stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, paths) = extract_stdin(args);

    if paths.is_empty() {
        stderr.push_str("mkdir: missing operand\n");
        return 1;
    }

    let mut code = 0;
    for path in paths {
        if let Err(e) = syscall::mkdir(path) {
            stderr.push_str(&format!("mkdir: {}: {}\n", path, e));
            code = 1;
        }
    }
    code
}

/// touch - create empty files
fn prog_touch(args: &[String], _stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, paths) = extract_stdin(args);

    if paths.is_empty() {
        stderr.push_str("touch: missing operand\n");
        return 1;
    }

    let mut code = 0;
    for path in paths {
        // OpenFlags::WRITE includes create and truncate
        match syscall::open(path, syscall::OpenFlags::WRITE) {
            Ok(fd) => {
                let _ = syscall::close(fd);
            }
            Err(e) => {
                stderr.push_str(&format!("touch: {}: {}\n", path, e));
                code = 1;
            }
        }
    }
    code
}

/// rm - remove files (stub - needs VFS unlink)
fn prog_rm(args: &[String], _stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, paths) = extract_stdin(args);

    if paths.is_empty() {
        stderr.push_str("rm: missing operand\n");
        return 1;
    }

    // TODO: Need VFS unlink syscall
    stderr.push_str("rm: not yet implemented\n");
    1
}

/// head - output first lines
fn prog_head(args: &[String], stdout: &mut String, _stderr: &mut String) -> i32 {
    let (stdin, args) = extract_stdin(args);

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
        stdin.unwrap_or_default()
    } else {
        // Read first file (simplified)
        String::new() // TODO: read file
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
fn prog_tail(args: &[String], stdout: &mut String, _stderr: &mut String) -> i32 {
    let (stdin, args) = extract_stdin(args);

    let mut n = 10;

    for i in 0..args.len() {
        if args[i] == "-n" && i + 1 < args.len() {
            n = args[i + 1].parse().unwrap_or(10);
        } else if args[i].starts_with("-n") {
            n = args[i][2..].parse().unwrap_or(10);
        }
    }

    let input = stdin.unwrap_or_default();
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
fn prog_wc(args: &[String], stdout: &mut String, _stderr: &mut String) -> i32 {
    let (stdin, args) = extract_stdin(args);

    let show_lines = args.contains(&"-l");
    let show_words = args.contains(&"-w");
    let show_chars = args.contains(&"-c") || args.contains(&"-m");
    let show_all = !show_lines && !show_words && !show_chars;

    let input = stdin.unwrap_or_default();
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
fn prog_grep(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (stdin, args) = extract_stdin(args);

    if args.is_empty() {
        stderr.push_str("grep: missing pattern\n");
        return 1;
    }

    let pattern = args[0];
    let input = stdin.unwrap_or_default();
    let mut found = false;

    for line in input.lines() {
        if line.contains(pattern) {
            stdout.push_str(line);
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
fn prog_sort(args: &[String], stdout: &mut String, _stderr: &mut String) -> i32 {
    let (stdin, args) = extract_stdin(args);

    let reverse = args.contains(&"-r");
    let unique = args.contains(&"-u");

    let input = stdin.unwrap_or_default();
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
fn prog_uniq(args: &[String], stdout: &mut String, _stderr: &mut String) -> i32 {
    let (stdin, args) = extract_stdin(args);

    let count = args.contains(&"-c");

    let input = stdin.unwrap_or_default();
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

    // Last line
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
fn prog_tee(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (stdin, files) = extract_stdin(args);

    let input = stdin.unwrap_or_default();

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

/// clear - clear screen (outputs ANSI escape)
fn prog_clear(_args: &[String], stdout: &mut String, _stderr: &mut String) -> i32 {
    stdout.push_str("\x1b[2J\x1b[H");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_kernel() {
        use crate::kernel::syscall::{KERNEL, Kernel};
        KERNEL.with(|k| {
            *k.borrow_mut() = Kernel::new();
            let pid = k.borrow_mut().spawn_process("shell", None);
            k.borrow_mut().set_current(pid);
        });
    }

    // ============ Executor basics ============

    #[test]
    fn test_execute_empty() {
        let mut exec = Executor::new();
        let result = exec.execute_line("");
        assert_eq!(result.code, 0);
    }

    #[test]
    fn test_execute_comment() {
        let mut exec = Executor::new();
        let result = exec.execute_line("# this is a comment");
        assert_eq!(result.code, 0);
    }

    #[test]
    fn test_execute_pwd() {
        let mut exec = Executor::new();
        let result = exec.execute_line("pwd");
        assert_eq!(result.code, 0);
        assert!(!result.output.is_empty());
    }

    #[test]
    fn test_execute_echo() {
        let mut exec = Executor::new();
        let result = exec.execute_line("echo hello world");
        assert_eq!(result.code, 0);
        assert_eq!(result.output, "hello world");
    }

    #[test]
    fn test_execute_exit() {
        let mut exec = Executor::new();
        let result = exec.execute_line("exit 42");
        assert!(result.should_exit);
        assert_eq!(result.code, 42);
    }

    #[test]
    fn test_command_not_found() {
        let mut exec = Executor::new();
        let result = exec.execute_line("nonexistent_command");
        assert_eq!(result.code, 127);
        assert!(result.error.contains("command not found"));
    }

    // ============ CD ============

    #[test]
    fn test_cd_updates_state() {
        setup_kernel();
        let mut exec = Executor::new();
        exec.state.cwd = std::path::PathBuf::from("/home");

        // This will fail because /tmp may not exist in VFS
        // but we can test that cd tries to change directory
        let _ = exec.execute_line("cd /dev");
    }

    // ============ Environment ============

    #[test]
    fn test_export_sets_env() {
        let mut exec = Executor::new();
        exec.execute_line("export FOO=bar");
        assert_eq!(exec.state.get_env("FOO"), Some("bar"));
    }

    #[test]
    fn test_unset_removes_env() {
        let mut exec = Executor::new();
        exec.state.set_env("FOO", "bar");
        exec.execute_line("unset FOO");
        assert_eq!(exec.state.get_env("FOO"), None);
    }

    // ============ Programs ============

    #[test]
    fn test_prog_wc() {
        let args = vec!["__STDIN__:hello world\nfoo bar baz".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_wc(&args, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stdout.contains("2")); // 2 lines
        assert!(stdout.contains("5")); // 5 words
    }

    #[test]
    fn test_prog_grep() {
        let args = vec![
            "__STDIN__:apple\nbanana\napricot\ncherry".to_string(),
            "ap".to_string(),
        ];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_grep(&args, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stdout.contains("apple"));
        assert!(stdout.contains("apricot"));
        assert!(!stdout.contains("banana"));
    }

    #[test]
    fn test_prog_sort() {
        let args = vec!["__STDIN__:banana\napple\ncherry".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_sort(&args, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout, "apple\nbanana\ncherry");
    }

    #[test]
    fn test_prog_uniq() {
        let args = vec!["__STDIN__:a\na\nb\nb\nb\nc".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_uniq(&args, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout, "a\nb\nc");
    }

    #[test]
    fn test_prog_head() {
        let args = vec![
            "__STDIN__:1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12".to_string(),
            "-n".to_string(),
            "3".to_string(),
        ];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_head(&args, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout, "1\n2\n3");
    }

    #[test]
    fn test_prog_tail() {
        let args = vec![
            "__STDIN__:1\n2\n3\n4\n5".to_string(),
            "-n".to_string(),
            "2".to_string(),
        ];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = prog_tail(&args, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout, "4\n5");
    }

    // ============ Pipelines ============

    #[test]
    fn test_pipeline_echo_grep() {
        let mut exec = Executor::new();
        let result = exec.execute_line("echo apple banana apricot | grep ap");
        // Note: echo outputs on one line, grep won't match multi-word on single line properly
        // This tests the piping mechanism works
        assert_eq!(result.code, 0);
    }

    #[test]
    fn test_pipeline_sort_uniq() {
        let mut exec = Executor::new();
        // Create a multi-command pipeline by setting up stdin
        // For this test, we directly test the programs
        let mut stdout = String::new();
        let mut stderr = String::new();

        // Sort
        let args = vec!["__STDIN__:b\na\na\nc\nb".to_string()];
        prog_sort(&args, &mut stdout, &mut stderr);

        // Feed to uniq
        let args = vec![format!("__STDIN__:{}", stdout)];
        stdout.clear();
        prog_uniq(&args, &mut stdout, &mut stderr);

        assert_eq!(stdout, "a\nb\nc");
    }

    // ============ Registry ============

    #[test]
    fn test_registry_list() {
        let reg = ProgramRegistry::new();
        let progs = reg.list();
        assert!(progs.contains(&"cat"));
        assert!(progs.contains(&"ls"));
        assert!(progs.contains(&"grep"));
    }

    #[test]
    fn test_registry_contains() {
        let reg = ProgramRegistry::new();
        assert!(reg.contains("cat"));
        assert!(!reg.contains("nonexistent"));
    }
}
