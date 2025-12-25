//! Built-in shell commands
//!
//! These commands are implemented directly in the shell, not as separate programs.
//! They need access to shell state (current directory, environment, etc.).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Result of executing a built-in command
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuiltinResult {
    /// Command succeeded, output to display
    Success(String),
    /// Command succeeded, no output
    Ok,
    /// Command failed with error message
    Error(String),
    /// Request to exit the shell with given code
    Exit(i32),
    /// Request to change directory
    Cd(PathBuf),
    /// Request to set environment variables (name, value pairs)
    Export(Vec<(String, String)>),
    /// Request to unset environment variables
    Unset(Vec<String>),
    /// Request to set aliases (name, value pairs)
    SetAlias(Vec<(String, String)>),
    /// Request to remove aliases
    UnsetAlias(Vec<String>),
}

/// Shell state accessible to built-in commands
pub struct ShellState {
    /// Current working directory
    pub cwd: PathBuf,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Shell aliases
    pub aliases: HashMap<String, String>,
    /// Last command exit code
    pub last_status: i32,
}

impl ShellState {
    pub fn new() -> Self {
        Self {
            cwd: PathBuf::from("/home"),
            env: HashMap::new(),
            aliases: HashMap::new(),
            last_status: 0,
        }
    }

    /// Get an environment variable
    pub fn get_env(&self, name: &str) -> Option<&str> {
        self.env.get(name).map(|s| s.as_str())
    }

    /// Set an environment variable
    pub fn set_env(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.env.insert(name.into(), value.into());
    }

    /// Remove an environment variable
    pub fn unset_env(&mut self, name: &str) -> bool {
        self.env.remove(name).is_some()
    }

    /// Get an alias
    pub fn get_alias(&self, name: &str) -> Option<&str> {
        self.aliases.get(name).map(|s| s.as_str())
    }

    /// Set an alias
    pub fn set_alias(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.aliases.insert(name.into(), value.into());
    }

    /// Remove an alias
    pub fn unalias(&mut self, name: &str) -> bool {
        self.aliases.remove(name).is_some()
    }
}

impl Default for ShellState {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a command name is a built-in
pub fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "cd" | "pwd" | "exit" | "echo" | "export" | "unset" | "env" | "true" | "false" | "help"
            | "alias" | "unalias"
    )
}

/// Execute a built-in command
pub fn execute(name: &str, args: &[String], state: &ShellState) -> BuiltinResult {
    match name {
        "cd" => builtin_cd(args, state),
        "pwd" => builtin_pwd(state),
        "exit" => builtin_exit(args),
        "echo" => builtin_echo(args),
        "export" => builtin_export(args, state),
        "unset" => builtin_unset(args),
        "env" => builtin_env(state),
        "true" => BuiltinResult::Ok,
        "false" => BuiltinResult::Error("".into()),
        "help" => builtin_help(),
        "alias" => builtin_alias(args, state),
        "unalias" => builtin_unalias(args),
        _ => BuiltinResult::Error(format!("{}: not a builtin", name)),
    }
}

/// cd - change directory
fn builtin_cd(args: &[String], state: &ShellState) -> BuiltinResult {
    let target = if args.is_empty() {
        // cd with no args goes to $HOME or /home
        state.get_env("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/home"))
    } else if args.len() == 1 {
        let arg = &args[0];
        if arg == "-" {
            // cd - goes to $OLDPWD
            match state.get_env("OLDPWD") {
                Some(old) => PathBuf::from(old),
                None => return BuiltinResult::Error("cd: OLDPWD not set".into()),
            }
        } else if arg == "~" || arg.starts_with("~/") {
            // Expand ~
            let home = state.get_env("HOME").unwrap_or("/home");
            if arg == "~" {
                PathBuf::from(home)
            } else {
                PathBuf::from(home).join(&arg[2..])
            }
        } else {
            resolve_path(&state.cwd, arg)
        }
    } else {
        return BuiltinResult::Error("cd: too many arguments".into());
    };

    BuiltinResult::Cd(target)
}

/// Resolve a path relative to cwd
fn resolve_path(cwd: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        normalize_path(path)
    } else {
        normalize_path(&cwd.join(path))
    }
}

