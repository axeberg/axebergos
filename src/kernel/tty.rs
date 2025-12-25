//! TTY (terminal) subsystem
//!
//! Provides terminal settings (termios-like) and job control.
//! In this WASM environment, we simulate terminal behavior
//! for the virtual console.

use std::collections::HashMap;

/// Terminal input modes (c_iflag)
#[derive(Debug, Clone, Copy, Default)]
pub struct InputModes {
    /// Ignore BREAK condition
    pub ignbrk: bool,
    /// Signal interrupt on BREAK
    pub brkint: bool,
    /// Ignore characters with parity errors
    pub ignpar: bool,
    /// Strip 8th bit off characters
    pub istrip: bool,
    /// Map NL to CR on input
    pub inlcr: bool,
    /// Ignore CR
    pub igncr: bool,
    /// Map CR to NL on input
    pub icrnl: bool,
    /// Enable XON/XOFF flow control on input
    pub ixon: bool,
    /// Enable XON/XOFF flow control on output
    pub ixoff: bool,
}

/// Terminal output modes (c_oflag)
#[derive(Debug, Clone, Copy, Default)]
pub struct OutputModes {
    /// Perform output processing
    pub opost: bool,
    /// Map NL to CR-NL on output
    pub onlcr: bool,
    /// Map CR to NL on output
    pub ocrnl: bool,
    /// No CR output at column 0
    pub onocr: bool,
    /// NL performs CR function
    pub onlret: bool,
}

/// Terminal control modes (c_cflag)
#[derive(Debug, Clone, Copy)]
pub struct ControlModes {
    /// Character size (5-8 bits)
    pub csize: u8,
    /// Two stop bits (otherwise one)
    pub cstopb: bool,
    /// Enable receiver
    pub cread: bool,
    /// Enable parity generation/checking
    pub parenb: bool,
    /// Odd parity (otherwise even)
    pub parodd: bool,
    /// Hang up on last close
    pub hupcl: bool,
    /// Ignore modem control lines
    pub clocal: bool,
}

impl Default for ControlModes {
    fn default() -> Self {
        Self {
            csize: 8,
            cstopb: false,
            cread: true,
            parenb: false,
            parodd: false,
            hupcl: false,
            clocal: true,
        }
    }
}

/// Terminal local modes (c_lflag)
#[derive(Debug, Clone, Copy)]
pub struct LocalModes {
    /// Enable signals (INTR, QUIT, SUSP)
    pub isig: bool,
    /// Canonical mode (line editing)
    pub icanon: bool,
    /// Enable echo
    pub echo: bool,
    /// Echo erase character as BS-SP-BS
    pub echoe: bool,
    /// Echo NL after kill character
    pub echok: bool,
    /// Echo NL even if ECHO is off
    pub echonl: bool,
    /// Disable flush after interrupt/quit
    pub noflsh: bool,
    /// Send SIGTTOU for background output
    pub tostop: bool,
    /// Enable extended input processing
    pub iexten: bool,
}

impl Default for LocalModes {
    fn default() -> Self {
        Self {
            isig: true,
            icanon: true,
            echo: true,
            echoe: true,
            echok: true,
            echonl: false,
            noflsh: false,
            tostop: false,
            iexten: true,
        }
    }
}

/// Special control characters
#[derive(Debug, Clone)]
pub struct ControlChars {
    /// Interrupt character (default: Ctrl-C)
    pub vintr: char,
    /// Quit character (default: Ctrl-\)
    pub vquit: char,
    /// Erase character (default: DEL/Backspace)
    pub verase: char,
    /// Kill line character (default: Ctrl-U)
    pub vkill: char,
    /// End-of-file character (default: Ctrl-D)
    pub veof: char,
    /// Time-out value for non-canonical read
    pub vtime: u8,
    /// Minimum chars for non-canonical read
    pub vmin: u8,
    /// Start character (default: Ctrl-Q)
    pub vstart: char,
    /// Stop character (default: Ctrl-S)
    pub vstop: char,
    /// Suspend character (default: Ctrl-Z)
    pub vsusp: char,
    /// End-of-line character
    pub veol: char,
    /// Reprint line character (default: Ctrl-R)
    pub vreprint: char,
    /// Word erase character (default: Ctrl-W)
    pub vwerase: char,
    /// Literal next character (default: Ctrl-V)
    pub vlnext: char,
}

