//! Built-in WASM Debugger
//!
//! Provides debugging capabilities for WASM modules:
//! - Syscall-level breakpoints
//! - Memory inspection
//! - Register/argument viewing
//! - Execution history
//! - Step-through debugging (at syscall boundaries)
//!
//! This debugger operates at the syscall level rather than instruction level,
//! providing practical debugging without requiring WASM bytecode manipulation.

use super::process::{Fd, Pid};
use super::task::TaskId;
use std::collections::{HashMap, HashSet, VecDeque};

/// Maximum execution history entries
const MAX_HISTORY: usize = 1000;

/// Maximum memory watch entries
const MAX_WATCHES: usize = 64;

/// Maximum breakpoints
const MAX_BREAKPOINTS: usize = 256;

// ============================================================================
// Breakpoints
// ============================================================================

/// Breakpoint identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BreakpointId(pub u64);

/// Breakpoint condition
#[derive(Debug, Clone)]
pub enum BreakpointCondition {
    /// Always break
    Always,
    /// Break when PID matches
    Pid(Pid),
    /// Break when argument equals value
    ArgEquals { index: usize, value: i32 },
    /// Break when argument is in range
    ArgInRange { index: usize, min: i32, max: i32 },
    /// Break on Nth hit
    HitCount(u32),
    /// Break when expression is true (simple comparisons)
    Expression(String),
}

impl BreakpointCondition {
    /// Check if condition is met
    pub fn check(&self, ctx: &BreakpointContext) -> bool {
        match self {
            BreakpointCondition::Always => true,
            BreakpointCondition::Pid(pid) => ctx.pid == Some(*pid),
            BreakpointCondition::ArgEquals { index, value } => {
                ctx.args.get(*index).copied() == Some(*value)
            }
            BreakpointCondition::ArgInRange { index, min, max } => ctx
                .args
                .get(*index)
                .map(|v| *v >= *min && *v <= *max)
                .unwrap_or(false),
            BreakpointCondition::HitCount(n) => ctx.hit_count == *n,
            BreakpointCondition::Expression(_expr) => {
                // Simple expression evaluation could be added here
                true
            }
        }
    }
}

/// Context for breakpoint evaluation
#[derive(Debug, Clone)]
pub struct BreakpointContext {
    /// Current process
    pub pid: Option<Pid>,
    /// Syscall arguments
    pub args: Vec<i32>,
    /// Hit count for this breakpoint
    pub hit_count: u32,
    /// Current timestamp
    pub timestamp: f64,
}

/// Action to take when breakpoint is hit
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakpointAction {
    /// Stop execution
    Break,
    /// Log and continue
    Log,
    /// Trace arguments and continue
    Trace,
    /// Ignore (disabled breakpoint)
    Ignore,
}

/// A breakpoint definition
#[derive(Debug, Clone)]
pub struct Breakpoint {
    /// Unique identifier
    pub id: BreakpointId,
    /// Syscall name to break on
    pub syscall: String,
    /// Condition for breaking
    pub condition: BreakpointCondition,
    /// Action when hit
    pub action: BreakpointAction,
    /// Is this breakpoint enabled?
    pub enabled: bool,
    /// Hit count
    pub hit_count: u32,
    /// Description/comment
    pub description: Option<String>,
}

impl Breakpoint {
    /// Create a new breakpoint
    pub fn new(id: BreakpointId, syscall: impl Into<String>) -> Self {
        Self {
            id,
            syscall: syscall.into(),
            condition: BreakpointCondition::Always,
            action: BreakpointAction::Break,
            enabled: true,
            hit_count: 0,
            description: None,
        }
    }

    /// Set condition
    pub fn with_condition(mut self, condition: BreakpointCondition) -> Self {
        self.condition = condition;
        self
    }