/// Normalize a path (resolve . and ..)
fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::CurDir => {}
            c => result.push(c),
        }
    }
    if result.as_os_str().is_empty() {
        PathBuf::from("/")
    } else {
        result
    }
}

/// pwd - print working directory
fn builtin_pwd(state: &ShellState) -> BuiltinResult {
    BuiltinResult::Success(state.cwd.display().to_string())
}

/// exit - exit the shell
fn builtin_exit(args: &[String]) -> BuiltinResult {
    let code = if args.is_empty() {
        0
    } else {
        match args[0].parse::<i32>() {
            Ok(n) => n,
            Err(_) => return BuiltinResult::Error(format!("exit: {}: numeric argument required", args[0])),
        }
    };
    BuiltinResult::Exit(code)
}

/// echo - print arguments
fn builtin_echo(args: &[String]) -> BuiltinResult {
    let mut newline = true;
    let mut escape = false;
    let mut start = 0;

    // Parse options
    for (i, arg) in args.iter().enumerate() {
        if arg == "-n" {
            newline = false;
            start = i + 1;
        } else if arg == "-e" {
            escape = true;
            start = i + 1;
        } else if arg == "-E" {
            escape = false;
            start = i + 1;
        } else if arg.starts_with('-') && arg.len() > 1 {
            // Combined options like -ne
            for c in arg[1..].chars() {
                match c {
                    'n' => newline = false,
                    'e' => escape = true,
                    'E' => escape = false,
                    _ => break,
                }
            }
            start = i + 1;
        } else {
            break;
        }
    }

    let output = args[start..].join(" ");
    let output = if escape {
        process_escapes(&output)
    } else {
        output
    };

    if newline {
        BuiltinResult::Success(output)
    } else {
        // No newline - special handling needed
        BuiltinResult::Success(output)
    }
}

/// Process escape sequences in a string
fn process_escapes(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('r') => result.push('\r'),
                Some('\\') => result.push('\\'),
                Some('0') => result.push('\0'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// export - set environment variable
fn builtin_export(args: &[String], state: &ShellState) -> BuiltinResult {
    if args.is_empty() {
        // List all exported variables
        let mut output = String::new();
        for (name, value) in &state.env {
            output.push_str(&format!("export {}=\"{}\"\n", name, value));
        }
        return BuiltinResult::Success(output.trim_end().to_string());
    }

    // Parse VAR=value or just VAR
    let mut to_set = Vec::new();
    for arg in args {
        if let Some(eq_pos) = arg.find('=') {
            let name = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];
            if !is_valid_var_name(name) {
                return BuiltinResult::Error(format!("export: `{}': not a valid identifier", name));
            }
            to_set.push((name.to_string(), value.to_string()));
        } else {
            // Just export existing variable (no-op in our simple implementation)
            if !is_valid_var_name(arg) {
                return BuiltinResult::Error(format!("export: `{}': not a valid identifier", arg));
            }
        }
    }

    // Return the variables to set (caller will apply them)
    if to_set.is_empty() {
        BuiltinResult::Ok
    } else {
        BuiltinResult::Export(to_set)
    }
}

/// Check if a string is a valid variable name
fn is_valid_var_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// unset - remove environment variable
fn builtin_unset(args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        return BuiltinResult::Ok;
    }

    let mut vars_to_unset = Vec::new();
    for arg in args {
        if !is_valid_var_name(arg) {
            return BuiltinResult::Error(format!("unset: `{}': not a valid identifier", arg));
        }
        vars_to_unset.push(arg.clone());
    }

    BuiltinResult::Unset(vars_to_unset)
}

/// env - list environment variables
fn builtin_env(state: &ShellState) -> BuiltinResult {
    let mut output = String::new();
    let mut vars: Vec<_> = state.env.iter().collect();
    vars.sort_by(|a, b| a.0.cmp(b.0));
    for (name, value) in vars {
        output.push_str(&format!("{}={}\n", name, value));
    }
    BuiltinResult::Success(output.trim_end().to_string())
}

