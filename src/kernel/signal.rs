//! Signal System
//!
//! Provides inter-process signaling for process control.
//!
//! Signals are asynchronous notifications sent to processes.
//! Each signal has a default action that can be overridden.
//!
//! ## Signal Numbers
//!
//! Note: axeberg uses its own signal numbering scheme for simplicity and clarity.
//! These numbers intentionally differ from POSIX conventions:
//!
//! | Signal   | axeberg | POSIX |
//! |----------|---------|-------|
//! | SIGTERM  | 1       | 15    |
//! | SIGKILL  | 2       | 9     |
//! | SIGSTOP  | 3       | 19    |
//! | SIGCONT  | 4       | 18    |
//! | SIGINT   | 5       | 2     |
//! | SIGQUIT  | 6       | 3     |
//! | SIGHUP   | 7       | 1     |
//! | SIGUSR1  | 8       | 10    |
//! | SIGUSR2  | 9       | 12    |
//! | SIGCHLD  | 10      | 17    |
//! | SIGALRM  | 11      | 14    |
//! | SIGPIPE  | 12      | 13    |
//!
//! The rationale for custom numbering:
//! - Simpler mental model (signals numbered 1-12)
//! - Easier to remember (no gaps like POSIX)
//! - axeberg is not POSIX-compatible, so no confusion expected

use super::process::Pid;
use std::collections::{HashMap, HashSet, VecDeque};

/// Signal types
///
/// See module documentation for signal number mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Signal {
    /// Terminate process (can be caught)
    SIGTERM = 1,
    /// Kill process (cannot be caught)
    SIGKILL = 2,
    /// Stop process
    SIGSTOP = 3,
    /// Continue stopped process
    SIGCONT = 4,
    /// Interrupt (Ctrl+C)
    SIGINT = 5,
    /// Quit with core dump
    SIGQUIT = 6,
    /// Hangup
    SIGHUP = 7,
    /// User-defined signal 1
    SIGUSR1 = 8,
    /// User-defined signal 2
    SIGUSR2 = 9,
    /// Child process terminated
    SIGCHLD = 10,
    /// Alarm timer expired
    SIGALRM = 11,
    /// Broken pipe
    SIGPIPE = 12,
}

impl Signal {
    /// Get signal from number
    pub fn from_num(n: u8) -> Option<Signal> {
        match n {
            1 => Some(Signal::SIGTERM),
            2 => Some(Signal::SIGKILL),
            3 => Some(Signal::SIGSTOP),
            4 => Some(Signal::SIGCONT),
            5 => Some(Signal::SIGINT),
            6 => Some(Signal::SIGQUIT),
            7 => Some(Signal::SIGHUP),
            8 => Some(Signal::SIGUSR1),
            9 => Some(Signal::SIGUSR2),
            10 => Some(Signal::SIGCHLD),
            11 => Some(Signal::SIGALRM),
            12 => Some(Signal::SIGPIPE),
            _ => None,
        }
    }

    /// Get signal number
    pub fn num(&self) -> u8 {
        *self as u8
    }

    /// Check if signal can be caught/ignored
    pub fn can_catch(&self) -> bool {
        !matches!(self, Signal::SIGKILL | Signal::SIGSTOP)
    }

    /// Get default action for this signal
    pub fn default_action(&self) -> SignalAction {
        match self {
            Signal::SIGTERM
            | Signal::SIGINT
            | Signal::SIGQUIT
            | Signal::SIGHUP
            | Signal::SIGPIPE => SignalAction::Terminate,
            Signal::SIGKILL => SignalAction::Kill,
            Signal::SIGSTOP => SignalAction::Stop,
            Signal::SIGCONT => SignalAction::Continue,
            Signal::SIGUSR1 | Signal::SIGUSR2 | Signal::SIGCHLD | Signal::SIGALRM => {
                SignalAction::Ignore
            }
        }
    }
}

impl std::fmt::Display for Signal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Signal::SIGTERM => write!(f, "SIGTERM"),
            Signal::SIGKILL => write!(f, "SIGKILL"),
            Signal::SIGSTOP => write!(f, "SIGSTOP"),
            Signal::SIGCONT => write!(f, "SIGCONT"),
            Signal::SIGINT => write!(f, "SIGINT"),
            Signal::SIGQUIT => write!(f, "SIGQUIT"),
            Signal::SIGHUP => write!(f, "SIGHUP"),
            Signal::SIGUSR1 => write!(f, "SIGUSR1"),
            Signal::SIGUSR2 => write!(f, "SIGUSR2"),
            Signal::SIGCHLD => write!(f, "SIGCHLD"),
            Signal::SIGALRM => write!(f, "SIGALRM"),
            Signal::SIGPIPE => write!(f, "SIGPIPE"),
        }
    }
}

