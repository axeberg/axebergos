//! WASM Command Runner
//!
//! High-level interface for running WASM commands from the shell.
//! Handles loading commands from /bin, executing them, and returning results.

use super::error::{CommandResult, WasmError, WasmResult};
#[cfg(target_arch = "wasm32")]
use super::executor::WasmExecutor;
#[cfg(target_arch = "wasm32")]
use super::loader::ModuleValidator;
use crate::kernel::syscall;
use std::collections::HashMap;

/// Default paths to search for WASM commands
pub const BIN_PATHS: &[&str] = &["/bin", "/usr/bin", "/usr/local/bin"];

/// WASM Command Runner
///
/// Provides a simple interface for executing WASM commands:
/// 1. Locates the command in /bin (or PATH)
/// 2. Loads and validates the WASM module
/// 3. Executes with proper environment
/// 4. Returns stdout/stderr/exit code
pub struct WasmCommandRunner {
    /// Environment variables
    env: HashMap<String, String>,
    /// Current working directory
    cwd: String,
    /// Cached module bytes (for repeated execution)
    cache: HashMap<String, Vec<u8>>,
}

impl WasmCommandRunner {
    /// Create a new command runner
    pub fn new() -> Self {
        Self {
            env: HashMap::new(),
            cwd: "/".to_string(),
            cache: HashMap::new(),
        }
    }

    /// Set environment variables
    pub fn set_env(&mut self, env: HashMap<String, String>) {
        self.env = env;
    }

    /// Add an environment variable
    pub fn add_env(&mut self, key: &str, value: &str) {
        self.env.insert(key.to_string(), value.to_string());
    }

    /// Set current working directory
    pub fn set_cwd(&mut self, cwd: &str) {
        self.cwd = cwd.to_string();
    }

    /// Check if a command exists as a WASM module
    pub fn command_exists(&self, name: &str) -> bool {
        self.find_command(name).is_some()
    }

    /// Find the path to a WASM command
    pub fn find_command(&self, name: &str) -> Option<String> {
        // Check absolute path first
        if name.starts_with('/') {
            if self.is_valid_wasm(name) {
                return Some(name.to_string());
            }
            return None;
        }

        // Check PATH environment variable
        if let Some(path_env) = self.env.get("PATH") {
            for dir in path_env.split(':') {
                let full_path = format!("{}/{}", dir, name);
                if self.is_valid_wasm(&full_path) {
                    return Some(full_path);
                }
                // Also try with .wasm extension
                let full_path_wasm = format!("{}/{}.wasm", dir, name);
                if self.is_valid_wasm(&full_path_wasm) {
                    return Some(full_path_wasm);
                }
            }
        }

        // Check default bin paths
        for dir in BIN_PATHS {
            let full_path = format!("{}/{}", dir, name);
            if self.is_valid_wasm(&full_path) {
                return Some(full_path);
            }
            let full_path_wasm = format!("{}/{}.wasm", dir, name);
            if self.is_valid_wasm(&full_path_wasm) {
                return Some(full_path_wasm);
            }
        }

        None
    }

    /// Check if a path points to a valid WASM file
    fn is_valid_wasm(&self, path: &str) -> bool {
        // Check file exists and is a regular file
        if let Ok(meta) = syscall::metadata(path)
            && meta.is_file {
                // Optionally check WASM magic number
                return true;
            }
        false
    }

    /// Run a WASM command with arguments and stdin
    ///
    /// This is the main entry point for executing WASM commands.
    #[cfg(target_arch = "wasm32")]
    pub async fn run(
        &mut self,
        name: &str,
        args: &[String],
        stdin: &str,
    ) -> WasmResult<CommandResult> {
        // Find the command
        let path = self.find_command(name).ok_or(WasmError::CommandNotFound {
            name: name.to_string(),
        })?;

        // Load the module
        let module_bytes = self.load_module(&path)?;

        // Validate the module
        ModuleValidator::validate(&module_bytes)?;

        // Prepare arguments (program name + args)
        let mut full_args: Vec<&str> = vec![name];
        for arg in args {
            full_args.push(arg);
        }

        // Create executor
        let mut executor = WasmExecutor::new();
        executor.set_env(self.env.clone());
        executor.set_cwd(&self.cwd);

        // Execute
        executor
            .execute(&module_bytes, &full_args, stdin.as_bytes())
            .await
    }

