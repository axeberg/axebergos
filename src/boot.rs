//! Boot sequence
//!
//! This is where axeberg comes to life. Following Radiant's philosophy:
//! boot should be immediate, comprehensible, and joyful.

#![cfg(target_arch = "wasm32")]

use crate::compositor;
use crate::console_log;
use crate::kernel::syscall::{self, OpenFlags};
use crate::kernel::{self, Priority};
use crate::runtime;
use crate::vfs::Persistence;

/// Boot the system
pub fn boot() {
    // Create init process (PID 1)
    let init_pid = syscall::spawn_process("init");
    syscall::set_current_process(init_pid);
    console_log!("[boot] Created init process: {:?}", init_pid);

    // Initialize filesystem and compositor (async)
    kernel::spawn_with_priority(
        async {
            wasm_bindgen_futures::spawn_local(async {
                // Try to restore persisted filesystem
                match restore_or_init_filesystem().await {
                    Ok(restored) => {
                        if restored {
                            console_log!("[boot] Restored filesystem from OPFS");
                        } else {
                            console_log!("[boot] Initialized fresh filesystem");
                        }
                    }
                    Err(e) => {
                        console_log!("[boot] Filesystem error: {}, using fresh", e);
                        init_filesystem();
                    }
                }

                // Initialize compositor
                init_compositor().await;
            });
        },
        Priority::Critical,
    );

    // Start the runtime loop (this returns immediately, loop runs via rAF)
    runtime::start();
}

/// Try to restore filesystem from OPFS, or initialize fresh
async fn restore_or_init_filesystem() -> Result<bool, String> {
    // Try to load from OPFS
    if let Some(fs) = Persistence::load().await? {
        // Restore the VFS
        let data = fs.to_json().map_err(|e| e.to_string())?;
        syscall::vfs_restore(&data).map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        // Fresh install - initialize filesystem
        init_filesystem();
        Ok(false)
    }
}

/// Set up initial filesystem structure using syscalls
fn init_filesystem() {
    // Create user home directory
    syscall::mkdir("/home/user").expect("create /home/user");

    // Write welcome file
    let welcome = r#"Welcome to axeberg!

This is your personal computing environment.
Tractable. Immediate. Yours.

Available commands:
  ls [path]      - List directory contents
  cd <path>      - Change directory
  pwd            - Print working directory
  cat <file>     - Display file contents
  echo <text>    - Print text
  mkdir <dir>    - Create directory
  touch <file>   - Create empty file
  rm <file>      - Remove file
  cp <src> <dst> - Copy file
  mv <src> <dst> - Move/rename file
  wc <file>      - Count lines/words/chars
  head <file>    - Show first lines
  tail <file>    - Show last lines
  clear          - Clear screen
  save           - Persist filesystem now
  help           - Show this help

Your files are automatically saved every 30 seconds and
when you close the browser. Use 'save' to save immediately.

Keyboard shortcuts:
  Ctrl+C  - Cancel current input
  Ctrl+L  - Clear screen
  Ctrl+U  - Clear line
  Ctrl+A  - Move to start of line
  Ctrl+E  - Move to end of line
  Up/Down - Command history
"#;
    let fd = syscall::open("/home/user/welcome.txt", OpenFlags::WRITE).expect("create welcome.txt");
    syscall::write(fd, welcome.as_bytes()).expect("write welcome.txt");
    syscall::close(fd).expect("close welcome.txt");

    // Write version file
    let fd = syscall::open("/etc/version", OpenFlags::WRITE).expect("create version");
    syscall::write(fd, b"axeberg 0.1.0\n").expect("write version");
    syscall::close(fd).expect("close version");
}

/// Initialize the compositor asynchronously
async fn init_compositor() {
    let mut comp = compositor::Compositor::new();

    if let Err(e) = comp.init().await {
        web_sys::console::error_1(&format!("[compositor] Surface init failed: {}", e).into());
        return;
    }

    // Create terminal window (takes full screen with single window)
    let owner = kernel::TaskId(0);
    let term_id = comp.create_terminal_window("Terminal", owner);

    // Print welcome message to terminal
    if let Some(term) = comp.get_terminal_mut(term_id) {
        term.print("axeberg v0.1.0");
        term.print("Type 'help' for available commands.");
        term.print("");
    }

    compositor::COMPOSITOR.with(|c| {
        *c.borrow_mut() = comp;
    });
}
