//! Performance Profiler
//!
//! Provides CPU and memory profiling capabilities for the kernel:
//! - Task execution time tracking
//! - Syscall performance analysis
//! - Memory allocation profiling
//! - Flame graph data generation
//! - Real-time sampling
//!
//! The profiler builds on the existing trace infrastructure to provide
//! deeper insights into system performance.

use super::process::Pid;
use super::task::TaskId;
use super::trace::{PerfCounters, TraceCategory};
use std::collections::{HashMap, VecDeque};

/// Maximum number of samples to keep in ring buffers
const MAX_SAMPLES: usize = 10000;

/// Maximum number of memory snapshots to retain
const MAX_MEMORY_SNAPSHOTS: usize = 1000;

/// Maximum call stack depth for flame graphs
#[allow(dead_code)]
const MAX_STACK_DEPTH: usize = 64;

// ============================================================================
// CPU Profiling
// ============================================================================

/// A snapshot of task state at a point in time
#[derive(Debug, Clone)]
pub struct TaskSample {
    /// When this sample was taken (ms)
    pub timestamp: f64,
    /// Task being sampled
    pub task_id: TaskId,
    /// Process owning the task
    pub pid: Option<Pid>,
    /// Task state at sample time
    pub state: TaskSampleState,
    /// Current call stack (if available)
    pub stack: Vec<String>,
}

/// Task state at sample time
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskSampleState {
    /// Task is actively running
    Running,
    /// Task is ready to run
    Ready,
    /// Task is waiting for I/O or event
    Waiting,
    /// Task is blocked on another task
    Blocked,
}

/// Aggregated statistics for a syscall
#[derive(Debug, Clone, Default)]
pub struct SyscallProfile {
    /// Syscall name
    pub name: String,
    /// Performance counters
    pub counters: PerfCounters,
    /// Histogram of call durations (buckets: 0-1ms, 1-10ms, 10-100ms, 100ms+)
    pub duration_histogram: [u64; 4],
    /// Recent call timestamps for rate calculation
    recent_calls: VecDeque<f64>,
}

impl SyscallProfile {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            counters: PerfCounters::new(),
            duration_histogram: [0; 4],
            recent_calls: VecDeque::with_capacity(100),
        }
    }

    /// Record a syscall execution
    pub fn record(&mut self, duration: f64, timestamp: f64) {
        self.counters.record(duration);

        // Update histogram
        let bucket = if duration < 1.0 {
            0
        } else if duration < 10.0 {
            1
        } else if duration < 100.0 {
            2
        } else {
            3
        };
        self.duration_histogram[bucket] += 1;

        // Track recent calls for rate calculation
        self.recent_calls.push_back(timestamp);
        while self.recent_calls.len() > 100 {
            self.recent_calls.pop_front();
        }
    }

    /// Record an error
    pub fn record_error(&mut self) {
        self.counters.record_error();
    }

    /// Calculate calls per second over recent window
    pub fn calls_per_second(&self) -> f64 {
        if self.recent_calls.len() < 2 {
            return 0.0;
        }
        let first = *self.recent_calls.front().unwrap();
        let last = *self.recent_calls.back().unwrap();
        let duration_sec = (last - first) / 1000.0;
        if duration_sec > 0.0 {
            self.recent_calls.len() as f64 / duration_sec
        } else {
            0.0
        }
    }
}

/// CPU profiling data
#[derive(Debug)]
pub struct CpuProfile {
    /// Task samples (ring buffer)
    samples: VecDeque<TaskSample>,
    /// Per-syscall statistics
    syscall_profiles: HashMap<String, SyscallProfile>,
    /// Time spent in each task (for CPU usage calculation)
    task_time: HashMap<TaskId, f64>,
    /// Per-process CPU time
    process_time: HashMap<Pid, f64>,
    /// Total profiled time
    total_time: f64,
    /// Sampling interval (ms)
    sample_interval: f64,
    /// Last sample timestamp
    last_sample: f64,
}