impl Default for ControlChars {
    fn default() -> Self {
        Self {
            vintr: '\x03',    // Ctrl-C
            vquit: '\x1c',    // Ctrl-\
            verase: '\x7f',   // DEL
            vkill: '\x15',    // Ctrl-U
            veof: '\x04',     // Ctrl-D
            vtime: 0,
            vmin: 1,
            vstart: '\x11',   // Ctrl-Q
            vstop: '\x13',    // Ctrl-S
            vsusp: '\x1a',    // Ctrl-Z
            veol: '\0',
            vreprint: '\x12', // Ctrl-R
            vwerase: '\x17',  // Ctrl-W
            vlnext: '\x16',   // Ctrl-V
        }
    }
}

/// Terminal settings (termios-like structure)
#[derive(Debug, Clone)]
pub struct Termios {
    /// Input modes
    pub iflag: InputModes,
    /// Output modes
    pub oflag: OutputModes,
    /// Control modes
    pub cflag: ControlModes,
    /// Local modes
    pub lflag: LocalModes,
    /// Control characters
    pub cc: ControlChars,
    /// Input baud rate (simulated)
    pub ispeed: u32,
    /// Output baud rate (simulated)
    pub ospeed: u32,
}

impl Default for Termios {
    fn default() -> Self {
        Self {
            iflag: InputModes {
                icrnl: true,
                ..Default::default()
            },
            oflag: OutputModes {
                opost: true,
                onlcr: true,
                ..Default::default()
            },
            cflag: ControlModes::default(),
            lflag: LocalModes::default(),
            cc: ControlChars::default(),
            ispeed: 38400,
            ospeed: 38400,
        }
    }
}

impl Termios {
    /// Create a "sane" terminal configuration
    pub fn sane() -> Self {
        Self::default()
    }

    /// Create a "raw" terminal configuration (no processing)
    pub fn raw() -> Self {
        Self {
            iflag: InputModes::default(),
            oflag: OutputModes::default(),
            cflag: ControlModes::default(),
            lflag: LocalModes {
                isig: false,
                icanon: false,
                echo: false,
                echoe: false,
                echok: false,
                echonl: false,
                noflsh: false,
                tostop: false,
                iexten: false,
            },
            cc: ControlChars {
                vmin: 1,
                vtime: 0,
                ..Default::default()
            },
            ispeed: 38400,
            ospeed: 38400,
        }
    }

    /// Create a "cooked" (canonical) configuration
    pub fn cooked() -> Self {
        Self::default()
    }
}

/// Terminal device
#[derive(Debug, Clone)]
pub struct Tty {
    /// Device name (e.g., "tty1", "pts/0")
    pub name: String,
    /// Terminal settings
    pub termios: Termios,
    /// Foreground process group
    pub pgrp: Option<u32>,
    /// Session ID
    pub session: Option<u32>,
    /// Number of rows
    pub rows: u16,
    /// Number of columns
    pub cols: u16,
}

impl Tty {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            termios: Termios::default(),
            pgrp: None,
            session: None,
            rows: 24,
            cols: 80,
        }
    }

    /// Get terminal size
    pub fn get_winsize(&self) -> (u16, u16) {
        (self.rows, self.cols)
    }

    /// Set terminal size
    pub fn set_winsize(&mut self, rows: u16, cols: u16) {
        self.rows = rows;
        self.cols = cols;
    }

    /// Get terminal settings
    pub fn get_termios(&self) -> &Termios {
        &self.termios
    }

    /// Set terminal settings
    pub fn set_termios(&mut self, termios: Termios) {
        self.termios = termios;
    }

    /// Check if terminal is in canonical mode
    pub fn is_canonical(&self) -> bool {
        self.termios.lflag.icanon
    }

    /// Check if echo is enabled
    pub fn is_echo(&self) -> bool {
        self.termios.lflag.echo
    }
}