    /// Set action
    pub fn with_action(mut self, action: BreakpointAction) -> Self {
        self.action = action;
        self
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Check if this breakpoint should trigger
    pub fn should_trigger(&mut self, syscall: &str, ctx: &BreakpointContext) -> bool {
        if !self.enabled || self.syscall != syscall {
            return false;
        }

        self.hit_count += 1;
        let ctx_with_count = BreakpointContext {
            hit_count: self.hit_count,
            ..ctx.clone()
        };

        self.condition.check(&ctx_with_count)
    }
}

// ============================================================================
// Memory Inspection
// ============================================================================

/// A memory watch point
#[derive(Debug, Clone)]
pub struct MemoryWatch {
    /// Watch identifier
    pub id: u32,
    /// Start address in WASM linear memory
    pub address: u32,
    /// Number of bytes to watch
    pub size: usize,
    /// Watch type
    pub watch_type: WatchType,
    /// Is this watch enabled?
    pub enabled: bool,
    /// Last known value (for change detection)
    pub last_value: Option<Vec<u8>>,
    /// Description
    pub description: Option<String>,
}

/// Type of memory watch
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchType {
    /// Break on any access
    Access,
    /// Break on write
    Write,
    /// Break on read
    Read,
    /// Break on value change
    Change,
}

/// Memory view/dump
#[derive(Debug, Clone)]
pub struct MemoryView {
    /// Starting address
    pub address: u32,
    /// Memory contents
    pub data: Vec<u8>,
    /// Associated ASCII representation
    pub ascii: String,
}

impl MemoryView {
    /// Create a memory view from raw data
    pub fn new(address: u32, data: Vec<u8>) -> Self {
        let ascii = data
            .iter()
            .map(|&b| {
                if b.is_ascii_graphic() || b == b' ' {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();

        Self {
            address,
            data,
            ascii,
        }
    }

    /// Render as hex dump
    pub fn render_hexdump(&self, bytes_per_line: usize) -> String {
        let mut output = String::new();

        for (i, chunk) in self.data.chunks(bytes_per_line).enumerate() {
            let addr = self.address + (i * bytes_per_line) as u32;

            // Address
            output.push_str(&format!("{:08x}  ", addr));

            // Hex bytes
            for (j, byte) in chunk.iter().enumerate() {
                output.push_str(&format!("{:02x} ", byte));
                if j == 7 {
                    output.push(' ');
                }
            }

            // Padding if incomplete line
            let padding = bytes_per_line - chunk.len();
            for _ in 0..padding {
                output.push_str("   ");
            }
            if chunk.len() <= 8 {
                output.push(' ');
            }

            output.push_str(" |");

            // ASCII
            for byte in chunk {
                if byte.is_ascii_graphic() || *byte == b' ' {
                    output.push(*byte as char);
                } else {
                    output.push('.');
                }
            }

            output.push_str("|\n");
        }

        output
    }

    /// Read a value at offset
    pub fn read_u8(&self, offset: usize) -> Option<u8> {
        self.data.get(offset).copied()
    }

    pub fn read_u16_le(&self, offset: usize) -> Option<u16> {
        if offset + 2 <= self.data.len() {
            Some(u16::from_le_bytes([
                self.data[offset],
                self.data[offset + 1],
            ]))
        } else {
            None
        }
    }

    pub fn read_u32_le(&self, offset: usize) -> Option<u32> {
        if offset + 4 <= self.data.len() {
            Some(u32::from_le_bytes([
                self.data[offset],
                self.data[offset + 1],
                self.data[offset + 2],
                self.data[offset + 3],
            ]))
        } else {
            None
        }
    }

    pub fn read_i32_le(&self, offset: usize) -> Option<i32> {
        self.read_u32_le(offset).map(|v| v as i32)
    }

    /// Read a null-terminated string
    pub fn read_cstring(&self, offset: usize) -> Option<String> {
        let start = offset;
        let mut end = start;
        while end < self.data.len() && self.data[end] != 0 {
            end += 1;
        }
        if end > start {
            String::from_utf8(self.data[start..end].to_vec()).ok()
        } else {
            None
        }
    }
}

// ============================================================================
// Execution History
// ============================================================================

/// A syscall execution record
#[derive(Debug, Clone)]
pub struct SyscallRecord {
    /// Sequence number
    pub seq: u64,
    /// Timestamp (ms)
    pub timestamp: f64,
    /// Process ID
    pub pid: Pid,
    /// Task ID
    pub task_id: TaskId,
    /// Syscall name
    pub syscall: String,
    /// Arguments
    pub args: Vec<SyscallArg>,
    /// Return value
    pub result: Option<SyscallResult>,
    /// Duration (ms)
    pub duration: f64,
    /// Memory snapshot at call time (optional)
    pub memory_before: Option<Vec<(u32, Vec<u8>)>>,
    /// Memory changes after call (optional)
    pub memory_after: Option<Vec<(u32, Vec<u8>)>>,
}

/// A syscall argument with type information
#[derive(Debug, Clone)]
pub struct SyscallArg {
    /// Argument name
    pub name: String,
    /// Raw value
    pub value: i32,
    /// Interpreted value
    pub interpreted: ArgValue,
}

/// Interpreted argument value
#[derive(Debug, Clone)]
pub enum ArgValue {
    /// Integer value
    Int(i32),
    /// Unsigned integer
    Uint(u32),
    /// File descriptor
    Fd(Fd),
    /// Pointer (address in WASM memory)
    Pointer(u32),
    /// Size/length
    Size(usize),
    /// Flags (with bit meanings)
    Flags(u32, Vec<String>),
    /// String value (read from memory)
    String(String),
    /// Path
    Path(String),
    /// Process ID
    Pid(Pid),
    /// Unknown
    Unknown,
}

impl std::fmt::Display for ArgValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArgValue::Int(v) => write!(f, "{}", v),
            ArgValue::Uint(v) => write!(f, "{}", v),
            ArgValue::Fd(fd) => write!(f, "fd:{}", fd.0),
            ArgValue::Pointer(addr) => write!(f, "0x{:08x}", addr),
            ArgValue::Size(sz) => write!(f, "{} bytes", sz),
            ArgValue::Flags(v, names) => {
                if names.is_empty() {
                    write!(f, "0x{:x}", v)
                } else {
                    write!(f, "{}", names.join("|"))
                }
            }
            ArgValue::String(s) => write!(f, "\"{}\"", s),
            ArgValue::Path(p) => write!(f, "\"{}\"", p),
            ArgValue::Pid(pid) => write!(f, "pid:{}", pid.0),
            ArgValue::Unknown => write!(f, "?"),
        }
    }
}

/// Syscall result
#[derive(Debug, Clone)]
pub enum SyscallResult {
    /// Success with value
    Success(i32),
    /// Error with code and message
    Error(i32, String),
}

impl std::fmt::Display for SyscallResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyscallResult::Success(v) => write!(f, "= {}", v),
            SyscallResult::Error(code, msg) => write!(f, "= {} ({})", code, msg),
        }
    }
}

