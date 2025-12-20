//! Instrumentation and Tracing System
//!
//! Provides tracing, statistics, and debugging capabilities for the kernel.
//!
//! Design:
//! - Lightweight event tracing with timestamps
//! - Performance counters for syscalls and operations
//! - Statistics collection for debugging and monitoring
//! - Ring buffer for recent events (bounded memory)
//! - Compile-time feature flags for zero-cost when disabled

use std::collections::VecDeque;

/// Maximum number of events to keep in the trace buffer
const TRACE_BUFFER_SIZE: usize = 1000;

/// Trace event categories
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceCategory {
    /// Syscall entry/exit
    Syscall,
    /// Process lifecycle (spawn, exit, signal)
    Process,
    /// Memory operations (alloc, free, shm)
    Memory,
    /// Timer events (set, fire, cancel)
    Timer,
    /// Signal delivery
    Signal,
    /// Task scheduling
    Scheduler,
    /// File operations
    File,
    /// IPC operations
    Ipc,
    /// Compositor/window events
    Compositor,
    /// Custom/user events
    Custom,
}

impl std::fmt::Display for TraceCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceCategory::Syscall => write!(f, "SYSCALL"),
            TraceCategory::Process => write!(f, "PROCESS"),
            TraceCategory::Memory => write!(f, "MEMORY"),
            TraceCategory::Timer => write!(f, "TIMER"),
            TraceCategory::Signal => write!(f, "SIGNAL"),
            TraceCategory::Scheduler => write!(f, "SCHED"),
            TraceCategory::File => write!(f, "FILE"),
            TraceCategory::Ipc => write!(f, "IPC"),
            TraceCategory::Compositor => write!(f, "COMP"),
            TraceCategory::Custom => write!(f, "CUSTOM"),
        }
    }
}

/// A single trace event
#[derive(Debug, Clone)]
pub struct TraceEvent {
    /// Timestamp in milliseconds (from kernel time)
    pub timestamp: f64,
    /// Event category
    pub category: TraceCategory,
    /// Event name/type
    pub name: String,
    /// Optional details
    pub detail: Option<String>,
    /// Associated process ID (if any)
    pub pid: Option<u32>,
    /// Duration in milliseconds (for span events)
    pub duration: Option<f64>,
}

impl TraceEvent {
    /// Create a new instant event
    pub fn instant(timestamp: f64, category: TraceCategory, name: impl Into<String>) -> Self {
        Self {
            timestamp,
            category,
            name: name.into(),
            detail: None,
            pid: None,
            duration: None,
        }
    }

    /// Create an event with details
    pub fn with_detail(
        timestamp: f64,
        category: TraceCategory,
        name: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            timestamp,
            category,
            name: name.into(),
            detail: Some(detail.into()),
            pid: None,
            duration: None,
        }
    }

    /// Add process ID
    pub fn with_pid(mut self, pid: u32) -> Self {
        self.pid = Some(pid);
        self
    }

    /// Add duration
    pub fn with_duration(mut self, duration: f64) -> Self {
        self.duration = Some(duration);
        self
    }
}

/// Performance counters for a category
#[derive(Debug, Clone, Default)]
pub struct PerfCounters {
    /// Total call count
    pub count: u64,
    /// Total time spent (ms)
    pub total_time: f64,
    /// Minimum call time (ms)
    pub min_time: f64,
    /// Maximum call time (ms)
    pub max_time: f64,
    /// Error count
    pub errors: u64,
}

impl PerfCounters {
    pub fn new() -> Self {
        Self {
            count: 0,
            total_time: 0.0,
            min_time: f64::MAX,
            max_time: 0.0,
            errors: 0,
        }
    }

    /// Record a successful operation
    pub fn record(&mut self, duration: f64) {
        self.count += 1;
        self.total_time += duration;
        if duration < self.min_time {
            self.min_time = duration;
        }
        if duration > self.max_time {
            self.max_time = duration;
        }
    }

    /// Record an error
    pub fn record_error(&mut self) {
        self.errors += 1;
    }

    /// Average time per call
    pub fn avg_time(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.total_time / self.count as f64
        }
    }

    /// Success rate
    pub fn success_rate(&self) -> f64 {
        let total = self.count + self.errors;
        if total == 0 {
            1.0
        } else {
            self.count as f64 / total as f64
        }
    }
}