/// TTY device manager
pub struct TtyManager {
    /// Active TTY devices
    ttys: HashMap<String, Tty>,
    /// Current (controlling) TTY name
    current: Option<String>,
}

impl TtyManager {
    pub fn new() -> Self {
        let mut mgr = Self {
            ttys: HashMap::new(),
            current: None,
        };

        // Create default console TTY
        mgr.create_tty("console");
        mgr.create_tty("tty1");
        mgr.current = Some("console".to_string());

        mgr
    }

    /// Create a new TTY device
    pub fn create_tty(&mut self, name: &str) -> &Tty {
        self.ttys.insert(name.to_string(), Tty::new(name));
        self.ttys.get(name).unwrap()
    }

    /// Get a TTY device
    pub fn get_tty(&self, name: &str) -> Option<&Tty> {
        self.ttys.get(name)
    }

    /// Get a TTY device mutably
    pub fn get_tty_mut(&mut self, name: &str) -> Option<&mut Tty> {
        self.ttys.get_mut(name)
    }

    /// Get current (controlling) TTY
    pub fn current_tty(&self) -> Option<&Tty> {
        self.current.as_ref().and_then(|name| self.ttys.get(name))
    }

    /// Get current TTY mutably
    pub fn current_tty_mut(&mut self) -> Option<&mut Tty> {
        if let Some(ref name) = self.current {
            self.ttys.get_mut(name)
        } else {
            None
        }
    }

    /// Set current TTY
    pub fn set_current(&mut self, name: &str) -> bool {
        if self.ttys.contains_key(name) {
            self.current = Some(name.to_string());
            true
        } else {
            false
        }
    }

    /// List all TTY names
    pub fn list(&self) -> Vec<&str> {
        self.ttys.keys().map(|s| s.as_str()).collect()
    }

    /// Get termios for a TTY
    pub fn tcgetattr(&self, name: &str) -> Option<Termios> {
        self.ttys.get(name).map(|t| t.termios.clone())
    }

    /// Set termios for a TTY
    pub fn tcsetattr(&mut self, name: &str, termios: Termios) -> bool {
        if let Some(tty) = self.ttys.get_mut(name) {
            tty.termios = termios;
            true
        } else {
            false
        }
    }
}

impl Default for TtyManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse stty-style setting string
pub fn parse_stty_setting(termios: &mut Termios, setting: &str) -> Result<(), String> {
    let (negate, setting) = if let Some(s) = setting.strip_prefix('-') {
        (true, s)
    } else {
        (false, setting)
    };

    match setting {
        // Input flags
        "ignbrk" => termios.iflag.ignbrk = !negate,
        "brkint" => termios.iflag.brkint = !negate,
        "ignpar" => termios.iflag.ignpar = !negate,
        "istrip" => termios.iflag.istrip = !negate,
        "inlcr" => termios.iflag.inlcr = !negate,
        "igncr" => termios.iflag.igncr = !negate,
        "icrnl" => termios.iflag.icrnl = !negate,
        "ixon" => termios.iflag.ixon = !negate,
        "ixoff" => termios.iflag.ixoff = !negate,

        // Output flags
        "opost" => termios.oflag.opost = !negate,
        "onlcr" => termios.oflag.onlcr = !negate,
        "ocrnl" => termios.oflag.ocrnl = !negate,
        "onocr" => termios.oflag.onocr = !negate,
        "onlret" => termios.oflag.onlret = !negate,

        // Local flags
        "isig" => termios.lflag.isig = !negate,
        "icanon" => termios.lflag.icanon = !negate,
        "echo" => termios.lflag.echo = !negate,
        "echoe" => termios.lflag.echoe = !negate,
        "echok" => termios.lflag.echok = !negate,
        "echonl" => termios.lflag.echonl = !negate,
        "noflsh" => termios.lflag.noflsh = !negate,
        "tostop" => termios.lflag.tostop = !negate,
        "iexten" => termios.lflag.iexten = !negate,

        // Control flags
        "cstopb" => termios.cflag.cstopb = !negate,
        "cread" => termios.cflag.cread = !negate,
        "parenb" => termios.cflag.parenb = !negate,
        "parodd" => termios.cflag.parodd = !negate,
        "hupcl" => termios.cflag.hupcl = !negate,
        "clocal" => termios.cflag.clocal = !negate,

        // Special modes
        "raw" => {
            if !negate {
                *termios = Termios::raw();
            }
        }
        "cooked" | "sane" => {
            if !negate {
                *termios = Termios::sane();
            }
        }

        // Character size
        "cs5" => termios.cflag.csize = 5,
        "cs6" => termios.cflag.csize = 6,
        "cs7" => termios.cflag.csize = 7,
        "cs8" => termios.cflag.csize = 8,

        _ => return Err(format!("unknown setting: {}", setting)),
    }

    Ok(())
}

