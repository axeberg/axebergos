//! Boot sequence
//!
//! This is where axeberg comes to life. Following Radiant's philosophy:
//! boot should be immediate, comprehensible, and joyful.

use crate::compositor;
use crate::console_log;
use crate::kernel::{self, channel, Priority};
use crate::runtime;
use crate::vfs::{self, FileSystem, MemoryFs};
use std::cell::RefCell;

thread_local! {
    /// Global filesystem - will be abstracted behind a proper VFS layer later
    pub static FS: RefCell<MemoryFs> = RefCell::new(MemoryFs::new());
}

/// Boot the system
pub fn boot() {
    console_log!("┌─────────────────────────────────────┐");
    console_log!("│         axeberg v0.1.0              │");
    console_log!("│   A personal OS, reimagined         │");
    console_log!("└─────────────────────────────────────┘");

    console_log!("\n[boot] Initializing kernel...");

    // Initialize the VFS with some demonstration content
    init_filesystem();

    // Spawn system tasks
    spawn_init_tasks();

    // Start the runtime loop (this returns immediately, loop runs via rAF)
    console_log!("[boot] Starting runtime...");
    runtime::start();

    console_log!("[boot] System ready.");
    console_log!("\n  axeberg is alive. Welcome home.\n");
}

/// Set up initial filesystem structure
fn init_filesystem() {
    console_log!("[boot] Mounting memory filesystem...");

    FS.with(|fs| {
        let mut fs = fs.borrow_mut();

        // Create directory structure
        fs.create_dir("/home").expect("create /home");
        fs.create_dir("/home/user").expect("create /home/user");
        fs.create_dir("/etc").expect("create /etc");
        fs.create_dir("/tmp").expect("create /tmp");

        // Write a welcome file
        vfs::write_string(
            &mut *fs,
            "/home/user/welcome.txt",
            "Welcome to axeberg!\n\nThis is your personal computing environment.\nTractable. Immediate. Yours.\n",
        )
        .expect("write welcome.txt");

        // Write system info
        vfs::write_string(&mut *fs, "/etc/version", "axeberg 0.1.0\n").expect("write version");

        console_log!("[boot] Filesystem initialized:");
        console_log!("       /home/user/welcome.txt");
        console_log!("       /etc/version");
    });
}

/// Spawn initial system tasks
fn spawn_init_tasks() {
    console_log!("[boot] Spawning init tasks...");

    // Task 1: Read and display the welcome message
    kernel::spawn(async {
        console_log!("[task:welcome] Starting...");

        let content = FS.with(|fs| {
            let mut fs = fs.borrow_mut();
            vfs::read_to_string(&mut *fs, "/home/user/welcome.txt")
        });

        match content {
            Ok(text) => {
                console_log!("[task:welcome] Read welcome.txt:");
                for line in text.lines() {
                    console_log!("  {}", line);
                }
            }
            Err(e) => {
                console_log!("[task:welcome] Error: {:?}", e);
            }
        }

        console_log!("[task:welcome] Complete.");
    });

    // Task 2: Demonstrate IPC between tasks
    let (tx, rx) = channel::<String>();

    kernel::spawn(async move {
        console_log!("[task:sender] Sending message via IPC...");
        tx.send("Hello from sender task!".to_string()).unwrap();
        tx.send("IPC is working!".to_string()).unwrap();
        tx.close();
        console_log!("[task:sender] Messages sent, channel closed.");
    });

    kernel::spawn(async move {
        console_log!("[task:receiver] Waiting for messages...");

        // Poll until we get messages
        loop {
            match rx.try_recv() {
                Ok(msg) => {
                    console_log!("[task:receiver] Got: {}", msg);
                }
                Err(crate::kernel::ipc::TryRecvError::Closed) => {
                    console_log!("[task:receiver] Channel closed, done.");
                    break;
                }
                Err(crate::kernel::ipc::TryRecvError::Empty) => {
                    // Yield to other tasks
                    futures::pending!();
                }
            }
        }
    });

    // Task 3: Compositor initialization (Critical priority)
    // Rendering happens in runtime.rs, this just sets up the surface
    kernel::spawn_with_priority(
        async {
            console_log!("[compositor] Initializing...");

            // Use spawn_local for async init since compositor::init needs async
            wasm_bindgen_futures::spawn_local(init_compositor());
        },
        Priority::Critical,
    );

    console_log!("[boot] Init tasks spawned.");
}

/// Initialize the compositor asynchronously
async fn init_compositor() {
    // Create a new compositor, init it, then swap it into the global
    let mut comp = compositor::Compositor::new();

    if let Err(e) = comp.init().await {
        console_log!("[compositor] Surface init failed: {}", e);
        return;
    }

    console_log!("[compositor] Surface ready, creating demo windows...");

    // Create demo windows
    let owner = kernel::TaskId(0);
    comp.create_window("Terminal", owner);
    comp.create_window("Files", owner);

    // Swap initialized compositor into global
    compositor::COMPOSITOR.with(|c| {
        *c.borrow_mut() = comp;
    });

    console_log!("[compositor] Compositor ready with 2 windows.");
}