impl CpuProfile {
    pub fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(MAX_SAMPLES),
            syscall_profiles: HashMap::new(),
            task_time: HashMap::new(),
            process_time: HashMap::new(),
            total_time: 0.0,
            sample_interval: 1.0, // 1ms default
            last_sample: 0.0,
        }
    }

    /// Set the sampling interval
    pub fn set_sample_interval(&mut self, interval_ms: f64) {
        self.sample_interval = interval_ms.max(0.1);
    }

    /// Check if it's time to take a sample
    pub fn should_sample(&self, now: f64) -> bool {
        now - self.last_sample >= self.sample_interval
    }

    /// Record a task sample
    pub fn record_sample(&mut self, sample: TaskSample) {
        self.last_sample = sample.timestamp;

        // Update task time tracking
        if sample.state == TaskSampleState::Running {
            *self.task_time.entry(sample.task_id).or_insert(0.0) += self.sample_interval;
            if let Some(pid) = sample.pid {
                *self.process_time.entry(pid).or_insert(0.0) += self.sample_interval;
            }
            self.total_time += self.sample_interval;
        }

        // Add to ring buffer
        if self.samples.len() >= MAX_SAMPLES {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    /// Record a syscall execution
    pub fn record_syscall(&mut self, name: &str, duration: f64, timestamp: f64, error: bool) {
        let profile = self
            .syscall_profiles
            .entry(name.to_string())
            .or_insert_with(|| SyscallProfile::new(name));

        if error {
            profile.record_error();
        } else {
            profile.record(duration, timestamp);
        }
    }

    /// Get CPU usage percentage for a task
    pub fn task_cpu_percent(&self, task_id: TaskId) -> f64 {
        if self.total_time == 0.0 {
            return 0.0;
        }
        let task_time = self.task_time.get(&task_id).copied().unwrap_or(0.0);
        (task_time / self.total_time) * 100.0
    }

    /// Get CPU usage percentage for a process
    pub fn process_cpu_percent(&self, pid: Pid) -> f64 {
        if self.total_time == 0.0 {
            return 0.0;
        }
        let proc_time = self.process_time.get(&pid).copied().unwrap_or(0.0);
        (proc_time / self.total_time) * 100.0
    }

    /// Get all syscall profiles
    pub fn syscall_profiles(&self) -> &HashMap<String, SyscallProfile> {
        &self.syscall_profiles
    }

    /// Get top N syscalls by call count
    pub fn top_syscalls_by_count(&self, n: usize) -> Vec<(&String, &SyscallProfile)> {
        let mut profiles: Vec<_> = self.syscall_profiles.iter().collect();
        profiles.sort_by(|a, b| b.1.counters.count.cmp(&a.1.counters.count));
        profiles.into_iter().take(n).collect()
    }

    /// Get top N syscalls by total time
    pub fn top_syscalls_by_time(&self, n: usize) -> Vec<(&String, &SyscallProfile)> {
        let mut profiles: Vec<_> = self.syscall_profiles.iter().collect();
        profiles.sort_by(|a, b| {
            b.1.counters
                .total_time
                .partial_cmp(&a.1.counters.total_time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        profiles.into_iter().take(n).collect()
    }

    /// Get recent samples
    pub fn recent_samples(&self, count: usize) -> impl Iterator<Item = &TaskSample> {
        self.samples.iter().rev().take(count)
    }

    /// Clear all profiling data
    pub fn clear(&mut self) {
        self.samples.clear();
        self.syscall_profiles.clear();
        self.task_time.clear();
        self.process_time.clear();
        self.total_time = 0.0;
    }
}

impl Default for CpuProfile {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Memory Profiling
// ============================================================================

/// Memory statistics for a single process at a point in time
#[derive(Debug, Clone)]
pub struct ProcessMemorySnapshot {
    /// Process ID
    pub pid: Pid,
    /// Process name
    pub name: String,
    /// Currently allocated bytes
    pub allocated: usize,
    /// Peak allocation
    pub peak: usize,
    /// Number of memory regions
    pub region_count: usize,
    /// COW fault count
    pub cow_faults: usize,
    /// Shared memory attached
    pub shm_size: usize,
}

/// System-wide memory snapshot
#[derive(Debug, Clone)]
pub struct MemorySnapshot {
    /// Timestamp (ms)
    pub timestamp: f64,
    /// Per-process memory stats
    pub processes: Vec<ProcessMemorySnapshot>,
    /// Total system allocation
    pub total_allocated: usize,
    /// System memory limit
    pub system_limit: usize,
    /// Total shared memory
    pub total_shm: usize,
    /// System-wide COW faults since last snapshot
    pub cow_faults_delta: usize,
}

impl MemorySnapshot {
    /// Calculate memory utilization percentage
    pub fn utilization_percent(&self) -> f64 {
        if self.system_limit == 0 {
            return 0.0;
        }
        (self.total_allocated as f64 / self.system_limit as f64) * 100.0
    }

    /// Find top N processes by memory usage
    pub fn top_by_memory(&self, n: usize) -> Vec<&ProcessMemorySnapshot> {
        let mut procs: Vec<_> = self.processes.iter().collect();
        procs.sort_by(|a, b| b.allocated.cmp(&a.allocated));
        procs.into_iter().take(n).collect()
    }
}

/// Allocation event for tracking allocation patterns
#[derive(Debug, Clone)]
pub struct AllocationEvent {
    /// When the allocation occurred (ms)
    pub timestamp: f64,
    /// Process that made the allocation
    pub pid: Pid,
    /// Size of allocation
    pub size: usize,
    /// Whether this is an allocation (true) or deallocation (false)
    pub is_alloc: bool,
    /// Optional allocation site identifier
    pub site: Option<String>,
}

/// Memory profiling data
#[derive(Debug)]
pub struct MemoryProfile {
    /// Memory snapshots over time
    snapshots: VecDeque<MemorySnapshot>,
    /// Allocation events (ring buffer)
    allocations: VecDeque<AllocationEvent>,
    /// Last COW fault count (for delta calculation)
    last_cow_faults: usize,
    /// Snapshot interval (ms)
    snapshot_interval: f64,
    /// Last snapshot timestamp
    last_snapshot: f64,
    /// Allocation size histogram (buckets: <1KB, 1-16KB, 16-256KB, 256KB+)
    size_histogram: [u64; 4],
}

impl MemoryProfile {
    pub fn new() -> Self {
        Self {
            snapshots: VecDeque::with_capacity(MAX_MEMORY_SNAPSHOTS),
            allocations: VecDeque::with_capacity(MAX_SAMPLES),
            last_cow_faults: 0,
            snapshot_interval: 100.0, // 100ms default
            last_snapshot: 0.0,
            size_histogram: [0; 4],
        }
    }

    /// Set snapshot interval
    pub fn set_snapshot_interval(&mut self, interval_ms: f64) {
        self.snapshot_interval = interval_ms.max(10.0);
    }

    /// Check if it's time to take a snapshot
    pub fn should_snapshot(&self, now: f64) -> bool {
        now - self.last_snapshot >= self.snapshot_interval
    }

    /// Record a memory snapshot
    pub fn record_snapshot(&mut self, mut snapshot: MemorySnapshot) {
        // Calculate COW fault delta
        let total_cow_faults: usize = snapshot.processes.iter().map(|p| p.cow_faults).sum();
        snapshot.cow_faults_delta = total_cow_faults.saturating_sub(self.last_cow_faults);
        self.last_cow_faults = total_cow_faults;

        self.last_snapshot = snapshot.timestamp;

        if self.snapshots.len() >= MAX_MEMORY_SNAPSHOTS {
            self.snapshots.pop_front();
        }
        self.snapshots.push_back(snapshot);
    }

    /// Record an allocation event
    pub fn record_allocation(&mut self, event: AllocationEvent) {
        // Update size histogram
        if event.is_alloc {
            let bucket = if event.size < 1024 {
                0
            } else if event.size < 16 * 1024 {
                1
            } else if event.size < 256 * 1024 {
                2
            } else {
                3
            };
            self.size_histogram[bucket] += 1;
        }

        if self.allocations.len() >= MAX_SAMPLES {
            self.allocations.pop_front();
        }
        self.allocations.push_back(event);
    }

    /// Get the most recent snapshot
    pub fn latest_snapshot(&self) -> Option<&MemorySnapshot> {
        self.snapshots.back()
    }

    /// Get memory usage over time (for charting)
    pub fn memory_timeline(&self) -> impl Iterator<Item = (f64, usize)> + '_ {
        self.snapshots
            .iter()
            .map(|s| (s.timestamp, s.total_allocated))
    }

    /// Get COW fault rate over time
    pub fn cow_fault_timeline(&self) -> impl Iterator<Item = (f64, usize)> + '_ {
        self.snapshots
            .iter()
            .map(|s| (s.timestamp, s.cow_faults_delta))
    }

    /// Get allocation rate (allocations per second over recent window)
    pub fn allocation_rate(&self) -> f64 {
        if self.allocations.len() < 2 {
            return 0.0;
        }
        let allocs: Vec<_> = self.allocations.iter().filter(|e| e.is_alloc).collect();
        if allocs.len() < 2 {
            return 0.0;
        }
        let first = allocs.first().unwrap().timestamp;
        let last = allocs.last().unwrap().timestamp;
        let duration_sec = (last - first) / 1000.0;
        if duration_sec > 0.0 {
            allocs.len() as f64 / duration_sec
        } else {
            0.0
        }
    }

    /// Get allocation size distribution
    pub fn size_distribution(&self) -> AllocationSizeDistribution {
        AllocationSizeDistribution {
            under_1kb: self.size_histogram[0],
            kb_1_to_16: self.size_histogram[1],
            kb_16_to_256: self.size_histogram[2],
            over_256kb: self.size_histogram[3],
        }
    }

    /// Clear all profiling data
    pub fn clear(&mut self) {
        self.snapshots.clear();
        self.allocations.clear();
        self.last_cow_faults = 0;
        self.size_histogram = [0; 4];
    }
}

impl Default for MemoryProfile {
    fn default() -> Self {
        Self::new()
    }
}

/// Allocation size distribution
#[derive(Debug, Clone)]
pub struct AllocationSizeDistribution {
    pub under_1kb: u64,
    pub kb_1_to_16: u64,
    pub kb_16_to_256: u64,
    pub over_256kb: u64,
}

// ============================================================================
// Flame Graph Support
// ============================================================================

/// A node in the flame graph tree
#[derive(Debug, Clone)]
pub struct FlameNode {
    /// Function/location name
    pub name: String,
    /// Self time (time spent in this node, not children)
    pub self_time: f64,
    /// Total time (including children)
    pub total_time: f64,
    /// Number of samples hitting this node
    pub sample_count: u64,
    /// Child nodes
    pub children: HashMap<String, FlameNode>,
}

impl FlameNode {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            self_time: 0.0,
            total_time: 0.0,
            sample_count: 0,
            children: HashMap::new(),
        }
    }

    /// Add time from a sample at this node
    pub fn add_sample(&mut self, time: f64) {
        self.self_time += time;
        self.total_time += time;
        self.sample_count += 1;
    }

    /// Get or create a child node
    pub fn get_or_create_child(&mut self, name: &str) -> &mut FlameNode {
        self.children
            .entry(name.to_string())
            .or_insert_with(|| FlameNode::new(name))
    }

    /// Recursively propagate time to parents
    fn propagate_time(&mut self) {
        for child in self.children.values_mut() {
            child.propagate_time();
            self.total_time += child.total_time;
        }
    }
}

/// Flame graph builder from task samples
#[derive(Debug)]
pub struct FlameGraphBuilder {
    /// Root nodes (typically process names)
    roots: HashMap<String, FlameNode>,
}

impl FlameGraphBuilder {
    pub fn new() -> Self {
        Self {
            roots: HashMap::new(),
        }
    }

    /// Add a sample with a call stack
    pub fn add_sample(&mut self, stack: &[String], time: f64) {
        if stack.is_empty() {
            return;
        }

        // Get or create root
        let root_name = &stack[0];
        let root = self
            .roots
            .entry(root_name.clone())
            .or_insert_with(|| FlameNode::new(root_name));

        // Walk down the stack
        let mut current = root;
        for frame in stack.iter().skip(1) {
            current.total_time += time;
            current = current.get_or_create_child(frame);
        }

        // Add time to leaf
        current.add_sample(time);
    }

    /// Build the flame graph (finalizes and returns roots)
    pub fn build(mut self) -> HashMap<String, FlameNode> {
        for root in self.roots.values_mut() {
            root.propagate_time();
        }
        self.roots
    }

    /// Generate collapsed stack format (for external flame graph tools)
    pub fn to_collapsed_stacks(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for root in self.roots.values() {
            Self::collect_collapsed(root, &[], &mut lines);
        }
        lines
    }

    fn collect_collapsed(node: &FlameNode, prefix: &[&str], lines: &mut Vec<String>) {
        let mut path = prefix.to_vec();
        path.push(&node.name);

        if node.sample_count > 0 {
            lines.push(format!("{} {}", path.join(";"), node.sample_count));
        }

        for child in node.children.values() {
            Self::collect_collapsed(child, &path, lines);
        }
    }
}

impl Default for FlameGraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Main Profiler
// ============================================================================

/// Profiler state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfilerState {
    /// Profiler is stopped
    Stopped,
    /// Profiler is actively recording
    Recording,
    /// Profiler is paused (data retained)
    Paused,
}

