//! Shell command executor
//!
//! Executes parsed pipelines by:
//! 1. Setting up pipes between commands
//! 2. Handling input/output redirections
//! 3. Running built-in commands directly
//! 4. Running external commands via the program registry
//! 5. Running WASM command modules from /bin

use super::builtins::{self, BuiltinResult, ShellState};
use super::parser::{CommandList, LogicalOp, Pipeline, SimpleCommand};
use super::programs;
use crate::kernel::syscall;
use crate::kernel::wasm::WasmCommandRunner;
use std::collections::HashMap;
use std::path::Path;

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
pub type ProgramFn =
    fn(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32;

/// Registry of available programs
pub struct ProgramRegistry {
    programs: HashMap<String, ProgramFn>,
}

impl ProgramRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            programs: HashMap::new(),
        };

        // File operations
        reg.register("cat", programs::prog_cat);
        reg.register("ls", programs::prog_ls);
        reg.register("mkdir", programs::prog_mkdir);
        reg.register("touch", programs::prog_touch);
        reg.register("rm", programs::prog_rm);
        reg.register("cp", programs::prog_cp);
        reg.register("mv", programs::prog_mv);
        reg.register("ln", programs::prog_ln);
        reg.register("readlink", programs::prog_readlink);
        reg.register("tree", programs::prog_tree);

        // Text processing
        reg.register("head", programs::prog_head);
        reg.register("tail", programs::prog_tail);
        reg.register("wc", programs::prog_wc);
        reg.register("grep", programs::prog_grep);
        reg.register("sort", programs::prog_sort);
        reg.register("uniq", programs::prog_uniq);
        reg.register("tee", programs::prog_tee);
        reg.register("rev", programs::prog_rev);
        reg.register("cut", programs::prog_cut);
        reg.register("tr", programs::prog_tr);
        reg.register("nl", programs::prog_nl);
        reg.register("fold", programs::prog_fold);
        reg.register("paste", programs::prog_paste);
        reg.register("comm", programs::prog_comm);
        reg.register("strings", programs::prog_strings);
        reg.register("diff", programs::prog_diff);

        // Filesystem management
        reg.register("save", programs::prog_save);
        reg.register("fsload", programs::prog_fsload);
        reg.register("fsreset", programs::prog_fsreset);
        reg.register("autosave", programs::prog_autosave);
        reg.register("find", programs::prog_find);
        reg.register("du", programs::prog_du);
        reg.register("df", programs::prog_df);

        // Network
        reg.register("curl", programs::prog_curl);
        reg.register("wget", programs::prog_wget);

        // System info
        reg.register("whoami", programs::prog_whoami);
        reg.register("hostname", programs::prog_hostname);
        reg.register("uname", programs::prog_uname);
        reg.register("uptime", programs::prog_uptime);
        reg.register("free", programs::prog_free);
        reg.register("id", programs::prog_id);
        reg.register("groups", programs::prog_groups);
        reg.register("ps", programs::prog_ps);
        reg.register("date", programs::prog_date);
        reg.register("time", programs::prog_time);

        // Process control
        reg.register("jobs", programs::prog_jobs);
        reg.register("fg", programs::prog_fg);
        reg.register("bg", programs::prog_bg);
        reg.register("strace", programs::prog_strace);
        reg.register("kill", programs::prog_kill);
        reg.register("sleep", programs::prog_sleep);

        // Shell utilities
        reg.register("history", programs::prog_history);
        reg.register("clear", programs::prog_clear);
        reg.register("printf", programs::prog_printf);
        reg.register("test", programs::prog_test);
        reg.register("[", programs::prog_test); // [ is an alias for test
        reg.register("expr", programs::prog_expr);
        reg.register("which", programs::prog_which);
        reg.register("type", programs::prog_type);
        reg.register("xargs", programs::prog_xargs);
        reg.register("seq", programs::prog_seq);
        reg.register("yes", programs::prog_yes);
        reg.register("basename", programs::prog_basename);
        reg.register("dirname", programs::prog_dirname);
        reg.register("cal", programs::prog_cal);
        reg.register("edit", programs::prog_edit);
        reg.register("man", programs::prog_man);
        reg.register("printenv", programs::prog_printenv);

        // Encoding utilities
        reg.register("base64", programs::prog_base64);
        reg.register("xxd", programs::prog_xxd);

        // User management
        reg.register("su", programs::prog_su);
        reg.register("sudo", programs::prog_sudo);
        reg.register("useradd", programs::prog_useradd);
        reg.register("groupadd", programs::prog_groupadd);
        reg.register("passwd", programs::prog_passwd);
        reg.register("login", programs::prog_login);
        reg.register("logout", programs::prog_logout);
        reg.register("who", programs::prog_who);
        reg.register("w", programs::prog_w);

        // Permissions
        reg.register("chmod", programs::prog_chmod);
        reg.register("chown", programs::prog_chown);
        reg.register("chgrp", programs::prog_chgrp);

        // System services
        reg.register("systemctl", programs::prog_systemctl);
        reg.register("reboot", programs::prog_reboot);
        reg.register("poweroff", programs::prog_poweroff);

        // IPC
        reg.register("mkfifo", programs::prog_mkfifo);
        reg.register("ipcs", programs::prog_ipcs);
        reg.register("ipcrm", programs::prog_ipcrm);

        // Mount
        reg.register("mount", programs::prog_mount);
        reg.register("umount", programs::prog_umount);
        reg.register("findmnt", programs::prog_findmnt);

        // TTY
        reg.register("stty", programs::prog_stty);
        reg.register("tty", programs::prog_tty);

        // Package manager
        reg.register("pkg", programs::prog_pkg);

        // Cron/scheduling
        reg.register("crontab", programs::prog_crontab);
        reg.register("at", programs::prog_at);

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
    /// WASM command runner for executing /bin/*.wasm modules
    pub wasm_runner: WasmCommandRunner,
}

