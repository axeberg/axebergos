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
            let input = if let Some(ref redir) = cmd.stdin {
                match self.read_file(&redir.path) {
                    Ok(content) => content,
                    Err(e) => return ExecResult::success().with_error(e),
                }
            } else {
                String::new()
            };

            // Expand glob patterns in arguments
            let expanded_args = self.expand_args(&cmd.args);

            // Prepare args with input if needed
            let args: Vec<String> = if input.is_empty() {
                expanded_args
            } else {
                // For programs that read stdin, we pass input via a special mechanism
                let mut args = expanded_args;
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
                }
            } else if let Some(prog) = self.registry.get(&cmd.program) {
                // Pass pipe input via special arg
                let mut args = expanded_args;
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
        // Expand glob patterns in arguments
        let expanded_args = self.expand_args(&cmd.args);
        let result = builtins::execute(&cmd.program, &expanded_args, &self.state);

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
                } else if output.starts_with("__ALIAS__:") {
                    let pairs = &output["__ALIAS__:".len()..];
                    for pair in pairs.split('\x00') {
                        if let Some(eq) = pair.find('=') {
                            let name = &pair[..eq];
                            let value = &pair[eq + 1..];
                            self.state.set_alias(name, value);
                        }
                    }
                    output.clear();
                } else if output.starts_with("__UNALIAS__:") {
                    let names = &output["__UNALIAS__:".len()..];
                    for name in names.split('\x00') {
                        self.state.unalias(name);
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

    if let Some(help) = check_help(&files, "Usage: cat [FILE]...\nConcatenate files and print to stdout. See 'man cat' for details.") {
        stdout.push_str(&help);
        return 0;
    }

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
fn prog_mkdir(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, paths) = extract_stdin(args);

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
fn prog_touch(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, paths) = extract_stdin(args);

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
fn prog_rm(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_cp(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_mv(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_head(args: &[String], stdout: &mut String, _stderr: &mut String) -> i32 {
    let (stdin, args) = extract_stdin(args);

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
        stdin.unwrap_or_default()
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
fn prog_tail(args: &[String], stdout: &mut String, _stderr: &mut String) -> i32 {
    let (stdin, args) = extract_stdin(args);

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

    if let Some(help) = check_help(&args, "Usage: wc [-lwc] [FILE]\nCount lines, words, and characters. See 'man wc' for details.") {
        stdout.push_str(&help);
        return 0;
    }

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
    let input = stdin.unwrap_or_default();
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
fn prog_sort(args: &[String], stdout: &mut String, _stderr: &mut String) -> i32 {
    let (stdin, args) = extract_stdin(args);

    if let Some(help) = check_help(&args, "Usage: sort [-ru] [FILE]\nSort lines of text. See 'man sort' for details.") {
        stdout.push_str(&help);
        return 0;
    }

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

    if let Some(help) = check_help(&args, "Usage: uniq [-c] [FILE]\nFilter adjacent duplicate lines. See 'man uniq' for details.") {
        stdout.push_str(&help);
        return 0;
    }

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

    if let Some(help) = check_help(&files, "Usage: tee [-a] FILE\nCopy stdin to file and stdout. See 'man tee' for details.") {
        stdout.push_str(&help);
        return 0;
    }

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

/// save - persist filesystem to OPFS
fn prog_save(_args: &[String], stdout: &mut String, _stderr: &mut String) -> i32 {
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
    stdout.push_str("Saving filesystem...");
    0
}

/// tree - display directory tree
fn prog_tree(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, paths) = extract_stdin(args);

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
fn prog_history(args: &[String], stdout: &mut String, _stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_sleep(args: &[String], _stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_ln(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_readlink(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_edit(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_man(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
        "bg" => include_str!("../../man/formatted/bg.txt"),
        "cat" => include_str!("../../man/formatted/cat.txt"),
        "cd" => include_str!("../../man/formatted/cd.txt"),
        "cp" => include_str!("../../man/formatted/cp.txt"),
        "cut" => include_str!("../../man/formatted/cut.txt"),
        "date" => include_str!("../../man/formatted/date.txt"),
        "df" => include_str!("../../man/formatted/df.txt"),
        "dirname" => include_str!("../../man/formatted/dirname.txt"),
        "du" => include_str!("../../man/formatted/du.txt"),
        "echo" => include_str!("../../man/formatted/echo.txt"),
        "edit" => include_str!("../../man/formatted/edit.txt"),
        "fg" => include_str!("../../man/formatted/fg.txt"),
        "find" => include_str!("../../man/formatted/find.txt"),
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
        "printenv" => include_str!("../../man/formatted/printenv.txt"),
        "ps" => include_str!("../../man/formatted/ps.txt"),
        "pwd" => include_str!("../../man/formatted/pwd.txt"),
        "rev" => include_str!("../../man/formatted/rev.txt"),
        "rm" => include_str!("../../man/formatted/rm.txt"),
        "seq" => include_str!("../../man/formatted/seq.txt"),
        "sort" => include_str!("../../man/formatted/sort.txt"),
        "strace" => include_str!("../../man/formatted/strace.txt"),
        "tail" => include_str!("../../man/formatted/tail.txt"),
        "tee" => include_str!("../../man/formatted/tee.txt"),
        "time" => include_str!("../../man/formatted/time.txt"),
        "touch" => include_str!("../../man/formatted/touch.txt"),
        "tr" => include_str!("../../man/formatted/tr.txt"),
        "tree" => include_str!("../../man/formatted/tree.txt"),
        "uname" => include_str!("../../man/formatted/uname.txt"),
        "uniq" => include_str!("../../man/formatted/uniq.txt"),
        "wc" => include_str!("../../man/formatted/wc.txt"),
        "whoami" => include_str!("../../man/formatted/whoami.txt"),
        "xargs" => include_str!("../../man/formatted/xargs.txt"),
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
fn prog_printenv(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_id(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

    if let Some(help) = check_help(&args, "Usage: id\nPrint process ID, parent PID, and process group.") {
        stdout.push_str(&help);
        return 0;
    }

    // Get process info from kernel
    match syscall::getpid() {
        Ok(pid) => {
            let ppid = syscall::getppid().ok().flatten();
            let pgid = syscall::getpgid(pid).ok();

            stdout.push_str(&format!("pid={}", pid.0));
            if let Some(ppid) = ppid {
                stdout.push_str(&format!(" ppid={}", ppid.0));
            } else {
                stdout.push_str(" ppid=(none)");
            }
            if let Some(pgid) = pgid {
                stdout.push_str(&format!(" pgid={}", pgid.0));
            }
            stdout.push('\n');

            // Also show environment info
            if let Ok(Some(user)) = syscall::getenv("USER") {
                stdout.push_str(&format!("user={}\n", user));
            }
            if let Ok(Some(home)) = syscall::getenv("HOME") {
                stdout.push_str(&format!("home={}\n", home));
            }

            0
        }
        Err(e) => {
            stderr.push_str(&format!("id: {}\n", e));
            1
        }
    }
}

/// jobs - list background jobs
fn prog_jobs(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_fg(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_bg(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_strace(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_whoami(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

    if let Some(help) = check_help(&args, "Usage: whoami\nPrint effective username.") {
        stdout.push_str(&help);
        return 0;
    }

    match syscall::getenv("USER") {
        Ok(Some(user)) => {
            stdout.push_str(&user);
            stdout.push('\n');
            0
        }
        Ok(None) => {
            stderr.push_str("whoami: cannot find username\n");
            1
        }
        Err(e) => {
            stderr.push_str(&format!("whoami: {}\n", e));
            1
        }
    }
}

/// hostname - show or set system hostname
fn prog_hostname(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_uname(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_find(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_du(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_df(args: &[String], stdout: &mut String, _stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

    if let Some(help) = check_help(&args, "Usage: df [-h]\nShow filesystem disk space usage.") {
        stdout.push_str(&help);
        return 0;
    }

    let human_readable = args.iter().any(|a| *a == "-h");

    // Get memory stats as a proxy for "disk" usage in our VFS
    let mem_stats = syscall::system_memstats().unwrap_or_default();

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
fn prog_ps(args: &[String], stdout: &mut String, _stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_kill(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_time(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_date(args: &[String], stdout: &mut String, _stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_seq(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_yes(args: &[String], stdout: &mut String, _stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_basename(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_dirname(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (_, args) = extract_stdin(args);

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
fn prog_rev(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (stdin, args) = extract_stdin(args);

    if let Some(help) = check_help(&args, "Usage: rev [FILE]\nReverse characters in each line.") {
        stdout.push_str(&help);
        return 0;
    }

    let content = if let Some(input) = stdin {
        input
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
fn prog_cut(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (stdin, args) = extract_stdin(args);

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

    let content = if let Some(input) = stdin {
        input
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
fn prog_tr(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (stdin, args) = extract_stdin(args);

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

    let content = stdin.unwrap_or_default();

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
fn prog_xargs(args: &[String], stdout: &mut String, stderr: &mut String) -> i32 {
    let (stdin, args) = extract_stdin(args);

    if let Some(help) = check_help(&args, "Usage: xargs [COMMAND] [ARGS]\nBuild command lines from stdin.") {
        stdout.push_str(&help);
        return 0;
    }

    // Get the command to run (default: echo)
    let cmd = if args.is_empty() { "echo" } else { args[0] };
    let cmd_args: Vec<&str> = if args.len() > 1 { args[1..].to_vec() } else { vec![] };

    // Read items from stdin
    let items: Vec<&str> = stdin
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .collect();

    if items.is_empty() {
        return 0;
    }

    // For now, just show what would be executed
    // (In a full implementation we'd actually run the command)
    let full_cmd = format!("{} {} {}", cmd, cmd_args.join(" "), items.join(" "));
    stdout.push_str(&format!("xargs: would execute: {}\n", full_cmd.trim()));

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