/// The main performance profiler
#[derive(Debug)]
pub struct Profiler {
    /// Current state
    state: ProfilerState,
    /// CPU profiling data
    pub cpu: CpuProfile,
    /// Memory profiling data
    pub memory: MemoryProfile,
    /// Recording start time
    start_time: f64,
    /// Total recording duration
    recording_duration: f64,
    /// Categories to profile
    categories: Vec<TraceCategory>,
}

impl Profiler {
    /// Create a new profiler
    pub fn new() -> Self {
        Self {
            state: ProfilerState::Stopped,
            cpu: CpuProfile::new(),
            memory: MemoryProfile::new(),
            start_time: 0.0,
            recording_duration: 0.0,
            categories: vec![
                TraceCategory::Syscall,
                TraceCategory::Process,
                TraceCategory::Memory,
                TraceCategory::Scheduler,
            ],
        }
    }

    /// Get profiler state
    pub fn state(&self) -> ProfilerState {
        self.state
    }

    /// Check if profiler is recording
    pub fn is_recording(&self) -> bool {
        self.state == ProfilerState::Recording
    }

    /// Start recording
    pub fn start(&mut self, now: f64) {
        if self.state == ProfilerState::Stopped {
            self.cpu.clear();
            self.memory.clear();
            self.start_time = now;
            self.recording_duration = 0.0;
        }
        self.state = ProfilerState::Recording;
    }