// ============================================================================
// Debugger State
// ============================================================================

/// Debugger execution mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugMode {
    /// Normal execution (no debugging)
    Run,
    /// Stopped at breakpoint
    Stopped,
    /// Single-stepping (stop at next syscall)
    Step,
    /// Step over (continue until current call returns)
    StepOver,
    /// Step out (continue until parent call returns)
    StepOut,
    /// Running until breakpoint
    Continue,
}

/// Debug target selection
#[derive(Debug, Clone)]
pub enum DebugTarget {
    /// Debug all processes
    All,
    /// Debug specific process
    Process(Pid),
    /// Debug specific task
    Task(TaskId),
}

/// The main debugger
#[derive(Debug)]
pub struct WasmDebugger {
    /// Current mode
    mode: DebugMode,
    /// Debug target
    target: DebugTarget,
    /// Breakpoints
    breakpoints: HashMap<BreakpointId, Breakpoint>,
    /// Next breakpoint ID
    next_breakpoint_id: u64,
    /// Memory watches
    watches: HashMap<u32, MemoryWatch>,
    /// Next watch ID
    next_watch_id: u32,
    /// Execution history
    history: VecDeque<SyscallRecord>,
    /// Next sequence number
    next_seq: u64,
    /// Syscalls to ignore
    ignore_syscalls: HashSet<String>,
    /// Saved memory regions for diff
    #[allow(dead_code)]
    saved_memory: HashMap<u32, Vec<u8>>,
    /// Call stack depth (for step over/out)
    call_depth: usize,
    /// Step target depth
    step_target_depth: Option<usize>,
    /// Is debugger enabled?
    enabled: bool,
    /// Verbose output
    verbose: bool,
}

impl WasmDebugger {
    /// Create a new debugger
    pub fn new() -> Self {
        Self {
            mode: DebugMode::Run,
            target: DebugTarget::All,
            breakpoints: HashMap::new(),
            next_breakpoint_id: 1,
            watches: HashMap::new(),
            next_watch_id: 1,
            history: VecDeque::with_capacity(MAX_HISTORY),
            next_seq: 1,
            ignore_syscalls: HashSet::new(),
            saved_memory: HashMap::new(),
            call_depth: 0,
            step_target_depth: None,
            enabled: false,
            verbose: false,
        }
    }

    /// Enable the debugger
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable the debugger
    pub fn disable(&mut self) {
        self.enabled = false;
        self.mode = DebugMode::Run;
    }

    /// Check if debugger is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get current mode
    pub fn mode(&self) -> DebugMode {
        self.mode
    }

    /// Set debug target
    pub fn set_target(&mut self, target: DebugTarget) {
        self.target = target;
    }

    /// Check if we should debug this process
    pub fn should_debug(&self, pid: Pid, task_id: TaskId) -> bool {
        if !self.enabled {
            return false;
        }
        match &self.target {
            DebugTarget::All => true,
            DebugTarget::Process(p) => *p == pid,
            DebugTarget::Task(t) => *t == task_id,
        }
    }