impl Executor {
    pub fn new() -> Self {
        let state = ShellState::new();
        // Sync kernel process cwd with shell's initial cwd
        if let Err(_e) = syscall::chdir(&state.cwd.display().to_string()) {
            #[cfg(all(target_arch = "wasm32", not(test)))]
            crate::console_log!("[shell] Warning: Failed to set initial cwd: {:?}", _e);
        }

        // Initialize WASM runner with default PATH
        let mut wasm_runner = WasmCommandRunner::new();
        wasm_runner.add_env("PATH", "/bin:/usr/bin:/usr/local/bin");
        wasm_runner.set_cwd(&state.cwd.display().to_string());

        Self {
            state,
            registry: ProgramRegistry::new(),
            wasm_runner,
        }
    }

    /// Sync WASM runner state with shell state
    #[cfg(target_arch = "wasm32")]
    fn sync_wasm_runner(&mut self) {
        self.wasm_runner
            .set_cwd(&self.state.cwd.display().to_string());
        // Sync environment variables
        let mut env = HashMap::new();
        for (k, v) in &self.state.env {
            env.insert(k.clone(), v.clone());
        }
        self.wasm_runner.set_env(env);
    }

    /// Check if a command is available as a WASM module
    pub fn is_wasm_command(&self, name: &str) -> bool {
        self.wasm_runner.command_exists(name)
    }

    /// Execute a WASM command asynchronously
    #[cfg(target_arch = "wasm32")]
    pub async fn execute_wasm_command(
        &mut self,
        name: &str,
        args: &[String],
        stdin: &str,
    ) -> ExecResult {
        self.sync_wasm_runner();

        match self.wasm_runner.run(name, args, stdin).await {
            Ok(result) => {
                self.state.last_status = result.exit_code;
                ExecResult {
                    code: result.exit_code,
                    output: result.stdout_str(),
                    error: result.stderr_str(),
                    should_exit: false,
                }
            }
            Err(e) => {
                self.state.last_status = 1;
                ExecResult::success()
                    .with_error(format!("{}: {}", name, e))
                    .with_code(1)
            }
        }
    }

