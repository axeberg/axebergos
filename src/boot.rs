//! Boot sequence
//!
//! This is where axeberg comes to life. Following Radiant's philosophy:
//! boot should be immediate, comprehensible, and joyful.

use crate::compositor;
use crate::console_log;
use crate::kernel::syscall::{self, OpenFlags};
use crate::kernel::{self, channel, Fd, Priority};
use crate::runtime;

/// Boot the system
pub fn boot() {
    console_log!("┌─────────────────────────────────────┐");
    console_log!("│         axeberg v0.1.0              │");
    console_log!("│   A personal OS, reimagined         │");
    console_log!("└─────────────────────────────────────┘");

    console_log!("\n[boot] Initializing kernel...");

    // Create init process (PID 1)
    let init_pid = syscall::spawn_process("init");
    syscall::set_current_process(init_pid);
    console_log!("[boot] Init process started ({})", init_pid);

    // Initialize the filesystem with content
    init_filesystem();

    // Spawn system processes
    spawn_init_processes();

    // Start the runtime loop (this returns immediately, loop runs via rAF)
    console_log!("[boot] Starting runtime...");
    runtime::start();

    console_log!("[boot] System ready.");
    console_log!("\n  axeberg is alive. Welcome home.\n");
}

/// Set up initial filesystem structure using syscalls
fn init_filesystem() {
    console_log!("[boot] Initializing filesystem...");

    // Create user home directory
    syscall::mkdir("/home/user").expect("create /home/user");

    // Write welcome file
    let welcome = "Welcome to axeberg!\n\nThis is your personal computing environment.\nTractable. Immediate. Yours.\n";
    let fd = syscall::open("/home/user/welcome.txt", OpenFlags::WRITE).expect("create welcome.txt");
    syscall::write(fd, welcome.as_bytes()).expect("write welcome.txt");
    syscall::close(fd).expect("close welcome.txt");

    // Write version file
    let fd = syscall::open("/etc/version", OpenFlags::WRITE).expect("create version");
    syscall::write(fd, b"axeberg 0.1.0\n").expect("write version");
    syscall::close(fd).expect("close version");

    console_log!("[boot] Filesystem initialized:");
    console_log!("       /home/user/welcome.txt");
    console_log!("       /etc/version");
}

/// Spawn initial system processes
fn spawn_init_processes() {
    console_log!("[boot] Spawning init processes...");

    // Process 1: Read and display the welcome message using syscalls
    kernel::spawn(async {
        console_log!("[proc:welcome] Starting...");

        // Read the welcome file via syscall
        match syscall::open("/home/user/welcome.txt", OpenFlags::READ) {
            Ok(fd) => {
                let mut buf = [0u8; 256];
                match syscall::read(fd, &mut buf) {
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&buf[..n]);
                        console_log!("[proc:welcome] Read welcome.txt:");
                        for line in text.lines() {
                            console_log!("  {}", line);
                        }
                    }
                    Err(e) => console_log!("[proc:welcome] Read error: {}", e),
                }
                let _ = syscall::close(fd);
            }
            Err(e) => console_log!("[proc:welcome] Open error: {}", e),
        }

        console_log!("[proc:welcome] Complete.");
    });

    // Process 2: Demonstrate writing to stdout (console)
    kernel::spawn(async {
        console_log!("[proc:hello] Writing to stdout...");

        // Write to stdout (fd 1) which goes to console
        let msg = b"Hello from axeberg!\n";
        match syscall::write(Fd::STDOUT, msg) {
            Ok(_) => console_log!("[proc:hello] Wrote to stdout"),
            Err(e) => console_log!("[proc:hello] Write error: {}", e),
        }
    });

    // Process 3: Demonstrate IPC between processes
    let (tx, rx) = channel::<String>();

    kernel::spawn(async move {
        console_log!("[proc:sender] Sending message via IPC...");
        tx.send("Hello from sender!".to_string()).unwrap();
        tx.send("IPC is working!".to_string()).unwrap();
        tx.close();
        console_log!("[proc:sender] Messages sent, channel closed.");
    });

    kernel::spawn(async move {
        console_log!("[proc:receiver] Waiting for messages...");

        loop {
            match rx.try_recv() {
                Ok(msg) => {
                    console_log!("[proc:receiver] Got: {}", msg);
                }
                Err(crate::kernel::ipc::TryRecvError::Closed) => {
                    console_log!("[proc:receiver] Channel closed, done.");
                    break;
                }
                Err(crate::kernel::ipc::TryRecvError::Empty) => {
                    futures::pending!();
                }
            }
        }
    });

    // Process 4: Compositor initialization (Critical priority)
    kernel::spawn_with_priority(
        async {
            console_log!("[compositor] Initializing...");
            wasm_bindgen_futures::spawn_local(init_compositor());
        },
        Priority::Critical,
    );

    console_log!("[boot] Init processes spawned.");
}

/// Initialize the compositor asynchronously
async fn init_compositor() {
    let mut comp = compositor::Compositor::new();

    if let Err(e) = comp.init().await {
        console_log!("[compositor] Surface init failed: {}", e);
        return;
    }

    console_log!("[compositor] Surface ready, creating demo windows...");

    // Create demo windows (using TaskId for now, will use syscall later)
    let owner = kernel::TaskId(0);
    comp.create_window("Terminal", owner);
    comp.create_window("Files", owner);

    compositor::COMPOSITOR.with(|c| {
        *c.borrow_mut() = comp;
    });

    console_log!("[compositor] Compositor ready with 2 windows.");
}