    // ========================================================================
    // Breakpoint Management
    // ========================================================================

    /// Add a breakpoint
    pub fn add_breakpoint(&mut self, syscall: impl Into<String>) -> BreakpointId {
        if self.breakpoints.len() >= MAX_BREAKPOINTS {
            // Remove oldest breakpoint
            if let Some(oldest) = self.breakpoints.keys().min().copied() {
                self.breakpoints.remove(&oldest);
            }
        }

        let id = BreakpointId(self.next_breakpoint_id);
        self.next_breakpoint_id += 1;

        let bp = Breakpoint::new(id, syscall);
        self.breakpoints.insert(id, bp);
        id
    }

    /// Add a breakpoint with condition
    pub fn add_conditional_breakpoint(
        &mut self,
        syscall: impl Into<String>,
        condition: BreakpointCondition,
    ) -> BreakpointId {
        let id = self.add_breakpoint(syscall);
        if let Some(bp) = self.breakpoints.get_mut(&id) {
            bp.condition = condition;
        }
        id
    }

    /// Remove a breakpoint
    pub fn remove_breakpoint(&mut self, id: BreakpointId) -> bool {
        self.breakpoints.remove(&id).is_some()
    }

    /// Enable a breakpoint
    pub fn enable_breakpoint(&mut self, id: BreakpointId) -> bool {
        if let Some(bp) = self.breakpoints.get_mut(&id) {
            bp.enabled = true;
            true
        } else {
            false
        }
    }

    /// Disable a breakpoint
    pub fn disable_breakpoint(&mut self, id: BreakpointId) -> bool {
        if let Some(bp) = self.breakpoints.get_mut(&id) {
            bp.enabled = false;
            true
        } else {
            false
        }
    }

    /// Get all breakpoints
    pub fn breakpoints(&self) -> impl Iterator<Item = &Breakpoint> {
        self.breakpoints.values()
    }

    /// Clear all breakpoints
    pub fn clear_breakpoints(&mut self) {
        self.breakpoints.clear();
    }

    // ========================================================================
    // Memory Watches
    // ========================================================================

    /// Add a memory watch
    pub fn add_watch(&mut self, address: u32, size: usize, watch_type: WatchType) -> u32 {
        if self.watches.len() >= MAX_WATCHES {
            // Remove oldest watch
            if let Some(oldest) = self.watches.keys().min().copied() {
                self.watches.remove(&oldest);
            }
        }

        let id = self.next_watch_id;
        self.next_watch_id += 1;

        let watch = MemoryWatch {
            id,
            address,
            size,
            watch_type,
            enabled: true,
            last_value: None,
            description: None,
        };

        self.watches.insert(id, watch);
        id
    }

    /// Remove a memory watch
    pub fn remove_watch(&mut self, id: u32) -> bool {
        self.watches.remove(&id).is_some()
    }

    /// Get all watches
    pub fn watches(&self) -> impl Iterator<Item = &MemoryWatch> {
        self.watches.values()
    }

    /// Clear all watches
    pub fn clear_watches(&mut self) {
        self.watches.clear();
    }

    // ========================================================================
    // Execution Control
    // ========================================================================

    /// Continue execution
    pub fn continue_execution(&mut self) {
        self.mode = DebugMode::Continue;
        self.step_target_depth = None;
    }

    /// Single step (stop at next syscall)
    pub fn step(&mut self) {
        self.mode = DebugMode::Step;
    }

    /// Step over (continue until current level returns)
    pub fn step_over(&mut self) {
        self.mode = DebugMode::StepOver;
        self.step_target_depth = Some(self.call_depth);
    }

    /// Step out (continue until parent level)
    pub fn step_out(&mut self) {
        self.mode = DebugMode::StepOut;
        self.step_target_depth = Some(self.call_depth.saturating_sub(1));
    }

    /// Stop execution
    pub fn stop(&mut self) {
        self.mode = DebugMode::Stopped;
    }

    /// Run without debugging
    pub fn run(&mut self) {
        self.mode = DebugMode::Run;
    }

    // ========================================================================
    // Syscall Interception
    // ========================================================================