    /// Execute a WASM command (non-WASM stub)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn execute_wasm_command(
        &mut self,
        name: &str,
        _args: &[String],
        _stdin: &str,
    ) -> ExecResult {
        self.state.last_status = 127;
        ExecResult::success()
            .with_error(format!("{}: command not found", name))
            .with_code(127)
    }

    /// Execute a single command, trying WASM if not built-in or in registry
    /// Returns Some(result) if executed synchronously, None if WASM command needs async
    pub fn try_execute_sync(&mut self, cmd: &SimpleCommand) -> Option<ExecResult> {
        // Built-ins are always sync
        if builtins::is_builtin(&cmd.program) {
            return Some(self.execute_builtin(cmd));
        }

        // Registry programs are sync
        if self.registry.contains(&cmd.program) {
            return Some(self.execute_single(cmd));
        }

        // Check for WASM command - return None to indicate async needed
        if self.is_wasm_command(&cmd.program) {
            return None;
        }

        // Command not found
        self.state.last_status = 127;
        Some(
            ExecResult::success()
                .with_error(format!("{}: command not found", cmd.program))
                .with_code(127),
        )
    }

    /// List available WASM commands
    pub fn list_wasm_commands(&self) -> Vec<String> {
        self.wasm_runner.list_commands()
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
                LogicalOp::Sequence => true,        // Always execute
                LogicalOp::And => result.code == 0, // Execute if previous succeeded
                LogicalOp::Or => result.code != 0,  // Execute if previous failed
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

        // Handle external programs from registry
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

        // Check for WASM command - note: this requires async execution
        // For sync execution, we return a special message indicating WASM
        if self.is_wasm_command(&cmd.program) {
            // Return a marker indicating WASM command needs async execution
            // The caller should use execute_wasm_command instead
            return ExecResult::success()
                .with_error(format!(
                    "{}: WASM command - use execute_single_async for async execution",
                    cmd.program
                ))
                .with_code(126); // 126 = command found but cannot execute (needs async)
        }

        // Command not found
        self.state.last_status = 127;
        ExecResult::success()
            .with_error(format!("{}: command not found", cmd.program))
            .with_code(127)
    }

    /// Execute a single command asynchronously (supports WASM commands)
    #[cfg(target_arch = "wasm32")]
    pub async fn execute_single_async(&mut self, cmd: &SimpleCommand) -> ExecResult {
        // Handle built-in commands (sync)
        if builtins::is_builtin(&cmd.program) {
            return self.execute_builtin(cmd);
        }

        // Handle registry programs (sync)
        if self.registry.contains(&cmd.program) {
            return self.execute_single(cmd);
        }

        // Handle WASM commands (async)
        if self.is_wasm_command(&cmd.program) {
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

            // Execute WASM command
            let mut result = self.execute_wasm_command(&cmd.program, &args, &stdin).await;

            // Handle output redirection
            if let Some(ref redir) = cmd.stdout {
                if let Err(e) = self.write_file(&redir.path, &result.output, redir.append) {
                    return ExecResult::success().with_error(e);
                }
                result.output.clear();
            }

            // Handle stderr redirection
            if let Some(ref redir) = cmd.stderr {
                if let Err(e) = self.write_file(&redir.path, &result.error, redir.append) {
                    return ExecResult::success().with_error(e);
                }
                result.error.clear();
            }

            return result;
        }

        // Command not found
        self.state.last_status = 127;
        ExecResult::success()
            .with_error(format!("{}: command not found", cmd.program))
            .with_code(127)
    }

    /// Execute a single command asynchronously (non-WASM stub)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn execute_single_async(&mut self, cmd: &SimpleCommand) -> ExecResult {
        self.execute_single(cmd)
    }

    /// Execute a pipeline of commands asynchronously
    #[cfg(target_arch = "wasm32")]
    pub async fn execute_piped_async(&mut self, commands: &[SimpleCommand]) -> ExecResult {
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
                    BuiltinResult::Alias { name, value } => {
                        self.state.set_alias(&name, &value);
                        last_code = 0;
                    }
                    BuiltinResult::Unalias(name) => {
                        self.state.remove_alias(&name);
                        last_code = 0;
                    }
                    BuiltinResult::Source(path) => {
                        let result = self.source_file(&path);
                        last_code = result.code;
                        stdout = result.output;
                        stderr = result.error;
                    }
                }
            } else if let Some(prog) = self.registry.get(&cmd.program) {
                // Registry program - pass pipe_input as stdin
                let args_ref: Vec<&str> = expanded_args.iter().map(|s| s.as_str()).collect();
                match prog.run(&args_ref, &pipe_input) {
                    Ok(output) => {
                        stdout = output;
                        last_code = 0;
                    }
                    Err(e) => {
                        stderr = format!("{}: {}", cmd.program, e);
                        last_code = 1;
                    }
                }
            } else if self.is_wasm_command(&cmd.program) {
                // WASM command - execute async with pipe_input
                let result = self
                    .execute_wasm_command(&cmd.program, &expanded_args, &pipe_input)
                    .await;
                stdout = result.output;
                stderr = result.error;
                last_code = result.code;
            } else {
                // Command not found
                stderr = format!("{}: command not found", cmd.program);
                last_code = 127;
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

    /// Execute a pipeline of commands asynchronously (non-WASM stub)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn execute_piped_async(&mut self, commands: &[SimpleCommand]) -> ExecResult {
        self.execute_piped(commands)
    }

    /// Execute a parsed pipeline asynchronously
    #[cfg(target_arch = "wasm32")]
    pub async fn execute_pipeline_async(&mut self, pipeline: &Pipeline) -> ExecResult {
        if pipeline.commands.is_empty() {
            return ExecResult::success();
        }

        // For single commands, execute directly
        if pipeline.commands.len() == 1 {
            return self.execute_single_async(&pipeline.commands[0]).await;
        }

        // For pipelines, chain the commands
        self.execute_piped_async(&pipeline.commands).await
    }

    /// Execute a parsed pipeline asynchronously (non-WASM stub)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn execute_pipeline_async(&mut self, pipeline: &Pipeline) -> ExecResult {
        self.execute_pipeline(pipeline)
    }

    /// Execute a command list asynchronously (multiple pipelines with &&, ||, ;)
    #[cfg(target_arch = "wasm32")]
    pub async fn execute_command_list_async(&mut self, cmd_list: &CommandList) -> ExecResult {
        // Execute the first pipeline
        let mut result = self.execute_pipeline_async(&cmd_list.first).await;

        // Short-circuit on exit
        if result.should_exit {
            return result;
        }

        // Execute remaining pipelines based on logical operators
        for (op, pipeline) in &cmd_list.rest {
            let should_execute = match op {
                LogicalOp::Sequence => true,        // Always execute
                LogicalOp::And => result.code == 0, // Execute if previous succeeded
                LogicalOp::Or => result.code != 0,  // Execute if previous failed
            };

            if should_execute {
                let next_result = self.execute_pipeline_async(pipeline).await;

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

    /// Execute a command list asynchronously (non-WASM stub)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn execute_command_list_async(&mut self, cmd_list: &CommandList) -> ExecResult {
        self.execute_command_list(cmd_list)
    }

    /// Execute a command line asynchronously (supports WASM commands)
    #[cfg(target_arch = "wasm32")]
    pub async fn execute_line_async(&mut self, line: &str) -> ExecResult {
        // Skip empty lines and comments
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return ExecResult::success();
        }

        // Expand aliases in the line
        let line = self.expand_aliases(line);

        // Expand command substitution
        let line = self.expand_substitution_in_line(&line);

        #[cfg(all(target_arch = "wasm32", not(test)))]
        crate::console_log!("[exec] Running async: {}", line);

        // Parse the command
        let cmd_list = match super::parser::parse_command_list(&line) {
            Ok(c) => c,
            Err(e) => return ExecResult::success().with_error(format!("parse error: {}", e)),
        };

        // Execute the command list asynchronously (full pipeline support)
        self.execute_command_list_async(&cmd_list).await
    }

    /// Execute a command line asynchronously (non-WASM stub)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn execute_line_async(&mut self, line: &str) -> ExecResult {
        self.execute_line(line)
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
            if is_first && let Some(ref redir) = cmd.stdin {
                match self.read_file(&redir.path) {
                    Ok(content) => pipe_input = content,
                    Err(e) => return ExecResult::success().with_error(e),
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
            BuiltinResult::Cd(path) => self.change_directory(&path),
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
    fn change_directory(&mut self, path: &Path) -> ExecResult {
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
                self.state.cwd = path.to_path_buf();
                self.state.set_env("PWD", &path_str);
                self.state.last_status = 0;
                ExecResult::success()
            }
            Ok(false) => {
                self.state.last_status = 1;
                ExecResult::success()
                    .with_error(format!("cd: {}: No such file or directory", path_str))
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

        let fd = syscall::open(&full_path, flags).map_err(|e| format!("{}: {}", path, e))?;

        syscall::write(fd, content.as_bytes()).map_err(|e| format!("{}: {}", path, e))?;

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
    fn extract_nested_paren(
        &self,
        chars: &mut std::iter::Peekable<std::str::Chars>,
    ) -> Option<String> {
        let mut content = String::new();
        let mut depth = 1;

        for c in chars.by_ref() {
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
    glob_match_chars(
        &mut pattern.chars().peekable(),
        &mut name.chars().peekable(),
    )
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
        let parts: Vec<&str> = pattern.splitn(2, ['*', '?', '[']).collect();
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
                if let Ok(meta) = syscall::metadata(&path)
                    && meta.is_dir
                {
                    expand_glob_segments(&path, remaining, results);
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
    let suffix = if parts.len() > 1 {
        parts[1].trim_start_matches('/')
    } else {
        ""
    };

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
                if let Ok(meta) = syscall::metadata(&path)
                    && meta.is_dir
                {
                    expand_glob_segments(&path, &segments[1..], results);
                }
            }
        }

        // Recurse into directories
        if let Ok(meta) = syscall::metadata(&path)
            && meta.is_dir
        {
            expand_glob_traverse(&path, suffix, results);
        }
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::programs;
    use super::*;

    fn setup_kernel() {
        use crate::kernel::syscall::{KERNEL, Kernel};
        use crate::kernel::users::{Gid, Uid};
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
        assert!(
            result.output.contains("file1.txt"),
            "ls output missing file1.txt: {}",
            result.output
        );
        assert!(
            result.output.contains("file2.txt"),
            "ls output missing file2.txt: {}",
            result.output
        );
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
        assert!(
            result.output.contains("myfile.txt"),
            "ls output missing myfile.txt: {}",
            result.output
        );
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
        assert!(
            result.output.contains("hello world"),
            "cat output wrong: {}",
            result.output
        );
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
        let code = programs::prog_wc(&args, stdin, &mut stdout, &mut stderr);
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
        let code = programs::prog_grep(&args, stdin, &mut stdout, &mut stderr);
        assert_eq!(code, 0, "grep failed with stderr: {}", stderr);
        // Output contains ANSI codes highlighting "ap", so check for the pattern and rest of words
        // Strip ANSI codes for easier checking
        let plain = strip_ansi(&stdout);
        assert!(plain.contains("apple"), "stdout missing apple: {:?}", plain);
        assert!(
            plain.contains("apricot"),
            "stdout missing apricot: {:?}",
            plain
        );
        assert!(
            !plain.contains("banana"),
            "stdout should not have banana: {:?}",
            plain
        );
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
        let code = programs::prog_sort(&args, stdin, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout, "apple\nbanana\ncherry");
    }

    #[test]
    fn test_prog_uniq() {
        let args: Vec<String> = vec![];
        let stdin = "a\na\nb\nb\nb\nc";
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = programs::prog_uniq(&args, stdin, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout, "a\nb\nc");
    }

    #[test]
    fn test_prog_head() {
        let args = vec!["-n".to_string(), "3".to_string()];
        let stdin = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12";
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = programs::prog_head(&args, stdin, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout, "1\n2\n3");
    }

    #[test]
    fn test_prog_tail() {
        let args = vec!["-n".to_string(), "2".to_string()];
        let stdin = "1\n2\n3\n4\n5";
        let mut stdout = String::new();
        let mut stderr = String::new();
        let code = programs::prog_tail(&args, stdin, &mut stdout, &mut stderr);
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
        let _exec = Executor::new();
        // Create a multi-command pipeline by setting up stdin
        // For this test, we directly test the programs
        let mut stdout = String::new();
        let mut stderr = String::new();

        // Sort
        let args: Vec<String> = vec![];
        let stdin = "b\na\na\nc\nb";
        programs::prog_sort(&args, stdin, &mut stdout, &mut stderr);

        // Feed to uniq (use sorted output as stdin)
        let sorted = stdout.clone();
        stdout.clear();
        programs::prog_uniq(&args, &sorted, &mut stdout, &mut stderr);

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
        assert!(
            result.output.contains("line1"),
            "missing line1 in: {:?}",
            result.output
        );
        assert!(
            result.output.contains("line2"),
            "missing line2 in: {:?}",
            result.output
        );
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
        let result =
            exec.execute_line("cat /tmp/test_pipe_src.txt | sort > /tmp/test_pipe_dst.txt");
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