/// Syscall-specific counters
#[derive(Debug, Clone, Default)]
pub struct SyscallStats {
    pub open: PerfCounters,
    pub close: PerfCounters,
    pub read: PerfCounters,
    pub write: PerfCounters,
    pub dup: PerfCounters,
    pub pipe: PerfCounters,
    pub seek: PerfCounters,
    pub mkdir: PerfCounters,
    pub readdir: PerfCounters,
    pub remove: PerfCounters,
    pub exists: PerfCounters,
    pub chdir: PerfCounters,
    pub getcwd: PerfCounters,
    pub getpid: PerfCounters,
    pub mem_alloc: PerfCounters,
    pub mem_free: PerfCounters,
    pub mem_read: PerfCounters,
    pub mem_write: PerfCounters,
    pub shm_ops: PerfCounters,
    pub timer_ops: PerfCounters,
    pub signal_ops: PerfCounters,
}

impl SyscallStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get total syscall count
    pub fn total_count(&self) -> u64 {
        self.open.count
            + self.close.count
            + self.read.count
            + self.write.count
            + self.dup.count
            + self.pipe.count
            + self.seek.count
            + self.mkdir.count
            + self.readdir.count
            + self.remove.count
            + self.exists.count
            + self.chdir.count
            + self.getcwd.count
            + self.getpid.count
            + self.mem_alloc.count
            + self.mem_free.count
            + self.mem_read.count
            + self.mem_write.count
            + self.shm_ops.count
            + self.timer_ops.count
            + self.signal_ops.count
    }

    /// Get total errors
    pub fn total_errors(&self) -> u64 {
        self.open.errors
            + self.close.errors
            + self.read.errors
            + self.write.errors
            + self.dup.errors
            + self.pipe.errors
            + self.seek.errors
            + self.mkdir.errors
            + self.readdir.errors
            + self.remove.errors
            + self.exists.errors
            + self.chdir.errors
            + self.getcwd.errors
            + self.getpid.errors
            + self.mem_alloc.errors
            + self.mem_free.errors
            + self.mem_read.errors
            + self.mem_write.errors
            + self.shm_ops.errors
            + self.timer_ops.errors
            + self.signal_ops.errors
    }
}

/// Scheduler statistics
#[derive(Debug, Clone, Default)]
pub struct SchedulerStats {
    /// Total ticks executed
    pub tick_count: u64,
    /// Tasks polled per tick (cumulative)
    pub tasks_polled: u64,
    /// Tasks completed
    pub tasks_completed: u64,
    /// Tasks spawned
    pub tasks_spawned: u64,
    /// Total tick time (ms)
    pub total_tick_time: f64,
    /// Maximum tick time
    pub max_tick_time: f64,
}

impl SchedulerStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a tick
    pub fn record_tick(&mut self, polled: usize, duration: f64) {
        self.tick_count += 1;
        self.tasks_polled += polled as u64;
        self.total_tick_time += duration;
        if duration > self.max_tick_time {
            self.max_tick_time = duration;
        }
    }

    /// Average tasks per tick
    pub fn avg_tasks_per_tick(&self) -> f64 {
        if self.tick_count == 0 {
            0.0
        } else {
            self.tasks_polled as f64 / self.tick_count as f64
        }
    }

    /// Average tick duration
    pub fn avg_tick_time(&self) -> f64 {
        if self.tick_count == 0 {
            0.0
        } else {
            self.total_tick_time / self.tick_count as f64
        }
    }
}

/// Kernel-wide statistics
#[derive(Debug, Clone, Default)]
pub struct KernelStats {
    /// Process spawn count
    pub processes_spawned: u64,
    /// Process exit count
    pub processes_exited: u64,
    /// Current process count (may differ from spawned - exited if zombies exist)
    pub current_process_count: u32,
    /// Peak process count
    pub peak_process_count: u32,
    /// Signals delivered
    pub signals_delivered: u64,
    /// Timers fired
    pub timers_fired: u64,
    /// Bytes read (I/O)
    pub bytes_read: u64,
    /// Bytes written (I/O)
    pub bytes_written: u64,
}

impl KernelStats {
    pub fn new() -> Self {
        Self::default()
    }
}

