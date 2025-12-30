# Bare Metal Boot Architecture

> ⚠️ **Status**: This document describes planned bare metal support. Currently only Browser (WASM) and WASI targets are implemented.

This document explores options for running AxebergOS on bare metal hardware, separate from the WASM/browser approach.

## Current Architecture (WASM)

The current AxebergOS runs in a browser via WebAssembly:

```
Browser
  └── WebAssembly Runtime
        └── AxebergOS WASM Module
              ├── Kernel (in-memory)
              ├── VFS (in-memory, OPFS persistence)
              └── Shell (xterm.js terminal)
```

Key characteristics:
- No direct hardware access
- Browser provides I/O (canvas, DOM events)
- OPFS for persistence
- Fetch API for networking
- Sandboxed execution

## Bare Metal Options

### Option 1: UEFI Application

Build AxebergOS as a UEFI application that runs directly from the bootloader.

**Advantages:**
- Simple boot process
- UEFI provides services (filesystem, networking, console)
- Works on modern x86_64 and ARM64 hardware
- No bootloader needed (IS the bootloader)

**Requirements:**
- `x86_64-unknown-uefi` or `aarch64-unknown-uefi` target
- UEFI protocols for I/O
- EFI system partition

**Crates:**
- `uefi` - UEFI runtime services
- `uefi-services` - Higher-level UEFI helpers

**Boot flow:**

```
UEFI Firmware
  └── Load axeberg.efi from EFI System Partition
        └── UEFI Application (AxebergOS)
              ├── Use UEFI Simple Text Output for console
              ├── Use UEFI Simple File System for storage
              └── Use UEFI Network Stack for networking
```

### Option 2: Multiboot2 Kernel (BIOS/Legacy)

Build as a Multiboot2-compliant kernel loaded by GRUB2.

**Advantages:**
- Works on older hardware (BIOS boot)
- Well-documented standard
- GRUB handles boot complexity

**Requirements:**
- Custom linker script
- Multiboot2 header
- `x86_64-unknown-none` target
- No_std environment

**Crates:**
- `bootloader` - Rust bootloader
- `multiboot2` - Multiboot2 info parsing

**Boot flow:**

```
BIOS
  └── GRUB2
        └── Load axeberg.elf
              └── Kernel Entry Point
                    ├── Set up GDT, IDT
                    ├── Enable paging
                    ├── Initialize hardware
                    └── Start scheduler
```

### Option 3: Custom Bootloader

Use the `bootloader` crate to create a complete bootable image.

**Advantages:**
- Full control over boot process
- Single build artifact
- Works on both BIOS and UEFI

**Requirements:**
- `bootloader` crate as build dependency
- Custom cargo configuration
- Disk image generation

**Example Cargo.toml:**

```toml
[dependencies]
bootloader = "0.11"

[package.metadata.bootloader]
kernel = "target/x86_64-axeberg/release/axeberg"
```

## Architecture Comparison

| Feature | WASM (Browser) | UEFI | Bare Metal |
|---------|----------------|------|------------|
| Target | wasm32-unknown-unknown | x86_64-unknown-uefi | x86_64-unknown-none |
| Std library | Partial | No | No |
| Hardware access | None | Via UEFI protocols | Direct |
| Graphics | Canvas/DOM | GOP (UEFI Graphics) | Framebuffer |
| Keyboard | DOM events | UEFI Input | PS/2, USB HID |
| Storage | OPFS | UEFI Filesystem | AHCI, NVMe |
| Networking | Fetch API | UEFI Network | E1000, Virtio |
| Memory alloc | Browser heap | UEFI alloc | Custom allocator |

## Shared Code Strategy

To support both WASM and bare metal, structure the code:

```
src/
├── lib.rs           # Core logic (platform-agnostic)
├── kernel/
│   ├── mod.rs       # Kernel abstractions
│   ├── process.rs   # Process management
│   └── syscall.rs   # Syscall interface
├── vfs/             # Virtual filesystem (separate module)
├── platform/
│   ├── mod.rs       # Platform trait
│   ├── web.rs       # Browser/WASM implementation
│   └── wasi.rs      # WASI CLI implementation
└── main.rs          # Entry point (cfg-gated)
```

The `Platform` trait abstracts hardware differences:

```rust
pub trait Platform {
    // ===== Terminal Output =====

    /// Write text to the terminal
    fn write(&mut self, text: &str);

    /// Clear the terminal screen
    fn clear(&mut self);

    /// Get terminal dimensions
    fn term_size(&self) -> TermSize;

    // ===== Input =====

    /// Poll for a key event (non-blocking)
    fn poll_key(&mut self) -> Option<KeyEvent>;

    // ===== Timing =====

    /// Get current time in milliseconds since some epoch
    fn now_ms(&self) -> f64;

    // ===== Persistence =====

    /// Save state to persistent storage
    fn save_state(&mut self, data: &[u8]) -> PlatformResult<()>;

    /// Load state from persistent storage
    fn load_state(&mut self) -> PlatformResult<Option<Vec<u8>>>;

    // ===== Lifecycle =====

    /// Called each frame/tick of the main loop
    fn tick(&mut self) {}

    /// Check if the platform wants to exit
    fn should_exit(&self) -> bool {
        false
    }
}
```

## Required Changes for Bare Metal

1. **No std**: Replace std dependencies with no_std alternatives
2. **Allocator**: Implement global allocator (bump, linked list, buddy)
3. **Panic handler**: Custom panic handling
4. **Entry point**: Architecture-specific entry
5. **Hardware drivers**: Console, storage, interrupts

## Recommended Path

1. **Start with UEFI** - Simplest path, UEFI provides many services
2. **Use `bootloader` crate** - For true bare metal, this handles boot complexity
3. **Incremental porting** - Start with console, then add storage, then networking

## Example: Minimal UEFI App

```rust
#![no_std]
#![no_main]

use uefi::prelude::*;

#[entry]
fn main(_image: Handle, mut system_table: SystemTable<Boot>) -> Status {
    // Clear screen
    system_table.stdout().clear().unwrap();

    // Print welcome
    writeln!(system_table.stdout(), "AxebergOS v0.1.0").unwrap();
    writeln!(system_table.stdout(), "Running on UEFI!").unwrap();

    // Wait for keypress
    system_table.boot_services().wait_for_event(
        &mut [system_table.stdin().wait_for_key_event().unwrap()]
    ).unwrap();

    Status::SUCCESS
}
```

## Conclusion

Bare metal support is a significant undertaking but feasible. The recommended approach:

1. Keep the current WASM version as the primary target
2. Abstract platform-specific code behind traits
3. Start with UEFI for the simplest bare metal path
4. Gradually add hardware drivers as needed

The core kernel logic (process management, VFS, syscalls) should work unchanged across platforms with proper abstraction.