    /// Pause recording
    pub fn pause(&mut self, now: f64) {
        if self.state == ProfilerState::Recording {
            self.recording_duration += now - self.start_time;
            self.state = ProfilerState::Paused;
        }
    }

    /// Resume recording
    pub fn resume(&mut self, now: f64) {
        if self.state == ProfilerState::Paused {
            self.start_time = now;
            self.state = ProfilerState::Recording;
        }
    }

    /// Stop recording
    pub fn stop(&mut self, now: f64) {
        if self.state == ProfilerState::Recording {
            self.recording_duration += now - self.start_time;
        }
        self.state = ProfilerState::Stopped;
    }

    /// Reset all data
    pub fn reset(&mut self) {
        self.state = ProfilerState::Stopped;
        self.cpu.clear();
        self.memory.clear();
        self.start_time = 0.0;
        self.recording_duration = 0.0;
    }

    /// Get total recording duration
    pub fn recording_duration(&self, now: f64) -> f64 {
        let current = if self.state == ProfilerState::Recording {
            now - self.start_time
        } else {
            0.0
        };
        self.recording_duration + current
    }

    /// Set categories to profile
    pub fn set_categories(&mut self, categories: Vec<TraceCategory>) {
        self.categories = categories;
    }

    /// Check if a category should be profiled
    pub fn should_profile(&self, category: TraceCategory) -> bool {
        self.state == ProfilerState::Recording && self.categories.contains(&category)
    }

