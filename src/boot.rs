//! Boot sequence
//!
//! This is where axeberg comes to life. Following Radiant's philosophy:
//! boot should be immediate, comprehensible, and joyful.

#![cfg(target_arch = "wasm32")]

use crate::console_log;
use crate::kernel::syscall::{self, OpenFlags};
use crate::kernel::{self, Priority};
use crate::terminal;
use crate::vfs::Persistence;

/// Boot the system
pub fn boot() {
    // Create init process (PID 1)
    let init_pid = syscall::spawn_process("init");
    syscall::set_current_process(init_pid);
    console_log!("[boot] Created init process: {:?}", init_pid);

    // Initialize filesystem (async)
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

                // Initialize xterm.js terminal
                if let Err(e) = terminal::init() {
                    web_sys::console::error_1(&format!("[terminal] Init failed: {:?}", e).into());
                }
            });
        },
        Priority::Critical,
    );
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
    let welcome = "Welcome to axeberg!\n\n\
        This is your personal computing environment.\n\
        Tractable. Immediate. Yours.\n\n\
        Type 'help' for available commands.\n";

    let fd = syscall::open("/home/user/welcome.txt", OpenFlags::WRITE).expect("create welcome.txt");
    syscall::write(fd, welcome.as_bytes()).expect("write welcome.txt");
    syscall::close(fd).expect("close welcome.txt");

    // Write version file
    let fd = syscall::open("/etc/version", OpenFlags::WRITE).expect("create version");
    syscall::write(fd, b"axeberg 0.1.0\n").expect("write version");
    syscall::close(fd).expect("close version");
}
