//! Shell - Command-line interpreter
//!
//! A simple but complete shell for the Axeberg OS. Features:
//! - Command parsing with pipes, redirections, and quotes
//! - Built-in commands (cd, pwd, exit, echo, etc.)
//! - Command execution via kernel syscalls
//! - Job control (background processes)
//!
//! Built incrementally with comprehensive tests at each step.

pub mod builtins;
pub mod parser;

pub use builtins::{execute as execute_builtin, is_builtin, BuiltinResult, ShellState};
pub use parser::{parse, ParseError, Pipeline, Redirect, SimpleCommand};
