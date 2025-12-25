# Signals

The signal system provides inter-process communication and process control, inspired by POSIX signals but adapted for our WASM environment.

## Overview

Signals are asynchronous notifications sent to processes. Each process can:
- Receive signals from other processes or the kernel
- Set custom dispositions (handle, ignore, default)
- Block signals temporarily
- Query pending signals

```
┌─────────────────────────────────────────────────┐
│                    Process                       │
│  ┌─────────────────┐  ┌─────────────────────┐   │
│  │   Disposition   │  │   Pending Queue     │   │
│  │ SIGTERM→Handle  │  │ [SIGUSR1, SIGALRM]  │   │
│  │ SIGINT →Ignore  │  └─────────────────────┘   │
│  │ SIGHUP →Default │  ┌─────────────────────┐   │
│  └─────────────────┘  │   Blocked Set       │   │
│                       │ {SIGUSR2}           │   │
│                       └─────────────────────┘   │
└─────────────────────────────────────────────────┘
```

## Available Signals

| Signal | Number | Default Action | Description |
|--------|--------|----------------|-------------|
| SIGTERM | 1 | Terminate | Graceful termination request |
| SIGKILL | 2 | Terminate | Immediate termination (cannot be caught) |
| SIGSTOP | 3 | Stop | Stop process (cannot be caught) |
| SIGCONT | 4 | Continue | Resume stopped process |
| SIGINT | 5 | Terminate | Interrupt (Ctrl+C equivalent) |
| SIGQUIT | 6 | Terminate | Quit with core dump |
| SIGHUP | 7 | Terminate | Hangup |
| SIGUSR1 | 8 | Terminate | User-defined signal 1 |
| SIGUSR2 | 9 | Terminate | User-defined signal 2 |
| SIGCHLD | 10 | Ignore | Child process status changed |
| SIGALRM | 11 | Terminate | Timer alarm |
| SIGPIPE | 12 | Terminate | Broken pipe |

## Signal Actions

```rust
pub enum SignalAction {
    Default,    // Use the signal's default action
    Ignore,     // Ignore the signal
    Terminate,  // Terminate the process
    Kill,       // Kill the process (unconditional)
    Stop,       // Stop (pause) the process
    Continue,   // Resume a stopped process
    Handle,     // Call a handler (task will be notified)
}
```

## API Reference

### Sending Signals

```rust
use axeberg::kernel::syscall::kill;
use axeberg::kernel::Signal;

// Send SIGTERM to process
kill(target_pid, Signal::SIGTERM)?;

// Send SIGKILL (cannot be blocked or caught)
kill(target_pid, Signal::SIGKILL)?;
```

### Setting Disposition

```rust
use axeberg::kernel::syscall::signal;
use axeberg::kernel::{Signal, SignalAction};

// Ignore SIGPIPE
signal(Signal::SIGPIPE, SignalAction::Ignore)?;

// Use default action for SIGTERM
signal(Signal::SIGTERM, SignalAction::Default)?;

// Note: SIGKILL and SIGSTOP cannot have their disposition changed
```

### Blocking Signals

```rust
use axeberg::kernel::syscall::{sigblock, sigunblock};

// Block SIGUSR1 (it will queue but not deliver)
sigblock(Signal::SIGUSR1)?;

// Do critical work...

// Unblock (queued signals now deliverable)
sigunblock(Signal::SIGUSR1)?;
```

### Checking Pending Signals

```rust
use axeberg::kernel::syscall::sigpending;

if sigpending()? {
    println!("There are pending signals!");
}
```

## Special Behaviors

### SIGKILL and SIGSTOP

These signals cannot be:
- Caught (disposition cannot be changed)
- Blocked (always delivered)
- Ignored

They always take their default action (terminate or stop).

### SIGCONT

When SIGCONT is delivered:
- The process resumes if stopped
- Any pending SIGSTOP is removed
- The signal itself is always delivered

### Signal Coalescing

Multiple instances of the same signal (except SIGKILL) are coalesced into one. If SIGUSR1 is sent twice before being delivered, only one is queued.

### Priority

When checking for pending signals:
1. SIGKILL is always checked first
2. SIGSTOP is checked second
3. Other signals in FIFO order

## Process States

Signals can change process state:

```
Running ──SIGSTOP──► Stopped
   ▲                    │
   └────SIGCONT─────────┘

Running ──SIGKILL──► Zombie(-signal_num)
Running ──SIGTERM──► Zombie(-signal_num) (if default action)
```

## Example: Graceful Shutdown

```rust
use axeberg::kernel::syscall::{signal, sigpending, getpid};
use axeberg::kernel::{Signal, SignalAction};

async fn main_loop() {
    // Set up signal handling - ignore SIGTERM instead of Handle
    // (Handle is for future use, not fully implemented yet)
    signal(Signal::SIGTERM, SignalAction::Ignore).unwrap();

    loop {
        // Check for signals
        if sigpending().unwrap() {
            println!("Received shutdown signal, cleaning up...");
            cleanup().await;
            break;
        }

        // Do work
        do_work().await;
    }
}
```

## Implementation Notes

- Signals are stored per-process in `ProcessSignals`
- The signal disposition is stored in `SignalDisposition`
- Blocked signals are stored in a `HashSet<Signal>`
- Pending signals are stored in a `VecDeque<Signal>` (FIFO except for priority signals)