/// Action to take when a signal is received
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalAction {
    /// Use the default action for this signal
    Default,
    /// Ignore the signal
    Ignore,
    /// Terminate the process
    Terminate,
    /// Kill the process (unconditional)
    Kill,
    /// Stop the process
    Stop,
    /// Continue a stopped process
    Continue,
    /// Call a handler (task will be notified)
    Handle,
}

/// Signal disposition - how a process handles each signal
#[derive(Debug, Clone)]
pub struct SignalDisposition {
    /// Action for each signal
    actions: HashMap<Signal, SignalAction>,
}

impl SignalDisposition {
    /// Create with default actions
    pub fn new() -> Self {
        Self {
            actions: HashMap::new(),
        }
    }

    /// Get action for a signal
    pub fn get_action(&self, signal: Signal) -> SignalAction {
        self.actions
            .get(&signal)
            .copied()
            .unwrap_or(SignalAction::Default)
    }

    /// Set action for a signal
    pub fn set_action(&mut self, signal: Signal, action: SignalAction) -> Result<(), SignalError> {
        if !signal.can_catch() && action != SignalAction::Default {
            return Err(SignalError::CannotCatch(signal));
        }
        self.actions.insert(signal, action);
        Ok(())
    }

    /// Reset a signal to default action
    pub fn reset(&mut self, signal: Signal) {
        self.actions.remove(&signal);
    }

    /// Reset all signals to defaults
    pub fn reset_all(&mut self) {
        self.actions.clear();
    }
}

impl Default for SignalDisposition {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-process signal state
#[derive(Debug)]
pub struct ProcessSignals {
    /// Signal disposition
    pub disposition: SignalDisposition,
    /// Pending signals (not yet delivered)
    pending: VecDeque<Signal>,
    /// Blocked signals (temporarily masked)
    blocked: HashSet<Signal>,
    /// Is process stopped?
    stopped: bool,
}

impl ProcessSignals {
    pub fn new() -> Self {
        Self {
            disposition: SignalDisposition::new(),
            pending: VecDeque::new(),
            blocked: HashSet::new(),
            stopped: false,
        }
    }

    /// Queue a signal for delivery
    pub fn send(&mut self, signal: Signal) {
        // SIGCONT always unblocks
        if signal == Signal::SIGCONT {
            self.stopped = false;
            // Remove any pending SIGSTOP
            self.pending.retain(|&s| s != Signal::SIGSTOP);
        }

        // Coalesce duplicate signals (except SIGKILL which always queues)
        if signal != Signal::SIGKILL && self.pending.contains(&signal) {
            return;
        }

        // Always add to pending - blocked signals are held until unblocked
        self.pending.push_back(signal);
    }

    /// Get the next pending signal (if any)
    pub fn next_pending(&mut self) -> Option<Signal> {
        // SIGKILL and SIGSTOP have priority
        if let Some(pos) = self.pending.iter().position(|&s| s == Signal::SIGKILL) {
            return Some(self.pending.remove(pos).unwrap());
        }
        if let Some(pos) = self.pending.iter().position(|&s| s == Signal::SIGSTOP) {
            return Some(self.pending.remove(pos).unwrap());
        }

        // Otherwise, first non-blocked signal
        for i in 0..self.pending.len() {
            let signal = self.pending[i];
            if !self.blocked.contains(&signal) {
                return Some(self.pending.remove(i).unwrap());
            }
        }
        None
    }

    /// Check if there are pending signals
    pub fn has_pending(&self) -> bool {
        self.pending
            .iter()
            .any(|s| !self.blocked.contains(s) || !s.can_catch())
    }

    /// Block a signal
    pub fn block(&mut self, signal: Signal) -> Result<(), SignalError> {
        if !signal.can_catch() {
            return Err(SignalError::CannotBlock(signal));
        }
        self.blocked.insert(signal);
        Ok(())
    }

    /// Unblock a signal
    pub fn unblock(&mut self, signal: Signal) {
        self.blocked.remove(&signal);
    }

    /// Get blocked signals
    pub fn get_blocked(&self) -> &HashSet<Signal> {
        &self.blocked
    }

    /// Is process stopped?
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// Stop the process
    pub fn stop(&mut self) {
        self.stopped = true;
    }

    /// Continue the process
    pub fn cont(&mut self) {
        self.stopped = false;
    }

    /// Count pending signals
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

impl Default for ProcessSignals {
    fn default() -> Self {
        Self::new()
    }
}

/// Signal errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalError {
    /// Signal cannot be caught
    CannotCatch(Signal),
    /// Signal cannot be blocked
    CannotBlock(Signal),
    /// Invalid signal number
    InvalidSignal(u8),
    /// Process not found
    ProcessNotFound(Pid),
    /// Permission denied
    PermissionDenied,
}

impl std::fmt::Display for SignalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignalError::CannotCatch(s) => write!(f, "cannot catch {}", s),
            SignalError::CannotBlock(s) => write!(f, "cannot block {}", s),
            SignalError::InvalidSignal(n) => write!(f, "invalid signal {}", n),
            SignalError::ProcessNotFound(p) => write!(f, "process not found: {}", p),
            SignalError::PermissionDenied => write!(f, "permission denied"),
        }
    }
}