    /// Run a WASM command (synchronous wrapper for non-WASM targets)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn run(
        &mut self,
        name: &str,
        _args: &[String],
        _stdin: &str,
    ) -> WasmResult<CommandResult> {
        // For native builds, return command not found
        Err(WasmError::CommandNotFound {
            name: name.to_string(),
        })
    }

    /// Run a WASM command from bytes directly (for testing or embedded commands)
    #[cfg(target_arch = "wasm32")]
    pub async fn run_bytes(
        &self,
        module_bytes: &[u8],
        args: &[&str],
        stdin: &[u8],
    ) -> WasmResult<CommandResult> {
        // Validate
        ModuleValidator::validate(module_bytes)?;

        // Create executor
        let mut executor = WasmExecutor::new();
        executor.set_env(self.env.clone());
        executor.set_cwd(&self.cwd);

        // Execute
        executor.execute(module_bytes, args, stdin).await
    }

    /// Run a WASM command from bytes (non-WASM stub)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn run_bytes(
        &self,
        _module_bytes: &[u8],
        _args: &[&str],
        _stdin: &[u8],
    ) -> WasmResult<CommandResult> {
        Ok(CommandResult::success())
    }

    /// Load a WASM module from the filesystem
    #[cfg(target_arch = "wasm32")]
    fn load_module(&mut self, path: &str) -> WasmResult<Vec<u8>> {
        // Check cache first
        if let Some(bytes) = self.cache.get(path) {
            return Ok(bytes.clone());
        }

        // Read from filesystem
        let fd = syscall::open(path, syscall::OpenFlags::READ).map_err(|e| WasmError::IoError {
            message: format!("failed to open {}: {}", path, e),
        })?;

        let mut content = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            match syscall::read(fd, &mut buf) {
                Ok(0) => break,
                Ok(n) => content.extend_from_slice(&buf[..n]),
                Err(e) => {
                    let _ = syscall::close(fd);
                    return Err(WasmError::IoError {
                        message: format!("failed to read {}: {}", path, e),
                    });
                }
            }
        }

        let _ = syscall::close(fd);

        // Cache for future use
        self.cache.insert(path.to_string(), content.clone());

        Ok(content)
    }

    /// Clear the module cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// List available WASM commands in /bin
    pub fn list_commands(&self) -> Vec<String> {
        let mut commands = Vec::new();

        for dir in BIN_PATHS {
            if let Ok(entries) = syscall::readdir(dir) {
                for entry in entries {
                    if entry.ends_with(".wasm") {
                        // Strip .wasm extension
                        let name = entry.trim_end_matches(".wasm");
                        if !commands.contains(&name.to_string()) {
                            commands.push(name.to_string());
                        }
                    } else if !entry.contains('.') {
                        // File without extension might also be WASM
                        let full_path = format!("{}/{}", dir, entry);
                        if self.is_valid_wasm(&full_path)
                            && !commands.contains(&entry) {
                                commands.push(entry);
                            }
                    }
                }
            }
        }

        commands.sort();
        commands
    }
}

impl Default for WasmCommandRunner {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function to run a WASM command
#[cfg(target_arch = "wasm32")]
pub async fn run_wasm_command(
    name: &str,
    args: &[String],
    stdin: &str,
    cwd: &str,
    env: HashMap<String, String>,
) -> WasmResult<CommandResult> {
    let mut runner = WasmCommandRunner::new();
    runner.set_cwd(cwd);
    runner.set_env(env);
    runner.run(name, args, stdin).await
}

/// Convenience function (non-WASM stub)
#[cfg(not(target_arch = "wasm32"))]
pub async fn run_wasm_command(
    name: &str,
    _args: &[String],
    _stdin: &str,
    _cwd: &str,
    _env: HashMap<String, String>,
) -> WasmResult<CommandResult> {
    Err(WasmError::CommandNotFound {
        name: name.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runner_new() {
        let runner = WasmCommandRunner::new();
        assert_eq!(runner.cwd, "/");
        assert!(runner.env.is_empty());
    }

    #[test]
    fn test_runner_set_env() {
        let mut runner = WasmCommandRunner::new();
        let mut env = HashMap::new();
        env.insert("HOME".to_string(), "/home/user".to_string());
        runner.set_env(env);
        assert_eq!(runner.env.get("HOME"), Some(&"/home/user".to_string()));
    }

    #[test]
    fn test_runner_add_env() {
        let mut runner = WasmCommandRunner::new();
        runner.add_env("PATH", "/bin:/usr/bin");
        assert_eq!(runner.env.get("PATH"), Some(&"/bin:/usr/bin".to_string()));
    }

    #[test]
    fn test_runner_set_cwd() {
        let mut runner = WasmCommandRunner::new();
        runner.set_cwd("/home/user");
        assert_eq!(runner.cwd, "/home/user");
    }

    #[test]
    fn test_bin_paths() {
        assert!(BIN_PATHS.contains(&"/bin"));
        assert!(BIN_PATHS.contains(&"/usr/bin"));
    }
}