    /// Called before a syscall executes
    /// Returns true if execution should stop
    pub fn on_syscall_enter(
        &mut self,
        syscall: &str,
        pid: Pid,
        task_id: TaskId,
        args: &[i32],
        timestamp: f64,
    ) -> bool {
        if !self.should_debug(pid, task_id) {
            return false;
        }

        if self.ignore_syscalls.contains(syscall) {
            return false;
        }

        self.call_depth += 1;

        // Check step modes
        let should_stop = match self.mode {
            DebugMode::Run => false,
            DebugMode::Stopped => true,
            DebugMode::Step => true,
            DebugMode::StepOver => self
                .step_target_depth
                .map(|d| self.call_depth <= d)
                .unwrap_or(false),
            DebugMode::StepOut => self
                .step_target_depth
                .map(|d| self.call_depth <= d)
                .unwrap_or(false),
            DebugMode::Continue => false,
        };

        if should_stop {
            self.mode = DebugMode::Stopped;
            return true;
        }

        // Check breakpoints
        let ctx = BreakpointContext {
            pid: Some(pid),
            args: args.to_vec(),
            hit_count: 0,
            timestamp,
        };

        for bp in self.breakpoints.values_mut() {
            if bp.should_trigger(syscall, &ctx) {
                match bp.action {
                    BreakpointAction::Break => {
                        self.mode = DebugMode::Stopped;
                        return true;
                    }
                    BreakpointAction::Log | BreakpointAction::Trace => {
                        // Continue execution
                    }
                    BreakpointAction::Ignore => {}
                }
            }
        }

        false
    }

    /// Called after a syscall completes
    #[allow(clippy::too_many_arguments)]
    pub fn on_syscall_exit(
        &mut self,
        syscall: &str,
        pid: Pid,
        task_id: TaskId,
        args: Vec<SyscallArg>,
        result: SyscallResult,
        duration: f64,
        timestamp: f64,
    ) {
        if !self.should_debug(pid, task_id) {
            return;
        }

        self.call_depth = self.call_depth.saturating_sub(1);

        // Record in history
        let record = SyscallRecord {
            seq: self.next_seq,
            timestamp,
            pid,
            task_id,
            syscall: syscall.to_string(),
            args,
            result: Some(result),
            duration,
            memory_before: None,
            memory_after: None,
        };

        self.next_seq += 1;

        if self.history.len() >= MAX_HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(record);
    }

    // ========================================================================
    // History
    // ========================================================================

    /// Get execution history
    pub fn history(&self) -> impl Iterator<Item = &SyscallRecord> {
        self.history.iter()
    }

    /// Get recent history
    pub fn recent_history(&self, count: usize) -> impl Iterator<Item = &SyscallRecord> {
        self.history.iter().rev().take(count)
    }

    /// Get history for a process
    pub fn history_for_pid(&self, pid: Pid) -> impl Iterator<Item = &SyscallRecord> {
        self.history.iter().filter(move |r| r.pid == pid)
    }