impl std::error::Error for SignalError {}

/// Resolve what action to take for a signal
pub fn resolve_action(signal: Signal, disposition: &SignalDisposition) -> SignalAction {
    let action = disposition.get_action(signal);
    match action {
        SignalAction::Default => signal.default_action(),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_numbers() {
        assert_eq!(Signal::SIGTERM.num(), 1);
        assert_eq!(Signal::SIGKILL.num(), 2);
        assert_eq!(Signal::from_num(1), Some(Signal::SIGTERM));
        assert_eq!(Signal::from_num(99), None);
    }

    #[test]
    fn test_signal_can_catch() {
        assert!(Signal::SIGTERM.can_catch());
        assert!(Signal::SIGINT.can_catch());
        assert!(!Signal::SIGKILL.can_catch());
        assert!(!Signal::SIGSTOP.can_catch());
    }

    #[test]
    fn test_default_actions() {
        assert_eq!(Signal::SIGTERM.default_action(), SignalAction::Terminate);
        assert_eq!(Signal::SIGKILL.default_action(), SignalAction::Kill);
        assert_eq!(Signal::SIGSTOP.default_action(), SignalAction::Stop);
        assert_eq!(Signal::SIGCONT.default_action(), SignalAction::Continue);
        assert_eq!(Signal::SIGUSR1.default_action(), SignalAction::Ignore);
    }

    #[test]
    fn test_disposition() {
        let mut disp = SignalDisposition::new();

        // Default is Default action
        assert_eq!(disp.get_action(Signal::SIGTERM), SignalAction::Default);

        // Set custom action
        disp.set_action(Signal::SIGTERM, SignalAction::Ignore)
            .unwrap();
        assert_eq!(disp.get_action(Signal::SIGTERM), SignalAction::Ignore);

        // Can't change SIGKILL
        assert!(
            disp.set_action(Signal::SIGKILL, SignalAction::Ignore)
                .is_err()
        );
    }

    #[test]
    fn test_process_signals_basic() {
        let mut ps = ProcessSignals::new();

        // Send a signal
        ps.send(Signal::SIGTERM);
        assert!(ps.has_pending());
        assert_eq!(ps.pending_count(), 1);

        // Receive it
        let sig = ps.next_pending();
        assert_eq!(sig, Some(Signal::SIGTERM));
        assert!(!ps.has_pending());
    }

    #[test]
    fn test_signal_coalescing() {
        let mut ps = ProcessSignals::new();

        // Same signal sent multiple times
        ps.send(Signal::SIGUSR1);
        ps.send(Signal::SIGUSR1);
        ps.send(Signal::SIGUSR1);

        // Only one delivered
        assert_eq!(ps.pending_count(), 1);
    }

    #[test]
    fn test_sigkill_priority() {
        let mut ps = ProcessSignals::new();

        ps.send(Signal::SIGUSR1);
        ps.send(Signal::SIGTERM);
        ps.send(Signal::SIGKILL);

        // SIGKILL comes first
        assert_eq!(ps.next_pending(), Some(Signal::SIGKILL));
    }

    #[test]
    fn test_signal_blocking() {
        let mut ps = ProcessSignals::new();

        // Block SIGUSR1
        ps.block(Signal::SIGUSR1).unwrap();

        // Send blocked signal
        ps.send(Signal::SIGUSR1);

        // Not delivered while blocked
        assert!(!ps.has_pending());

        // Unblock
        ps.unblock(Signal::SIGUSR1);

        // Now delivered
        assert!(ps.has_pending());
        assert_eq!(ps.next_pending(), Some(Signal::SIGUSR1));
    }

    #[test]
    fn test_cannot_block_sigkill() {
        let mut ps = ProcessSignals::new();
        assert!(ps.block(Signal::SIGKILL).is_err());
        assert!(ps.block(Signal::SIGSTOP).is_err());
    }

    #[test]
    fn test_sigcont_clears_stop() {
        let mut ps = ProcessSignals::new();

        ps.stop();
        assert!(ps.is_stopped());

        ps.send(Signal::SIGCONT);
        assert!(!ps.is_stopped());
    }

    #[test]
    fn test_resolve_action() {
        let mut disp = SignalDisposition::new();

        // Default -> use signal's default
        assert_eq!(
            resolve_action(Signal::SIGTERM, &disp),
            SignalAction::Terminate
        );

        // Custom action
        disp.set_action(Signal::SIGTERM, SignalAction::Ignore)
            .unwrap();
        assert_eq!(resolve_action(Signal::SIGTERM, &disp), SignalAction::Ignore);
    }
}