/// Format termios settings for stty output
pub fn format_stty_settings(termios: &Termios) -> String {
    let mut output = String::new();

    // Speed
    output.push_str(&format!("speed {} baud; ", termios.ospeed));
    output.push_str(&format!("rows 24; columns 80;\n"));

    // Control characters
    output.push_str(&format!(
        "intr = ^C; quit = ^\\; erase = ^?; kill = ^U; eof = ^D;\n"
    ));
    output.push_str(&format!(
        "susp = ^Z; start = ^Q; stop = ^S;\n"
    ));

    // Input flags
    let mut iflags = Vec::new();
    if termios.iflag.icrnl { iflags.push("icrnl"); } else { iflags.push("-icrnl"); }
    if termios.iflag.ixon { iflags.push("ixon"); } else { iflags.push("-ixon"); }
    if termios.iflag.istrip { iflags.push("istrip"); } else { iflags.push("-istrip"); }
    output.push_str(&iflags.join(" "));
    output.push('\n');

    // Output flags
    let mut oflags = Vec::new();
    if termios.oflag.opost { oflags.push("opost"); } else { oflags.push("-opost"); }
    if termios.oflag.onlcr { oflags.push("onlcr"); } else { oflags.push("-onlcr"); }
    output.push_str(&oflags.join(" "));
    output.push('\n');

    // Local flags
    let mut lflags = Vec::new();
    if termios.lflag.isig { lflags.push("isig"); } else { lflags.push("-isig"); }
    if termios.lflag.icanon { lflags.push("icanon"); } else { lflags.push("-icanon"); }
    if termios.lflag.echo { lflags.push("echo"); } else { lflags.push("-echo"); }
    if termios.lflag.echoe { lflags.push("echoe"); } else { lflags.push("-echoe"); }
    if termios.lflag.echok { lflags.push("echok"); } else { lflags.push("-echok"); }
    output.push_str(&lflags.join(" "));
    output.push('\n');

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_termios_default() {
        let termios = Termios::default();
        assert!(termios.lflag.icanon);
        assert!(termios.lflag.echo);
        assert!(termios.oflag.opost);
    }

    #[test]
    fn test_termios_raw() {
        let termios = Termios::raw();
        assert!(!termios.lflag.icanon);
        assert!(!termios.lflag.echo);
        assert!(!termios.lflag.isig);
    }

    #[test]
    fn test_tty_manager() {
        let mut mgr = TtyManager::new();

        assert!(mgr.get_tty("console").is_some());
        assert!(mgr.get_tty("tty1").is_some());

        mgr.create_tty("pts/0");
        assert!(mgr.get_tty("pts/0").is_some());
    }

    #[test]
    fn test_parse_stty_setting() {
        let mut termios = Termios::default();

        parse_stty_setting(&mut termios, "-echo").unwrap();
        assert!(!termios.lflag.echo);

        parse_stty_setting(&mut termios, "echo").unwrap();
        assert!(termios.lflag.echo);

        parse_stty_setting(&mut termios, "raw").unwrap();
        assert!(!termios.lflag.icanon);
    }

    #[test]
    fn test_winsize() {
        let mut tty = Tty::new("tty1");
        assert_eq!(tty.get_winsize(), (24, 80));

        tty.set_winsize(40, 120);
        assert_eq!(tty.get_winsize(), (40, 120));
    }
}