    /// Generate a profile summary
    pub fn summary(&self, now: f64) -> ProfileSummary {
        let duration = self.recording_duration(now);

        ProfileSummary {
            state: self.state,
            duration_ms: duration,
            cpu_samples: self.cpu.samples.len(),
            syscall_types: self.cpu.syscall_profiles.len(),
            total_syscalls: self
                .cpu
                .syscall_profiles
                .values()
                .map(|p| p.counters.count)
                .sum(),
            memory_snapshots: self.memory.snapshots.len(),
            allocation_events: self.memory.allocations.len(),
            latest_memory: self.memory.latest_snapshot().map(|s| s.total_allocated),
            allocation_rate: self.memory.allocation_rate(),
        }
    }

    /// Build a flame graph from collected samples
    pub fn build_flame_graph(&self) -> FlameGraphBuilder {
        let mut builder = FlameGraphBuilder::new();
        let interval = self.cpu.sample_interval;

        for sample in &self.cpu.samples {
            if !sample.stack.is_empty() {
                builder.add_sample(&sample.stack, interval);
            }
        }

        builder
    }

    /// Export profile data as JSON
    pub fn export_json(&self, now: f64) -> String {
        let summary = self.summary(now);

        // Build syscall data
        let syscalls: Vec<_> = self
            .cpu
            .syscall_profiles
            .iter()
            .map(|(name, profile)| {
                format!(
                    r#"{{"name":"{}","count":{},"total_time":{:.3},"avg_time":{:.3},"errors":{}}}"#,
                    name,
                    profile.counters.count,
                    profile.counters.total_time,
                    profile.counters.avg_time(),
                    profile.counters.errors
                )
            })
            .collect();

        // Build memory timeline
        let memory_timeline: Vec<_> = self
            .memory
            .memory_timeline()
            .map(|(t, m)| format!("[{:.1},{}]", t, m))
            .collect();

        format!(
            r#"{{"state":"{}","duration_ms":{:.1},"cpu_samples":{},"syscalls":[{}],"memory_timeline":[{}]}}"#,
            match summary.state {
                ProfilerState::Stopped => "stopped",
                ProfilerState::Recording => "recording",
                ProfilerState::Paused => "paused",
            },
            summary.duration_ms,
            summary.cpu_samples,
            syscalls.join(","),
            memory_timeline.join(",")
        )
    }
}

