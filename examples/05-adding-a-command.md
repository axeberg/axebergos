# Tutorial 5: Adding a Shell Command

Extend axeberg with your own custom command.

## Overview

axeberg's shell has two types of commands:
1. **Builtins**: Implemented directly in the shell (cd, export, etc.)
2. **Programs**: Separate modules in `src/shell/programs/`

We'll add a new program command.

## Step 1: Create the Command Module

Create a new file `src/shell/programs/hello.rs`:

```rust
//! hello - A friendly greeting command
//!
//! Usage: hello [NAME]
//!
//! Prints a greeting. If NAME is provided, greets that person.

use crate::kernel::Kernel;

pub async fn run(args: &[String], kernel: &Kernel, _env: &std::collections::HashMap<String, String>) -> i32 {
    let name = if args.len() > 1 {
        &args[1]
    } else {
        "World"
    };

    let message = format!("Hello, {}!\n", name);
    kernel.write_stdout(&message);

    0  // Exit code: 0 = success
}
```

## Step 2: Register the Command

Edit `src/shell/programs/mod.rs`:

```rust
// Add the module declaration
mod hello;

// In the command dispatch function, add:
pub fn get_program(name: &str) -> Option<ProgramFn> {
    match name {
        // ... existing commands ...
        "hello" => Some(hello::run),
        _ => None,
    }
}
```

## Step 3: Test It

Build and run:

```bash
$ cargo test                    # Make sure tests pass
$ wasm-pack build --target web
$ cargo run --bin serve
```

Then in the browser:

```bash
$ hello
Hello, World!

$ hello Alice
Hello, Alice!
```

## A More Complex Example: `upcase`

Let's build a command that uppercases input:

```rust
//! upcase - Convert input to uppercase
//!
//! Usage: upcase [TEXT]
//!        echo text | upcase
//!
//! Converts text to uppercase. Reads from stdin if no arguments.

use crate::kernel::Kernel;

pub async fn run(args: &[String], kernel: &Kernel, _env: &std::collections::HashMap<String, String>) -> i32 {
    let text = if args.len() > 1 {
        // Arguments provided
        args[1..].join(" ")
    } else {
        // Read from stdin
        match kernel.read_stdin_line().await {
            Ok(line) => line,
            Err(e) => {
                kernel.write_stderr(&format!("upcase: {}\n", e));
                return 1;
            }
        }
    };

    let upper = text.to_uppercase();
    kernel.write_stdout(&format!("{}\n", upper));
    0
}
```

Usage:

```bash
$ upcase hello world
HELLO WORLD

$ echo "mixed Case" | upcase
MIXED CASE
```

## Handling Options

For commands with options, parse them manually or use a pattern:

```rust
//! reverse - Reverse text
//!
//! Usage: reverse [-w] TEXT
//!
//! Options:
//!   -w    Reverse words instead of characters

pub async fn run(args: &[String], kernel: &Kernel, _env: &std::collections::HashMap<String, String>) -> i32 {
    let mut reverse_words = false;
    let mut text_args = Vec::new();

    for arg in &args[1..] {
        if arg == "-w" {
            reverse_words = true;
        } else if arg.starts_with('-') {
            kernel.write_stderr(&format!("reverse: unknown option: {}\n", arg));
            return 1;
        } else {
            text_args.push(arg.as_str());
        }
    }

    let text = text_args.join(" ");
    let result = if reverse_words {
        text.split_whitespace().rev().collect::<Vec<_>>().join(" ")
    } else {
        text.chars().rev().collect::<String>()
    };

    kernel.write_stdout(&format!("{}\n", result));
    0
}
```

## Working with Files

Commands often need file I/O:

```rust
//! linecount - Count lines in files
//!
//! Usage: linecount FILE...

pub async fn run(args: &[String], kernel: &Kernel, _env: &std::collections::HashMap<String, String>) -> i32 {
    if args.len() < 2 {
        kernel.write_stderr("Usage: linecount FILE...\n");
        return 1;
    }

    let mut total = 0;
    for path in &args[1..] {
        match kernel.vfs().read(path) {
            Ok(content) => {
                let lines = content.iter().filter(|&&b| b == b'\n').count();
                kernel.write_stdout(&format!("{:8} {}\n", lines, path));
                total += lines;
            }
            Err(e) => {
                kernel.write_stderr(&format!("linecount: {}: {}\n", path, e));
            }
        }
    }

    if args.len() > 2 {
        kernel.write_stdout(&format!("{:8} total\n", total));
    }

    0
}
```

## Adding Tests

Add tests in the same file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hello_default() {
        // Unit tests for pure logic
    }

    // Integration tests go in tests/integration.rs
}
```

## Adding a Man Page

Create `man/man1/hello.1.md`:

```markdown
# HELLO(1)

## NAME
hello - print a greeting

## SYNOPSIS
**hello** [_NAME_]

## DESCRIPTION
Print a friendly greeting. If NAME is provided, greet that person specifically.

## EXAMPLES
```
$ hello
Hello, World!

$ hello Alice
Hello, Alice!
```

## EXIT STATUS
0 on success.

## SEE ALSO
echo(1)
```

## Best Practices

### 1. Follow Conventions
- Exit 0 on success, non-zero on error
- Write errors to stderr
- Support `-h` or `--help`

### 2. Be Composable
- Read from stdin when no file arguments
- Write to stdout (let user redirect)
- One thing well

### 3. Document
- Add module-level doc comments
- Create a man page
- Include examples

### 4. Test
- Unit tests for logic
- Integration tests for I/O

## Exercise: Create Your Own

Create a command called `freq` that counts word frequency:

```bash
$ echo "the quick brown fox jumps over the lazy dog" | freq
      2 the
      1 quick
      1 brown
      1 fox
      1 jumps
      1 over
      1 lazy
      1 dog
```

Hints:
1. Split input on whitespace
2. Use a HashMap to count
3. Sort by count descending

## What's Next?

Continue to [Understanding the Kernel](06-understanding-kernel.md) for a deep dive into axeberg's internals.
