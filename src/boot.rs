//! Boot sequence
//!
//! This is where axeberg comes to life. Following Radiant's philosophy:
//! boot should be immediate, comprehensible, and joyful.

use crate::compositor;
use crate::kernel::syscall::{self, OpenFlags};
use crate::kernel::{self, Priority};
use crate::runtime;

/// Boot the system
pub fn boot() {
    // Create init process (PID 1)
    let init_pid = syscall::spawn_process("init");
    syscall::set_current_process(init_pid);

    // Initialize the filesystem with content
    init_filesystem();

    // Start the compositor (Critical priority)
    kernel::spawn_with_priority(
        async {
            wasm_bindgen_futures::spawn_local(init_compositor());
        },
        Priority::Critical,
    );

    // Start the runtime loop (this returns immediately, loop runs via rAF)
    runtime::start();
}

/// Set up initial filesystem structure using syscalls
fn init_filesystem() {
    // Create user home directory
    syscall::mkdir("/home/user").expect("create /home/user");

    // Write welcome file
    let welcome = r#"Welcome to axeberg!

This is your personal computing environment.
Tractable. Immediate. Yours.

Keyboard shortcuts:
  Terminal: Type commands and press Enter
  File Browser:
    Arrow keys - Navigate
    Enter      - Open file/directory
    Backspace  - Go up
    n          - New file
    N          - New directory
    d/Delete   - Delete
    r/F2       - Rename
    c          - Copy
    x          - Cut
    p/v        - Paste
    Ctrl+R     - Refresh
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

    // Create windows
    let owner = kernel::TaskId(0);
    comp.create_terminal_window("Terminal", owner);
    comp.create_filebrowser_window("Files", owner);

    // Print welcome message to terminal
    if let Some(term) = comp.get_terminal_mut(compositor::WindowId(1)) {
        term.print("axeberg v0.1.0");
        term.print("Type 'help' for available commands, or 'cat /home/user/welcome.txt'");
        term.print("");
    }

    compositor::COMPOSITOR.with(|c| {
        *c.borrow_mut() = comp;
    });
}
