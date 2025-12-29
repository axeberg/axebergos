//! Kernel Visualization
//!
//! Provides real-time visualization of kernel state:
//! - Process tree view
//! - Memory usage dashboard
//! - Task scheduler queue
//! - Syscall activity monitor
//! - Resource utilization graphs
//!
//! This module generates visualization data that can be rendered by the compositor
//! or exported for external visualization tools.

use super::executor::Priority;
use super::memory::{MemoryStats, ProcessCowStats};
use super::process::{Pid, ProcessState, RlimitResource};
use super::task::TaskId;
use std::collections::HashMap;

// ============================================================================
// Process Tree Visualization
// ============================================================================

/// A node in the process tree
#[derive(Debug, Clone)]
pub struct ProcessTreeNode {
    /// Process ID
    pub pid: Pid,
    /// Process name
    pub name: String,
    /// Parent PID (None for init)
    pub parent: Option<Pid>,
    /// Process state
    pub state: ProcessState,
    /// User ID
    pub uid: u32,
    /// CPU usage percentage (from profiler)
    pub cpu_percent: f64,
    /// Memory usage in bytes
    pub memory: usize,
    /// Number of open file descriptors
    pub open_fds: usize,
    /// Number of threads/tasks
    pub thread_count: usize,
    /// Child PIDs
    pub children: Vec<Pid>,
}

/// Process tree for visualization
#[derive(Debug, Clone)]
pub struct ProcessTree {
    /// All processes by PID
    pub processes: HashMap<Pid, ProcessTreeNode>,
    /// Root process (typically init, PID 1)
    pub root: Option<Pid>,
    /// Total process count
    pub total_count: usize,
    /// Running process count
    pub running_count: usize,
    /// Sleeping process count
    pub sleeping_count: usize,
    /// Zombie process count
    pub zombie_count: usize,
}

impl ProcessTree {
    /// Create a new empty process tree
    pub fn new() -> Self {
        Self {
            processes: HashMap::new(),
            root: None,
            total_count: 0,
            running_count: 0,
            sleeping_count: 0,
            zombie_count: 0,
        }
    }

    /// Add a process to the tree
    pub fn add_process(&mut self, node: ProcessTreeNode) {
        // Update counters
        match node.state {
            ProcessState::Running => self.running_count += 1,
            ProcessState::Sleeping | ProcessState::Blocked(_) => self.sleeping_count += 1,
            ProcessState::Zombie(_) => self.zombie_count += 1,
            ProcessState::Stopped => {}
        }
        self.total_count += 1;

        // Set root if this is PID 1
        if node.pid.0 == 1 {
            self.root = Some(node.pid);
        }

        // Update parent's children list
        if let Some(parent_pid) = node.parent
            && let Some(parent) = self.processes.get_mut(&parent_pid)
        {
            parent.children.push(node.pid);
        }

        self.processes.insert(node.pid, node);
    }

    /// Get process by PID
    pub fn get(&self, pid: Pid) -> Option<&ProcessTreeNode> {
        self.processes.get(&pid)
    }

