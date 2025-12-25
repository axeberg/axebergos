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
pub mod parser;
pub mod programs;
pub mod terminal;

pub use builtins::{execute as execute_builtin, is_builtin, BuiltinResult, ShellState};
pub use executor::{ExecResult, Executor, ProgramRegistry};
pub use parser::{parse, ParseError, Pipeline, Redirect, SimpleCommand};
pub use terminal::{Selection, TermPos, Terminal};

use std::cell::RefCell;

thread_local! {
    static EXECUTOR: RefCell<Executor> = RefCell::new(Executor::new());
}

/// Execute a command and return the output
pub fn execute_command(line: &str) -> String {
    EXECUTOR.with(|exec| {
        let result = exec.borrow_mut().execute_line(line);
        let mut output = String::new();

        if !result.output.is_empty() {
            output.push_str(&result.output);
        }
        if !result.error.is_empty() {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&result.error);
        }

        output
    })
}