    /// Get history for a syscall
    pub fn history_for_syscall<'a>(
        &'a self,
        syscall: &'a str,
    ) -> impl Iterator<Item = &'a SyscallRecord> + 'a {
        self.history.iter().filter(move |r| r.syscall == syscall)
    }

    /// Clear history
    pub fn clear_history(&mut self) {
        self.history.clear();
        self.next_seq = 1;
    }

    // ========================================================================
    // Configuration
    // ========================================================================

    /// Ignore a syscall (don't record or break)
    pub fn ignore_syscall(&mut self, syscall: impl Into<String>) {
        self.ignore_syscalls.insert(syscall.into());
    }

    /// Stop ignoring a syscall
    pub fn unignore_syscall(&mut self, syscall: &str) {
        self.ignore_syscalls.remove(syscall);
    }

    /// Set verbose mode
    pub fn set_verbose(&mut self, verbose: bool) {
        self.verbose = verbose;
    }

    // ========================================================================
    // Status/Rendering
    // ========================================================================

    /// Get debugger status
    pub fn status(&self) -> DebuggerStatus {
        DebuggerStatus {
            enabled: self.enabled,
            mode: self.mode,
            breakpoint_count: self.breakpoints.len(),
            active_breakpoints: self.breakpoints.values().filter(|b| b.enabled).count(),
            watch_count: self.watches.len(),
            history_count: self.history.len(),
            call_depth: self.call_depth,
        }
    }

    /// Render status as string
    pub fn render_status(&self) -> String {
        let status = self.status();
        let mut output = String::new();

        output.push_str("╔══════════════════════════════════════════╗\n");
        output.push_str("║           WASM DEBUGGER STATUS           ║\n");
        output.push_str("╠══════════════════════════════════════════╣\n");

        output.push_str(&format!(
            "║ Enabled: {:>32} ║\n",
            if status.enabled { "YES" } else { "NO" }
        ));

        let mode_str = match status.mode {
            DebugMode::Run => "Run",
            DebugMode::Stopped => "STOPPED",
            DebugMode::Step => "Step",
            DebugMode::StepOver => "Step Over",
            DebugMode::StepOut => "Step Out",
            DebugMode::Continue => "Continue",
        };
        output.push_str(&format!("║ Mode: {:>35} ║\n", mode_str));

        output.push_str(&format!(
            "║ Breakpoints: {:>6} ({} active)            ║\n",
            status.breakpoint_count, status.active_breakpoints
        ));
        output.push_str(&format!("║ Watches: {:>32} ║\n", status.watch_count));
        output.push_str(&format!("║ History: {:>32} ║\n", status.history_count));
        output.push_str(&format!("║ Call depth: {:>29} ║\n", status.call_depth));

        output.push_str("╚══════════════════════════════════════════╝\n");

        output
    }

    /// Render breakpoints list
    pub fn render_breakpoints(&self) -> String {
        let mut output = String::new();
        output.push_str("=== Breakpoints ===\n");
        output.push_str("ID    Enabled  Syscall          Hits    Condition\n");
        output.push_str("────────────────────────────────────────────────────\n");

        if self.breakpoints.is_empty() {
            output.push_str("  (no breakpoints set)\n");
        } else {
            let mut bps: Vec<_> = self.breakpoints.values().collect();
            bps.sort_by_key(|b| b.id.0);

            for bp in bps {
                let enabled = if bp.enabled { "✓" } else { " " };
                let condition = match &bp.condition {
                    BreakpointCondition::Always => "always".to_string(),
                    BreakpointCondition::Pid(pid) => format!("pid={}", pid.0),
                    BreakpointCondition::HitCount(n) => format!("hit={}", n),
                    BreakpointCondition::ArgEquals { index, value } => {
                        format!("arg[{}]=={}", index, value)
                    }
                    BreakpointCondition::ArgInRange { index, min, max } => {
                        format!("arg[{}] in {}..{}", index, min, max)
                    }
                    BreakpointCondition::Expression(expr) => expr.clone(),
                };

                output.push_str(&format!(
                    "{:<5} {:^8} {:16} {:>5}   {}\n",
                    bp.id.0, enabled, bp.syscall, bp.hit_count, condition
                ));
            }
        }

        output
    }

    /// Render execution history
    pub fn render_history(&self, count: usize) -> String {
        let mut output = String::new();
        output.push_str("=== Execution History ===\n");
        output.push_str("Seq    Time      PID   Syscall          Duration  Result\n");
        output.push_str("─────────────────────────────────────────────────────────\n");

        for record in self.history.iter().rev().take(count) {
            let result = record
                .result
                .as_ref()
                .map(|r| format!("{}", r))
                .unwrap_or_else(|| "?".to_string());

            output.push_str(&format!(
                "{:<6} {:>8.1}  {:>4}  {:16} {:>7.2}ms  {}\n",
                record.seq, record.timestamp, record.pid.0, record.syscall, record.duration, result
            ));
        }

        output
    }
}

impl Default for WasmDebugger {
    fn default() -> Self {
        Self::new()
    }
}

/// Debugger status summary
#[derive(Debug, Clone)]
pub struct DebuggerStatus {
    pub enabled: bool,
    pub mode: DebugMode,
    pub breakpoint_count: usize,
    pub active_breakpoints: usize,
    pub watch_count: usize,
    pub history_count: usize,
    pub call_depth: usize,
}

// ============================================================================
// Helper for interpreting syscall arguments
// ============================================================================

