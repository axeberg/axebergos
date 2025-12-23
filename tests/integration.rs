//! Integration tests for axebergOS shell
//!
//! Tests end-to-end functionality across shell, kernel, and VFS.
//! Note: Tests share a thread-local kernel state, so each test uses unique paths.

use axeberg::kernel::syscall;
use axeberg::shell::Executor;

/// Initialize kernel with a process for file operations
fn init_test() -> Executor {
    let pid = syscall::spawn_process("test");
    syscall::set_current_process(pid);
    let mut exec = Executor::new();
    // Create common directories if they don't exist
    let _ = exec.execute_line("mkdir -p /tmp");
    let _ = exec.execute_line("mkdir -p /home");
    exec
}

/// Helper to run a command and get output
fn run_cmd(executor: &mut Executor, cmd: &str) -> (String, String, i32) {
    let result = executor.execute_line(cmd);
    (result.output, result.error, result.code)
}

// ============================================================================
// Basic Shell Operations
// ============================================================================

#[test]
fn test_echo_command() {
    let mut exec = init_test();
    let (stdout, _, code) = run_cmd(&mut exec, "echo hello world");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "hello world");
}

#[test]
fn test_pwd_command() {
    let mut exec = init_test();
    let (stdout, _, code) = run_cmd(&mut exec, "pwd");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "/home");
}

#[test]
fn test_file_write_and_read() {
    let mut exec = init_test();

    // Write a file
    run_cmd(&mut exec, "echo 'test content' > /tmp/i_test1.txt");

    // Read it back
    let (stdout, _, code) = run_cmd(&mut exec, "cat /tmp/i_test1.txt");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "test content");
}

#[test]
fn test_file_append() {
    let mut exec = init_test();

    run_cmd(&mut exec, "echo 'line1' > /tmp/i_append.txt");
    run_cmd(&mut exec, "echo 'line2' >> /tmp/i_append.txt");

    let (stdout, _, code) = run_cmd(&mut exec, "cat /tmp/i_append.txt");
    assert_eq!(code, 0);
    assert!(stdout.contains("line1"));
    assert!(stdout.contains("line2"));
}

#[test]
fn test_mkdir_and_ls() {
    let mut exec = init_test();

    run_cmd(&mut exec, "mkdir /tmp/i_testdir");
    run_cmd(&mut exec, "touch /tmp/i_testdir/file.txt");

    let (stdout, _, code) = run_cmd(&mut exec, "ls /tmp/i_testdir");
    assert_eq!(code, 0);
    assert!(stdout.contains("file.txt"));
}

#[test]
fn test_cd_command() {
    let mut exec = init_test();

    run_cmd(&mut exec, "mkdir /tmp/i_cdtest");
    run_cmd(&mut exec, "cd /tmp/i_cdtest");

    let (stdout, _, _) = run_cmd(&mut exec, "pwd");
    assert_eq!(stdout.trim(), "/tmp/i_cdtest");
}

// ============================================================================
// Pipeline Tests
// ============================================================================

#[test]
fn test_simple_pipe() {
    let mut exec = init_test();

    let (stdout, _, code) = run_cmd(&mut exec, "echo 'hello world' | grep world");
    assert_eq!(code, 0);
    assert!(stdout.contains("world"));
}

#[test]
fn test_pipe_with_wc() {
    let mut exec = init_test();

    run_cmd(&mut exec, "echo 'a\nb\nc' > /tmp/i_wc.txt");
    let (stdout, _, code) = run_cmd(&mut exec, "cat /tmp/i_wc.txt | wc -l");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "3");
}

#[test]
fn test_head_command() {
    let mut exec = init_test();

    run_cmd(&mut exec, "echo '1\n2\n3\n4\n5' > /tmp/i_head.txt");
    let (stdout, _, code) = run_cmd(&mut exec, "head -n 2 /tmp/i_head.txt");
    assert_eq!(code, 0);
    assert!(stdout.contains("1"));
    assert!(stdout.contains("2"));
    assert!(!stdout.contains("3"));
}

#[test]
fn test_tail_via_pipe() {
    let mut exec = init_test();

    run_cmd(&mut exec, "echo '1\n2\n3\n4\n5' > /tmp/i_tail.txt");
    let (stdout, _, code) = run_cmd(&mut exec, "cat /tmp/i_tail.txt | tail -n 2");
    assert_eq!(code, 0);
    assert!(!stdout.contains("3"));
    assert!(stdout.contains("4"));
    assert!(stdout.contains("5"));
}

#[test]
fn test_sort_via_pipe() {
    let mut exec = init_test();

    let (stdout, _, code) = run_cmd(&mut exec, "echo 'c\na\nb' | sort");
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines, vec!["a", "b", "c"]);
}

#[test]
fn test_uniq_via_pipe() {
    let mut exec = init_test();

    let (stdout, _, code) = run_cmd(&mut exec, "echo 'a\na\nb\nb\nc' | uniq");
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines, vec!["a", "b", "c"]);
}

// ============================================================================
// Command Substitution
// ============================================================================

#[test]
fn test_command_substitution() {
    let mut exec = init_test();

    let (stdout, _, code) = run_cmd(&mut exec, "echo $(echo hello)");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "hello");
}

#[test]
fn test_nested_substitution() {
    let mut exec = init_test();

    let (stdout, _, code) = run_cmd(&mut exec, "echo $(echo $(echo nested))");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "nested");
}

// ============================================================================
// Alias
// ============================================================================