/// The main tracer/instrumentation system
#[derive(Debug)]
pub struct Tracer {
    /// Whether tracing is enabled
    enabled: bool,
    /// Category filter (None = all)
    filter: Option<Vec<TraceCategory>>,
    /// Ring buffer of recent events
    events: VecDeque<TraceEvent>,
    /// Syscall performance counters
    pub syscalls: SyscallStats,
    /// Scheduler statistics
    pub scheduler: SchedulerStats,
    /// Kernel-wide statistics
    pub kernel: KernelStats,
    /// Start time for uptime calculation
    start_time: f64,
}

impl Tracer {
    /// Create a new tracer
    pub fn new() -> Self {
        Self {
            enabled: false,
            filter: None,
            events: VecDeque::with_capacity(TRACE_BUFFER_SIZE),
            syscalls: SyscallStats::new(),
            scheduler: SchedulerStats::new(),
            kernel: KernelStats::new(),
            start_time: 0.0,
        }
    }

    /// Set the start time (for uptime calculation)
    pub fn set_start_time(&mut self, time: f64) {
        self.start_time = time;
    }

    /// Get uptime
    pub fn uptime(&self, now: f64) -> f64 {
        now - self.start_time
    }

    /// Enable tracing
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable tracing
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Check if tracing is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set category filter (None = trace all)
    pub fn set_filter(&mut self, categories: Option<Vec<TraceCategory>>) {
        self.filter = categories;
    }

    /// Check if category should be traced
    fn should_trace(&self, category: TraceCategory) -> bool {
        if !self.enabled {
            return false;
        }
        match &self.filter {
            None => true,
            Some(cats) => cats.contains(&category),
        }
    }