/// Interpret raw syscall arguments based on syscall name
pub fn interpret_syscall_args(syscall: &str, raw_args: &[i32]) -> Vec<SyscallArg> {
    match syscall {
        "open" => {
            let mut args = Vec::new();
            if let Some(&path_ptr) = raw_args.first() {
                args.push(SyscallArg {
                    name: "path".to_string(),
                    value: path_ptr,
                    interpreted: ArgValue::Pointer(path_ptr as u32),
                });
            }
            if let Some(&flags) = raw_args.get(1) {
                args.push(SyscallArg {
                    name: "flags".to_string(),
                    value: flags,
                    interpreted: interpret_open_flags(flags as u32),
                });
            }
            if let Some(&mode) = raw_args.get(2) {
                args.push(SyscallArg {
                    name: "mode".to_string(),
                    value: mode,
                    interpreted: ArgValue::Uint(mode as u32),
                });
            }
            args
        }
        "read" | "write" => {
            let mut args = Vec::new();
            if let Some(&fd) = raw_args.first() {
                args.push(SyscallArg {
                    name: "fd".to_string(),
                    value: fd,
                    interpreted: ArgValue::Fd(Fd(fd as u32)),
                });
            }
            if let Some(&buf) = raw_args.get(1) {
                args.push(SyscallArg {
                    name: "buf".to_string(),
                    value: buf,
                    interpreted: ArgValue::Pointer(buf as u32),
                });
            }
            if let Some(&count) = raw_args.get(2) {
                args.push(SyscallArg {
                    name: "count".to_string(),
                    value: count,
                    interpreted: ArgValue::Size(count as usize),
                });
            }
            args
        }
        "close" => {
            let mut args = Vec::new();
            if let Some(&fd) = raw_args.first() {
                args.push(SyscallArg {
                    name: "fd".to_string(),
                    value: fd,
                    interpreted: ArgValue::Fd(Fd(fd as u32)),
                });
            }
            args
        }
        "fork" | "getpid" | "getppid" => Vec::new(),
        "kill" => {
            let mut args = Vec::new();
            if let Some(&pid) = raw_args.first() {
                args.push(SyscallArg {
                    name: "pid".to_string(),
                    value: pid,
                    interpreted: ArgValue::Pid(Pid(pid as u32)),
                });
            }
            if let Some(&sig) = raw_args.get(1) {
                args.push(SyscallArg {
                    name: "signal".to_string(),
                    value: sig,
                    interpreted: ArgValue::Int(sig),
                });
            }
            args
        }
        _ => {
            // Generic: treat all args as integers
            raw_args
                .iter()
                .enumerate()
                .map(|(i, &v)| SyscallArg {
                    name: format!("arg{}", i),
                    value: v,
                    interpreted: ArgValue::Int(v),
                })
                .collect()
        }
    }
}

