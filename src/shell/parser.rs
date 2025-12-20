//! Shell command parser
//!
//! Parses command lines into structured commands. Built incrementally:
//! 1. Simple commands with arguments
//! 2. Quoted strings (single and double)
//! 3. Pipes
//! 4. Redirections
//! 5. Environment variable expansion
//! 6. Background execution

use std::iter::Peekable;
use std::str::Chars;

/// A single command (program + arguments)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleCommand {
    /// Program name
    pub program: String,
    /// Arguments (not including program name)
    pub args: Vec<String>,
    /// Input redirection: < file
    pub stdin: Option<Redirect>,
    /// Output redirection: > file or >> file
    pub stdout: Option<Redirect>,
    /// Error redirection: 2> file or 2>> file
    pub stderr: Option<Redirect>,
}

impl SimpleCommand {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            stdin: None,
            stdout: None,
            stderr: None,
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(|s| s.into()));
        self
    }
}

/// Redirection specification
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirect {
    /// Target file path
    pub path: String,
    /// Append mode (>> vs >)
    pub append: bool,
}

impl Redirect {
    pub fn new(path: impl Into<String>, append: bool) -> Self {
        Self {
            path: path.into(),
            append,
        }
    }
}

/// A pipeline of commands connected by pipes
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pipeline {
    /// Commands in the pipeline, left to right
    pub commands: Vec<SimpleCommand>,
    /// Run in background (&)
    pub background: bool,
}

impl Pipeline {
    pub fn new(cmd: SimpleCommand) -> Self {
        Self {
            commands: vec![cmd],
            background: false,
        }
    }

    pub fn pipe(mut self, cmd: SimpleCommand) -> Self {
        self.commands.push(cmd);
        self
    }

    pub fn background(mut self) -> Self {
        self.background = true;
        self
    }
}

/// Parse error
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Unexpected end of input
    UnexpectedEnd,
    /// Unterminated quoted string
    UnterminatedQuote(char),
    /// Empty command
    EmptyCommand,
    /// Missing filename after redirection
    MissingRedirectTarget,
    /// Unexpected token
    UnexpectedToken(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedEnd => write!(f, "unexpected end of input"),
            Self::UnterminatedQuote(c) => write!(f, "unterminated {} quote", c),
            Self::EmptyCommand => write!(f, "empty command"),
            Self::MissingRedirectTarget => write!(f, "missing redirect target"),
            Self::UnexpectedToken(t) => write!(f, "unexpected token: {}", t),
        }
    }
}

impl std::error::Error for ParseError {}

/// Tokenizer for shell input
struct Lexer<'a> {
    chars: Peekable<Chars<'a>>,
}