#[test]
fn test_alias_basic() {
    let mut exec = init_test();

    run_cmd(&mut exec, "alias say='echo'");
    let (stdout, _, code) = run_cmd(&mut exec, "say hello");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "hello");
}


// ============================================================================
// File Operations
// ============================================================================

#[test]
fn test_cp_command() {
    let mut exec = init_test();

    run_cmd(&mut exec, "echo 'original' > /tmp/i_cp_src.txt");
    run_cmd(&mut exec, "cp /tmp/i_cp_src.txt /tmp/i_cp_dst.txt");

    let (stdout, _, code) = run_cmd(&mut exec, "cat /tmp/i_cp_dst.txt");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "original");
}

#[test]
fn test_mv_command() {
    let mut exec = init_test();

    run_cmd(&mut exec, "echo 'move me' > /tmp/i_mv_src.txt");
    run_cmd(&mut exec, "mv /tmp/i_mv_src.txt /tmp/i_mv_dst.txt");

    // Source gone
    let (_, _, code) = run_cmd(&mut exec, "cat /tmp/i_mv_src.txt");
    assert_ne!(code, 0);

    // Destination exists
    let (stdout, _, code) = run_cmd(&mut exec, "cat /tmp/i_mv_dst.txt");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "move me");
}

#[test]
fn test_rm_command() {
    let mut exec = init_test();

    run_cmd(&mut exec, "echo 'delete' > /tmp/i_rm.txt");
    run_cmd(&mut exec, "rm /tmp/i_rm.txt");

    let (_, _, code) = run_cmd(&mut exec, "cat /tmp/i_rm.txt");
    assert_ne!(code, 0);
}

// ============================================================================
// Symlinks
// ============================================================================

#[test]
fn test_readlink_command() {
    let mut exec = init_test();

    run_cmd(&mut exec, "echo 'x' > /tmp/i_rl_target.txt");
    run_cmd(&mut exec, "ln -s /tmp/i_rl_target.txt /tmp/i_rl_link.txt");

    let (stdout, _, code) = run_cmd(&mut exec, "readlink /tmp/i_rl_link.txt");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "/tmp/i_rl_target.txt");
}

// ============================================================================
// Error Handling
// ============================================================================

#[test]
fn test_command_not_found() {
    let mut exec = init_test();

    let (_, stderr, code) = run_cmd(&mut exec, "nonexistent_cmd_xyz");
    assert_ne!(code, 0);
    assert!(stderr.contains("not found") || stderr.contains("unknown"));
}

#[test]
fn test_file_not_found() {
    let mut exec = init_test();

    let (_, stderr, code) = run_cmd(&mut exec, "cat /no/such/file.txt");
    assert_ne!(code, 0);
    assert!(!stderr.is_empty());
}

// ============================================================================
// Special Commands
// ============================================================================

#[test]
fn test_sleep_zero() {
    let mut exec = init_test();

    let (_, _, code) = run_cmd(&mut exec, "sleep 0");
    assert_eq!(code, 0);
}

#[test]
fn test_true_false() {
    let mut exec = init_test();

    let (_, _, code) = run_cmd(&mut exec, "true");
    assert_eq!(code, 0);

    let (_, _, code) = run_cmd(&mut exec, "false");
    assert_eq!(code, 1);
}

// ============================================================================
// Logical Operators
// ============================================================================

#[test]
fn test_and_operator() {
    let mut exec = init_test();

    // Both succeed
    let (stdout, _, code) = run_cmd(&mut exec, "echo first && echo second");
    assert_eq!(code, 0);
    assert!(stdout.contains("first"));
    assert!(stdout.contains("second"));

    // First fails, second should not run
    let (stdout, _, code) = run_cmd(&mut exec, "false && echo should_not_run");
    assert_ne!(code, 0);
    assert!(!stdout.contains("should_not_run"));
}

#[test]
fn test_or_operator() {
    let mut exec = init_test();

    // First fails, second runs
    let (stdout, _, code) = run_cmd(&mut exec, "false || echo fallback");
    assert_eq!(code, 0);
    assert!(stdout.contains("fallback"));

    // First succeeds, second should not run
    let (stdout, _, code) = run_cmd(&mut exec, "true || echo should_not_run");
    assert_eq!(code, 0);
    assert!(!stdout.contains("should_not_run"));
}

#[test]
fn test_semicolon_operator() {
    let mut exec = init_test();

    // Both run regardless of exit codes
    let (stdout, _, _) = run_cmd(&mut exec, "echo first; echo second");
    assert!(stdout.contains("first"));
    assert!(stdout.contains("second"));

    // Second runs even if first fails
    let (stdout, _, _) = run_cmd(&mut exec, "false; echo runs_anyway");
    assert!(stdout.contains("runs_anyway"));
}

#[test]
fn test_logical_with_file_ops() {
    let mut exec = init_test();

    // Create file only if directory creation succeeds
    run_cmd(&mut exec, "mkdir /tmp/i_logic && touch /tmp/i_logic/file.txt");

    let (stdout, _, code) = run_cmd(&mut exec, "ls /tmp/i_logic");
    assert_eq!(code, 0);
    assert!(stdout.contains("file.txt"));
}

#[test]
fn test_logical_chain() {
    let mut exec = init_test();

    // Chain of &&
    let (stdout, _, code) = run_cmd(&mut exec, "echo a && echo b && echo c");
    assert_eq!(code, 0);
    assert!(stdout.contains("a"));
    assert!(stdout.contains("b"));
    assert!(stdout.contains("c"));
}