/// Interpret open flags
fn interpret_open_flags(flags: u32) -> ArgValue {
    let mut names = Vec::new();

    if flags & 0x01 != 0 {
        names.push("O_WRONLY".to_string());
    }
    if flags & 0x02 != 0 {
        names.push("O_RDWR".to_string());
    }
    if names.is_empty() {
        names.push("O_RDONLY".to_string());
    }
    if flags & 0x40 != 0 {
        names.push("O_CREAT".to_string());
    }
    if flags & 0x200 != 0 {
        names.push("O_TRUNC".to_string());
    }
    if flags & 0x400 != 0 {
        names.push("O_APPEND".to_string());
    }

    ArgValue::Flags(flags, names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debugger_lifecycle() {
        let mut dbg = WasmDebugger::new();
        assert!(!dbg.is_enabled());
        assert_eq!(dbg.mode(), DebugMode::Run);

        dbg.enable();
        assert!(dbg.is_enabled());

        dbg.stop();
        assert_eq!(dbg.mode(), DebugMode::Stopped);

        dbg.continue_execution();
        assert_eq!(dbg.mode(), DebugMode::Continue);

        dbg.disable();
        assert!(!dbg.is_enabled());
        assert_eq!(dbg.mode(), DebugMode::Run);
    }

    #[test]
    fn test_breakpoints() {
        let mut dbg = WasmDebugger::new();
        dbg.enable();

        let bp1 = dbg.add_breakpoint("open");
        let bp2 = dbg.add_breakpoint("read");

        assert_eq!(dbg.breakpoints().count(), 2);

        dbg.disable_breakpoint(bp1);
        let bp = dbg.breakpoints.get(&bp1).unwrap();
        assert!(!bp.enabled);

        dbg.remove_breakpoint(bp2);
        assert_eq!(dbg.breakpoints().count(), 1);

        dbg.clear_breakpoints();
        assert_eq!(dbg.breakpoints().count(), 0);
    }

    #[test]
    fn test_conditional_breakpoint() {
        let mut dbg = WasmDebugger::new();
        dbg.enable();

        let bp_id = dbg.add_conditional_breakpoint(
            "read",
            BreakpointCondition::ArgEquals { index: 0, value: 3 },
        );

        let bp = dbg.breakpoints.get_mut(&bp_id).unwrap();

        // Should not trigger for fd 5
        let ctx = BreakpointContext {
            pid: Some(Pid(1)),
            args: vec![5, 0, 100],
            hit_count: 0,
            timestamp: 0.0,
        };
        assert!(!bp.should_trigger("read", &ctx));

        // Should trigger for fd 3
        let ctx = BreakpointContext {
            pid: Some(Pid(1)),
            args: vec![3, 0, 100],
            hit_count: 0,
            timestamp: 0.0,
        };
        assert!(bp.should_trigger("read", &ctx));
    }

    #[test]
    fn test_memory_watches() {
        let mut dbg = WasmDebugger::new();

        let w1 = dbg.add_watch(0x1000, 16, WatchType::Write);
        let _w2 = dbg.add_watch(0x2000, 8, WatchType::Change);

        assert_eq!(dbg.watches().count(), 2);

        dbg.remove_watch(w1);
        assert_eq!(dbg.watches().count(), 1);

        dbg.clear_watches();
        assert_eq!(dbg.watches().count(), 0);
    }

    #[test]
    fn test_memory_view() {
        let data = vec![
            0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
            0x09, 0x0a,
        ];
        let view = MemoryView::new(0x1000, data);

        assert_eq!(view.read_u8(0), Some(0x48));
        assert_eq!(view.read_cstring(0), Some("Hello".to_string()));
        assert_eq!(view.read_u32_le(6), Some(0x04030201));

        let hexdump = view.render_hexdump(16);
        assert!(hexdump.contains("00001000"));
        assert!(hexdump.contains("48 65 6c 6c"));
        assert!(hexdump.contains("|Hello"));
    }

    #[test]
    fn test_syscall_interception() {
        let mut dbg = WasmDebugger::new();
        dbg.enable();
        dbg.add_breakpoint("open");

        let should_stop =
            dbg.on_syscall_enter("open", Pid(1), TaskId(1), &[0x1000, 0, 0644], 100.0);

        assert!(should_stop);
        assert_eq!(dbg.mode(), DebugMode::Stopped);
    }

    #[test]
    fn test_execution_history() {
        let mut dbg = WasmDebugger::new();
        dbg.enable();

        dbg.on_syscall_exit(
            "open",
            Pid(1),
            TaskId(1),
            vec![SyscallArg {
                name: "path".to_string(),
                value: 0x1000,
                interpreted: ArgValue::Path("/etc/passwd".to_string()),
            }],
            SyscallResult::Success(3),
            0.5,
            100.0,
        );

        assert_eq!(dbg.history().count(), 1);

        let record = dbg.history().next().unwrap();
        assert_eq!(record.syscall, "open");
        assert_eq!(record.pid, Pid(1));
    }

    #[test]
    fn test_step_modes() {
        let mut dbg = WasmDebugger::new();
        dbg.enable();
        dbg.continue_execution();

        dbg.step();
        assert_eq!(dbg.mode(), DebugMode::Step);

        // Step mode should stop on next syscall
        let should_stop = dbg.on_syscall_enter("read", Pid(1), TaskId(1), &[], 100.0);
        assert!(should_stop);
    }

    #[test]
    fn test_interpret_syscall_args() {
        let args = interpret_syscall_args("read", &[3, 0x1000, 100]);

        assert_eq!(args.len(), 3);
        assert_eq!(args[0].name, "fd");
        assert!(matches!(args[0].interpreted, ArgValue::Fd(Fd(3))));
        assert_eq!(args[2].name, "count");
        assert!(matches!(args[2].interpreted, ArgValue::Size(100)));
    }

    #[test]
    fn test_open_flags_interpretation() {
        let flags = interpret_open_flags(0x42); // O_RDWR | O_CREAT
        if let ArgValue::Flags(_, names) = flags {
            assert!(names.contains(&"O_RDWR".to_string()));
            assert!(names.contains(&"O_CREAT".to_string()));
        } else {
            panic!("Expected Flags variant");
        }
    }

    #[test]
    fn test_debugger_status() {
        let mut dbg = WasmDebugger::new();
        dbg.enable();
        dbg.add_breakpoint("open");
        dbg.add_watch(0x1000, 16, WatchType::Write);

        let status = dbg.status();
        assert!(status.enabled);
        assert_eq!(status.breakpoint_count, 1);
        assert_eq!(status.active_breakpoints, 1);
        assert_eq!(status.watch_count, 1);
    }

    #[test]
    fn test_render_status() {
        let mut dbg = WasmDebugger::new();
        dbg.enable();

        let output = dbg.render_status();
        assert!(output.contains("WASM DEBUGGER STATUS"));
        assert!(output.contains("YES"));
    }

    #[test]
    fn test_ignore_syscalls() {
        let mut dbg = WasmDebugger::new();
        dbg.enable();
        dbg.add_breakpoint("getpid");
        dbg.ignore_syscall("getpid");

        // Should not stop because getpid is ignored
        let should_stop = dbg.on_syscall_enter("getpid", Pid(1), TaskId(1), &[], 100.0);
        assert!(!should_stop);
    }
}