/// Token types
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    /// A word (program name, argument, filename)
    Word(String),
    /// Pipe: |
    Pipe,
    /// Input redirect: <
    RedirectIn,
    /// Output redirect: >
    RedirectOut,
    /// Append redirect: >>
    RedirectAppend,
    /// Error redirect: 2>
    RedirectErr,
    /// Error append redirect: 2>>
    RedirectErrAppend,
    /// Background: &
    Background,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            chars: input.chars().peekable(),
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(&c) = self.chars.peek() {
            if c.is_whitespace() {
                self.chars.next();
            } else {
                break;
            }
        }
    }

    fn next_token(&mut self) -> Result<Option<Token>, ParseError> {
        self.skip_whitespace();

        let c = match self.chars.peek() {
            Some(&c) => c,
            None => return Ok(None),
        };

        match c {
            '|' => {
                self.chars.next();
                Ok(Some(Token::Pipe))
            }
            '&' => {
                self.chars.next();
                Ok(Some(Token::Background))
            }
            '<' => {
                self.chars.next();
                Ok(Some(Token::RedirectIn))
            }
            '>' => {
                self.chars.next();
                if self.chars.peek() == Some(&'>') {
                    self.chars.next();
                    Ok(Some(Token::RedirectAppend))
                } else {
                    Ok(Some(Token::RedirectOut))
                }
            }
            '2' => {
                // Check for 2> or 2>>
                let mut lookahead = self.chars.clone();
                lookahead.next(); // consume '2'
                if lookahead.peek() == Some(&'>') {
                    self.chars.next(); // consume '2'
                    self.chars.next(); // consume '>'
                    if self.chars.peek() == Some(&'>') {
                        self.chars.next();
                        Ok(Some(Token::RedirectErrAppend))
                    } else {
                        Ok(Some(Token::RedirectErr))
                    }
                } else {
                    // Just a word starting with '2'
                    self.read_word()
                }
            }
            '"' | '\'' => self.read_quoted_string(c),
            _ => self.read_word(),
        }
    }

    fn read_word(&mut self) -> Result<Option<Token>, ParseError> {
        let mut word = String::new();

        while let Some(&c) = self.chars.peek() {
            match c {
                // These terminate a word
                ' ' | '\t' | '\n' | '\r' | '|' | '&' | '<' | '>' => break,
                // Quotes can appear mid-word: foo"bar"baz
                '"' | '\'' => {
                    self.chars.next();
                    word.push_str(&self.read_quoted_content(c)?);
                }
                _ => {
                    word.push(c);
                    self.chars.next();
                }
            }
        }

        if word.is_empty() {
            Ok(None)
        } else {
            Ok(Some(Token::Word(word)))
        }
    }

    fn read_quoted_string(&mut self, quote: char) -> Result<Option<Token>, ParseError> {
        self.chars.next(); // consume opening quote
        let content = self.read_quoted_content(quote)?;
        Ok(Some(Token::Word(content)))
    }

    fn read_quoted_content(&mut self, quote: char) -> Result<String, ParseError> {
        let mut content = String::new();

        loop {
            match self.chars.next() {
                Some(c) if c == quote => break,
                Some('\\') if quote == '"' => {
                    // Escape sequences only in double quotes
                    match self.chars.next() {
                        Some(escaped) => content.push(escaped),
                        None => return Err(ParseError::UnterminatedQuote(quote)),
                    }
                }
                Some(c) => content.push(c),
                None => return Err(ParseError::UnterminatedQuote(quote)),
            }
        }

        Ok(content)
    }
}

/// Parse a command line into a pipeline
pub fn parse(input: &str) -> Result<Pipeline, ParseError> {
    let mut lexer = Lexer::new(input);
    let mut commands = Vec::new();
    let mut current_words = Vec::new();
    let mut stdin = None;
    let mut stdout = None;
    let mut stderr = None;
    let mut background = false;
    let mut expecting_command = true; // True at start and after pipe

    loop {
        match lexer.next_token()? {
            None => break,
            Some(Token::Word(w)) => {
                current_words.push(w);
                expecting_command = false;
            }
            Some(Token::Pipe) => {
                if current_words.is_empty() {
                    return Err(ParseError::EmptyCommand);
                }
                let cmd = build_command(&mut current_words, stdin.take(), stdout.take(), stderr.take());
                commands.push(cmd);
                expecting_command = true; // Expecting command after pipe
            }
            Some(Token::Background) => {
                background = true;
                // & should be at the end
            }
            Some(Token::RedirectIn) => {
                let target = expect_word(&mut lexer)?;
                stdin = Some(Redirect::new(target, false));
            }
            Some(Token::RedirectOut) => {
                let target = expect_word(&mut lexer)?;
                stdout = Some(Redirect::new(target, false));
            }
            Some(Token::RedirectAppend) => {
                let target = expect_word(&mut lexer)?;
                stdout = Some(Redirect::new(target, true));
            }
            Some(Token::RedirectErr) => {
                let target = expect_word(&mut lexer)?;
                stderr = Some(Redirect::new(target, false));
            }
            Some(Token::RedirectErrAppend) => {
                let target = expect_word(&mut lexer)?;
                stderr = Some(Redirect::new(target, true));
            }
        }
    }

    // Build final command
    if current_words.is_empty() {
        if expecting_command {
            // Either empty input, or trailing pipe
            return Err(ParseError::EmptyCommand);
        }
    } else {
        let cmd = build_command(&mut current_words, stdin, stdout, stderr);
        commands.push(cmd);
    }

    Ok(Pipeline { commands, background })
}

fn expect_word(lexer: &mut Lexer) -> Result<String, ParseError> {
    match lexer.next_token()? {
        Some(Token::Word(w)) => Ok(w),
        Some(t) => Err(ParseError::UnexpectedToken(format!("{:?}", t))),
        None => Err(ParseError::MissingRedirectTarget),
    }
}