/// help - show available commands
fn builtin_help() -> BuiltinResult {
    BuiltinResult::Success(
        "Built-in commands:
  cd [dir]       Change directory
  pwd            Print working directory
  exit [code]    Exit the shell
  echo [args]    Print arguments
  export [VAR=val] Set environment variable
  unset VAR      Remove environment variable
  env            List environment variables
  true           Return success
  false          Return failure
  help           Show this help

File commands:
  ls [path]      List directory contents
  cat [file]     Print file contents
  mkdir <dir>    Create directory
  touch <file>   Create empty file
  rm [-r] <path> Remove file or directory
  cp <src> <dst> Copy file
  mv <src> <dst> Move/rename file
  ln -s <target> <link> Create symbolic link
  readlink <link> Show symlink target
  tree [path]    Show directory tree

Text processing:
  grep <pat>     Search for pattern
  head [-n N]    Show first N lines
  tail [-n N]    Show last N lines
  sort [-r]      Sort lines
  uniq [-c]      Filter duplicate lines
  wc [-lwc]      Count lines/words/chars
  tee <file>     Copy stdin to file and stdout

Shell:
  alias [name=value] Define or list aliases
  unalias <name>   Remove an alias

Editor:
  edit [file]    Open text editor (Ctrl+Q to quit, Ctrl+S to save)

Other:
  clear          Clear screen
  history [N]    Show command history
  sleep <N>      Wait N seconds
  save           Persist filesystem to storage
  man <cmd>      Display manual page for command"
            .to_string(),
    )
}

/// alias - define or list aliases
fn builtin_alias(args: &[String], state: &ShellState) -> BuiltinResult {
    if args.is_empty() {
        // List all aliases
        if state.aliases.is_empty() {
            return BuiltinResult::Ok;
        }
        let mut output = String::new();
        let mut aliases: Vec<_> = state.aliases.iter().collect();
        aliases.sort_by(|a, b| a.0.cmp(b.0));
        for (name, value) in aliases {
            // Quote value if it contains spaces
            if value.contains(' ') {
                output.push_str(&format!("alias {}='{}'\n", name, value));
            } else {
                output.push_str(&format!("alias {}={}\n", name, value));
            }
        }
        return BuiltinResult::Success(output.trim_end().to_string());
    }

    // Set aliases
    let mut to_set = Vec::new();
    for arg in args {
        if let Some(eq_pos) = arg.find('=') {
            let name = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];
            // Strip quotes if present
            let value = value.trim_matches('\'').trim_matches('"');
            if name.is_empty() {
                return BuiltinResult::Error("alias: invalid alias name".into());
            }
            to_set.push((name.to_string(), value.to_string()));
        } else {
            // Show specific alias
            if let Some(value) = state.aliases.get(arg) {
                return BuiltinResult::Success(format!("alias {}='{}'", arg, value));
            } else {
                return BuiltinResult::Error(format!("alias: {}: not found", arg));
            }
        }
    }

    // Return aliases to set (caller will apply them)
    if to_set.is_empty() {
        BuiltinResult::Ok
    } else {
        BuiltinResult::SetAlias(to_set)
    }
}

