//! axeberg CLI - WASI binary entry point
//!
//! Run with: wasmtime --dir=. target/wasm32-wasip1/debug/axeberg-cli.wasm

use std::io::{self, BufRead, Write};

fn main() {
    println!("axeberg v0.1.0 (WASI CLI)");
    println!("Type 'help' for available commands.\n");

    // Simple REPL using host filesystem
    // The full kernel/shell is in the WASM web target

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        // Print prompt
        print!("$ ");
        let _ = stdout.flush();

        // Read line
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                // EOF
                println!();
                break;
            }
            Ok(_) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                match line {
                    "exit" | "quit" => {
                        println!("Goodbye!");
                        break;
                    }
                    "help" => {
                        println!("axeberg CLI - Available commands:");
                        println!("  help    - Show this help");
                        println!("  exit    - Exit the shell");
                        println!("  pwd     - Print working directory");
                        println!("  ls      - List directory");
                        println!("  cat     - Display file contents");
                        println!();
                        println!("Note: This is a minimal WASI CLI.");
                        println!("Full kernel integration coming soon.");
                    }
                    "pwd" => match std::env::current_dir() {
                        Ok(path) => println!("{}", path.display()),
                        Err(e) => eprintln!("pwd: {}", e),
                    },
                    cmd if cmd.starts_with("ls") => {
                        let path = cmd.strip_prefix("ls").unwrap().trim();
                        let path = if path.is_empty() { "." } else { path };

                        match std::fs::read_dir(path) {
                            Ok(entries) => {
                                for entry in entries.flatten() {
                                    let name = entry.file_name();
                                    let meta = entry.metadata().ok();
                                    let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                                    if is_dir {
                                        println!("{}/", name.to_string_lossy());
                                    } else {
                                        println!("{}", name.to_string_lossy());
                                    }
                                }
                            }
                            Err(e) => eprintln!("ls: {}: {}", path, e),
                        }
                    }
                    cmd if cmd.starts_with("cat ") => {
                        let path = cmd.strip_prefix("cat ").unwrap().trim();
                        match std::fs::read_to_string(path) {
                            Ok(content) => print!("{}", content),
                            Err(e) => eprintln!("cat: {}: {}", path, e),
                        }
                    }
                    cmd if cmd.starts_with("echo ") => {
                        println!("{}", cmd.strip_prefix("echo ").unwrap());
                    }
                    _ => {
                        eprintln!(
                            "{}: command not found",
                            line.split_whitespace().next().unwrap_or(line)
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading input: {}", e);
                break;
            }
        }
    }
}
