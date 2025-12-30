# Bare Metal Boot Plan

> ⚠️ **Status**: Planning phase. None of this is implemented yet.

Goal: Run axeberg directly on hardware (or QEMU) without a host OS.

## Background

Currently axeberg runs as WASM in:
- Browser (via wasm-bindgen, web-sys)
- WASI runtimes (wasmtime, wasmer) - planned

This plan outlines how to boot axeberg on bare metal by forking/learning from
[munal-os](https://github.com/Askannz/munal-os).

## Why munal-os as Reference

munal-os makes unconventional simplifications that align with axeberg's philosophy:

| Traditional OS | munal-os | Benefit |
|----------------|----------|---------|
| Bootloader (GRUB) | Single EFI binary | Simpler build |
| Virtual memory | Identity-mapped + WASM sandboxing | No page tables |
| Interrupts | Polling-based drivers | No interrupt handling |
| Preemptive scheduling | Cooperative (event loop) | Simpler scheduler |
| Multiple processes | WASM sandboxed apps | Memory isolation via WASM |

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                        axeberg.efi                          │
│  (single UEFI binary containing everything)                 │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────┐    │
│  │  axeberg kernel (Rust, #![no_std])                  │    │
│  │  - VFS (MemoryFs)                                   │    │
│  │  - Process table                                    │    │
│  │  - Shell + builtins                                 │    │
│  │  - Syscall interface                                │    │
│  └─────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  WASM Runtime (wasmi)                               │    │
│  │  - Loads .wasm command modules                      │    │
│  │  - Provides syscall imports                         │    │
│  └─────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  Platform Layer                                     │    │
│  │  - VirtIO keyboard driver (polling)                 │    │
│  │  - VirtIO GPU driver (framebuffer)                  │    │
│  │  - VirtIO network driver (optional)                 │    │
│  │  - VirtIO block driver (persistence)                │    │
│  └─────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  Terminal Renderer                                  │    │
│  │  - Text grid → framebuffer                          │    │
│  │  - Font rendering (embedded bitmap font)            │    │
│  └─────────────────────────────────────────────────────┘    │
├─────────────────────────────────────────────────────────────┤
│  UEFI Boot Services (used briefly, then exited)             │
├─────────────────────────────────────────────────────────────┤
│  Hardware (QEMU with VirtIO, or real UEFI machine)          │
└─────────────────────────────────────────────────────────────┘
```

## Phase 1: Hello World Boot

**Goal:** Boot to a blinking cursor in QEMU.

### Steps

1. **Set up toolchain**
   ```bash
   rustup target add x86_64-unknown-uefi
   cargo install cargo-make  # or use justfile
   ```

2. **Create minimal EFI binary**
   ```rust
   #![no_std]
   #![no_main]

   use uefi::prelude::*;

   #[entry]
   fn main(_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
       // Clear screen
       system_table.stdout().clear().unwrap();

       // Print hello
       writeln!(system_table.stdout(), "axeberg booting...").unwrap();

       // Exit boot services (we're on our own now)
       let (_runtime, _memory_map) = system_table.exit_boot_services();

       // Hang (we have no event loop yet)
       loop {
           core::hint::spin_loop();
       }
   }
   ```

3. **Create QEMU launch script**
   ```bash
   qemu-system-x86_64 \
     -enable-kvm \
     -m 512M \
     -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE.fd \
     -drive if=pflash,format=raw,file=OVMF_VARS.fd \
     -drive format=raw,file=fat:rw:esp \
     -device virtio-keyboard-pci \
     -device virtio-gpu-pci \
     -serial stdio
   ```

4. **Directory structure**
   ```
   axeberg-bare/
   ├── Cargo.toml
   ├── src/
   │   └── main.rs
   ├── esp/                    # EFI System Partition
   │   └── EFI/
   │       └── BOOT/
   │           └── BOOTX64.EFI
   └── run.sh
   ```

### Success Criteria
- QEMU boots
- "axeberg booting..." appears on screen
- System hangs cleanly (no triple fault)

## Phase 2: Framebuffer Terminal

**Goal:** Render text to GPU framebuffer.

### Steps

1. **Get GOP (Graphics Output Protocol) from UEFI**
   ```rust
   let gop = system_table
       .boot_services()
       .locate_protocol::<GraphicsOutput>()
       .unwrap();

   let mode_info = gop.current_mode_info();
   let framebuffer = gop.frame_buffer().as_mut_ptr();
   ```

2. **Embed a bitmap font**
   - Use a simple 8x16 VGA font (public domain)
   - Or use `noto-sans-mono-bitmap` crate

3. **Implement terminal renderer**
   ```rust
   struct Framebuffer {
       ptr: *mut u32,
       width: usize,
       height: usize,
       stride: usize,
   }

   impl Framebuffer {
       fn put_pixel(&mut self, x: usize, y: usize, color: u32) {
           unsafe {
               *self.ptr.add(y * self.stride + x) = color;
           }
       }

       fn draw_char(&mut self, x: usize, y: usize, c: char) {
           let glyph = FONT.get_glyph(c);
           for (row, bits) in glyph.iter().enumerate() {
               for col in 0..8 {
                   if bits & (1 << (7 - col)) != 0 {
                       self.put_pixel(x + col, y + row, 0xFFFFFF);
                   }
               }
           }
       }
   }
   ```

4. **Port Terminal struct**
   - Adapt terminal rendering code (currently in `src/shell/terminal.rs` for web) to use framebuffer instead of Canvas2D

### Success Criteria
- Text renders to screen
- Can print "axeberg v0.1.0"
- Cursor blinks (via polling loop)

## Phase 3: Keyboard Input

**Goal:** Read keyboard via VirtIO.

### Steps

1. **Enumerate PCI devices**
   - Walk PCI configuration space
   - Find VirtIO keyboard (vendor 0x1AF4, device 0x1052)

2. **Initialize VirtIO device**
   - Set up virtqueues
   - Allocate descriptor tables
   - Reference: munal-os `kernel/src/virtio/`

3. **Polling keyboard**
   ```rust
   fn poll_keyboard(&mut self) -> Option<KeyEvent> {
       // Check virtqueue for new events
       if let Some(event) = self.virtqueue.pop() {
           return Some(parse_hid_event(event));
       }
       None
   }
   ```

4. **Connect to shell**
   - Feed key events to existing shell/terminal code

### Success Criteria
- Typing appears on screen
- Backspace works
- Enter submits command

## Phase 4: Integrate axeberg Kernel

**Goal:** Run existing axeberg kernel code.

### Steps

1. **Make kernel `no_std` compatible** (requires major refactoring)
   - Remove `std` dependencies from core kernel
   - Use `alloc` crate for Vec, String, HashMap
   - Implement global allocator (linked_list_allocator or similar)

2. **Create Platform trait**
   ```rust
   pub trait Platform {
       fn write_stdout(&mut self, s: &str);
       fn read_key(&mut self) -> Option<KeyEvent>;
       fn now_ms(&self) -> u64;
       fn save_state(&mut self, data: &[u8]) -> Result<(), Error>;
       fn load_state(&mut self) -> Result<Option<Vec<u8>>, Error>;
   }
   ```

3. **Implement BareMetal platform**
   ```rust
   struct BareMetalPlatform {
       framebuffer: Framebuffer,
       keyboard: VirtioKeyboard,
       terminal: Terminal,
       disk: Option<VirtioDisk>,
   }

   impl Platform for BareMetalPlatform {
       fn write_stdout(&mut self, s: &str) {
           self.terminal.print(s);
           self.terminal.render(&mut self.framebuffer);
       }

       fn read_key(&mut self) -> Option<KeyEvent> {
           self.keyboard.poll()
       }

       // ...
   }
   ```

4. **Main loop**
   ```rust
   fn main_loop(platform: &mut BareMetalPlatform) -> ! {
       let mut kernel = Kernel::new();
       kernel.boot();

       loop {
           // Poll input
           while let Some(key) = platform.read_key() {
               kernel.handle_key(key);
           }

           // Tick kernel
           kernel.tick();

           // Render
           platform.render();

           // Small delay to avoid burning CPU
           spin_delay_us(1000);
       }
   }
   ```

### Success Criteria
- Shell prompt appears
- Can run `ls`, `pwd`, `echo`
- VFS works

## Phase 5: Persistence

**Goal:** Save/load filesystem to VirtIO disk.

### Steps

1. **Add VirtIO block driver**
   - Similar structure to keyboard driver
   - Read/write sectors

2. **Simple disk format**
   ```
   Sector 0: Magic + metadata
   Sector 1-N: JSON filesystem snapshot (same format as OPFS)
   ```

3. **Hook into save/load**
   ```rust
   impl Platform for BareMetalPlatform {
       fn save_state(&mut self, data: &[u8]) -> Result<(), Error> {
           self.disk.write_sectors(1, data)
       }

       fn load_state(&mut self) -> Result<Option<Vec<u8>>, Error> {
           self.disk.read_sectors(1)
       }
   }
   ```

### Success Criteria
- Create file, reboot, file still exists
- Works in QEMU with disk image

## Phase 6: WASM App Loading

**Goal:** Run .wasm binaries from filesystem.

### Steps

1. **Embed wasmi runtime**
   - Add `wasmi` to dependencies (it's `no_std` compatible)

2. **Implement WASM loader**
   - Reuse existing `src/kernel/wasm/` module
   - Adapt for wasmi instead of browser WebAssembly

3. **Syscall bridge**
   - Connect wasmi imports to kernel syscalls
   - Same ABI as defined in `docs/kernel/wasm-modules.md`

### Success Criteria
- Can load and run a simple .wasm program
- WASM can call write() syscall

## Dependencies

### Rust Crates (no_std)

| Crate | Purpose |
|-------|---------|
| `uefi` | UEFI boot interface |
| `linked_list_allocator` | Heap allocation |
| `spin` | Spinlocks |
| `volatile` | MMIO access |
| `wasmi` | WASM runtime |
| `serde` + `serde_json` | Persistence (needs alloc) |

### Build Requirements

- Rust stable (1.83+) (for `#![no_std]` features)
- QEMU 8.0+ with OVMF firmware
- `x86_64-unknown-uefi` target

## File Structure

**Note:** The following structure is PROPOSED and not yet implemented. It represents the target architecture for bare metal support.

```
axeberg/
├── kernel/                    # Shared kernel code (no_std)
│   ├── src/
│   │   ├── vfs/
│   │   ├── process/
│   │   ├── shell/
│   │   └── wasm/
│   └── Cargo.toml
├── platform-web/              # Browser platform
│   ├── src/
│   │   ├── lib.rs
│   │   ├── runtime.rs
│   │   └── compositor.rs
│   └── Cargo.toml
├── platform-wasi/             # WASI CLI platform
│   ├── src/
│   │   └── main.rs
│   └── Cargo.toml
├── platform-bare/             # Bare metal platform
│   ├── src/
│   │   ├── main.rs
│   │   ├── framebuffer.rs
│   │   ├── virtio/
│   │   └── terminal.rs
│   └── Cargo.toml
└── Cargo.toml                 # Workspace
```

## Milestones

1. **M1: Boot** - EFI binary boots, prints to serial
2. **M2: Display** - Text renders to framebuffer
3. **M3: Input** - Keyboard works
4. **M4: Shell** - axeberg shell runs commands
5. **M5: Persist** - Filesystem survives reboot
6. **M6: WASM** - Can run .wasm binaries

## Estimated Complexity

| Phase | Effort | Notes |
|-------|--------|-------|
| Phase 1 | Low | Mostly boilerplate |
| Phase 2 | Medium | Font rendering, framebuffer math |
| Phase 3 | High | VirtIO is complex |
| Phase 4 | Medium | Refactoring existing code |
| Phase 5 | Medium | Block device I/O |
| Phase 6 | Low | wasmi is well-documented |

## References

- [munal-os](https://github.com/Askannz/munal-os) - Primary reference
- [Writing an OS in Rust](https://os.phil-opp.com/) - Foundational tutorial
- [UEFI spec](https://uefi.org/specs/UEFI/2.10/) - Boot protocol
- [VirtIO spec](https://docs.oasis-open.org/virtio/virtio/v1.1/virtio-v1.1.html) - Device drivers
- [wasmi](https://github.com/wasmi-labs/wasmi) - no_std WASM runtime

## Open Questions

1. **Real hardware?** VirtIO only works in QEMU. Real hardware needs PS/2, AHCI, etc.
2. **Networking?** VirtIO-net is straightforward, but do we need it?
3. **Multi-core?** Single-threaded for now, but could use other cores later.
4. **Interrupts?** Polling works but is power-hungry. Add interrupts later?

---

*This plan can be executed incrementally. Each phase builds on the previous.*