fn build_command(
    words: &mut Vec<String>,
    stdin: Option<Redirect>,
    stdout: Option<Redirect>,
    stderr: Option<Redirect>,
) -> SimpleCommand {
    let program = words.remove(0);
    let args = std::mem::take(words);
    SimpleCommand {
        program,
        args,
        stdin,
        stdout,
        stderr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============ Simple Commands ============

    #[test]
    fn test_simple_command() {
        let result = parse("ls").unwrap();
        assert_eq!(result.commands.len(), 1);
        assert_eq!(result.commands[0].program, "ls");
        assert!(result.commands[0].args.is_empty());
        assert!(!result.background);
    }

    #[test]
    fn test_command_with_args() {
        let result = parse("ls -la /home").unwrap();
        assert_eq!(result.commands[0].program, "ls");
        assert_eq!(result.commands[0].args, vec!["-la", "/home"]);
    }

    #[test]
    fn test_command_with_many_args() {
        let result = parse("echo one two three four five").unwrap();
        assert_eq!(result.commands[0].program, "echo");
        assert_eq!(result.commands[0].args, vec!["one", "two", "three", "four", "five"]);
    }

    #[test]
    fn test_extra_whitespace() {
        let result = parse("  ls   -la   /home  ").unwrap();
        assert_eq!(result.commands[0].program, "ls");
        assert_eq!(result.commands[0].args, vec!["-la", "/home"]);
    }

    #[test]
    fn test_empty_input() {
        let result = parse("");
        assert!(matches!(result, Err(ParseError::EmptyCommand)));
    }

    #[test]
    fn test_only_whitespace() {
        let result = parse("   ");
        assert!(matches!(result, Err(ParseError::EmptyCommand)));
    }

    // ============ Quoted Strings ============

    #[test]
    fn test_double_quoted_string() {
        let result = parse(r#"echo "hello world""#).unwrap();
        assert_eq!(result.commands[0].program, "echo");
        assert_eq!(result.commands[0].args, vec!["hello world"]);
    }

    #[test]
    fn test_single_quoted_string() {
        let result = parse("echo 'hello world'").unwrap();
        assert_eq!(result.commands[0].program, "echo");
        assert_eq!(result.commands[0].args, vec!["hello world"]);
    }

    #[test]
    fn test_mixed_quotes() {
        let result = parse(r#"echo "hello" 'world'"#).unwrap();
        assert_eq!(result.commands[0].args, vec!["hello", "world"]);
    }

    #[test]
    fn test_quotes_with_special_chars() {
        let result = parse(r#"echo "hello | world""#).unwrap();
        assert_eq!(result.commands[0].args, vec!["hello | world"]);
    }

    #[test]
    fn test_escaped_quote_in_double_quotes() {
        let result = parse(r#"echo "hello \"world\"""#).unwrap();
        assert_eq!(result.commands[0].args, vec!["hello \"world\""]);
    }

    #[test]
    fn test_concatenated_quotes() {
        let result = parse(r#"echo foo"bar"baz"#).unwrap();
        assert_eq!(result.commands[0].args, vec!["foobarbaz"]);
    }

    #[test]
    fn test_unterminated_double_quote() {
        let result = parse(r#"echo "hello"#);
        assert!(matches!(result, Err(ParseError::UnterminatedQuote('"'))));
    }

    #[test]
    fn test_unterminated_single_quote() {
        let result = parse("echo 'hello");
        assert!(matches!(result, Err(ParseError::UnterminatedQuote('\''))));
    }

    // ============ Pipes ============

    #[test]
    fn test_simple_pipe() {
        let result = parse("ls | grep foo").unwrap();
        assert_eq!(result.commands.len(), 2);
        assert_eq!(result.commands[0].program, "ls");
        assert_eq!(result.commands[1].program, "grep");
        assert_eq!(result.commands[1].args, vec!["foo"]);
    }

    #[test]
    fn test_multi_pipe() {
        let result = parse("cat file | grep pattern | wc -l").unwrap();
        assert_eq!(result.commands.len(), 3);
        assert_eq!(result.commands[0].program, "cat");
        assert_eq!(result.commands[1].program, "grep");
        assert_eq!(result.commands[2].program, "wc");
    }

    #[test]
    fn test_pipe_no_spaces() {
        let result = parse("ls|grep foo").unwrap();
        assert_eq!(result.commands.len(), 2);
        assert_eq!(result.commands[0].program, "ls");
        assert_eq!(result.commands[1].program, "grep");
    }

    #[test]
    fn test_empty_pipe_left() {
        let result = parse("| grep foo");
        assert!(matches!(result, Err(ParseError::EmptyCommand)));
    }

    #[test]
    fn test_empty_pipe_right() {
        let result = parse("ls |");
        assert!(matches!(result, Err(ParseError::EmptyCommand)));
    }

    // ============ Redirections ============

    #[test]
    fn test_output_redirect() {
        let result = parse("echo hello > file.txt").unwrap();
        assert_eq!(result.commands[0].program, "echo");
        assert_eq!(result.commands[0].args, vec!["hello"]);
        assert_eq!(result.commands[0].stdout, Some(Redirect::new("file.txt", false)));
    }

    #[test]
    fn test_output_append() {
        let result = parse("echo hello >> file.txt").unwrap();
        assert_eq!(result.commands[0].stdout, Some(Redirect::new("file.txt", true)));
    }

    #[test]
    fn test_input_redirect() {
        let result = parse("cat < input.txt").unwrap();
        assert_eq!(result.commands[0].program, "cat");
        assert_eq!(result.commands[0].stdin, Some(Redirect::new("input.txt", false)));
    }

    #[test]
    fn test_stderr_redirect() {
        let result = parse("cmd 2> errors.txt").unwrap();
        assert_eq!(result.commands[0].stderr, Some(Redirect::new("errors.txt", false)));
    }

    #[test]
    fn test_stderr_append() {
        let result = parse("cmd 2>> errors.txt").unwrap();
        assert_eq!(result.commands[0].stderr, Some(Redirect::new("errors.txt", true)));
    }

    #[test]
    fn test_multiple_redirects() {
        let result = parse("cmd < in.txt > out.txt 2> err.txt").unwrap();
        assert_eq!(result.commands[0].stdin, Some(Redirect::new("in.txt", false)));
        assert_eq!(result.commands[0].stdout, Some(Redirect::new("out.txt", false)));
        assert_eq!(result.commands[0].stderr, Some(Redirect::new("err.txt", false)));
    }

    #[test]
    fn test_redirect_no_space() {
        let result = parse("echo hello>file.txt").unwrap();
        assert_eq!(result.commands[0].args, vec!["hello"]);
        assert_eq!(result.commands[0].stdout, Some(Redirect::new("file.txt", false)));
    }

    #[test]
    fn test_missing_redirect_target() {
        let result = parse("echo hello >");
        assert!(matches!(result, Err(ParseError::MissingRedirectTarget)));
    }

    // ============ Background ============

    #[test]
    fn test_background() {
        let result = parse("sleep 10 &").unwrap();
        assert_eq!(result.commands[0].program, "sleep");
        assert!(result.background);
    }

    #[test]
    fn test_background_no_space() {
        let result = parse("sleep 10&").unwrap();
        assert_eq!(result.commands[0].program, "sleep");
        assert_eq!(result.commands[0].args, vec!["10"]);
        assert!(result.background);
    }

    // ============ Complex Cases ============

    #[test]
    fn test_pipe_with_redirect() {
        let result = parse("cat file.txt | grep pattern > output.txt").unwrap();
        assert_eq!(result.commands.len(), 2);
        assert_eq!(result.commands[0].program, "cat");
        assert_eq!(result.commands[1].program, "grep");
        assert_eq!(result.commands[1].stdout, Some(Redirect::new("output.txt", false)));
    }

    #[test]
    fn test_complex_pipeline() {
        let result = parse("cat < input.txt | sort | uniq > output.txt &").unwrap();
        assert_eq!(result.commands.len(), 3);
        assert_eq!(result.commands[0].stdin, Some(Redirect::new("input.txt", false)));
        assert_eq!(result.commands[2].stdout, Some(Redirect::new("output.txt", false)));
        assert!(result.background);
    }

    #[test]
    fn test_quoted_redirect_target() {
        let result = parse(r#"echo hello > "file with spaces.txt""#).unwrap();
        assert_eq!(result.commands[0].stdout, Some(Redirect::new("file with spaces.txt", false)));
    }
}