impl Default for Profiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of profile data
#[derive(Debug, Clone)]
pub struct ProfileSummary {
    /// Current profiler state
    pub state: ProfilerState,
    /// Total recording duration (ms)
    pub duration_ms: f64,
    /// Number of CPU samples collected
    pub cpu_samples: usize,
    /// Number of distinct syscall types recorded
    pub syscall_types: usize,
    /// Total number of syscalls recorded
    pub total_syscalls: u64,
    /// Number of memory snapshots
    pub memory_snapshots: usize,
    /// Number of allocation events
    pub allocation_events: usize,
    /// Latest memory usage (if available)
    pub latest_memory: Option<usize>,
    /// Allocation rate (allocs/sec)
    pub allocation_rate: f64,
}

impl std::fmt::Display for ProfileSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Profile Summary ===")?;
        writeln!(
            f,
            "State: {}",
            match self.state {
                ProfilerState::Stopped => "Stopped",
                ProfilerState::Recording => "Recording",
                ProfilerState::Paused => "Paused",
            }
        )?;
        writeln!(f, "Duration: {:.2}s", self.duration_ms / 1000.0)?;
        writeln!(f)?;
        writeln!(f, "--- CPU ---")?;
        writeln!(f, "Samples: {}", self.cpu_samples)?;
        writeln!(f, "Syscall types: {}", self.syscall_types)?;
        writeln!(f, "Total syscalls: {}", self.total_syscalls)?;
        writeln!(f)?;
        writeln!(f, "--- Memory ---")?;
        writeln!(f, "Snapshots: {}", self.memory_snapshots)?;
        writeln!(f, "Allocation events: {}", self.allocation_events)?;
        if let Some(mem) = self.latest_memory {
            writeln!(f, "Current usage: {} bytes", mem)?;
        }
        writeln!(f, "Allocation rate: {:.1}/sec", self.allocation_rate)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profiler_lifecycle() {
        let mut profiler = Profiler::new();
        assert_eq!(profiler.state(), ProfilerState::Stopped);

        profiler.start(0.0);
        assert_eq!(profiler.state(), ProfilerState::Recording);
        assert!(profiler.is_recording());

        profiler.pause(100.0);
        assert_eq!(profiler.state(), ProfilerState::Paused);

        profiler.resume(200.0);
        assert_eq!(profiler.state(), ProfilerState::Recording);

        profiler.stop(300.0);
        assert_eq!(profiler.state(), ProfilerState::Stopped);

        // Duration should be 100ms (0-100) + 100ms (200-300) = 200ms
        assert!((profiler.recording_duration(300.0) - 200.0).abs() < 0.001);
    }

    #[test]
    fn test_cpu_profile_syscalls() {
        let mut cpu = CpuProfile::new();

        cpu.record_syscall("open", 1.5, 100.0, false);
        cpu.record_syscall("open", 2.0, 110.0, false);
        cpu.record_syscall("read", 0.5, 120.0, false);
        cpu.record_syscall("open", 1.0, 130.0, true);

        let profiles = cpu.syscall_profiles();
        assert_eq!(profiles.len(), 2);

        let open = profiles.get("open").unwrap();
        assert_eq!(open.counters.count, 2);
        assert_eq!(open.counters.errors, 1);
        assert!((open.counters.total_time - 3.5).abs() < 0.001);
    }

    #[test]
    fn test_syscall_histogram() {
        let mut profile = SyscallProfile::new("test");

        profile.record(0.5, 0.0); // <1ms
        profile.record(5.0, 1.0); // 1-10ms
        profile.record(50.0, 2.0); // 10-100ms
        profile.record(150.0, 3.0); // 100ms+

        assert_eq!(profile.duration_histogram, [1, 1, 1, 1]);
    }

    #[test]
    fn test_memory_profile() {
        let mut mem = MemoryProfile::new();

        let snapshot = MemorySnapshot {
            timestamp: 100.0,
            processes: vec![ProcessMemorySnapshot {
                pid: Pid(1),
                name: "init".to_string(),
                allocated: 1024,
                peak: 2048,
                region_count: 1,
                cow_faults: 5,
                shm_size: 0,
            }],
            total_allocated: 1024,
            system_limit: 1024 * 1024,
            total_shm: 0,
            cow_faults_delta: 0,
        };

        mem.record_snapshot(snapshot);
        assert_eq!(mem.snapshots.len(), 1);

        let latest = mem.latest_snapshot().unwrap();
        assert_eq!(latest.total_allocated, 1024);
    }

    #[test]
    fn test_allocation_histogram() {
        let mut mem = MemoryProfile::new();

        mem.record_allocation(AllocationEvent {
            timestamp: 0.0,
            pid: Pid(1),
            size: 512,
            is_alloc: true,
            site: None,
        });
        mem.record_allocation(AllocationEvent {
            timestamp: 1.0,
            pid: Pid(1),
            size: 8192,
            is_alloc: true,
            site: None,
        });
        mem.record_allocation(AllocationEvent {
            timestamp: 2.0,
            pid: Pid(1),
            size: 100_000,
            is_alloc: true,
            site: None,
        });
        mem.record_allocation(AllocationEvent {
            timestamp: 3.0,
            pid: Pid(1),
            size: 500_000,
            is_alloc: true,
            site: None,
        });

        let dist = mem.size_distribution();
        assert_eq!(dist.under_1kb, 1);
        assert_eq!(dist.kb_1_to_16, 1);
        assert_eq!(dist.kb_16_to_256, 1);
        assert_eq!(dist.over_256kb, 1);
    }

    #[test]
    fn test_flame_graph_builder() {
        let mut builder = FlameGraphBuilder::new();

        builder.add_sample(
            &["main".to_string(), "foo".to_string(), "bar".to_string()],
            1.0,
        );
        builder.add_sample(
            &["main".to_string(), "foo".to_string(), "bar".to_string()],
            1.0,
        );
        builder.add_sample(
            &["main".to_string(), "foo".to_string(), "baz".to_string()],
            1.0,
        );
        builder.add_sample(&["main".to_string(), "qux".to_string()], 1.0);

        let roots = builder.build();
        assert_eq!(roots.len(), 1);

        let main = roots.get("main").unwrap();
        assert_eq!(main.children.len(), 2); // foo and qux

        let foo = main.children.get("foo").unwrap();
        assert_eq!(foo.children.len(), 2); // bar and baz
    }

    #[test]
    fn test_collapsed_stacks() {
        let mut builder = FlameGraphBuilder::new();

        builder.add_sample(&["main".to_string(), "foo".to_string()], 1.0);
        builder.add_sample(&["main".to_string(), "foo".to_string()], 1.0);
        builder.add_sample(&["main".to_string(), "bar".to_string()], 1.0);

        let collapsed = builder.to_collapsed_stacks();
        assert!(collapsed.iter().any(|s| s.contains("main;foo 2")));
        assert!(collapsed.iter().any(|s| s.contains("main;bar 1")));
    }

    #[test]
    fn test_task_sample() {
        let mut cpu = CpuProfile::new();
        cpu.set_sample_interval(10.0);

        let sample = TaskSample {
            timestamp: 100.0,
            task_id: TaskId(1),
            pid: Some(Pid(1)),
            state: TaskSampleState::Running,
            stack: vec!["main".to_string()],
        };

        cpu.record_sample(sample);

        assert_eq!(cpu.samples.len(), 1);
        assert!((cpu.task_cpu_percent(TaskId(1)) - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_profiler_summary() {
        let mut profiler = Profiler::new();
        profiler.start(0.0);

        profiler.cpu.record_syscall("open", 1.0, 10.0, false);
        profiler.cpu.record_syscall("read", 1.0, 20.0, false);

        let summary = profiler.summary(100.0);
        assert_eq!(summary.syscall_types, 2);
        assert_eq!(summary.total_syscalls, 2);
        assert!((summary.duration_ms - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_profiler_export_json() {
        let mut profiler = Profiler::new();
        profiler.start(0.0);
        profiler.cpu.record_syscall("open", 1.5, 10.0, false);

        let json = profiler.export_json(100.0);
        assert!(json.contains("\"state\":\"recording\""));
        assert!(json.contains("\"name\":\"open\""));
    }
}