    /// Record a trace event
    pub fn trace(&mut self, event: TraceEvent) {
        if !self.should_trace(event.category) {
            return;
        }

        // Maintain ring buffer size
        if self.events.len() >= TRACE_BUFFER_SIZE {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    /// Quick trace without building a full event
    pub fn trace_instant(&mut self, timestamp: f64, category: TraceCategory, name: &str) {
        if self.should_trace(category) {
            self.trace(TraceEvent::instant(timestamp, category, name));
        }
    }

    /// Trace with detail
    pub fn trace_detail(
        &mut self,
        timestamp: f64,
        category: TraceCategory,
        name: &str,
        detail: &str,
    ) {
        if self.should_trace(category) {
            self.trace(TraceEvent::with_detail(timestamp, category, name, detail));
        }
    }

    /// Get recent events
    pub fn events(&self) -> &VecDeque<TraceEvent> {
        &self.events
    }

    /// Get events of a specific category
    pub fn events_by_category(&self, category: TraceCategory) -> Vec<&TraceEvent> {
        self.events
            .iter()
            .filter(|e| e.category == category)
            .collect()
    }

    /// Get events for a specific process
    pub fn events_by_pid(&self, pid: u32) -> Vec<&TraceEvent> {
        self.events
            .iter()
            .filter(|e| e.pid == Some(pid))
            .collect()
    }

    /// Clear the event buffer
    pub fn clear_events(&mut self) {
        self.events.clear();
    }

    /// Reset all statistics
    pub fn reset_stats(&mut self) {
        self.syscalls = SyscallStats::new();
        self.scheduler = SchedulerStats::new();
        self.kernel = KernelStats::new();
    }

    /// Reset everything (events and stats)
    pub fn reset(&mut self) {
        self.clear_events();
        self.reset_stats();
    }

    /// Get a summary report
    pub fn summary(&self, now: f64) -> TraceSummary {
        TraceSummary {
            uptime: self.uptime(now),
            enabled: self.enabled,
            event_count: self.events.len(),
            syscall_count: self.syscalls.total_count(),
            syscall_errors: self.syscalls.total_errors(),
            tick_count: self.scheduler.tick_count,
            avg_tick_time: self.scheduler.avg_tick_time(),
            max_tick_time: self.scheduler.max_tick_time,
            processes_spawned: self.kernel.processes_spawned,
            processes_exited: self.kernel.processes_exited,
            signals_delivered: self.kernel.signals_delivered,
            timers_fired: self.kernel.timers_fired,
            bytes_read: self.kernel.bytes_read,
            bytes_written: self.kernel.bytes_written,
        }
    }
}

impl Default for Tracer {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of trace/stats data
#[derive(Debug, Clone)]
pub struct TraceSummary {
    pub uptime: f64,
    pub enabled: bool,
    pub event_count: usize,
    pub syscall_count: u64,
    pub syscall_errors: u64,
    pub tick_count: u64,
    pub avg_tick_time: f64,
    pub max_tick_time: f64,
    pub processes_spawned: u64,
    pub processes_exited: u64,
    pub signals_delivered: u64,
    pub timers_fired: u64,
    pub bytes_read: u64,
    pub bytes_written: u64,
}

impl std::fmt::Display for TraceSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Kernel Statistics ===")?;
        writeln!(f, "Uptime: {:.2}s", self.uptime / 1000.0)?;
        writeln!(
            f,
            "Tracing: {}",
            if self.enabled { "ON" } else { "OFF" }
        )?;
        writeln!(f, "Events buffered: {}", self.event_count)?;
        writeln!(f)?;
        writeln!(f, "--- Syscalls ---")?;
        writeln!(f, "Total: {}", self.syscall_count)?;
        writeln!(f, "Errors: {}", self.syscall_errors)?;
        writeln!(f)?;
        writeln!(f, "--- Scheduler ---")?;
        writeln!(f, "Ticks: {}", self.tick_count)?;
        writeln!(f, "Avg tick: {:.3}ms", self.avg_tick_time)?;
        writeln!(f, "Max tick: {:.3}ms", self.max_tick_time)?;
        writeln!(f)?;
        writeln!(f, "--- Processes ---")?;
        writeln!(f, "Spawned: {}", self.processes_spawned)?;
        writeln!(f, "Exited: {}", self.processes_exited)?;
        writeln!(f)?;
        writeln!(f, "--- Events ---")?;
        writeln!(f, "Signals: {}", self.signals_delivered)?;
        writeln!(f, "Timers: {}", self.timers_fired)?;
        writeln!(f)?;
        writeln!(f, "--- I/O ---")?;
        writeln!(f, "Read: {} bytes", self.bytes_read)?;
        writeln!(f, "Written: {} bytes", self.bytes_written)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracer_disabled_by_default() {
        let tracer = Tracer::new();
        assert!(!tracer.is_enabled());
    }

    #[test]
    fn test_tracer_enable_disable() {
        let mut tracer = Tracer::new();
        assert!(!tracer.is_enabled());

        tracer.enable();
        assert!(tracer.is_enabled());

        tracer.disable();
        assert!(!tracer.is_enabled());
    }

    #[test]
    fn test_trace_event_creation() {
        let event = TraceEvent::instant(100.0, TraceCategory::Syscall, "open");
        assert_eq!(event.timestamp, 100.0);
        assert_eq!(event.category, TraceCategory::Syscall);
        assert_eq!(event.name, "open");
        assert!(event.detail.is_none());
        assert!(event.pid.is_none());
        assert!(event.duration.is_none());
    }

    #[test]
    fn test_trace_event_with_detail() {
        let event = TraceEvent::with_detail(100.0, TraceCategory::File, "open", "/etc/passwd");
        assert_eq!(event.detail, Some("/etc/passwd".to_string()));
    }

    #[test]
    fn test_trace_event_with_pid() {
        let event = TraceEvent::instant(100.0, TraceCategory::Process, "spawn").with_pid(42);
        assert_eq!(event.pid, Some(42));
    }

    #[test]
    fn test_tracer_records_events() {
        let mut tracer = Tracer::new();
        tracer.enable();

        tracer.trace_instant(100.0, TraceCategory::Syscall, "open");
        tracer.trace_instant(200.0, TraceCategory::Syscall, "read");
        tracer.trace_instant(300.0, TraceCategory::Syscall, "close");

        assert_eq!(tracer.events().len(), 3);
    }

    #[test]
    fn test_tracer_filter() {
        let mut tracer = Tracer::new();
        tracer.enable();
        tracer.set_filter(Some(vec![TraceCategory::Syscall]));

        tracer.trace_instant(100.0, TraceCategory::Syscall, "open");
        tracer.trace_instant(200.0, TraceCategory::Memory, "alloc");
        tracer.trace_instant(300.0, TraceCategory::Syscall, "close");

        // Only syscall events should be recorded
        assert_eq!(tracer.events().len(), 2);
    }

    #[test]
    fn test_tracer_ring_buffer() {
        let mut tracer = Tracer::new();
        tracer.enable();

        // Fill beyond capacity
        for i in 0..TRACE_BUFFER_SIZE + 100 {
            tracer.trace_instant(i as f64, TraceCategory::Syscall, "test");
        }

        // Should be capped at TRACE_BUFFER_SIZE
        assert_eq!(tracer.events().len(), TRACE_BUFFER_SIZE);

        // First event should be from position 100 (first 100 were evicted)
        assert_eq!(tracer.events().front().unwrap().timestamp, 100.0);
    }

    #[test]
    fn test_perf_counters() {
        let mut counters = PerfCounters::new();

        counters.record(10.0);
        counters.record(20.0);
        counters.record(15.0);

        assert_eq!(counters.count, 3);
        assert_eq!(counters.total_time, 45.0);
        assert_eq!(counters.min_time, 10.0);
        assert_eq!(counters.max_time, 20.0);
        assert_eq!(counters.avg_time(), 15.0);
    }

    #[test]
    fn test_perf_counters_errors() {
        let mut counters = PerfCounters::new();

        counters.record(10.0);
        counters.record_error();
        counters.record_error();

        assert_eq!(counters.count, 1);
        assert_eq!(counters.errors, 2);
        assert!((counters.success_rate() - 0.333).abs() < 0.01);
    }

    #[test]
    fn test_scheduler_stats() {
        let mut stats = SchedulerStats::new();

        stats.record_tick(5, 1.0);
        stats.record_tick(3, 2.0);
        stats.record_tick(7, 0.5);

        assert_eq!(stats.tick_count, 3);
        assert_eq!(stats.tasks_polled, 15);
        assert_eq!(stats.avg_tasks_per_tick(), 5.0);
        assert_eq!(stats.max_tick_time, 2.0);
    }

    #[test]
    fn test_summary() {
        let mut tracer = Tracer::new();
        tracer.enable();
        tracer.set_start_time(0.0);

        tracer.kernel.processes_spawned = 5;
        tracer.kernel.processes_exited = 2;
        tracer.syscalls.open.count = 10;
        tracer.syscalls.read.count = 50;

        let summary = tracer.summary(1000.0);
        assert_eq!(summary.uptime, 1000.0);
        assert!(summary.enabled);
        assert_eq!(summary.syscall_count, 60);
        assert_eq!(summary.processes_spawned, 5);
    }

    #[test]
    fn test_events_by_category() {
        let mut tracer = Tracer::new();
        tracer.enable();

        tracer.trace_instant(100.0, TraceCategory::Syscall, "open");
        tracer.trace_instant(200.0, TraceCategory::Memory, "alloc");
        tracer.trace_instant(300.0, TraceCategory::Syscall, "close");
        tracer.trace_instant(400.0, TraceCategory::Process, "spawn");

        let syscalls = tracer.events_by_category(TraceCategory::Syscall);
        assert_eq!(syscalls.len(), 2);
    }

    #[test]
    fn test_events_by_pid() {
        let mut tracer = Tracer::new();
        tracer.enable();

        tracer.trace(TraceEvent::instant(100.0, TraceCategory::Syscall, "open").with_pid(1));
        tracer.trace(TraceEvent::instant(200.0, TraceCategory::Syscall, "read").with_pid(2));
        tracer.trace(TraceEvent::instant(300.0, TraceCategory::Syscall, "write").with_pid(1));

        let pid1_events = tracer.events_by_pid(1);
        assert_eq!(pid1_events.len(), 2);
    }

    #[test]
    fn test_reset() {
        let mut tracer = Tracer::new();
        tracer.enable();

        tracer.trace_instant(100.0, TraceCategory::Syscall, "test");
        tracer.syscalls.open.count = 10;
        tracer.kernel.processes_spawned = 5;

        tracer.reset();

        assert_eq!(tracer.events().len(), 0);
        assert_eq!(tracer.syscalls.open.count, 0);
        assert_eq!(tracer.kernel.processes_spawned, 0);
    }

    #[test]
    fn test_disabled_tracer_no_events() {
        let mut tracer = Tracer::new();
        // Tracer is disabled by default

        tracer.trace_instant(100.0, TraceCategory::Syscall, "open");
        tracer.trace_instant(200.0, TraceCategory::Syscall, "read");

        // Nothing should be recorded
        assert_eq!(tracer.events().len(), 0);
    }
}
