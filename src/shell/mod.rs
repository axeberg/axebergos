//! Shell - Command-line interpreter
//!
//! A simple but complete shell for the Axeberg OS. Features:
//! - Command parsing with pipes, redirections, and quotes
//! - Built-in commands (cd, pwd, exit, echo, etc.)
//! - Command execution via kernel syscalls
//! - Job control (background processes)
//! - Terminal with scrollback and line editing
//!
//! Built incrementally with comprehensive tests at each step.

pub mod builtins;
pub mod executor;
pub mod filebrowser;
pub mod parser;
pub mod terminal;

pub use builtins::{execute as execute_builtin, is_builtin, BuiltinResult, ShellState};
pub use executor::{ExecResult, Executor, ProgramRegistry};
pub use filebrowser::{ClipboardEntry, Entry, EntryType, FileBrowser, InputMode, StatusMessage};
pub use parser::{parse, ParseError, Pipeline, Redirect, SimpleCommand};
pub use terminal::Terminal;