    /// Get all children of a process
    pub fn children(&self, pid: Pid) -> Vec<&ProcessTreeNode> {
        self.processes
            .get(&pid)
            .map(|p| {
                p.children
                    .iter()
                    .filter_map(|child_pid| self.processes.get(child_pid))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get depth of a process in the tree (0 for root)
    pub fn depth(&self, pid: Pid) -> usize {
        let mut depth = 0;
        let mut current = pid;
        while let Some(node) = self.processes.get(&current) {
            if let Some(parent) = node.parent {
                depth += 1;
                current = parent;
            } else {
                break;
            }
        }
        depth
    }

    /// Render as ASCII tree
    pub fn render_ascii(&self) -> String {
        let mut output = String::new();
        if let Some(root) = self.root {
            self.render_node_ascii(root, "", true, &mut output);
        }
        output
    }

    fn render_node_ascii(&self, pid: Pid, prefix: &str, is_last: bool, output: &mut String) {
        let Some(node) = self.processes.get(&pid) else {
            return;
        };

        // Draw connector
        let connector = if is_last { "└── " } else { "├── " };

        // Draw node
        let state_char = match &node.state {
            ProcessState::Running => 'R',
            ProcessState::Sleeping => 'S',
            ProcessState::Blocked(_) => 'D',
            ProcessState::Stopped => 'T',
            ProcessState::Zombie(_) => 'Z',
        };

        output.push_str(&format!(
            "{}{}{} [{}] ({}) - {}KB\n",
            prefix,
            connector,
            node.name,
            node.pid.0,
            state_char,
            node.memory / 1024
        ));

        // Draw children
        let child_prefix = if is_last {
            format!("{}    ", prefix)
        } else {
            format!("{}│   ", prefix)
        };

        let children: Vec<_> = node.children.clone();
        for (i, child_pid) in children.iter().enumerate() {
            let is_last_child = i == children.len() - 1;
            self.render_node_ascii(*child_pid, &child_prefix, is_last_child, output);
        }
    }

    /// Get processes sorted by CPU usage
    pub fn by_cpu_usage(&self) -> Vec<&ProcessTreeNode> {
        let mut procs: Vec<_> = self.processes.values().collect();
        procs.sort_by(|a, b| {
            b.cpu_percent
                .partial_cmp(&a.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        procs
    }

    /// Get processes sorted by memory usage
    pub fn by_memory_usage(&self) -> Vec<&ProcessTreeNode> {
        let mut procs: Vec<_> = self.processes.values().collect();
        procs.sort_by(|a, b| b.memory.cmp(&a.memory));
        procs
    }
}

impl Default for ProcessTree {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Memory Visualization
// ============================================================================

/// Memory region type for visualization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryRegionType {
    /// Code/text segment
    Code,
    /// Data segment
    Data,
    /// Heap
    Heap,
    /// Stack
    Stack,
    /// Shared memory
    Shared,
    /// Memory-mapped file
    MappedFile,
    /// Anonymous mapping
    Anonymous,
}

/// A memory region for visualization
#[derive(Debug, Clone)]
pub struct MemoryRegionView {
    /// Region start address (virtual)
    pub start: u64,
    /// Region end address
    pub end: u64,
    /// Region size in bytes
    pub size: usize,
    /// Region type
    pub region_type: MemoryRegionType,
    /// Protection flags as string (e.g., "rwx")
    pub protection: String,
    /// Is this a COW region?
    pub is_cow: bool,
    /// Optional file backing
    pub file: Option<String>,
}

/// Memory layout visualization for a process
#[derive(Debug, Clone)]
pub struct ProcessMemoryLayout {
    /// Process ID
    pub pid: Pid,
    /// Memory regions
    pub regions: Vec<MemoryRegionView>,
    /// Overall statistics
    pub stats: MemoryStats,
    /// COW statistics
    pub cow_stats: Option<ProcessCowStats>,
    /// Total virtual memory size
    pub virtual_size: usize,
    /// Resident set size (actually allocated)
    pub resident_size: usize,
    /// Shared memory size
    pub shared_size: usize,
}

impl ProcessMemoryLayout {
    /// Render as ASCII memory map
    pub fn render_ascii(&self) -> String {
        let mut output = String::new();
        output.push_str(&format!("=== Memory Map for PID {} ===\n", self.pid.0));
        output.push_str(&format!(
            "Virtual: {} KB  Resident: {} KB  Shared: {} KB\n\n",
            self.virtual_size / 1024,
            self.resident_size / 1024,
            self.shared_size / 1024
        ));

        output.push_str("Address Range          Size       Type      Prot   Flags\n");
        output.push_str("─────────────────────────────────────────────────────────\n");

        for region in &self.regions {
            let type_str = match region.region_type {
                MemoryRegionType::Code => "code  ",
                MemoryRegionType::Data => "data  ",
                MemoryRegionType::Heap => "heap  ",
                MemoryRegionType::Stack => "stack ",
                MemoryRegionType::Shared => "shared",
                MemoryRegionType::MappedFile => "mmap  ",
                MemoryRegionType::Anonymous => "anon  ",
            };

            let flags = if region.is_cow { "COW" } else { "   " };

            output.push_str(&format!(
                "{:08x}-{:08x}  {:>8}  {}  {}   {}\n",
                region.start,
                region.end,
                format_size(region.size),
                type_str,
                region.protection,
                flags
            ));
        }

        if let Some(cow) = &self.cow_stats {
            output.push_str(&format!(
                "\nCOW: {} total pages, {} shared, {} private, {} faults\n",
                cow.total_pages, cow.shared_pages, cow.private_pages, cow.total_cow_faults
            ));
        }

        output
    }
}

/// System-wide memory visualization
#[derive(Debug, Clone)]
pub struct SystemMemoryView {
    /// Total physical memory
    pub total_memory: usize,
    /// Used memory
    pub used_memory: usize,
    /// Free memory
    pub free_memory: usize,
    /// Shared memory total
    pub shared_memory: usize,
    /// Kernel memory usage
    pub kernel_memory: usize,
    /// Per-process memory (top N)
    pub top_processes: Vec<(Pid, String, usize)>,
    /// Usage percentage
    pub usage_percent: f64,
    /// Memory pressure indicator (0.0 = no pressure, 1.0 = critical)
    pub pressure: f64,
}

impl SystemMemoryView {
    /// Render as ASCII bar chart
    pub fn render_bar(&self, width: usize) -> String {
        let filled = (self.usage_percent / 100.0 * width as f64) as usize;
        let empty = width - filled;

        let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));

        format!(
            "Memory: [{}] {:.1}% ({}/{})",
            bar,
            self.usage_percent,
            format_size(self.used_memory),
            format_size(self.total_memory)
        )
    }

    /// Render detailed view
    pub fn render_detailed(&self) -> String {
        let mut output = String::new();
        output.push_str("=== System Memory ===\n");
        output.push_str(&format!(
            "Total:  {} ({} bytes)\n",
            format_size(self.total_memory),
            self.total_memory
        ));
        output.push_str(&format!(
            "Used:   {} ({:.1}%)\n",
            format_size(self.used_memory),
            self.usage_percent
        ));
        output.push_str(&format!("Free:   {}\n", format_size(self.free_memory)));
        output.push_str(&format!("Shared: {}\n", format_size(self.shared_memory)));
        output.push_str(&format!("Kernel: {}\n", format_size(self.kernel_memory)));
        output.push_str(&format!("Pressure: {:.2}\n\n", self.pressure));

        output.push_str("Top Processes by Memory:\n");
        for (pid, name, size) in &self.top_processes {
            output.push_str(&format!(
                "  {:>5} {:16} {}\n",
                pid.0,
                name,
                format_size(*size)
            ));
        }

        output
    }
}

// ============================================================================
// Scheduler Visualization
// ============================================================================

/// Task state for scheduler visualization
#[derive(Debug, Clone)]
pub struct TaskView {
    /// Task ID
    pub task_id: TaskId,
    /// Owning process (if any)
    pub pid: Option<Pid>,
    /// Task name/description
    pub name: String,
    /// Priority level
    pub priority: Priority,
    /// Current state
    pub state: TaskViewState,
    /// Time in current state (ms)
    pub state_time: f64,
    /// Total CPU time consumed (ms)
    pub cpu_time: f64,
}

/// Task state for visualization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskViewState {
    /// Currently executing
    Running,
    /// Ready to run (in queue)
    Ready,
    /// Waiting for I/O or event
    Waiting,
    /// Blocked on synchronization
    Blocked,
}

/// Scheduler queue visualization
#[derive(Debug, Clone)]
pub struct SchedulerView {
    /// Currently running tasks
    pub running: Vec<TaskView>,
    /// Ready queue by priority
    pub ready_critical: Vec<TaskView>,
    pub ready_normal: Vec<TaskView>,
    pub ready_background: Vec<TaskView>,
    /// Waiting/blocked tasks
    pub waiting: Vec<TaskView>,
    /// Total task count
    pub total_tasks: usize,
    /// Scheduler tick count
    pub tick_count: u64,
    /// Average tick duration (ms)
    pub avg_tick_time: f64,
    /// Tasks completed since start
    pub completed_tasks: u64,
}

impl SchedulerView {
    /// Render as ASCII queue visualization
    pub fn render_ascii(&self) -> String {
        let mut output = String::new();
        output.push_str("=== Scheduler State ===\n");
        output.push_str(&format!(
            "Total: {} tasks  |  Ticks: {}  |  Avg tick: {:.2}ms\n\n",
            self.total_tasks, self.tick_count, self.avg_tick_time
        ));

        // Running
        output.push_str("RUNNING:\n");
        if self.running.is_empty() {
            output.push_str("  (idle)\n");
        } else {
            for task in &self.running {
                output.push_str(&format!("  ► {} [{}]\n", task.name, task.task_id.0));
            }
        }

        // Ready queues
        output.push_str("\nREADY QUEUES:\n");
        output.push_str(&format!("  Critical ({}):", self.ready_critical.len()));
        if self.ready_critical.is_empty() {
            output.push_str(" (empty)");
        }
        output.push('\n');
        for task in &self.ready_critical {
            output.push_str(&format!("    • {} [{}]\n", task.name, task.task_id.0));
        }

        output.push_str(&format!("  Normal ({}):", self.ready_normal.len()));
        if self.ready_normal.is_empty() {
            output.push_str(" (empty)");
        }
        output.push('\n');
        for task in self.ready_normal.iter().take(5) {
            output.push_str(&format!("    • {} [{}]\n", task.name, task.task_id.0));
        }
        if self.ready_normal.len() > 5 {
            output.push_str(&format!(
                "    ... and {} more\n",
                self.ready_normal.len() - 5
            ));
        }

        output.push_str(&format!("  Background ({}):", self.ready_background.len()));
        if self.ready_background.is_empty() {
            output.push_str(" (empty)");
        }
        output.push('\n');
        for task in self.ready_background.iter().take(3) {
            output.push_str(&format!("    • {} [{}]\n", task.name, task.task_id.0));
        }
        if self.ready_background.len() > 3 {
            output.push_str(&format!(
                "    ... and {} more\n",
                self.ready_background.len() - 3
            ));
        }

        // Waiting
        output.push_str(&format!("\nWAITING ({}):\n", self.waiting.len()));
        for task in self.waiting.iter().take(5) {
            output.push_str(&format!(
                "  ◦ {} [{}] - {:.0}ms\n",
                task.name, task.task_id.0, task.state_time
            ));
        }
        if self.waiting.len() > 5 {
            output.push_str(&format!("  ... and {} more\n", self.waiting.len() - 5));
        }

        output
    }
}

// ============================================================================
// Resource Visualization
// ============================================================================

/// Resource limit view for a process
#[derive(Debug, Clone)]
pub struct ResourceLimitView {
    /// Resource type
    pub resource: RlimitResource,
    /// Current usage
    pub current: u64,
    /// Soft limit
    pub soft_limit: u64,
    /// Hard limit
    pub hard_limit: u64,
    /// Usage percentage (of soft limit)
    pub usage_percent: f64,
}

/// Resource utilization dashboard
#[derive(Debug, Clone)]
pub struct ResourceDashboard {
    /// CPU utilization (0-100%)
    pub cpu_percent: f64,
    /// Memory utilization (0-100%)
    pub memory_percent: f64,
    /// Open file descriptors / max
    pub fd_usage: (usize, usize),
    /// Process count / max
    pub process_usage: (usize, usize),
    /// I/O bytes read/written per second
    pub io_read_rate: f64,
    pub io_write_rate: f64,
    /// Syscalls per second
    pub syscall_rate: f64,
    /// Active timers
    pub timer_count: usize,
    /// Uptime in seconds
    pub uptime_seconds: f64,
}

impl ResourceDashboard {
    /// Render as ASCII dashboard
    pub fn render_ascii(&self) -> String {
        let mut output = String::new();
        output.push_str("╔════════════════════════════════════════════════╗\n");
        output.push_str("║           SYSTEM RESOURCE DASHBOARD            ║\n");
        output.push_str("╠════════════════════════════════════════════════╣\n");

        // CPU bar
        output.push_str(&format!(
            "║ CPU:     {} {:>5.1}% ║\n",
            Self::render_bar(self.cpu_percent, 28),
            self.cpu_percent
        ));

        // Memory bar
        output.push_str(&format!(
            "║ Memory:  {} {:>5.1}% ║\n",
            Self::render_bar(self.memory_percent, 28),
            self.memory_percent
        ));

        // FD bar
        let fd_percent = if self.fd_usage.1 > 0 {
            (self.fd_usage.0 as f64 / self.fd_usage.1 as f64) * 100.0
        } else {
            0.0
        };
        output.push_str(&format!(
            "║ FDs:     {} {:>4}/{:<4}║\n",
            Self::render_bar(fd_percent, 28),
            self.fd_usage.0,
            self.fd_usage.1
        ));

        output.push_str("╠════════════════════════════════════════════════╣\n");

        // I/O rates
        output.push_str(&format!(
            "║ I/O Read:  {:>10}/s   Write: {:>10}/s ║\n",
            format_size(self.io_read_rate as usize),
            format_size(self.io_write_rate as usize)
        ));

        // Syscall rate
        output.push_str(&format!(
            "║ Syscalls: {:>8.1}/s   Timers: {:>12}  ║\n",
            self.syscall_rate, self.timer_count
        ));

        // Processes
        output.push_str(&format!(
            "║ Processes: {:>4}/{:<4}   Uptime: {:>12.1}s ║\n",
            self.process_usage.0, self.process_usage.1, self.uptime_seconds
        ));

        output.push_str("╚════════════════════════════════════════════════╝\n");

        output
    }

    fn render_bar(percent: f64, width: usize) -> String {
        let filled = ((percent / 100.0) * width as f64) as usize;
        let empty = width.saturating_sub(filled);
        format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
    }
}

// ============================================================================
// Syscall Activity Visualization
// ============================================================================

/// Syscall activity entry
#[derive(Debug, Clone)]
pub struct SyscallActivity {
    /// Timestamp (ms)
    pub timestamp: f64,
    /// Process ID
    pub pid: Pid,
    /// Process name
    pub process_name: String,
    /// Syscall name
    pub syscall: String,
    /// Duration (ms)
    pub duration: f64,
    /// Success or error
    pub success: bool,
    /// Brief result description
    pub result: String,
}

/// Syscall activity monitor
#[derive(Debug, Clone)]
pub struct SyscallMonitor {
    /// Recent syscall activity
    pub recent: Vec<SyscallActivity>,
    /// Syscall counts by name
    pub counts: HashMap<String, u64>,
    /// Error counts by syscall
    pub errors: HashMap<String, u64>,
    /// Total syscalls in monitoring window
    pub total: u64,
    /// Monitoring window start time
    pub window_start: f64,
}

impl SyscallMonitor {
    /// Render as activity log
    pub fn render_log(&self, max_entries: usize) -> String {
        let mut output = String::new();
        output.push_str("=== Recent Syscall Activity ===\n");
        output.push_str("Time       PID   Process          Syscall      Duration  Result\n");
        output.push_str("────────────────────────────────────────────────────────────────\n");

        for activity in self.recent.iter().rev().take(max_entries) {
            let status = if activity.success { "OK" } else { "ERR" };
            output.push_str(&format!(
                "{:>9.1}  {:>4}  {:16} {:12} {:>7.2}ms  {}\n",
                activity.timestamp,
                activity.pid.0,
                truncate(&activity.process_name, 16),
                activity.syscall,
                activity.duration,
                status
            ));
        }

        output
    }

    /// Render syscall frequency table
    pub fn render_frequency(&self) -> String {
        let mut output = String::new();
        output.push_str("=== Syscall Frequency ===\n");
        output.push_str("Syscall          Count    Errors   Error%\n");
        output.push_str("──────────────────────────────────────────\n");

        let mut counts: Vec<_> = self.counts.iter().collect();
        counts.sort_by(|a, b| b.1.cmp(a.1));

        for (name, count) in counts.iter().take(15) {
            let errors = self.errors.get(*name).copied().unwrap_or(0);
            let error_pct = if **count > 0 {
                (errors as f64 / **count as f64) * 100.0
            } else {
                0.0
            };
            output.push_str(&format!(
                "{:16} {:>6}   {:>6}   {:>5.1}%\n",
                name, count, errors, error_pct
            ));
        }

        output
    }
}

// ============================================================================
// Visualization Update/Snapshot
// ============================================================================

/// Complete kernel visualization snapshot
#[derive(Debug, Clone)]
pub struct KernelSnapshot {
    /// Timestamp when snapshot was taken
    pub timestamp: f64,
    /// Process tree
    pub process_tree: ProcessTree,
    /// System memory view
    pub memory: SystemMemoryView,
    /// Scheduler state
    pub scheduler: SchedulerView,
    /// Resource dashboard
    pub resources: ResourceDashboard,
    /// Syscall monitor
    pub syscalls: SyscallMonitor,
}

impl KernelSnapshot {
    /// Render full dashboard
    pub fn render_full(&self) -> String {
        let mut output = String::new();
        output.push_str(&format!("Kernel Snapshot @ {:.1}ms\n", self.timestamp));
        output.push_str("═══════════════════════════════════════════════════════════\n\n");

        output.push_str(&self.resources.render_ascii());
        output.push('\n');
        output.push_str(&self.memory.render_bar(40));
        output.push_str("\n\n");
        output.push_str(&self.process_tree.render_ascii());
        output.push('\n');
        output.push_str(&self.scheduler.render_ascii());

        output
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Format a size in human-readable form
fn format_size(bytes: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = KB * 1024;
    const GB: usize = MB * 1024;

    if bytes >= GB {
        format!("{:.1}GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}KB", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}

/// Truncate string to max length
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_tree() {
        let mut tree = ProcessTree::new();

        tree.add_process(ProcessTreeNode {
            pid: Pid(1),
            name: "init".to_string(),
            parent: None,
            state: ProcessState::Running,
            uid: 0,
            cpu_percent: 0.5,
            memory: 4096,
            open_fds: 3,
            thread_count: 1,
            children: vec![],
        });

        tree.add_process(ProcessTreeNode {
            pid: Pid(2),
            name: "bash".to_string(),
            parent: Some(Pid(1)),
            state: ProcessState::Sleeping,
            uid: 1000,
            cpu_percent: 0.1,
            memory: 8192,
            open_fds: 5,
            thread_count: 1,
            children: vec![],
        });

        assert_eq!(tree.total_count, 2);
        assert_eq!(tree.running_count, 1);
        assert_eq!(tree.sleeping_count, 1);
        assert_eq!(tree.root, Some(Pid(1)));

        let depth = tree.depth(Pid(2));
        assert_eq!(depth, 1);
    }

    #[test]
    fn test_process_tree_render() {
        let mut tree = ProcessTree::new();

        tree.add_process(ProcessTreeNode {
            pid: Pid(1),
            name: "init".to_string(),
            parent: None,
            state: ProcessState::Running,
            uid: 0,
            cpu_percent: 0.0,
            memory: 1024,
            open_fds: 3,
            thread_count: 1,
            children: vec![],
        });

        let output = tree.render_ascii();
        assert!(output.contains("init"));
        assert!(output.contains("[1]"));
    }

    #[test]
    fn test_system_memory_view() {
        let view = SystemMemoryView {
            total_memory: 1024 * 1024 * 100, // 100MB
            used_memory: 1024 * 1024 * 50,   // 50MB
            free_memory: 1024 * 1024 * 50,
            shared_memory: 1024 * 1024,
            kernel_memory: 1024 * 1024 * 5,
            top_processes: vec![],
            usage_percent: 50.0,
            pressure: 0.2,
        };

        let bar = view.render_bar(20);
        assert!(bar.contains("50.0%"));
        assert!(bar.contains("█"));
        assert!(bar.contains("░"));
    }

    #[test]
    fn test_scheduler_view_render() {
        let view = SchedulerView {
            running: vec![TaskView {
                task_id: TaskId(1),
                pid: Some(Pid(1)),
                name: "init".to_string(),
                priority: Priority::Normal,
                state: TaskViewState::Running,
                state_time: 100.0,
                cpu_time: 500.0,
            }],
            ready_critical: vec![],
            ready_normal: vec![],
            ready_background: vec![],
            waiting: vec![],
            total_tasks: 1,
            tick_count: 1000,
            avg_tick_time: 0.5,
            completed_tasks: 50,
        };

        let output = view.render_ascii();
        assert!(output.contains("init"));
        assert!(output.contains("RUNNING"));
    }

    #[test]
    fn test_resource_dashboard() {
        let dashboard = ResourceDashboard {
            cpu_percent: 25.0,
            memory_percent: 50.0,
            fd_usage: (100, 1024),
            process_usage: (10, 1024),
            io_read_rate: 1024.0 * 100.0,
            io_write_rate: 1024.0 * 50.0,
            syscall_rate: 1000.0,
            timer_count: 5,
            uptime_seconds: 3600.0,
        };

        let output = dashboard.render_ascii();
        assert!(output.contains("CPU:"));
        assert!(output.contains("Memory:"));
        assert!(output.contains("25.0%"));
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500B");
        assert_eq!(format_size(1024), "1.0KB");
        assert_eq!(format_size(1536), "1.5KB");
        assert_eq!(format_size(1024 * 1024), "1.0MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0GB");
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello w…");
    }

    #[test]
    fn test_syscall_monitor() {
        let monitor = SyscallMonitor {
            recent: vec![
                SyscallActivity {
                    timestamp: 100.0,
                    pid: Pid(1),
                    process_name: "test".to_string(),
                    syscall: "open".to_string(),
                    duration: 0.5,
                    success: true,
                    result: "fd=3".to_string(),
                },
                SyscallActivity {
                    timestamp: 101.0,
                    pid: Pid(1),
                    process_name: "test".to_string(),
                    syscall: "read".to_string(),
                    duration: 0.1,
                    success: true,
                    result: "100 bytes".to_string(),
                },
            ],
            counts: [("open".to_string(), 10), ("read".to_string(), 50)]
                .into_iter()
                .collect(),
            errors: [("open".to_string(), 2)].into_iter().collect(),
            total: 60,
            window_start: 0.0,
        };

        let log = monitor.render_log(10);
        assert!(log.contains("open"));
        assert!(log.contains("read"));

        let freq = monitor.render_frequency();
        assert!(freq.contains("read"));
        assert!(freq.contains("50"));
    }
}
