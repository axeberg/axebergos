# Compositor

The compositor is responsible for rendering the user interface to the browser canvas.

## Current Implementation

axeberg currently uses **xterm.js** for terminal rendering instead of a custom compositor. This provides:

- Full terminal emulation with ANSI escape sequences
- Efficient text rendering with WebGL acceleration
- Selection, copy/paste, and search functionality
- Unicode and emoji support
- Configurable fonts and colors

```
┌─────────────────────────────────────────────────────────┐
│                      Browser Tab                         │
│  ┌───────────────────────────────────────────────────┐  │
│  │                   xterm.js                         │  │
│  │  ┌─────────────────────────────────────────────┐  │  │
│  │  │ user@axeberg:~$ ls -la                      │  │  │
│  │  │ drwxr-xr-x  5 user user 4096 Dec 26 README  │  │  │
│  │  │ drwxr-xr-x  3 user user 4096 Dec 26 src     │  │  │
│  │  │ -rw-r--r--  1 user user 1234 Dec 26 file.txt│  │  │
│  │  │ user@axeberg:~$ _                           │  │  │
│  │  └─────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

## Terminal Integration

The shell communicates with xterm.js through the platform layer:

```rust
// Platform trait for terminal I/O
pub trait Platform {
    fn write(&self, text: &str);
    fn read_key(&self) -> Option<KeyEvent>;
    fn get_size(&self) -> TermSize;
}

// Web implementation uses xterm.js
pub struct WebPlatform {
    terminal: xterm::Terminal,
}
```

### Key Events

Keyboard input flows from xterm.js to the shell:

1. User presses key in browser
2. xterm.js captures event
3. Platform layer converts to `KeyEvent`
4. Shell processes the input

```rust
pub enum Key {
    Char(char),
    Enter,
    Backspace,
    Tab,
    Up, Down, Left, Right,
    Home, End,
    Delete,
    Ctrl(char),
    Alt(char),
    Escape,
}
```

### Output Rendering

Shell output is written through the platform:

```rust
impl Platform for WebPlatform {
    fn write(&self, text: &str) {
        self.terminal.write(text);
    }
}
```

The shell uses ANSI escape sequences for formatting:

| Sequence | Effect |
|----------|--------|
| `\x1b[0m` | Reset styling |
| `\x1b[1m` | Bold |
| `\x1b[32m` | Green foreground |
| `\x1b[44m` | Blue background |
| `\x1b[2J` | Clear screen |
| `\x1b[H` | Move cursor home |

## Future: Custom Compositor

A custom Canvas2D/WebGPU compositor is planned for:

- Multiple windows with tiling layout
- Native-quality text rendering
- Custom window decorations
- Animation support

See the [Compositor Plan](../plans/compositor.md) for detailed design.

## Related Documentation

- [Shell](shell.md) - Command-line interpreter
- [Standard I/O](stdio.md) - Console and pipes
- [Compositor Plan](../plans/compositor.md) - Future window manager design
