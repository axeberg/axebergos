# Shell Implementation

## The Prompt

> Implement a Unix-like shell. It should support:
> - Simple commands: `ls`, `cat file.txt`
> - Pipes: `cat file | grep pattern | wc -l`
> - Redirects: `echo hello > file`, `cat < input`, `cmd 2> errors`
> - Background jobs: `long_command &`
> - Quoting: `echo "hello world"`, `echo 'no $expansion'`
>
> Make the parser robust but understandable. Not a full bash clone, just the essentials.

## Design Approach

### Lexer First

Break input into tokens:

```rust
enum Token {
    Word(String),       // command, argument
    Pipe,               // |
    RedirectIn,         // <
    RedirectOut,        // >
    RedirectAppend,     // >>
    RedirectErr,        // 2>
    Background,         // &
    Semicolon,          // ;
}
```

### Then Parse

Build a command structure:

```rust
struct Command {
    program: String,
    args: Vec<String>,
    stdin: Option<Redirect>,
    stdout: Option<Redirect>,
    stderr: Option<Redirect>,
}

struct Pipeline {
    commands: Vec<Command>,
    background: bool,
}
```

### Then Execute

```rust
async fn execute_pipeline(pipeline: Pipeline, kernel: &Kernel) {
    let mut prev_stdout = None;

    for (i, cmd) in pipeline.commands.iter().enumerate() {
        let stdin = prev_stdout.take();
        let stdout = if i < pipeline.commands.len() - 1 {
            // Create pipe to next command
            let (read, write) = kernel.pipe()?;
            prev_stdout = Some(read);
            Some(write)
        } else {
            None // Last command writes to terminal
        };

        kernel.spawn_with_io(cmd, stdin, stdout)?;
    }
}
```

## Key Implementation Details

### Quote Handling

```rust
fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = vec![];
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    for ch in input.chars() {
        match ch {
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
            }
            ' ' if !in_single_quote && !in_double_quote => {
                if !current.is_empty() {
                    tokens.push(Token::Word(current.clone()));
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    // ... handle remaining
}
```

### Redirect Parsing

```rust
// "cmd > file" -> RedirectOut to "file"
// "cmd >> file" -> RedirectAppend to "file"
// "cmd 2> file" -> RedirectErr to "file"
// "cmd < file" -> RedirectIn from "file"
```

### Pipe Execution

The trickiest part is managing the pipes correctly:

1. Create all pipes before spawning any process
2. Connect stdout of process N to stdin of process N+1
3. Close unused pipe ends in each process
4. Wait for all processes (or don't, if background)

## Iteration Notes

### First attempt: Too simple

Initial version didn't handle:
- Multiple redirects in one command
- Escaped characters
- Empty arguments (`echo ""`)

### Second attempt: Better

Added proper state machine for tokenizing. But still missed:
- Heredocs (`<<EOF`)
- Command substitution (`$(cmd)`)

### Final decision

> "Let's not implement heredocs or command substitution. They add complexity and aren't essential for the demo. Keep it simple."

This is a key lesson: **know when to stop**. A shell that handles 95% of use cases in 500 lines is better than one that handles 99% in 5000 lines.

## Testing Approach

```rust
#[test]
fn test_parse_simple() {
    let cmd = parse("echo hello").unwrap();
    assert_eq!(cmd.program, "echo");
    assert_eq!(cmd.args, vec!["hello"]);
}

#[test]
fn test_parse_pipe() {
    let pipeline = parse("cat file | grep pattern").unwrap();
    assert_eq!(pipeline.commands.len(), 2);
}

#[test]
fn test_parse_redirect() {
    let cmd = parse("echo hello > file.txt").unwrap();
    assert!(cmd.stdout.is_some());
}

#[test]
fn test_quoted_string() {
    let cmd = parse(r#"echo "hello world""#).unwrap();
    assert_eq!(cmd.args, vec!["hello world"]);
}
```

## Result

~800 lines total for parser + executor. Handles:
- Commands with arguments
- Pipes (any length)
- Input/output/error redirects
- Append redirects
- Background execution
- Single and double quotes
- Semicolon-separated commands

Doesn't handle (intentionally):
- Command substitution `$()`
- Process substitution `<()`
- Heredocs `<<EOF`
- Arrays
- Complex variable expansion
