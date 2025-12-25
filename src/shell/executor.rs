//! Shell command executor
//!
//! Executes parsed pipelines by:
//! 1. Setting up pipes between commands
//! 2. Handling input/output redirections
//! 3. Running built-in commands directly
//! 4. Running external commands via the program registry

use super::builtins::{self, BuiltinResult, ShellState};
use super::parser::{CommandList, LogicalOp, Pipeline, SimpleCommand};
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
///
/// Parameters:
/// - args: Command line arguments (not including stdin data)
/// - stdin: Standard input data (from pipe or input redirection)
/// - stdout: Buffer for standard output
/// - stderr: Buffer for standard error
///
/// Returns: Exit code (0 for success)
pub type ProgramFn = fn(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32;

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
        reg.register("cp", prog_cp);
        reg.register("mv", prog_mv);
        reg.register("head", prog_head);
        reg.register("tail", prog_tail);
        reg.register("wc", prog_wc);
        reg.register("grep", prog_grep);
        reg.register("sort", prog_sort);
        reg.register("uniq", prog_uniq);
        reg.register("tee", prog_tee);
        reg.register("clear", prog_clear);
        reg.register("save", prog_save);
        reg.register("fsload", prog_fsload);
        reg.register("fsreset", prog_fsreset);
        reg.register("autosave", prog_autosave);
        reg.register("curl", prog_curl);
        reg.register("wget", prog_wget);
        reg.register("tree", prog_tree);
        reg.register("sleep", prog_sleep);
        reg.register("history", prog_history);
        reg.register("ln", prog_ln);
        reg.register("readlink", prog_readlink);
        reg.register("edit", prog_edit);
        reg.register("man", prog_man);
        reg.register("printenv", prog_printenv);
        reg.register("id", prog_id);
        reg.register("jobs", prog_jobs);
        reg.register("fg", prog_fg);
        reg.register("bg", prog_bg);
        reg.register("strace", prog_strace);
        reg.register("whoami", prog_whoami);
        reg.register("hostname", prog_hostname);
        reg.register("uname", prog_uname);
        reg.register("find", prog_find);
        reg.register("du", prog_du);
        reg.register("df", prog_df);
        reg.register("ps", prog_ps);
        reg.register("kill", prog_kill);
        reg.register("time", prog_time);
        reg.register("date", prog_date);
        reg.register("seq", prog_seq);
        reg.register("yes", prog_yes);
        reg.register("basename", prog_basename);
        reg.register("dirname", prog_dirname);
        reg.register("rev", prog_rev);
        reg.register("cut", prog_cut);
        reg.register("tr", prog_tr);
        reg.register("xargs", prog_xargs);
        reg.register("cal", prog_cal);
        reg.register("printf", prog_printf);
        reg.register("test", prog_test);
        reg.register("[", prog_test);  // [ is an alias for test
        reg.register("expr", prog_expr);
        reg.register("which", prog_which);
        reg.register("type", prog_type);
        reg.register("uptime", prog_uptime);
        reg.register("free", prog_free);
        reg.register("diff", prog_diff);
        reg.register("base64", prog_base64);
        reg.register("xxd", prog_xxd);
        reg.register("nl", prog_nl);
        reg.register("fold", prog_fold);
        reg.register("paste", prog_paste);
        reg.register("comm", prog_comm);
        reg.register("strings", prog_strings);
        reg.register("groups", prog_groups);
        reg.register("su", prog_su);
        reg.register("sudo", prog_sudo);
        reg.register("useradd", prog_useradd);
        reg.register("groupadd", prog_groupadd);
        reg.register("passwd", prog_passwd);
        reg.register("chmod", prog_chmod);
        reg.register("chown", prog_chown);
        reg.register("chgrp", prog_chgrp);
        reg.register("systemctl", prog_systemctl);
        reg.register("reboot", prog_reboot);
        reg.register("poweroff", prog_poweroff);

        // IPC commands
        reg.register("mkfifo", prog_mkfifo);
        reg.register("ipcs", prog_ipcs);
        reg.register("ipcrm", prog_ipcrm);

        // Mount commands
        reg.register("mount", prog_mount);
        reg.register("umount", prog_umount);
        reg.register("findmnt", prog_findmnt);

        // TTY commands
        reg.register("stty", prog_stty);
        reg.register("tty", prog_tty);

        // Package manager commands
        reg.register("pkg", prog_pkg);

        // User login commands
        reg.register("login", prog_login);
        reg.register("logout", prog_logout);
        // Note: su and sudo are already registered above with user management commands
        reg.register("who", prog_who);
        reg.register("w", prog_w);

        // Cron/scheduling commands
        reg.register("crontab", prog_crontab);
        reg.register("at", prog_at);

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
        let state = ShellState::new();
        // Sync kernel process cwd with shell's initial cwd
        if let Err(_e) = syscall::chdir(&state.cwd.display().to_string()) {
            #[cfg(all(target_arch = "wasm32", not(test)))]
            crate::console_log!("[shell] Warning: Failed to set initial cwd: {:?}", _e);
        }
        Self {
            state,
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

        // Expand aliases in the line
        let line = self.expand_aliases(line);

        // Expand command substitution $(cmd) and `cmd` in the line BEFORE parsing
        let line = self.expand_substitution_in_line(&line);

        #[cfg(all(target_arch = "wasm32", not(test)))]
        crate::console_log!("[exec] Running: {}", line);

        // Parse the command list (handles &&, ||, ;)
        let cmd_list = match super::parser::parse_command_list(&line) {
            Ok(c) => c,
            Err(e) => return ExecResult::success().with_error(format!("parse error: {}", e)),
        };

        let result = self.execute_command_list(&cmd_list);

        #[cfg(all(target_arch = "wasm32", not(test)))]
        if !result.error.is_empty() {
            crate::console_log!("[exec] Error: {}", result.error);
        }

        result
    }

    /// Execute a command list (multiple pipelines with &&, ||, ;)
    pub fn execute_command_list(&mut self, cmd_list: &CommandList) -> ExecResult {
        // Execute the first pipeline
        let mut result = self.execute_pipeline(&cmd_list.first);

        // Short-circuit on exit
        if result.should_exit {
            return result;
        }

        // Execute remaining pipelines based on logical operators
        for (op, pipeline) in &cmd_list.rest {
            let should_execute = match op {
                LogicalOp::Sequence => true, // Always execute
                LogicalOp::And => result.code == 0, // Execute if previous succeeded
                LogicalOp::Or => result.code != 0, // Execute if previous failed
            };

            if should_execute {
                let next_result = self.execute_pipeline(pipeline);

                // Combine outputs
                if !result.output.is_empty() && !next_result.output.is_empty() {
                    result.output.push('\n');
                }
                result.output.push_str(&next_result.output);

                if !result.error.is_empty() && !next_result.error.is_empty() {
                    result.error.push('\n');
                }
                result.error.push_str(&next_result.error);

                // Update exit code to the last executed command
                result.code = next_result.code;

                // Short-circuit on exit
                if next_result.should_exit {
                    result.should_exit = true;
                    return result;
                }
            }
        }

        result
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
            let stdin = if let Some(ref redir) = cmd.stdin {
                match self.read_file(&redir.path) {
                    Ok(content) => content,
                    Err(e) => return ExecResult::success().with_error(e),
                }
            } else {
                String::new()
            };

            // Expand glob patterns in arguments
            let args = self.expand_args(&cmd.args);

            // Execute program with stdin passed directly
            let code = prog(&args, &stdin, &mut stdout, &mut stderr);

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

            // Expand glob patterns in arguments
            let expanded_args = self.expand_args(&cmd.args);

            if builtins::is_builtin(&cmd.program) {
                // Builtins in a pipeline get the pipe input as implicit stdin
                let result = builtins::execute(&cmd.program, &expanded_args, &self.state);
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
                    BuiltinResult::Export(pairs) => {
                        for (name, value) in pairs {
                            self.state.set_env(&name, &value);
                        }
                        last_code = 0;
                    }
                    BuiltinResult::Unset(vars) => {
                        for var in vars {
                            self.state.unset_env(&var);
                        }
                        last_code = 0;
                    }
                    BuiltinResult::SetAlias(pairs) => {
                        for (name, value) in pairs {
                            self.state.set_alias(&name, &value);
                        }
                        last_code = 0;
                    }
                    BuiltinResult::UnsetAlias(names) => {
                        for name in names {
                            self.state.unalias(&name);
                        }
                        last_code = 0;
                    }
                }
            } else if let Some(prog) = self.registry.get(&cmd.program) {
                // Pass pipe input directly via stdin parameter
                last_code = prog(&expanded_args, &pipe_input, &mut stdout, &mut stderr);
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
        // Expand glob patterns in arguments
        let expanded_args = self.expand_args(&cmd.args);
        let result = builtins::execute(&cmd.program, &expanded_args, &self.state);

        match result {
            BuiltinResult::Success(output) => {
                // Handle output redirection
                let final_output = if let Some(ref redir) = cmd.stdout {
                    if let Err(e) = self.write_file(&redir.path, &output, redir.append) {
                        return ExecResult::success().with_error(e);
                    }
                    String::new()
                } else {
                    output
                };

                self.state.last_status = 0;
                ExecResult::success().with_output(final_output)
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
            BuiltinResult::Export(pairs) => {
                for (name, value) in pairs {
                    self.state.set_env(&name, &value);
                }
                self.state.last_status = 0;
                ExecResult::success()
            }
            BuiltinResult::Unset(vars) => {
                for var in vars {
                    self.state.unset_env(&var);
                }
                self.state.last_status = 0;
                ExecResult::success()
            }
            BuiltinResult::SetAlias(pairs) => {
                for (name, value) in pairs {
                    self.state.set_alias(&name, &value);
                }
                self.state.last_status = 0;
                ExecResult::success()
            }
            BuiltinResult::UnsetAlias(names) => {
                for name in names {
                    self.state.unalias(&name);
                }
                self.state.last_status = 0;
                ExecResult::success()
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

                // Update kernel process cwd (for relative path resolution)
                if let Err(e) = syscall::chdir(&path_str) {
                    self.state.last_status = 1;
                    return ExecResult::success().with_error(format!("cd: {}: {}", path_str, e));
                }

                // Change directory in shell state
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

    /// Expand glob patterns in arguments
    fn expand_args(&self, args: &[String]) -> Vec<String> {
        let mut expanded = Vec::new();
        for arg in args {
            if is_glob_pattern(arg) {
                let matches = expand_glob(arg, &self.state.cwd.display().to_string());
                if matches.is_empty() {
                    // No match - keep the original pattern (bash behavior)
                    expanded.push(arg.clone());
                } else {
                    expanded.extend(matches);
                }
            } else {
                expanded.push(arg.clone());
            }
        }
        expanded
    }

    /// Expand command substitution in a full line (before parsing)
    fn expand_substitution_in_line(&mut self, line: &str) -> String {
        self.expand_substitution_in_arg(line)
    }

    /// Expand command substitutions in a single argument/string
    fn expand_substitution_in_arg(&mut self, arg: &str) -> String {
        let mut result = String::new();
        let mut chars = arg.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '$' && chars.peek() == Some(&'(') {
                // $(...) substitution
                chars.next(); // consume '('
                if let Some(cmd) = self.extract_nested_paren(&mut chars) {
                    let output = self.execute_substitution(&cmd);
                    result.push_str(&output);
                } else {
                    // Malformed - just keep it as-is
                    result.push_str("$(");
                }
            } else if c == '`' {
                // Backtick substitution
                let mut cmd = String::new();
                let mut found_closing = false;
                while let Some(bc) = chars.next() {
                    if bc == '`' {
                        found_closing = true;
                        break;
                    }
                    // Handle escaped backtick
                    if bc == '\\' && chars.peek() == Some(&'`') {
                        cmd.push(chars.next().unwrap());
                    } else {
                        cmd.push(bc);
                    }
                }
                if found_closing {
                    let output = self.execute_substitution(&cmd);
                    result.push_str(&output);
                } else {
                    // Malformed - keep as-is
                    result.push('`');
                    result.push_str(&cmd);
                }
            } else {
                result.push(c);
            }
        }

        result
    }

    /// Extract content from nested parentheses, handling nesting
    fn extract_nested_paren(&self, chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<String> {
        let mut content = String::new();
        let mut depth = 1;

        while let Some(c) = chars.next() {
            match c {
                '(' => {
                    depth += 1;
                    content.push(c);
                }
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(content);
                    }
                    content.push(c);
                }
                _ => content.push(c),
            }
        }

        None // Unbalanced
    }

    /// Execute a command for substitution and return its output
    fn execute_substitution(&mut self, cmd: &str) -> String {
        // Recursively expand any nested substitutions first
        let expanded_cmd = self.expand_substitution_in_line(cmd);

        // Parse and execute the command
        match super::parser::parse(&expanded_cmd) {
            Ok(pipeline) => {
                let result = self.execute_pipeline(&pipeline);
                // Trim trailing newline for substitution (bash behavior)
                result.output.trim_end_matches('\n').to_string()
            }
            Err(_) => String::new(),
        }
    }

    /// Expand aliases in a command line
    fn expand_aliases(&self, line: &str) -> String {
        // Split line into potential command segments (separated by |, ;, &&, ||)
        // For simplicity, we'll just expand the first word of each pipe segment
        let mut result = String::new();
        let mut current_segment = String::new();
        let mut in_quote = false;
        let mut quote_char = ' ';
        let mut chars = line.chars().peekable();

        while let Some(c) = chars.next() {
            match c {
                '"' | '\'' if !in_quote => {
                    in_quote = true;
                    quote_char = c;
                    current_segment.push(c);
                }
                c if in_quote && c == quote_char => {
                    in_quote = false;
                    current_segment.push(c);
                }
                '|' | ';' if !in_quote => {
                    // End of segment, expand and add
                    result.push_str(&self.expand_alias_in_segment(&current_segment));
                    result.push(c);
                    current_segment.clear();
                }
                '&' if !in_quote && chars.peek() == Some(&'&') => {
                    chars.next();
                    result.push_str(&self.expand_alias_in_segment(&current_segment));
                    result.push_str("&&");
                    current_segment.clear();
                }
                _ => {
                    current_segment.push(c);
                }
            }
        }

        // Handle last segment
        if !current_segment.is_empty() {
            result.push_str(&self.expand_alias_in_segment(&current_segment));
        }

        result
    }

    /// Expand alias in a single command segment
    fn expand_alias_in_segment(&self, segment: &str) -> String {
        let trimmed = segment.trim_start();
        if trimmed.is_empty() {
            return segment.to_string();
        }

        // Find the first word (the command)
        let first_word_end = trimmed
            .find(|c: char| c.is_whitespace())
            .unwrap_or(trimmed.len());
        let first_word = &trimmed[..first_word_end];
        let rest = &trimmed[first_word_end..];

        // Check if it's an alias
        if let Some(alias_value) = self.state.aliases.get(first_word) {
            // Preserve leading whitespace from original segment
            let leading_ws = &segment[..segment.len() - trimmed.len()];
            format!("{}{}{}", leading_ws, alias_value, rest)
        } else {
            segment.to_string()
        }
    }
}

/// Check if a string contains glob pattern characters
fn is_glob_pattern(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[')
}

/// Match a pattern against a filename (not full path)
fn glob_match(pattern: &str, name: &str) -> bool {
    glob_match_chars(&mut pattern.chars().peekable(), &mut name.chars().peekable())
}

fn glob_match_chars(
    pattern: &mut std::iter::Peekable<std::str::Chars>,
    name: &mut std::iter::Peekable<std::str::Chars>,
) -> bool {
    while let Some(p) = pattern.next() {
        match p {
            '*' => {
                // Handle ** for recursive matching
                if pattern.peek() == Some(&'*') {
                    pattern.next();
                    // ** matches any characters including /
                    // Try matching rest of pattern at every position
                    let rest_pattern: String = pattern.collect();
                    let rest_name: String = name.collect();
                    if rest_pattern.is_empty() {
                        return true;
                    }
                    for i in 0..=rest_name.len() {
                        if glob_match(&rest_pattern, &rest_name[i..]) {
                            return true;
                        }
                    }
                    return false;
                }
                // * matches zero or more characters except /
                let rest_pattern: String = pattern.collect();
                if rest_pattern.is_empty() {
                    // Pattern ends with * - match if name has no more /
                    return !name.any(|c| c == '/');
                }
                // Try matching rest at every position
                let rest_name: String = name.collect();
                for (i, c) in rest_name.char_indices() {
                    if c == '/' {
                        // Can't match across /
                        return glob_match(&rest_pattern, &rest_name[i..]);
                    }
                    if glob_match(&rest_pattern, &rest_name[i..]) {
                        return true;
                    }
                }
                return glob_match(&rest_pattern, "");
            }
            '?' => {
                // ? matches any single character except /
                match name.next() {
                    Some(c) if c != '/' => continue,
                    _ => return false,
                }
            }
            '[' => {
                // Character class
                let mut chars_in_class = Vec::new();
                let mut negated = false;

                if pattern.peek() == Some(&'!') || pattern.peek() == Some(&'^') {
                    negated = true;
                    pattern.next();
                }

                while let Some(c) = pattern.next() {
                    if c == ']' {
                        break;
                    }
                    if c == '-' && !chars_in_class.is_empty() && pattern.peek() != Some(&']') {
                        // Range
                        if let Some(end) = pattern.next() {
                            let start = *chars_in_class.last().unwrap();
                            for ch in start..=end {
                                chars_in_class.push(ch);
                            }
                        }
                    } else {
                        chars_in_class.push(c);
                    }
                }

                match name.next() {
                    Some(c) => {
                        let in_class = chars_in_class.contains(&c);
                        if negated == in_class {
                            return false;
                        }
                    }
                    None => return false,
                }
            }
            c => {
                // Literal character
                if name.next() != Some(c) {
                    return false;
                }
            }
        }
    }
    // Pattern consumed - name should also be consumed
    name.next().is_none()
}

/// Expand a glob pattern against the filesystem
fn expand_glob(pattern: &str, cwd: &str) -> Vec<String> {
    let mut results = Vec::new();

    // Determine base path and pattern
    let (base, pat) = if pattern.starts_with('/') {
        // Absolute path
        let parts: Vec<&str> = pattern.splitn(2, |c| c == '*' || c == '?' || c == '[').collect();
        if parts.len() == 1 {
            // No glob chars - just return if exists
            if syscall::exists(pattern).unwrap_or(false) {
                return vec![pattern.to_string()];
            }
            return vec![];
        }
        // Find the last / before the glob
        let prefix = parts[0];
        let last_slash = prefix.rfind('/').unwrap_or(0);
        (&pattern[..=last_slash], &pattern[last_slash + 1..])
    } else {
        // Relative path
        (cwd, pattern)
    };

    // Handle recursive patterns (**)
    if pat.contains("**") {
        expand_glob_recursive(base, pat, &mut results);
    } else {
        expand_glob_simple(base, pat, &mut results);
    }

    results.sort();
    results
}

/// Expand a simple glob pattern (no **) in a directory
fn expand_glob_simple(dir: &str, pattern: &str, results: &mut Vec<String>) {
    // Split pattern into segments
    let segments: Vec<&str> = pattern.split('/').collect();
    expand_glob_segments(dir, &segments, results);
}

fn expand_glob_segments(dir: &str, segments: &[&str], results: &mut Vec<String>) {
    if segments.is_empty() {
        return;
    }

    let segment = segments[0];
    let remaining = &segments[1..];

    // List directory
    let entries = match syscall::readdir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries {
        // Skip . and ..
        if entry == "." || entry == ".." {
            continue;
        }

        // Check if entry matches the segment pattern
        if glob_match(segment, &entry) {
            let path = if dir.ends_with('/') {
                format!("{}{}", dir, entry)
            } else {
                format!("{}/{}", dir, entry)
            };

            if remaining.is_empty() {
                results.push(path);
            } else {
                // Check if it's a directory for further matching
                if let Ok(meta) = syscall::metadata(&path) {
                    if meta.is_dir {
                        expand_glob_segments(&path, remaining, results);
                    }
                }
            }
        }
    }
}

/// Expand a recursive glob pattern (**)
fn expand_glob_recursive(base: &str, pattern: &str, results: &mut Vec<String>) {
    // Split on ** to get prefix and suffix
    let parts: Vec<&str> = pattern.splitn(2, "**").collect();
    let prefix = parts[0].trim_end_matches('/');
    let suffix = if parts.len() > 1 { parts[1].trim_start_matches('/') } else { "" };

    // Start directory
    let start_dir = if prefix.is_empty() {
        base.to_string()
    } else if prefix.starts_with('/') {
        prefix.to_string()
    } else {
        format!("{}/{}", base, prefix)
    };

    // Traverse recursively
    expand_glob_traverse(&start_dir, suffix, results);
}

fn expand_glob_traverse(dir: &str, suffix: &str, results: &mut Vec<String>) {
    let entries = match syscall::readdir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries {
        if entry == "." || entry == ".." {
            continue;
        }

        let path = format!("{}/{}", dir.trim_end_matches('/'), entry);

        // Check if this path matches the suffix
        if suffix.is_empty() || glob_match(suffix, &entry) {
            results.push(path.clone());
        } else if suffix.contains('/') {
            // Multi-segment suffix - check partial matches
            let segments: Vec<&str> = suffix.split('/').collect();
            if glob_match(segments[0], &entry) {
                // Check if remaining segments match
                if let Ok(meta) = syscall::metadata(&path) {
                    if meta.is_dir {
                        expand_glob_segments(&path, &segments[1..], results);
                    }
                }
            }
        }

        // Recurse into directories
        if let Ok(meta) = syscall::metadata(&path) {
            if meta.is_dir {
                expand_glob_traverse(&path, suffix, results);
            }
        }
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

// ============ Built-in Programs ============

/// Check if args contain -h or --help and return usage message if so
fn check_help(args: &[&str], usage: &str) -> Option<String> {
    if args.iter().any(|a| *a == "-h" || *a == "--help") {
        Some(usage.to_string())
    } else {
        None
    }
}

/// Helper to read file content as string
fn read_file_content(path: &str) -> Result<String, String> {
    match syscall::open(path, syscall::OpenFlags::READ) {
        Ok(fd) => {
            let mut content = String::new();
            let mut buf = [0u8; 4096];
            loop {
                match syscall::read(fd, &mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Ok(s) = std::str::from_utf8(&buf[..n]) {
                            content.push_str(s);
                        }
                    }
                    Err(e) => {
                        let _ = syscall::close(fd);
                        return Err(e.to_string());
                    }
                }
            }
            let _ = syscall::close(fd);
            Ok(content)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Extract stdin from args if present
/// Convert String slice to &str slice for easier handling
fn args_to_strs(args: &[String]) -> Vec<&str> {
    args.iter().map(|s| s.as_str()).collect()
}

/// cat - concatenate files or stdin
fn prog_cat(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let files = args_to_strs(args);

    if let Some(help) = check_help(&files, "Usage: cat [FILE]...\nConcatenate files and print to stdout. See 'man cat' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    if files.is_empty() {
        // Read from stdin
        if !stdin.is_empty() {
            stdout.push_str(stdin);
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
fn prog_ls(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let paths = args_to_strs(args);

    if let Some(help) = check_help(&paths, "Usage: ls [-la] [PATH]...\nList directory contents. See 'man ls' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    let paths: Vec<&str> = paths.into_iter().filter(|p| !p.starts_with('-') || *p == "-" ).collect();
    let paths = if paths.is_empty() {
        vec!["."]
    } else {
        paths
    };

    // ANSI color codes
    const BLUE: &str = "\x1b[34m";   // directories
    const CYAN: &str = "\x1b[36m";   // symlinks (future)
    const RESET: &str = "\x1b[0m";

    let mut code = 0;
    for path in paths {
        match syscall::readdir(path) {
            Ok(entries) => {
                for entry in entries {
                    // Check if it's a directory
                    let full_path = if path == "." {
                        entry.clone()
                    } else if path == "/" {
                        format!("/{}", entry)
                    } else {
                        format!("{}/{}", path, entry)
                    };

                    let meta = syscall::metadata(&full_path);
                    let is_dir = meta.as_ref().map(|m| m.is_dir).unwrap_or(false);
                    let is_symlink = meta.as_ref().map(|m| m.is_symlink).unwrap_or(false);
                    let symlink_target = meta.as_ref().ok().and_then(|m| m.symlink_target.clone());

                    if is_symlink {
                        stdout.push_str(CYAN);
                        stdout.push_str(&entry);
                        stdout.push_str(RESET);
                        if let Some(target) = symlink_target {
                            stdout.push_str(" -> ");
                            stdout.push_str(&target);
                        }
                    } else if is_dir {
                        stdout.push_str(BLUE);
                        stdout.push_str(&entry);
                        stdout.push_str(RESET);
                    } else {
                        stdout.push_str(&entry);
                    }
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
fn prog_mkdir(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let paths = args_to_strs(args);

    if let Some(help) = check_help(&paths, "Usage: mkdir DIRECTORY...\nCreate directories. See 'man mkdir' for details.") {
        stdout.push_str(&help);
        return 0;
    }

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
fn prog_touch(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let paths = args_to_strs(args);

    if let Some(help) = check_help(&paths, "Usage: touch FILE...\nCreate empty files or update timestamps. See 'man touch' for details.") {
        stdout.push_str(&help);
        return 0;
    }

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

/// rm - remove files
fn prog_rm(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: rm [-r] FILE...\nRemove files or directories. See 'man rm' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    if args.is_empty() {
        stderr.push_str("rm: missing operand\n");
        return 1;
    }

    let recursive = args.iter().any(|&a| a == "-r" || a == "-rf" || a == "-fr");
    let paths: Vec<&str> = args.iter()
        .copied()
        .filter(|a| !a.starts_with('-'))
        .collect();

    if paths.is_empty() {
        stderr.push_str("rm: missing operand\n");
        return 1;
    }

    let mut failed = false;
    for path in paths {
        // Check if it's a directory
        match syscall::metadata(path) {
            Ok(meta) if meta.is_dir => {
                if recursive {
                    if let Err(e) = syscall::remove_dir(path) {
                        stderr.push_str(&format!("rm: cannot remove '{}': {}\n", path, e));
                        failed = true;
                    }
                } else {
                    stderr.push_str(&format!("rm: cannot remove '{}': Is a directory\n", path));
                    failed = true;
                }
            }
            Ok(_) => {
                if let Err(e) = syscall::remove_file(path) {
                    stderr.push_str(&format!("rm: cannot remove '{}': {}\n", path, e));
                    failed = true;
                }
            }
            Err(e) => {
                stderr.push_str(&format!("rm: cannot remove '{}': {}\n", path, e));
                failed = true;
            }
        }
    }

    if failed { 1 } else { 0 }
}

/// cp - copy files
fn prog_cp(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: cp SOURCE DEST\nCopy files. See 'man cp' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    if args.len() < 2 {
        stderr.push_str("cp: missing operand\n");
        return 1;
    }

    let src = &args[0];
    let dst = &args[1];

    match syscall::copy_file(src, dst) {
        Ok(_) => 0,
        Err(e) => {
            stderr.push_str(&format!("cp: cannot copy '{}' to '{}': {}\n", src, dst, e));
            1
        }
    }
}

/// mv - move/rename files
fn prog_mv(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: mv SOURCE DEST\nMove or rename files. See 'man mv' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    if args.len() < 2 {
        stderr.push_str("mv: missing operand\n");
        return 1;
    }

    let src = &args[0];
    let dst = &args[1];

    match syscall::rename(src, dst) {
        Ok(()) => 0,
        Err(e) => {
            stderr.push_str(&format!("mv: cannot move '{}' to '{}': {}\n", src, dst, e));
            1
        }
    }
}

/// head - output first lines
fn prog_head(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
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
fn prog_tail(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
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
fn prog_wc(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
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
fn prog_grep(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_sort(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
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
fn prog_uniq(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
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
fn prog_tee(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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

/// clear - clear screen (outputs ANSI escape)
fn prog_clear(_args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    stdout.push_str("\x1b[2J\x1b[H");
    0
}

/// save - persist filesystem to OPFS
fn prog_save(_args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    // Queue the async save operation
    #[cfg(target_arch = "wasm32")]
    {
        use crate::vfs::Persistence;
        wasm_bindgen_futures::spawn_local(async {
            let data = match syscall::vfs_snapshot() {
                Ok(d) => d,
                Err(e) => {
                    crate::console_log!("[save] Snapshot failed: {}", e);
                    return;
                }
            };

            let fs = match crate::vfs::MemoryFs::from_json(&data) {
                Ok(f) => f,
                Err(e) => {
                    crate::console_log!("[save] Deserialize failed: {}", e);
                    return;
                }
            };

            if let Err(e) = Persistence::save(&fs).await {
                crate::console_log!("[save] Save failed: {}", e);
            } else {
                crate::console_log!("[save] Filesystem saved to OPFS");
            }
        });
    }
    stdout.push_str("Saving filesystem to OPFS...\n");
    stdout.push_str("(Check browser console for result)\n");
    0
}

/// fsload - reload filesystem from OPFS
fn prog_fsload(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);
    if let Some(help) = check_help(&args, "Usage: fsload\nReload filesystem from OPFS storage.\nSee 'man fsload' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    #[cfg(target_arch = "wasm32")]
    {
        use crate::vfs::Persistence;
        wasm_bindgen_futures::spawn_local(async {
            match Persistence::load().await {
                Ok(Some(fs)) => {
                    // Serialize and restore
                    match fs.to_json() {
                        Ok(data) => {
                            if let Err(e) = syscall::vfs_restore(&data) {
                                crate::console_log!("[fsload] Restore failed: {}", e);
                            } else {
                                crate::console_log!("[fsload] Filesystem restored from OPFS");
                            }
                        }
                        Err(e) => {
                            crate::console_log!("[fsload] Serialize failed: {}", e);
                        }
                    }
                }
                Ok(None) => {
                    crate::console_log!("[fsload] No saved filesystem found in OPFS");
                }
                Err(e) => {
                    crate::console_log!("[fsload] Load failed: {}", e);
                }
            }
        });
    }
    stdout.push_str("Loading filesystem from OPFS...\n");
    stdout.push_str("(Check browser console for result)\n");
    0
}

/// fsreset - clear OPFS and reset to fresh filesystem
fn prog_fsreset(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);
    if let Some(help) = check_help(&args, "Usage: fsreset [-f]\nClear OPFS storage and reset filesystem.\n  -f  Force reset without confirmation\nSee 'man fsreset' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    let force = args.iter().any(|a| *a == "-f" || *a == "--force");

    if !force {
        stderr.push_str("fsreset: This will clear all saved data!\n");
        stderr.push_str("fsreset: Use 'fsreset -f' to confirm.\n");
        return 1;
    }

    #[cfg(target_arch = "wasm32")]
    {
        use crate::vfs::Persistence;
        wasm_bindgen_futures::spawn_local(async {
            if let Err(e) = Persistence::clear().await {
                crate::console_log!("[fsreset] Clear failed: {}", e);
            } else {
                crate::console_log!("[fsreset] OPFS storage cleared");
            }
        });
    }
    stdout.push_str("Clearing OPFS storage...\n");
    stdout.push_str("(Reload the page for a fresh filesystem)\n");
    0
}

/// autosave - configure automatic filesystem saving
fn prog_autosave(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);
    if let Some(help) = check_help(&args, "Usage: autosave [on|off|status|interval N]\nConfigure automatic filesystem saving.\n  on       Enable auto-save\n  off      Disable auto-save\n  status   Show current settings\n  interval Set commands between saves (default: 10)\nSee 'man autosave' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    #[cfg(target_arch = "wasm32")]
    {
        use crate::terminal;

        if args.is_empty() || (args.len() == 1 && args[0] == "status") {
            let (enabled, interval) = terminal::get_autosave_settings();
            stdout.push_str(&format!("Auto-save: {}\n", if enabled { "enabled" } else { "disabled" }));
            stdout.push_str(&format!("Interval: every {} commands\n", interval));
            return 0;
        }

        match args[0] {
            "on" => {
                terminal::set_autosave(true);
                stdout.push_str("Auto-save enabled\n");
            }
            "off" => {
                terminal::set_autosave(false);
                stdout.push_str("Auto-save disabled\n");
            }
            "interval" => {
                if args.len() < 2 {
                    stderr.push_str("autosave: interval requires a number\n");
                    return 1;
                }
                match args[1].parse::<usize>() {
                    Ok(n) => {
                        terminal::set_autosave_interval(n);
                        stdout.push_str(&format!("Auto-save interval set to {} commands\n", n));
                    }
                    Err(_) => {
                        stderr.push_str("autosave: invalid interval\n");
                        return 1;
                    }
                }
            }
            _ => {
                stderr.push_str("autosave: unknown option. Use 'autosave --help' for usage.\n");
                return 1;
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = stderr;
        stdout.push_str("autosave: not available in this build\n");
    }

    0
}

/// curl - transfer URL (HTTP client)
#[allow(unused_variables)]
fn prog_curl(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);
    if let Some(help) = check_help(&args, "Usage: curl [OPTIONS] URL\nTransfer data from URL.\n  -i  Include headers in output\n  -s  Silent mode\n  -X METHOD  Specify request method\n  -H HEADER  Add custom header\nSee 'man curl' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    // Parse arguments
    let mut url = String::new();
    let mut include_headers = false;
    let mut method = "GET";
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut i = 0;

    #[allow(unused_assignments)]
    while i < args.len() {
        match args[i] {
            "-i" => include_headers = true,
            "-s" => {} // silent mode
            "-X" => {
                i += 1;
                if i < args.len() {
                    method = args[i];
                }
            }
            "-H" => {
                i += 1;
                if i < args.len() {
                    if let Some(pos) = args[i].find(':') {
                        let name = args[i][..pos].trim().to_string();
                        let value = args[i][pos+1..].trim().to_string();
                        headers.push((name, value));
                    }
                }
            }
            s if !s.starts_with('-') => {
                url = s.to_string();
            }
            _ => {}
        }
        i += 1;
    }

    if url.is_empty() {
        stderr.push_str("curl: no URL specified\n");
        return 1;
    }

    #[cfg(target_arch = "wasm32")]
    {
        use crate::kernel::network::{HttpMethod, HttpRequest};

        let http_method = match method.to_uppercase().as_str() {
            "GET" => HttpMethod::Get,
            "POST" => HttpMethod::Post,
            "PUT" => HttpMethod::Put,
            "DELETE" => HttpMethod::Delete,
            "HEAD" => HttpMethod::Head,
            "PATCH" => HttpMethod::Patch,
            _ => {
                stderr.push_str(&format!("curl: unsupported method: {}\n", method));
                return 1;
            }
        };

        let url_clone = url.clone();
        let include_headers_clone = include_headers;
        let headers_clone = headers.clone();

        wasm_bindgen_futures::spawn_local(async move {
            let mut req = HttpRequest::new(http_method, &url_clone);
            for (name, value) in headers_clone {
                req = req.header(&name, &value);
            }

            match req.send().await {
                Ok(resp) => {
                    if include_headers_clone {
                        crate::console_log!("HTTP/{} {}", resp.status, resp.status_text);
                        for (name, value) in &resp.headers {
                            crate::console_log!("{}: {}", name, value);
                        }
                        crate::console_log!("");
                    }
                    match resp.text() {
                        Ok(text) => crate::console_log!("{}", text),
                        Err(_) => crate::console_log!("[binary data: {} bytes]", resp.body.len()),
                    }
                }
                Err(e) => {
                    crate::console_log!("curl: {}", e);
                }
            }
        });
        stdout.push_str(&format!("Fetching {}...\n", url));
        stdout.push_str("(Check browser console for result)\n");
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        stdout.push_str("curl: not available in this build (requires WASM)\n");
    }

    0
}

/// wget - download file from URL
#[allow(unused_variables)]
fn prog_wget(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);
    if let Some(help) = check_help(&args, "Usage: wget [OPTIONS] URL\nDownload file from URL.\n  -O FILE  Save to FILE instead of default\n  -q       Quiet mode\nSee 'man wget' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    // Parse arguments
    let mut url = String::new();
    let mut output_file = String::new();
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-q" => {} // quiet mode
            "-O" => {
                i += 1;
                if i < args.len() {
                    output_file = args[i].to_string();
                }
            }
            s if !s.starts_with('-') => {
                url = s.to_string();
            }
            _ => {}
        }
        i += 1;
    }

    if url.is_empty() {
        stderr.push_str("wget: no URL specified\n");
        return 1;
    }

    // Determine output filename
    let filename = if output_file.is_empty() {
        // Extract filename from URL
        url.rsplit('/').next().unwrap_or("index.html").to_string()
    } else {
        output_file
    };

    #[cfg(target_arch = "wasm32")]
    {
        use crate::kernel::network::HttpRequest;

        let url_clone = url.clone();
        let filename_clone = filename.clone();

        wasm_bindgen_futures::spawn_local(async move {
            match HttpRequest::get(&url_clone).send().await {
                Ok(resp) => {
                    if resp.status >= 200 && resp.status < 300 {
                        // Write to file
                        match syscall::write_file(&filename_clone, &String::from_utf8_lossy(&resp.body)) {
                            Ok(_) => {
                                crate::console_log!("Downloaded {} -> {} ({} bytes)",
                                    url_clone, filename_clone, resp.body.len());
                            }
                            Err(e) => {
                                crate::console_log!("wget: failed to write {}: {}", filename_clone, e);
                            }
                        }
                    } else {
                        crate::console_log!("wget: HTTP {} {}", resp.status, resp.status_text);
                    }
                }
                Err(e) => {
                    crate::console_log!("wget: {}", e);
                }
            }
        });
        stdout.push_str(&format!("Downloading {} -> {}...\n", url, filename));
        stdout.push_str("(Check browser console for result)\n");
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        stdout.push_str("wget: not available in this build (requires WASM)\n");
    }

    0
}

/// tree - display directory tree
fn prog_tree(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let paths = args_to_strs(args);

    if let Some(help) = check_help(&paths, "Usage: tree [DIRECTORY]\nDisplay directory tree. See 'man tree' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    let path = if paths.is_empty() { "." } else { paths[0] };

    // ANSI colors
    const BLUE: &str = "\x1b[34m";
    const RESET: &str = "\x1b[0m";

    fn print_tree(
        path: &str,
        prefix: &str,
        stdout: &mut String,
        _is_last: bool,
        dir_count: &mut usize,
        file_count: &mut usize,
    ) -> Result<(), String> {
        let entries = syscall::readdir(path).map_err(|e| e.to_string())?;
        let mut entries: Vec<_> = entries.into_iter().collect();
        entries.sort();

        for (i, entry) in entries.iter().enumerate() {
            let is_last_entry = i == entries.len() - 1;
            let connector = if is_last_entry { "└── " } else { "├── " };
            let child_prefix = if is_last_entry { "    " } else { "│   " };

            let full_path = if path == "/" {
                format!("/{}", entry)
            } else if path == "." {
                entry.clone()
            } else {
                format!("{}/{}", path, entry)
            };

            let meta = syscall::metadata(&full_path);
            let is_dir = meta.as_ref().map(|m| m.is_dir).unwrap_or(false);
            let is_symlink = meta.as_ref().map(|m| m.is_symlink).unwrap_or(false);
            let symlink_target = meta.as_ref().ok().and_then(|m| m.symlink_target.clone());

            if is_symlink {
                *file_count += 1;
                let target_str = symlink_target.map(|t| format!(" -> {}", t)).unwrap_or_default();
                stdout.push_str(&format!("{}{}\x1b[36m{}\x1b[0m{}\n", prefix, connector, entry, target_str));
            } else if is_dir {
                *dir_count += 1;
                stdout.push_str(&format!("{}{}{}{}{}\n", prefix, connector, BLUE, entry, RESET));
                let new_prefix = format!("{}{}", prefix, child_prefix);
                let _ = print_tree(&full_path, &new_prefix, stdout, is_last_entry, dir_count, file_count);
            } else {
                *file_count += 1;
                stdout.push_str(&format!("{}{}{}\n", prefix, connector, entry));
            }
        }
        Ok(())
    }

    // Print root
    let is_dir = syscall::metadata(path).map(|m| m.is_dir).unwrap_or(false);
    if !is_dir {
        stderr.push_str(&format!("tree: {}: Not a directory\n", path));
        return 1;
    }

    stdout.push_str(&format!("{}{}{}\n", BLUE, path, RESET));

    let mut dir_count = 0usize;
    let mut file_count = 0usize;

    if let Err(e) = print_tree(path, "", stdout, false, &mut dir_count, &mut file_count) {
        stderr.push_str(&format!("tree: {}\n", e));
        return 1;
    }

    stdout.push_str(&format!("\n{} directories, {} files", dir_count, file_count));
    0
}

/// history - display command history
fn prog_history(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
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

/// sleep - pause for specified seconds
fn prog_sleep(args: &[String], _stdin: &str, _stdout: &mut String, stderr: &mut String) -> i32 {
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

/// ln - create links (symlinks with -s)
fn prog_ln(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: ln -s TARGET LINK_NAME\nCreate symbolic links. See 'man ln' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    // Parse flags
    let mut symbolic = false;
    let mut force = false;
    let mut targets: Vec<&str> = Vec::new();

    for arg in &args {
        if *arg == "-s" || *arg == "--symbolic" {
            symbolic = true;
        } else if *arg == "-f" || *arg == "--force" {
            force = true;
        } else if arg.starts_with('-') {
            // Handle combined flags like -sf
            for c in arg[1..].chars() {
                match c {
                    's' => symbolic = true,
                    'f' => force = true,
                    _ => {
                        stderr.push_str(&format!("ln: unknown option: -{}\n", c));
                        return 1;
                    }
                }
            }
        } else {
            targets.push(arg);
        }
    }

    if targets.len() < 2 {
        stderr.push_str("ln: missing file operand\n");
        stderr.push_str("Usage: ln [-sf] TARGET LINK_NAME\n");
        return 1;
    }

    if !symbolic {
        stderr.push_str("ln: hard links not supported, use -s for symbolic links\n");
        return 1;
    }

    let target = targets[0];
    let link_name = targets[1];

    // If force, try to remove existing link
    if force {
        let _ = syscall::remove_file(link_name);
    }

    match syscall::symlink(target, link_name) {
        Ok(_) => 0,
        Err(e) => {
            stderr.push_str(&format!("ln: {}: {}\n", link_name, e));
            1
        }
    }
}

/// readlink - print value of a symbolic link
fn prog_readlink(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if args.is_empty() {
        stderr.push_str("readlink: missing file operand\n");
        return 1;
    }

    let path = &args[0];

    match syscall::read_link(path) {
        Ok(target) => {
            stdout.push_str(&target);
            0
        }
        Err(e) => {
            stderr.push_str(&format!("readlink: {}: {}\n", path, e));
            1
        }
    }
}

/// Text editor - opens a file for editing
#[allow(unused_variables)]
fn prog_edit(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_man(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
        "basename" => include_str!("../../man/formatted/basename.txt"),
        "base64" => include_str!("../../man/formatted/base64.txt"),
        "bg" => include_str!("../../man/formatted/bg.txt"),
        "cal" => include_str!("../../man/formatted/cal.txt"),
        "cat" => include_str!("../../man/formatted/cat.txt"),
        "cd" => include_str!("../../man/formatted/cd.txt"),
        "comm" => include_str!("../../man/formatted/comm.txt"),
        "cp" => include_str!("../../man/formatted/cp.txt"),
        "cut" => include_str!("../../man/formatted/cut.txt"),
        "date" => include_str!("../../man/formatted/date.txt"),
        "df" => include_str!("../../man/formatted/df.txt"),
        "diff" => include_str!("../../man/formatted/diff.txt"),
        "dirname" => include_str!("../../man/formatted/dirname.txt"),
        "du" => include_str!("../../man/formatted/du.txt"),
        "echo" => include_str!("../../man/formatted/echo.txt"),
        "edit" => include_str!("../../man/formatted/edit.txt"),
        "expr" => include_str!("../../man/formatted/expr.txt"),
        "fg" => include_str!("../../man/formatted/fg.txt"),
        "find" => include_str!("../../man/formatted/find.txt"),
        "fold" => include_str!("../../man/formatted/fold.txt"),
        "free" => include_str!("../../man/formatted/free.txt"),
        "grep" => include_str!("../../man/formatted/grep.txt"),
        "head" => include_str!("../../man/formatted/head.txt"),
        "hostname" => include_str!("../../man/formatted/hostname.txt"),
        "id" => include_str!("../../man/formatted/id.txt"),
        "jobs" => include_str!("../../man/formatted/jobs.txt"),
        "kill" => include_str!("../../man/formatted/kill.txt"),
        "ln" => include_str!("../../man/formatted/ln.txt"),
        "ls" => include_str!("../../man/formatted/ls.txt"),
        "man" => include_str!("../../man/formatted/man.txt"),
        "mkdir" => include_str!("../../man/formatted/mkdir.txt"),
        "mv" => include_str!("../../man/formatted/mv.txt"),
        "nl" => include_str!("../../man/formatted/nl.txt"),
        "paste" => include_str!("../../man/formatted/paste.txt"),
        "printenv" => include_str!("../../man/formatted/printenv.txt"),
        "printf" => include_str!("../../man/formatted/printf.txt"),
        "ps" => include_str!("../../man/formatted/ps.txt"),
        "pwd" => include_str!("../../man/formatted/pwd.txt"),
        "rev" => include_str!("../../man/formatted/rev.txt"),
        "rm" => include_str!("../../man/formatted/rm.txt"),
        "seq" => include_str!("../../man/formatted/seq.txt"),
        "sort" => include_str!("../../man/formatted/sort.txt"),
        "strace" => include_str!("../../man/formatted/strace.txt"),
        "strings" => include_str!("../../man/formatted/strings.txt"),
        "tail" => include_str!("../../man/formatted/tail.txt"),
        "tee" => include_str!("../../man/formatted/tee.txt"),
        "test" => include_str!("../../man/formatted/test.txt"),
        "[" => include_str!("../../man/formatted/test.txt"),
        "time" => include_str!("../../man/formatted/time.txt"),
        "touch" => include_str!("../../man/formatted/touch.txt"),
        "tr" => include_str!("../../man/formatted/tr.txt"),
        "tree" => include_str!("../../man/formatted/tree.txt"),
        "type" => include_str!("../../man/formatted/type.txt"),
        "uname" => include_str!("../../man/formatted/uname.txt"),
        "uniq" => include_str!("../../man/formatted/uniq.txt"),
        "uptime" => include_str!("../../man/formatted/uptime.txt"),
        "wc" => include_str!("../../man/formatted/wc.txt"),
        "which" => include_str!("../../man/formatted/which.txt"),
        "whoami" => include_str!("../../man/formatted/whoami.txt"),
        "xargs" => include_str!("../../man/formatted/xargs.txt"),
        "xxd" => include_str!("../../man/formatted/xxd.txt"),
        "yes" => include_str!("../../man/formatted/yes.txt"),
        _ => {
            stderr.push_str(&format!("No manual entry for {}\n", page));
            return 1;
        }
    };

    stdout.push_str(content.trim());
    0
}

/// printenv - print environment variables (uses kernel syscalls)
fn prog_printenv(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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

/// id - print process and user IDs (uses kernel syscalls)
fn prog_id(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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

/// jobs - list background jobs
fn prog_jobs(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: jobs [-l]\nList background jobs.") {
        stdout.push_str(&help);
        return 0;
    }

    let long_format = args.iter().any(|a| *a == "-l");

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
fn prog_fg(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
        if matches!(state, syscall::ProcessState::Stopped) {
            if let Err(e) = syscall::kill(*pid, crate::kernel::signal::Signal::SIGCONT) {
                stderr.push_str(&format!("fg: {}\n", e));
                return 1;
            }
        }
        stdout.push_str(&format!("{}\n", name));
        0
    } else {
        stderr.push_str("fg: no such job\n");
        1
    }
}

/// bg - continue job in background
fn prog_bg(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_strace(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: strace [-c] COMMAND [ARGS...]\nTrace system calls.") {
        stdout.push_str(&help);
        return 0;
    }

    if args.is_empty() {
        stderr.push_str("strace: must have COMMAND to run\n");
        return 1;
    }

    let count_mode = args.iter().any(|a| *a == "-c");
    let cmd_args: Vec<_> = args.iter()
        .filter(|a| !a.starts_with('-'))
        .map(|s| *s)
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

/// whoami - print effective username
fn prog_whoami(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_groups(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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

/// su - switch user (simulated)
fn prog_su(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_sudo(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_useradd(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_groupadd(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_passwd(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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

/// chmod - change file permissions
fn prog_chmod(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if args.len() < 2 || args.first().map(|s| s.as_ref()) == Some("--help") {
        stdout.push_str("Usage: chmod MODE FILE...\nChange file permissions.\n\n");
        stdout.push_str("MODE can be:\n");
        stdout.push_str("  Octal: 755, 644, etc.\n");
        stdout.push_str("  Symbolic: u+x, go-w, a=r, etc.\n");
        return if args.is_empty() { 0 } else { 1 };
    }

    let mode_str = &args[0];
    let mode = if let Ok(octal) = u16::from_str_radix(mode_str, 8) {
        octal
    } else {
        // Parse symbolic mode (simplified)
        stderr.push_str(&format!("chmod: invalid mode: '{}' (use octal for now)\n", mode_str));
        return 1;
    };

    let mut errors = 0;
    for path in &args[1..] {
        match syscall::chmod(path, mode) {
            Ok(()) => {}
            Err(e) => {
                stderr.push_str(&format!("chmod: {}: {}\n", path, e));
                errors += 1;
            }
        }
    }

    if errors > 0 { 1 } else { 0 }
}

/// chown - change file owner
fn prog_chown(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if args.len() < 2 || args.first().map(|s| s.as_ref()) == Some("--help") {
        stdout.push_str("Usage: chown [OWNER][:GROUP] FILE...\nChange file owner and group.\n");
        return if args.is_empty() { 0 } else { 1 };
    }

    let owner_str = &args[0];

    // Parse owner:group or owner.group or just owner
    let (uid, gid) = if owner_str.contains(':') || owner_str.contains('.') {
        let sep = if owner_str.contains(':') { ':' } else { '.' };
        let parts: Vec<&str> = owner_str.splitn(2, sep).collect();
        let uid = if parts[0].is_empty() {
            None
        } else if let Ok(n) = parts[0].parse::<u32>() {
            Some(n)
        } else if let Some(user) = syscall::get_user_by_name(parts[0]) {
            Some(user.uid.0)
        } else {
            stderr.push_str(&format!("chown: invalid user: '{}'\n", parts[0]));
            return 1;
        };

        let gid = if parts.len() > 1 && !parts[1].is_empty() {
            if let Ok(n) = parts[1].parse::<u32>() {
                Some(n)
            } else if let Some(group) = syscall::get_group_by_name(parts[1]) {
                Some(group.gid.0)
            } else {
                stderr.push_str(&format!("chown: invalid group: '{}'\n", parts[1]));
                return 1;
            }
        } else {
            None
        };

        (uid, gid)
    } else {
        // Just owner
        let uid = if let Ok(n) = owner_str.parse::<u32>() {
            Some(n)
        } else if let Some(user) = syscall::get_user_by_name(owner_str) {
            Some(user.uid.0)
        } else {
            stderr.push_str(&format!("chown: invalid user: '{}'\n", owner_str));
            return 1;
        };
        (uid, None)
    };

    let mut errors = 0;
    for path in &args[1..] {
        match syscall::chown(path, uid, gid) {
            Ok(()) => {}
            Err(e) => {
                stderr.push_str(&format!("chown: {}: {}\n", path, e));
                errors += 1;
            }
        }
    }

    if errors > 0 { 1 } else { 0 }
}

/// chgrp - change file group
fn prog_chgrp(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if args.len() < 2 || args.first().map(|s| s.as_ref()) == Some("--help") {
        stdout.push_str("Usage: chgrp GROUP FILE...\nChange file group.\n");
        return if args.is_empty() { 0 } else { 1 };
    }

    let group_str = &args[0];
    let gid = if let Ok(n) = group_str.parse::<u32>() {
        n
    } else if let Some(group) = syscall::get_group_by_name(group_str) {
        group.gid.0
    } else {
        stderr.push_str(&format!("chgrp: invalid group: '{}'\n", group_str));
        return 1;
    };

    let mut errors = 0;
    for path in &args[1..] {
        match syscall::chown(path, None, Some(gid)) {
            Ok(()) => {}
            Err(e) => {
                stderr.push_str(&format!("chgrp: {}: {}\n", path, e));
                errors += 1;
            }
        }
    }

    if errors > 0 { 1 } else { 0 }
}

/// hostname - show or set system hostname
fn prog_hostname(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_uname(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
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

/// find - search for files
fn prog_find(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: find [PATH] [-name PATTERN] [-type TYPE]\nSearch for files.") {
        stdout.push_str(&help);
        return 0;
    }

    // Parse arguments
    let mut start_path = ".";
    let mut name_pattern: Option<&str> = None;
    let mut type_filter: Option<char> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "-name" if i + 1 < args.len() => {
                name_pattern = Some(args[i + 1]);
                i += 2;
            }
            "-type" if i + 1 < args.len() => {
                type_filter = args[i + 1].chars().next();
                i += 2;
            }
            s if !s.starts_with('-') && i == 0 => {
                start_path = s;
                i += 1;
            }
            _ => i += 1,
        }
    }

    // Recursive find helper
    fn find_recursive(
        path: &str,
        name_pattern: Option<&str>,
        type_filter: Option<char>,
        stdout: &mut String,
    ) -> Result<(), String> {
        let entries = syscall::readdir(path).map_err(|e| e.to_string())?;

        for entry in entries {
            let full_path = if path == "/" {
                format!("/{}", entry)
            } else {
                format!("{}/{}", path, entry)
            };

            let meta = match syscall::metadata(&full_path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            // Type filter
            let type_match = match type_filter {
                Some('f') => meta.is_file,
                Some('d') => meta.is_dir,
                Some('l') => meta.is_symlink,
                Some(_) | None => true,
            };

            // Name filter (simple glob with * support)
            let name_match = match name_pattern {
                Some(pattern) => {
                    if pattern.contains('*') {
                        let parts: Vec<&str> = pattern.split('*').collect();
                        if parts.len() == 2 {
                            let (prefix, suffix) = (parts[0], parts[1]);
                            entry.starts_with(prefix) && entry.ends_with(suffix)
                        } else if pattern.starts_with('*') {
                            entry.ends_with(&pattern[1..])
                        } else if pattern.ends_with('*') {
                            entry.starts_with(&pattern[..pattern.len()-1])
                        } else {
                            entry == pattern
                        }
                    } else {
                        entry == pattern
                    }
                }
                None => true,
            };

            if type_match && name_match {
                stdout.push_str(&full_path);
                stdout.push('\n');
            }

            // Recurse into directories
            if meta.is_dir && !meta.is_symlink {
                let _ = find_recursive(&full_path, name_pattern, type_filter, stdout);
            }
        }
        Ok(())
    }

    // Resolve start path
    let resolved = if start_path == "." {
        syscall::getcwd().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| "/".to_string())
    } else if start_path.starts_with('/') {
        start_path.to_string()
    } else {
        let cwd = syscall::getcwd().unwrap_or_else(|_| std::path::PathBuf::from("/"));
        format!("{}/{}", cwd.display(), start_path)
    };

    if let Err(e) = find_recursive(&resolved, name_pattern, type_filter, stdout) {
        stderr.push_str(&format!("find: {}\n", e));
        return 1;
    }

    0
}

/// du - disk usage
fn prog_du(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: du [-s] [-h] [PATH...]\nEstimate file space usage.") {
        stdout.push_str(&help);
        return 0;
    }

    let summary_only = args.iter().any(|a| *a == "-s");
    let human_readable = args.iter().any(|a| *a == "-h");
    let paths: Vec<&str> = args.iter()
        .filter(|a| !a.starts_with('-'))
        .map(|s| *s)
        .collect();

    let paths = if paths.is_empty() { vec!["."] } else { paths };

    fn format_size(size: u64, human: bool) -> String {
        if human {
            if size >= 1024 * 1024 * 1024 {
                format!("{:.1}G", size as f64 / (1024.0 * 1024.0 * 1024.0))
            } else if size >= 1024 * 1024 {
                format!("{:.1}M", size as f64 / (1024.0 * 1024.0))
            } else if size >= 1024 {
                format!("{:.1}K", size as f64 / 1024.0)
            } else {
                format!("{}", size)
            }
        } else {
            format!("{}", (size + 1023) / 1024) // blocks
        }
    }

    fn du_recursive(path: &str, human: bool, summary: bool, stdout: &mut String) -> u64 {
        let mut total: u64 = 0;

        match syscall::metadata(path) {
            Ok(meta) => {
                if meta.is_file {
                    total = meta.size;
                } else if meta.is_dir {
                    if let Ok(entries) = syscall::readdir(path) {
                        for entry in entries {
                            let full = if path == "/" {
                                format!("/{}", entry)
                            } else {
                                format!("{}/{}", path, entry)
                            };
                            let sub_size = du_recursive(&full, human, true, stdout);
                            total += sub_size;
                        }
                    }
                }
            }
            Err(_) => {}
        }

        if !summary {
            stdout.push_str(&format!("{}\t{}\n", format_size(total, human), path));
        }

        total
    }

    for path in paths {
        let resolved = if path == "." {
            syscall::getcwd().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| "/".to_string())
        } else if path.starts_with('/') {
            path.to_string()
        } else {
            let cwd = syscall::getcwd().unwrap_or_else(|_| std::path::PathBuf::from("/"));
            format!("{}/{}", cwd.display(), path)
        };

        let total = du_recursive(&resolved, human_readable, summary_only, stdout);
        if summary_only {
            stdout.push_str(&format!("{}\t{}\n", format_size(total, human_readable), path));
        }
    }

    0
}

/// df - filesystem space
fn prog_df(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: df [-h]\nShow filesystem disk space usage.") {
        stdout.push_str(&help);
        return 0;
    }

    let human_readable = args.iter().any(|a| *a == "-h");

    // Calculate total VFS size by walking the filesystem
    fn count_size(path: &str) -> u64 {
        let mut total: u64 = 0;
        if let Ok(meta) = syscall::metadata(path) {
            if meta.is_file {
                total = meta.size;
            } else if meta.is_dir {
                if let Ok(entries) = syscall::readdir(path) {
                    for entry in entries {
                        let full = if path == "/" {
                            format!("/{}", entry)
                        } else {
                            format!("{}/{}", path, entry)
                        };
                        total += count_size(&full);
                    }
                }
            }
        }
        total
    }

    let used = count_size("/");
    let total: u64 = 1024 * 1024 * 100; // 100MB virtual filesystem
    let available = total.saturating_sub(used);
    let use_pct = if total > 0 { (used * 100 / total) as u32 } else { 0 };

    fn format_size(size: u64, human: bool) -> String {
        if human {
            if size >= 1024 * 1024 * 1024 {
                format!("{:.1}G", size as f64 / (1024.0 * 1024.0 * 1024.0))
            } else if size >= 1024 * 1024 {
                format!("{:.1}M", size as f64 / (1024.0 * 1024.0))
            } else if size >= 1024 {
                format!("{:.1}K", size as f64 / 1024.0)
            } else {
                format!("{}B", size)
            }
        } else {
            format!("{}", (size + 1023) / 1024)
        }
    }

    stdout.push_str("Filesystem      Size  Used Avail Use% Mounted on\n");
    stdout.push_str(&format!(
        "axeberg-vfs  {:>6} {:>5} {:>5} {:>3}% /\n",
        format_size(total, human_readable),
        format_size(used, human_readable),
        format_size(available, human_readable),
        use_pct
    ));

    0
}

/// ps - process status
fn prog_ps(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
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

/// kill - send signal to process
fn prog_kill(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: kill [-s SIGNAL] PID...\nSend signal to processes.") {
        stdout.push_str(&help);
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

/// time - time command execution
fn prog_time(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_date(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
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

/// seq - print sequence of numbers
fn prog_seq(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_yes(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
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
fn prog_basename(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_dirname(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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

/// rev - reverse lines
fn prog_rev(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_cut(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_tr(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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

/// xargs - build and execute commands from stdin
fn prog_xargs(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
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
fn prog_cal(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
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
fn prog_printf(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_test(args: &[String], _stdin: &str, _stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_expr(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_which(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_type(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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

/// uptime - show how long system has been running
fn prog_uptime(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
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
fn prog_free(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
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

/// diff - compare files line by line
fn prog_diff(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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

/// base64 - encode/decode base64
fn prog_base64(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: base64 [-d] [FILE]\nBase64 encode or decode.\n  -d  Decode") {
        stdout.push_str(&help);
        return 0;
    }

    let decode = args.iter().any(|a| *a == "-d" || *a == "--decode");
    let file_args: Vec<&str> = args.iter().filter(|a| !a.starts_with('-')).map(|s| s.as_ref()).collect();

    let input = if let Some(file) = file_args.first() {
        match read_file_content(file) {
            Ok(c) => c,
            Err(e) => {
                stderr.push_str(&format!("base64: {}: {}\n", file, e));
                return 1;
            }
        }
    } else {
        stdin.to_string()
    };

    if decode {
        // Simple base64 decode
        let chars: Vec<char> = input.chars().filter(|c| !c.is_whitespace()).collect();
        let mut result = Vec::new();
        let mut i = 0;

        while i < chars.len() {
            let chunk: Vec<u8> = chars[i..].iter().take(4).map(|c| base64_decode_char(*c)).collect();
            if chunk.len() < 4 { break; }

            let val = ((chunk[0] as u32) << 18) | ((chunk[1] as u32) << 12) | ((chunk[2] as u32) << 6) | (chunk[3] as u32);
            result.push((val >> 16) as u8);
            if chunk[2] < 64 { result.push((val >> 8) as u8); }
            if chunk[3] < 64 { result.push(val as u8); }
            i += 4;
        }

        if let Ok(s) = String::from_utf8(result) {
            stdout.push_str(&s);
        } else {
            stderr.push_str("base64: invalid encoding\n");
            return 1;
        }
    } else {
        // Base64 encode
        let bytes = input.as_bytes();
        let mut result = String::new();

        for chunk in bytes.chunks(3) {
            let val = match chunk.len() {
                3 => ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32),
                2 => ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8),
                1 => (chunk[0] as u32) << 16,
                _ => break,
            };

            result.push(base64_encode_val((val >> 18) & 0x3F));
            result.push(base64_encode_val((val >> 12) & 0x3F));
            result.push(if chunk.len() > 1 { base64_encode_val((val >> 6) & 0x3F) } else { '=' });
            result.push(if chunk.len() > 2 { base64_encode_val(val & 0x3F) } else { '=' });
        }

        stdout.push_str(&result);
        stdout.push('\n');
    }

    0
}

fn base64_encode_val(v: u32) -> char {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    CHARS[v as usize] as char
}

fn base64_decode_char(c: char) -> u8 {
    match c {
        'A'..='Z' => (c as u8) - b'A',
        'a'..='z' => (c as u8) - b'a' + 26,
        '0'..='9' => (c as u8) - b'0' + 52,
        '+' => 62,
        '/' => 63,
        '=' => 64, // padding
        _ => 64,
    }
}

/// xxd - hex dump
fn prog_xxd(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: xxd [FILE]\nMake a hexdump.") {
        stdout.push_str(&help);
        return 0;
    }

    let input = if let Some(file) = args.first() {
        match read_file_content(file) {
            Ok(c) => c,
            Err(e) => {
                stderr.push_str(&format!("xxd: {}: {}\n", file, e));
                return 1;
            }
        }
    } else {
        stdin.to_string()
    };

    let bytes = input.as_bytes();

    for (offset, chunk) in bytes.chunks(16).enumerate() {
        // Offset
        stdout.push_str(&format!("{:08x}: ", offset * 16));

        // Hex bytes
        for (i, byte) in chunk.iter().enumerate() {
            stdout.push_str(&format!("{:02x}", byte));
            if i % 2 == 1 { stdout.push(' '); }
        }

        // Padding for incomplete lines
        for i in chunk.len()..16 {
            stdout.push_str("  ");
            if i % 2 == 1 { stdout.push(' '); }
        }

        // ASCII representation
        stdout.push(' ');
        for byte in chunk {
            if *byte >= 0x20 && *byte < 0x7f {
                stdout.push(*byte as char);
            } else {
                stdout.push('.');
            }
        }
        stdout.push('\n');
    }

    0
}

/// nl - number lines
fn prog_nl(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_fold(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_paste(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_comm(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_strings(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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

/// systemctl - service management
fn prog_systemctl(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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

    let cmd = &args[0][..];
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
                        &svc.config.name,
                        state_str,
                        &svc.config.description
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
            if let Some(target) = Target::from_str(target_str) {
                syscall::KERNEL.with(|k| {
                    let mut kernel = k.borrow_mut();
                    kernel.init_mut().set_target(target);
                    stdout.push_str(&format!("Created symlink /etc/systemd/system/default.target -> {}\n", target.as_str()));
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
fn prog_reboot(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
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
fn prog_poweroff(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
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

// ========== IPC COMMANDS ==========

fn prog_mkfifo(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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

fn prog_ipcs(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
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

fn prog_ipcrm(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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

// ========== MOUNT COMMANDS ==========

fn prog_mount(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: mount [-t TYPE] [-o OPTIONS] SOURCE TARGET\n       mount (show all mounts)\n\nMount a filesystem.\n\nOptions:\n  -t TYPE   Filesystem type (proc, sysfs, devfs, tmpfs)\n  -o OPTS   Mount options (ro, noexec, noatime, etc.)") {
        stdout.push_str(&help);
        return 0;
    }

    // No arguments: list all mounts
    if args.is_empty() {
        syscall::KERNEL.with(|k| {
            let kernel = k.borrow();
            for entry in kernel.mounts().list() {
                stdout.push_str(&format!(
                    "{} on {} type {} ({})\n",
                    entry.source,
                    entry.target,
                    entry.fstype.as_str(),
                    entry.options.to_string()
                ));
            }
        });
        return 0;
    }

    // Parse arguments
    let mut fstype = "tmpfs".to_string();
    let mut options = "rw".to_string();
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i][..];
        match arg {
            "-t" => {
                if i + 1 < args.len() {
                    i += 1;
                    fstype = args[i].to_string();
                } else {
                    stderr.push_str("mount: option requires an argument -- 't'\n");
                    return 1;
                }
            }
            "-o" => {
                if i + 1 < args.len() {
                    i += 1;
                    options = args[i].to_string();
                } else {
                    stderr.push_str("mount: option requires an argument -- 'o'\n");
                    return 1;
                }
            }
            _ if !arg.starts_with('-') => {
                positional.push(args[i].to_string());
            }
            _ => {
                // Unknown option
            }
        }
        i += 1;
    }

    if positional.len() < 2 {
        stderr.push_str("mount: usage: mount [-t type] [-o options] source target\n");
        return 1;
    }

    let source = &positional[0];
    let target = &positional[1];

    use crate::kernel::mount::{FsType, MountOptions};

    let fs = FsType::from_str(&fstype);
    let opts = MountOptions::parse(&options);
    let now = syscall::KERNEL.with(|k| k.borrow().now());

    let result = syscall::KERNEL.with(|k| {
        let mut kernel = k.borrow_mut();
        kernel.mounts_mut().mount(source, target, fs, opts, now)
    });

    match result {
        Ok(()) => 0,
        Err(e) => {
            stderr.push_str(&format!("mount: {:?}\n", e));
            1
        }
    }
}

fn prog_umount(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: umount TARGET\nUnmount a filesystem.") {
        stdout.push_str(&help);
        return 0;
    }

    if args.is_empty() {
        stderr.push_str("umount: usage: umount target\n");
        return 1;
    }

    let target = &args[0];

    let result = syscall::KERNEL.with(|k| {
        k.borrow_mut().mounts_mut().umount(target)
    });

    match result {
        Ok(_) => 0,
        Err(e) => {
            stderr.push_str(&format!("umount: {}: {:?}\n", target, e));
            1
        }
    }
}

fn prog_findmnt(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: findmnt [TARGET]\nFind a filesystem mount point.\n\nWith no arguments, lists all mounts in a tree-like format.") {
        stdout.push_str(&help);
        return 0;
    }

    syscall::KERNEL.with(|k| {
        let kernel = k.borrow();

        if args.is_empty() {
            // List all mounts
            stdout.push_str("TARGET                  SOURCE     FSTYPE   OPTIONS\n");
            let mut mounts: Vec<_> = kernel.mounts().list();
            mounts.sort_by(|a, b| a.target.cmp(&b.target));

            for entry in mounts {
                stdout.push_str(&format!(
                    "{:<23} {:<10} {:<8} {}\n",
                    entry.target,
                    if entry.source.len() > 10 { &entry.source[..10] } else { &entry.source },
                    entry.fstype.as_str(),
                    entry.options.to_string()
                ));
            }
        } else {
            // Find specific mount point
            let target = &args[0];
            if let Some(entry) = kernel.mounts().get_mount(target) {
                stdout.push_str(&format!(
                    "TARGET: {}\nSOURCE: {}\nFSTYPE: {}\nOPTIONS: {}\n",
                    entry.target,
                    entry.source,
                    entry.fstype.as_str(),
                    entry.options.to_string()
                ));
            } else if let Some(entry) = kernel.mounts().get_containing_mount(target) {
                stdout.push_str(&format!(
                    "{} is under mount point:\nTARGET: {}\nSOURCE: {}\nFSTYPE: {}\n",
                    target,
                    entry.target,
                    entry.source,
                    entry.fstype.as_str()
                ));
            } else {
                stdout.push_str(&format!("{}: not a mount point\n", target));
            }
        }
    });

    0
}

// ========== TTY COMMANDS ==========

fn prog_stty(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: stty [SETTING]...\n       stty -a\n       stty sane\n       stty raw\n\nChange and print terminal line settings.\n\nSettings:\n  -echo/-icanon/-isig  Toggle flags\n  sane                 Reset to sane defaults\n  raw                  Set raw mode\n  -a                   Print all settings") {
        stdout.push_str(&help);
        return 0;
    }

    use crate::kernel::tty::{format_stty_settings, parse_stty_setting, Termios};

    syscall::KERNEL.with(|k| {
        let mut kernel = k.borrow_mut();

        // If no args or -a, print current settings
        if args.is_empty() || args.iter().any(|a| *a == "-a") {
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

fn prog_tty(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: tty\nPrint the file name of the terminal connected to standard input.") {
        stdout.push_str(&help);
        return 0;
    }

    let silent = args.iter().any(|a| *a == "-s");

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

// ========== PACKAGE MANAGER COMMANDS ==========

/// pkg - simple package manager
///
/// Packages are stored in /var/packages as executable scripts.
/// The package registry is stored in /etc/packages.json
fn prog_pkg(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: pkg <command> [args]\n\nPackage manager for axeberg.\n\nCommands:\n  install <name> <script>   Install a package (script content)\n  remove <name>             Remove a package\n  list                      List installed packages\n  run <name> [args]         Run an installed package\n  info <name>               Show package info\n\nPackages are stored in /var/packages/") {
        stdout.push_str(&help);
        return 0;
    }

    if args.is_empty() {
        stderr.push_str("pkg: missing command\nTry 'pkg --help' for more information.\n");
        return 1;
    }

    // Ensure package directories exist
    let _ = syscall::mkdir("/var");
    let _ = syscall::mkdir("/var/packages");

    match &args[0][..] {
        "install" => {
            if args.len() < 3 {
                stderr.push_str("pkg install: usage: pkg install <name> <script>\n");
                return 1;
            }
            let name = args[1];
            // Join remaining args as the script content (or use quoted string)
            let script = args[2..].join(" ");

            // Validate package name
            if name.contains('/') || name.contains('\0') || name.is_empty() {
                stderr.push_str("pkg install: invalid package name\n");
                return 1;
            }

            let pkg_path = format!("/var/packages/{}", name);

            // Write the package script
            match syscall::open(&pkg_path, syscall::OpenFlags::WRITE) {
                Ok(fd) => {
                    let _ = syscall::write(fd, script.as_bytes());
                    let _ = syscall::close(fd);

                    // Make it executable (mode 755)
                    let _ = syscall::chmod(&pkg_path, 0o755);

                    stdout.push_str(&format!("Installed package '{}'\n", name));
                    0
                }
                Err(e) => {
                    stderr.push_str(&format!("pkg install: failed to install '{}': {:?}\n", name, e));
                    1
                }
            }
        }
        "remove" | "uninstall" => {
            if args.len() < 2 {
                stderr.push_str("pkg remove: usage: pkg remove <name>\n");
                return 1;
            }
            let name = args[1];
            let pkg_path = format!("/var/packages/{}", name);

            match syscall::remove_file(&pkg_path) {
                Ok(()) => {
                    stdout.push_str(&format!("Removed package '{}'\n", name));
                    0
                }
                Err(e) => {
                    stderr.push_str(&format!("pkg remove: '{}': {:?}\n", name, e));
                    1
                }
            }
        }
        "list" | "ls" => {
            match syscall::readdir("/var/packages") {
                Ok(entries) => {
                    if entries.is_empty() {
                        stdout.push_str("No packages installed.\n");
                    } else {
                        stdout.push_str("Installed packages:\n");
                        for entry in entries {
                            stdout.push_str(&format!("  {}\n", entry));
                        }
                    }
                    0
                }
                Err(_) => {
                    stdout.push_str("No packages installed.\n");
                    0
                }
            }
        }
        "run" | "exec" => {
            if args.len() < 2 {
                stderr.push_str("pkg run: usage: pkg run <name> [args]\n");
                return 1;
            }
            let name = args[1];
            let pkg_path = format!("/var/packages/{}", name);

            // Read the package script
            match syscall::open(&pkg_path, syscall::OpenFlags::READ) {
                Ok(fd) => {
                    let mut buf = vec![0u8; 65536];
                    match syscall::read(fd, &mut buf) {
                        Ok(n) => {
                            let _ = syscall::close(fd);
                            let script = String::from_utf8_lossy(&buf[..n]).to_string();

                            // Execute each line of the script
                            for line in script.lines() {
                                let line = line.trim();
                                if line.is_empty() || line.starts_with('#') {
                                    continue;
                                }
                                // Note: In a full implementation, we'd parse and execute properly
                                stdout.push_str(&format!("> {}\n", line));
                            }
                            0
                        }
                        Err(e) => {
                            let _ = syscall::close(fd);
                            stderr.push_str(&format!("pkg run: failed to read '{}': {:?}\n", name, e));
                            1
                        }
                    }
                }
                Err(_) => {
                    stderr.push_str(&format!("pkg run: package '{}' not found\n", name));
                    1
                }
            }
        }
        "info" | "show" => {
            if args.len() < 2 {
                stderr.push_str("pkg info: usage: pkg info <name>\n");
                return 1;
            }
            let name = args[1];
            let pkg_path = format!("/var/packages/{}", name);

            match syscall::metadata(&pkg_path) {
                Ok(meta) => {
                    stdout.push_str(&format!("Package: {}\n", name));
                    stdout.push_str(&format!("Path: {}\n", pkg_path));
                    stdout.push_str(&format!("Size: {} bytes\n", meta.size));

                    // Show first few lines of script
                    if let Ok(fd) = syscall::open(&pkg_path, syscall::OpenFlags::READ) {
                        let mut buf = vec![0u8; 512];
                        if let Ok(n) = syscall::read(fd, &mut buf) {
                            let preview = String::from_utf8_lossy(&buf[..n]);
                            stdout.push_str("\nScript preview:\n");
                            for (i, line) in preview.lines().take(5).enumerate() {
                                stdout.push_str(&format!("  {}: {}\n", i + 1, line));
                            }
                        }
                        let _ = syscall::close(fd);
                    }
                    0
                }
                Err(_) => {
                    stderr.push_str(&format!("pkg info: package '{}' not found\n", name));
                    1
                }
            }
        }
        cmd => {
            stderr.push_str(&format!("pkg: unknown command '{}'\n", cmd));
            stderr.push_str("Try 'pkg --help' for available commands.\n");
            1
        }
    }
}

// ========== USER LOGIN COMMANDS ==========

/// login - log in as a user with password authentication
/// This behaves like real Linux login(1): it spawns a NEW shell process
/// as the target user with proper session management.
fn prog_login(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
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
fn prog_logout(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
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
fn prog_who(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
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
fn prog_w(args: &[String], stdin: &str, stdout: &mut String, _stderr: &mut String) -> i32 {
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

// ========== CRON/SCHEDULING COMMANDS ==========

/// crontab - maintain cron tables
///
/// Crontab format: minute hour day month weekday command
/// Special strings: @reboot, @hourly, @daily, @weekly, @monthly
fn prog_crontab(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: crontab [-l | -e | -r] [file]\n\nMaintain cron tables for scheduled jobs.\n\nOptions:\n  -l        List current crontab\n  -e        Edit crontab (prints current, use crontab file to set)\n  -r        Remove crontab\n  file      Install crontab from file\n\nCrontab format:\n  minute hour day month weekday command\n  @reboot  Run at startup\n  @hourly  Run every hour (0 * * * *)\n  @daily   Run daily (0 0 * * *)\n\nExamples:\n  */5 * * * * echo 'every 5 min'    Run every 5 minutes\n  0 * * * * date                    Run at the top of every hour\n  @reboot /var/packages/startup     Run at boot") {
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
        let uid = kernel.current_process()
            .map(|p| p.uid.0)
            .unwrap_or(1000);
        kernel.users().get_user(crate::kernel::users::Uid(uid))
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

    match &args[0][..] {
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
            stderr.push_str("crontab: use 'crontab <file>' or 'echo ... > /var/spool/cron/username'\n");
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

                                    stdout.push_str(&format!("crontab: installed {} entries for {}\n", entry_count, username));
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
fn prog_at(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);

    if let Some(help) = check_help(&args, "Usage: at <time> <command>\n       at -l         List pending jobs\n       at -r <id>    Remove a job\n\nSchedule a command to run at a specific time.\n\nTime formats:\n  +5m    5 minutes from now\n  +1h    1 hour from now\n  +30s   30 seconds from now\n\nExamples:\n  at +5m echo 'Hello'     Run in 5 minutes\n  at +1h date             Run in 1 hour") {
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

    match &args[0][..] {
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
            let delay_ms: u64 = if time_spec.starts_with('+') {
                let spec = &time_spec[1..];
                if spec.ends_with('s') {
                    spec[..spec.len()-1].parse::<u64>().unwrap_or(0) * 1000
                } else if spec.ends_with('m') {
                    spec[..spec.len()-1].parse::<u64>().unwrap_or(0) * 60 * 1000
                } else if spec.ends_with('h') {
                    spec[..spec.len()-1].parse::<u64>().unwrap_or(0) * 60 * 60 * 1000
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

                    stdout.push_str(&format!("Job {} scheduled to run in {}\n", job_id, time_spec));
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

    fn setup_kernel() {
        use crate::kernel::syscall::{KERNEL, Kernel};
        use crate::kernel::users::{Uid, Gid};
        KERNEL.with(|k| {
            *k.borrow_mut() = Kernel::new();
            let pid = k.borrow_mut().spawn_process("shell", None);
            k.borrow_mut().set_current(pid);
            // Set test process to run as root for permission checks
            if let Some(proc) = k.borrow_mut().current_process_mut() {
                proc.uid = Uid(0);
                proc.euid = Uid(0);
                proc.gid = Gid(0);
                proc.egid = Gid(0);
            }
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

        // Create test directory
        exec.execute_line("mkdir /test_cd");

        // cd to it
        let result = exec.execute_line("cd /test_cd");
        assert_eq!(result.code, 0, "cd failed: {}", result.error);

        // Verify shell state updated
        assert_eq!(exec.state.cwd.display().to_string(), "/test_cd");

        // Verify PWD env var updated
        assert_eq!(exec.state.get_env("PWD"), Some("/test_cd"));
    }

    #[test]
    fn test_cd_then_ls_relative_path() {
        setup_kernel();
        let mut exec = Executor::new();

        // Create directory structure
        exec.execute_line("mkdir /test_ls");
        exec.execute_line("touch /test_ls/file1.txt");
        exec.execute_line("touch /test_ls/file2.txt");

        // cd to the directory
        let result = exec.execute_line("cd /test_ls");
        assert_eq!(result.code, 0, "cd failed: {}", result.error);

        // ls with current directory (relative path)
        let result = exec.execute_line("ls .");
        assert_eq!(result.code, 0, "ls . failed: {}", result.error);
        assert!(result.output.contains("file1.txt"), "ls output missing file1.txt: {}", result.output);
        assert!(result.output.contains("file2.txt"), "ls output missing file2.txt: {}", result.output);
    }

    #[test]
    fn test_ls_without_args_uses_cwd() {
        setup_kernel();
        let mut exec = Executor::new();

        // Create and cd to directory
        exec.execute_line("mkdir /test_ls_cwd");
        exec.execute_line("touch /test_ls_cwd/myfile.txt");
        exec.execute_line("cd /test_ls_cwd");

        // ls without arguments should list current directory
        let result = exec.execute_line("ls");
        assert_eq!(result.code, 0, "ls failed: {}", result.error);
        assert!(result.output.contains("myfile.txt"), "ls output missing myfile.txt: {}", result.output);
    }

    #[test]
    fn test_cat_relative_path() {
        setup_kernel();
        let mut exec = Executor::new();

        // Create a file with content
        exec.execute_line("mkdir /test_cat");
        exec.execute_line("echo hello world > /test_cat/greeting.txt");
        exec.execute_line("cd /test_cat");

        // cat with relative path
        let result = exec.execute_line("cat greeting.txt");
        assert_eq!(result.code, 0, "cat failed: {}", result.error);
        assert!(result.output.contains("hello world"), "cat output wrong: {}", result.output);
    }

    #[test]
    fn test_cd_to_nonexistent() {
        setup_kernel();
        let mut exec = Executor::new();

        let result = exec.execute_line("cd /nonexistent_dir_xyz");
        assert_ne!(result.code, 0);
        assert!(result.error.contains("No such file"));
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
        assert_eq!(code, 0, "grep failed with stderr: {}", stderr);
        // Output contains ANSI codes highlighting "ap", so check for the pattern and rest of words
        // Strip ANSI codes for easier checking
        let plain = strip_ansi(&stdout);
        assert!(plain.contains("apple"), "stdout missing apple: {:?}", plain);
        assert!(plain.contains("apricot"), "stdout missing apricot: {:?}", plain);
        assert!(!plain.contains("banana"), "stdout should not have banana: {:?}", plain);
    }

    /// Strip ANSI escape codes from a string
    fn strip_ansi(s: &str) -> String {
        let mut result = String::new();
        let mut in_escape = false;
        for c in s.chars() {
            if c == '\x1b' {
                in_escape = true;
            } else if in_escape {
                if c == 'm' {
                    in_escape = false;
                }
            } else {
                result.push(c);
            }
        }
        result
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
        let args: Vec<String> = vec![];
        let stdin = "b\na\na\nc\nb";
        prog_sort(&args, stdin, &mut stdout, &mut stderr);

        // Feed to uniq (use sorted output as stdin)
        let sorted = stdout.clone();
        stdout.clear();
        prog_uniq(&args, &sorted, &mut stdout, &mut stderr);

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

    // ============ I/O Redirections ============

    /// Helper to set up test environment (initializes kernel and creates /tmp)
    fn setup_redirect_test() -> Executor {
        // Initialize kernel with a test process
        syscall::KERNEL.with(|k| {
            use crate::kernel::syscall::Kernel;
            *k.borrow_mut() = Kernel::new();
            let pid = k.borrow_mut().spawn_process("test", None);
            k.borrow_mut().set_current(pid);
        });

        let mut exec = Executor::new();
        // Create /tmp directory for tests
        exec.execute_line("mkdir /tmp");
        exec
    }

    #[test]
    fn test_redirect_stdout_to_file() {
        let mut exec = setup_redirect_test();

        // Create a file via redirect
        let result = exec.execute_line("echo hello world > /tmp/test_redirect.txt");
        assert_eq!(result.code, 0, "echo failed: {}", result.error);
        assert!(result.output.is_empty()); // Output went to file, not stdout

        // Read the file back
        let result = exec.execute_line("cat /tmp/test_redirect.txt");
        assert_eq!(result.code, 0, "cat failed: {}", result.error);
        assert_eq!(result.output.trim(), "hello world");
    }

    #[test]
    fn test_redirect_stdout_overwrite() {
        let mut exec = setup_redirect_test();

        // Write first content
        exec.execute_line("echo first > /tmp/test_overwrite.txt");

        // Overwrite with new content
        exec.execute_line("echo second > /tmp/test_overwrite.txt");

        // Verify only second content exists
        let result = exec.execute_line("cat /tmp/test_overwrite.txt");
        assert_eq!(result.output.trim(), "second");
    }

    #[test]
    fn test_redirect_stdout_append() {
        let mut exec = setup_redirect_test();

        // Write first line
        let r1 = exec.execute_line("echo line1 > /tmp/test_append.txt");
        assert_eq!(r1.code, 0, "first echo failed: {}", r1.error);

        // Append second line
        let r2 = exec.execute_line("echo line2 >> /tmp/test_append.txt");
        assert_eq!(r2.code, 0, "second echo failed: {}", r2.error);

        // Verify both lines exist
        let result = exec.execute_line("cat /tmp/test_append.txt");
        assert_eq!(result.code, 0, "cat failed: {}", result.error);
        assert!(result.output.contains("line1"), "missing line1 in: {:?}", result.output);
        assert!(result.output.contains("line2"), "missing line2 in: {:?}", result.output);
    }

    #[test]
    fn test_redirect_stdin_from_file() {
        let mut exec = setup_redirect_test();

        // Create a file with content
        exec.execute_line("echo apple banana cherry > /tmp/test_stdin.txt");

        // Use input redirection with grep
        let result = exec.execute_line("grep banana < /tmp/test_stdin.txt");
        assert_eq!(result.code, 0);
        assert!(result.output.contains("banana"));
    }

    #[test]
    fn test_redirect_pipeline_to_file() {
        let mut exec = setup_redirect_test();

        // Create source file with multiline content
        exec.execute_line("echo cherry > /tmp/test_pipe_src.txt");
        exec.execute_line("echo apple >> /tmp/test_pipe_src.txt");
        exec.execute_line("echo banana >> /tmp/test_pipe_src.txt");

        // Pipeline with final output redirect
        let result = exec.execute_line("cat /tmp/test_pipe_src.txt | sort > /tmp/test_pipe_dst.txt");
        assert_eq!(result.code, 0);

        // Verify sorted output
        let result = exec.execute_line("cat /tmp/test_pipe_dst.txt");
        let lines: Vec<&str> = result.output.lines().collect();
        assert!(!lines.is_empty());
        // First line should be alphabetically first
        assert!(lines[0].contains("apple"));
    }

    #[test]
    fn test_redirect_relative_path() {
        let mut exec = setup_redirect_test();
        // cwd is /home by default, ensure /home/user exists
        exec.execute_line("mkdir /home");
        exec.execute_line("mkdir /home/user");
        exec.execute_line("cd /home/user");

        // Write to relative path
        exec.execute_line("echo relative test > reltest.txt");

        // Read back with absolute path
        let result = exec.execute_line("cat /home/user/reltest.txt");
        assert_eq!(result.output.trim(), "relative test");
    }

    #[test]
    fn test_redirect_file_not_found() {
        let mut exec = setup_redirect_test();

        // Try to read from non-existent file
        let result = exec.execute_line("cat < /nonexistent/file.txt");
        assert!(result.code != 0 || !result.error.is_empty());
    }

    #[test]
    fn test_redirect_wc_from_file() {
        let mut exec = setup_redirect_test();

        // Create a file with known content
        exec.execute_line("echo one two three > /tmp/test_wc.txt");
        exec.execute_line("echo four five >> /tmp/test_wc.txt");

        // Count words from file
        let result = exec.execute_line("wc < /tmp/test_wc.txt");
        assert_eq!(result.code, 0);
        // wc output should contain line/word/char counts
        assert!(!result.output.is_empty());
    }

    // ============ Glob Pattern Matching ============

    #[test]
    fn test_glob_match_star() {
        assert!(glob_match("*.txt", "file.txt"));
        assert!(glob_match("*.txt", "another.txt"));
        assert!(!glob_match("*.txt", "file.rs"));
        assert!(!glob_match("*.txt", "file.txt.bak"));
    }

    #[test]
    fn test_glob_match_question() {
        assert!(glob_match("file?.txt", "file1.txt"));
        assert!(glob_match("file?.txt", "fileA.txt"));
        assert!(!glob_match("file?.txt", "file.txt"));
        assert!(!glob_match("file?.txt", "file12.txt"));
    }

    #[test]
    fn test_glob_match_bracket() {
        assert!(glob_match("[abc].txt", "a.txt"));
        assert!(glob_match("[abc].txt", "b.txt"));
        assert!(!glob_match("[abc].txt", "d.txt"));
    }

    #[test]
    fn test_glob_match_bracket_range() {
        assert!(glob_match("[a-z].txt", "f.txt"));
        assert!(!glob_match("[a-z].txt", "F.txt"));
        assert!(glob_match("[0-9].txt", "5.txt"));
    }

    #[test]
    fn test_glob_match_complex() {
        assert!(glob_match("test_*.rs", "test_foo.rs"));
        assert!(glob_match("src/*/*.rs", "src/shell/mod.rs"));
    }

    #[test]
    fn test_glob_expansion_simple() {
        let mut exec = setup_redirect_test();
        exec.execute_line("mkdir /tmp/glob");
        exec.execute_line("touch /tmp/glob/file1.txt");
        exec.execute_line("touch /tmp/glob/file2.txt");
        exec.execute_line("touch /tmp/glob/other.rs");
        exec.execute_line("cd /tmp/glob");

        // ls with glob should expand
        let result = exec.execute_line("echo *.txt");
        assert!(result.output.contains("file1.txt"));
        assert!(result.output.contains("file2.txt"));
        assert!(!result.output.contains("other.rs"));
    }

    // ============ Aliases ============

    #[test]
    fn test_alias_set_and_use() {
        let mut exec = Executor::new();
        exec.execute_line("alias ll='ls -la'");

        assert!(exec.state.aliases.contains_key("ll"));
        assert_eq!(exec.state.get_alias("ll"), Some("ls -la"));
    }

    #[test]
    fn test_alias_list() {
        let mut exec = Executor::new();
        exec.execute_line("alias foo=bar");
        exec.execute_line("alias baz=qux");

        let result = exec.execute_line("alias");
        assert!(result.output.contains("foo"));
        assert!(result.output.contains("bar"));
        assert!(result.output.contains("baz"));
    }

    #[test]
    fn test_unalias() {
        let mut exec = Executor::new();
        exec.execute_line("alias test=echo");
        assert!(exec.state.aliases.contains_key("test"));

        exec.execute_line("unalias test");
        assert!(!exec.state.aliases.contains_key("test"));
    }

    #[test]
    fn test_alias_expansion() {
        let mut exec = Executor::new();
        exec.execute_line("alias greet='echo hello'");

        let result = exec.execute_line("greet world");
        assert_eq!(result.output, "hello world");
    }

    // ============ Command Substitution ============

    #[test]
    fn test_substitution_dollar_paren() {
        let mut exec = Executor::new();
        let result = exec.execute_line("echo $(echo hello)");
        assert_eq!(result.output, "hello");
    }

    #[test]
    fn test_substitution_nested() {
        let mut exec = Executor::new();
        let result = exec.execute_line("echo $(echo $(echo nested))");
        assert_eq!(result.output, "nested");
    }

    #[test]
    fn test_substitution_in_args() {
        let mut exec = Executor::new();
        exec.state.set_env("TEST_VAL", "world");
        let result = exec.execute_line("echo hello $(echo world)");
        assert_eq!(result.output, "hello world");
    }

    // ============ Symlinks ============

    #[test]
    fn test_symlink_create() {
        let mut exec = setup_redirect_test();
        exec.execute_line("echo content > /tmp/original.txt");

        let result = exec.execute_line("ln -s /tmp/original.txt /tmp/link.txt");
        assert_eq!(result.code, 0, "ln failed: {}", result.error);

        // Check link exists via readlink
        let result = exec.execute_line("readlink /tmp/link.txt");
        assert_eq!(result.code, 0);
        assert_eq!(result.output, "/tmp/original.txt");
    }

    #[test]
    fn test_symlink_in_ls() {
        let mut exec = setup_redirect_test();
        exec.execute_line("echo test > /tmp/target.txt");
        exec.execute_line("ln -s /tmp/target.txt /tmp/mylink");

        let result = exec.execute_line("ls /tmp");
        // Should show link with arrow
        assert!(result.output.contains("mylink"));
        assert!(result.output.contains("->"));
    }

    #[test]
    fn test_symlink_rm() {
        let mut exec = setup_redirect_test();
        exec.execute_line("echo test > /tmp/file.txt");
        exec.execute_line("ln -s /tmp/file.txt /tmp/link");

        // Remove the link
        let result = exec.execute_line("rm /tmp/link");
        assert_eq!(result.code, 0);

        // Link should be gone but original file remains
        let result = exec.execute_line("readlink /tmp/link");
        assert_ne!(result.code, 0); // Should fail - link doesn't exist

        let result = exec.execute_line("cat /tmp/file.txt");
        assert_eq!(result.code, 0); // Original still exists
    }

    #[test]
    fn test_ln_requires_s_flag() {
        let mut exec = setup_redirect_test();
        exec.execute_line("touch /tmp/src.txt");

        // Without -s should fail
        let result = exec.execute_line("ln /tmp/src.txt /tmp/dst.txt");
        assert_ne!(result.code, 0);
        assert!(result.error.contains("hard links not supported"));
    }

    // ============ Logical Operators ============

    #[test]
    fn test_and_operator_success() {
        let mut exec = Executor::new();
        let result = exec.execute_line("echo first && echo second");
        assert_eq!(result.code, 0);
        assert!(result.output.contains("first"));
        assert!(result.output.contains("second"));
    }

    #[test]
    fn test_and_operator_first_fails() {
        let mut exec = Executor::new();
        let result = exec.execute_line("false && echo should_not_run");
        assert_eq!(result.code, 1); // false returns 1
        assert!(!result.output.contains("should_not_run"));
    }

    #[test]
    fn test_or_operator_first_fails() {
        let mut exec = Executor::new();
        let result = exec.execute_line("false || echo fallback");
        assert_eq!(result.code, 0);
        assert!(result.output.contains("fallback"));
    }

    #[test]
    fn test_or_operator_first_succeeds() {
        let mut exec = Executor::new();
        let result = exec.execute_line("true || echo should_not_run");
        assert_eq!(result.code, 0);
        assert!(!result.output.contains("should_not_run"));
    }

    #[test]
    fn test_semicolon_always_runs() {
        let mut exec = Executor::new();
        let result = exec.execute_line("echo first; echo second");
        assert!(result.output.contains("first"));
        assert!(result.output.contains("second"));
    }

    #[test]
    fn test_semicolon_after_failure() {
        let mut exec = Executor::new();
        let result = exec.execute_line("false; echo runs_anyway");
        assert!(result.output.contains("runs_anyway"));
    }

    #[test]
    fn test_trailing_semicolon() {
        let mut exec = Executor::new();
        let result = exec.execute_line("echo hello;");
        assert_eq!(result.code, 0);
        assert_eq!(result.output.trim(), "hello");
    }

    #[test]
    fn test_chained_and_all_succeed() {
        let mut exec = Executor::new();
        let result = exec.execute_line("echo a && echo b && echo c");
        assert_eq!(result.code, 0);
        assert!(result.output.contains("a"));
        assert!(result.output.contains("b"));
        assert!(result.output.contains("c"));
    }

    #[test]
    fn test_chained_and_middle_fails() {
        let mut exec = Executor::new();
        let result = exec.execute_line("echo a && false && echo c");
        assert_ne!(result.code, 0);
        assert!(result.output.contains("a"));
        assert!(!result.output.contains("c")); // c should not run
    }

    #[test]
    fn test_mixed_and_or() {
        let mut exec = Executor::new();
        // true && false || echo fallback
        // true succeeds, then false runs (due to &&), then fallback runs (due to ||)
        let result = exec.execute_line("true && false || echo fallback");
        assert_eq!(result.code, 0);
        assert!(result.output.contains("fallback"));
    }

    #[test]
    fn test_complex_logic() {
        let mut exec = Executor::new();
        // false || true && echo yes
        // false fails, true runs (due to ||), then yes runs (due to &&)
        let result = exec.execute_line("false || true && echo yes");
        assert_eq!(result.code, 0);
        assert!(result.output.contains("yes"));
    }

    #[test]
    fn test_exit_in_chain() {
        let mut exec = Executor::new();
        let result = exec.execute_line("echo before && exit 42 && echo after");
        assert!(result.should_exit);
        assert_eq!(result.code, 42);
        assert!(result.output.contains("before"));
        assert!(!result.output.contains("after"));
    }
}