/// unalias - remove alias
fn builtin_unalias(args: &[String]) -> BuiltinResult {
    if args.is_empty() {
        return BuiltinResult::Error("unalias: usage: unalias name [name ...]".into());
    }

    BuiltinResult::UnsetAlias(args.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> ShellState {
        let mut state = ShellState::new();
        state.cwd = PathBuf::from("/home/user");
        state.set_env("HOME", "/home/user");
        state.set_env("PATH", "/bin:/usr/bin");
        state
    }

    // ============ cd ============

    #[test]
    fn test_cd_no_args() {
        let state = make_state();
        let result = execute("cd", &[], &state);
        assert_eq!(result, BuiltinResult::Cd(PathBuf::from("/home/user")));
    }

    #[test]
    fn test_cd_absolute_path() {
        let state = make_state();
        let result = execute("cd", &["/tmp".into()], &state);
        assert_eq!(result, BuiltinResult::Cd(PathBuf::from("/tmp")));
    }

    #[test]
    fn test_cd_relative_path() {
        let state = make_state();
        let result = execute("cd", &["documents".into()], &state);
        assert_eq!(result, BuiltinResult::Cd(PathBuf::from("/home/user/documents")));
    }

    #[test]
    fn test_cd_parent_dir() {
        let state = make_state();
        let result = execute("cd", &["..".into()], &state);
        assert_eq!(result, BuiltinResult::Cd(PathBuf::from("/home")));
    }

    #[test]
    fn test_cd_tilde() {
        let state = make_state();
        let result = execute("cd", &["~".into()], &state);
        assert_eq!(result, BuiltinResult::Cd(PathBuf::from("/home/user")));
    }

    #[test]
    fn test_cd_tilde_path() {
        let state = make_state();
        let result = execute("cd", &["~/documents".into()], &state);
        assert_eq!(result, BuiltinResult::Cd(PathBuf::from("/home/user/documents")));
    }

    #[test]
    fn test_cd_dash() {
        let mut state = make_state();
        state.set_env("OLDPWD", "/tmp");
        let result = execute("cd", &["-".into()], &state);
        assert_eq!(result, BuiltinResult::Cd(PathBuf::from("/tmp")));
    }

    #[test]
    fn test_cd_dash_no_oldpwd() {
        let state = make_state();
        let result = execute("cd", &["-".into()], &state);
        assert!(matches!(result, BuiltinResult::Error(_)));
    }

    #[test]
    fn test_cd_too_many_args() {
        let state = make_state();
        let result = execute("cd", &["a".into(), "b".into()], &state);
        assert!(matches!(result, BuiltinResult::Error(_)));
    }

    #[test]
    fn test_cd_complex_path() {
        let state = make_state();
        let result = execute("cd", &["../other/../user/./docs".into()], &state);
        assert_eq!(result, BuiltinResult::Cd(PathBuf::from("/home/user/docs")));
    }

    // ============ pwd ============

    #[test]
    fn test_pwd() {
        let state = make_state();
        let result = execute("pwd", &[], &state);
        assert_eq!(result, BuiltinResult::Success("/home/user".into()));
    }

    // ============ exit ============

    #[test]
    fn test_exit_no_args() {
        let state = make_state();
        let result = execute("exit", &[], &state);
        assert_eq!(result, BuiltinResult::Exit(0));
    }

    #[test]
    fn test_exit_with_code() {
        let state = make_state();
        let result = execute("exit", &["42".into()], &state);
        assert_eq!(result, BuiltinResult::Exit(42));
    }

    #[test]
    fn test_exit_negative_code() {
        let state = make_state();
        let result = execute("exit", &["-1".into()], &state);
        assert_eq!(result, BuiltinResult::Exit(-1));
    }

    #[test]
    fn test_exit_invalid_code() {
        let state = make_state();
        let result = execute("exit", &["abc".into()], &state);
        assert!(matches!(result, BuiltinResult::Error(_)));
    }

    // ============ echo ============

    #[test]
    fn test_echo_simple() {
        let state = make_state();
        let result = execute("echo", &["hello".into()], &state);
        assert_eq!(result, BuiltinResult::Success("hello".into()));
    }

    #[test]
    fn test_echo_multiple_args() {
        let state = make_state();
        let result = execute("echo", &["hello".into(), "world".into()], &state);
        assert_eq!(result, BuiltinResult::Success("hello world".into()));
    }

    #[test]
    fn test_echo_no_args() {
        let state = make_state();
        let result = execute("echo", &[], &state);
        assert_eq!(result, BuiltinResult::Success("".into()));
    }

    #[test]
    fn test_echo_escape_n() {
        let state = make_state();
        let result = execute("echo", &["-e".into(), "hello\\nworld".into()], &state);
        assert_eq!(result, BuiltinResult::Success("hello\nworld".into()));
    }

    #[test]
    fn test_echo_escape_t() {
        let state = make_state();
        let result = execute("echo", &["-e".into(), "hello\\tworld".into()], &state);
        assert_eq!(result, BuiltinResult::Success("hello\tworld".into()));
    }

    #[test]
    fn test_echo_no_escape() {
        let state = make_state();
        let result = execute("echo", &["-E".into(), "hello\\nworld".into()], &state);
        assert_eq!(result, BuiltinResult::Success("hello\\nworld".into()));
    }

    // ============ export ============

    #[test]
    fn test_export_list() {
        let state = make_state();
        let result = execute("export", &[], &state);
        match result {
            BuiltinResult::Success(s) => {
                assert!(s.contains("HOME="));
                assert!(s.contains("PATH="));
            }
            _ => panic!("expected Success"),
        }
    }

    #[test]
    fn test_export_set() {
        let state = make_state();
        let result = execute("export", &["FOO=bar".into()], &state);
        match result {
            BuiltinResult::Export(pairs) => {
                assert_eq!(pairs.len(), 1);
                assert_eq!(pairs[0], ("FOO".to_string(), "bar".to_string()));
            }
            _ => panic!("expected Export, got {:?}", result),
        }
    }

    #[test]
    fn test_export_invalid_name() {
        let state = make_state();
        let result = execute("export", &["123=bad".into()], &state);
        assert!(matches!(result, BuiltinResult::Error(_)));
    }

    // ============ unset ============

    #[test]
    fn test_unset() {
        let state = make_state();
        let result = execute("unset", &["FOO".into()], &state);
        match result {
            BuiltinResult::Unset(vars) => {
                assert_eq!(vars.len(), 1);
                assert_eq!(vars[0], "FOO");
            }
            _ => panic!("expected Unset, got {:?}", result),
        }
    }

    #[test]
    fn test_unset_invalid_name() {
        let state = make_state();
        let result = execute("unset", &["123".into()], &state);
        assert!(matches!(result, BuiltinResult::Error(_)));
    }

    // ============ env ============

    #[test]
    fn test_env() {
        let state = make_state();
        let result = execute("env", &[], &state);
        match result {
            BuiltinResult::Success(s) => {
                assert!(s.contains("HOME=/home/user"));
                assert!(s.contains("PATH=/bin:/usr/bin"));
            }
            _ => panic!("expected Success"),
        }
    }

    #[test]
    fn test_env_sorted() {
        let mut state = make_state();
        state.set_env("ZZZ", "last");
        state.set_env("AAA", "first");
        let result = execute("env", &[], &state);
        match result {
            BuiltinResult::Success(s) => {
                let pos_a = s.find("AAA=").unwrap();
                let pos_z = s.find("ZZZ=").unwrap();
                assert!(pos_a < pos_z, "env output should be sorted");
            }
            _ => panic!("expected Success"),
        }
    }

    // ============ true/false ============

    #[test]
    fn test_true() {
        let state = make_state();
        let result = execute("true", &[], &state);
        assert_eq!(result, BuiltinResult::Ok);
    }

    #[test]
    fn test_false() {
        let state = make_state();
        let result = execute("false", &[], &state);
        assert!(matches!(result, BuiltinResult::Error(_)));
    }

    // ============ help ============

    #[test]
    fn test_help() {
        let state = make_state();
        let result = execute("help", &[], &state);
        match result {
            BuiltinResult::Success(s) => {
                assert!(s.contains("cd"));
                assert!(s.contains("pwd"));
                assert!(s.contains("exit"));
            }
            _ => panic!("expected Success"),
        }
    }

    // ============ is_builtin ============

    #[test]
    fn test_is_builtin() {
        assert!(is_builtin("cd"));
        assert!(is_builtin("pwd"));
        assert!(is_builtin("exit"));
        assert!(is_builtin("echo"));
        assert!(!is_builtin("ls"));
        assert!(!is_builtin("grep"));
    }

    // ============ path resolution ============

    #[test]
    fn test_normalize_path_simple() {
        assert_eq!(normalize_path(Path::new("/a/b/c")), PathBuf::from("/a/b/c"));
    }

    #[test]
    fn test_normalize_path_dots() {
        assert_eq!(normalize_path(Path::new("/a/./b/../c")), PathBuf::from("/a/c"));
    }

    #[test]
    fn test_normalize_path_trailing_dots() {
        assert_eq!(normalize_path(Path::new("/a/b/..")), PathBuf::from("/a"));
    }

    #[test]
    fn test_normalize_path_root() {
        assert_eq!(normalize_path(Path::new("/..")), PathBuf::from("/"));
    }

    #[test]
    fn test_is_valid_var_name() {
        assert!(is_valid_var_name("FOO"));
        assert!(is_valid_var_name("_foo"));
        assert!(is_valid_var_name("foo123"));
        assert!(is_valid_var_name("_"));
        assert!(!is_valid_var_name(""));
        assert!(!is_valid_var_name("123"));
        assert!(!is_valid_var_name("foo-bar"));
    }
}
