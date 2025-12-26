//! Timer System
//!
//! Provides timers for delayed execution and sleep functionality.
//!
//! Design:
//! - TimerQueue is a min-heap sorted by deadline
//! - Each timer can wake a task when it expires
//! - Integrates with the executor's tick loop
//! - Time comes from browser (performance.now / rAF timestamp)

use super::task::TaskId;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Unique identifier for a timer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimerId(pub u64);

/// Timer state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerState {
    /// Timer is active and waiting
    Pending,
    /// Timer has fired
    Fired,
    /// Timer was cancelled
    Cancelled,
}

/// A timer that fires at a specific deadline
#[derive(Debug)]
pub struct Timer {
    /// Unique identifier
    pub id: TimerId,
    /// When this timer should fire (monotonic milliseconds)
    pub deadline: f64,
    /// Task to wake when timer fires (if any)
    pub wake_task: Option<TaskId>,
    /// Current state
    pub state: TimerState,
    /// Whether this is a repeating timer
    pub interval: Option<f64>,
}

impl Timer {
    /// Create a one-shot timer
    pub fn oneshot(id: TimerId, deadline: f64, wake_task: Option<TaskId>) -> Self {
        Self {
            id,
            deadline,
            wake_task,
            state: TimerState::Pending,
            interval: None,
        }
    }

    /// Create a repeating interval timer
    pub fn interval(id: TimerId, deadline: f64, interval_ms: f64, wake_task: Option<TaskId>) -> Self {
        Self {
            id,
            deadline,
            wake_task,
            state: TimerState::Pending,
            interval: Some(interval_ms),
        }
    }

    /// Check if timer has expired
    pub fn is_expired(&self, now: f64) -> bool {
        self.state == TimerState::Pending && now >= self.deadline
    }

    /// Fire the timer, returning the task to wake (if any)
    pub fn fire(&mut self) -> Option<TaskId> {
        if self.state == TimerState::Pending {
            self.state = TimerState::Fired;
            self.wake_task
        } else {
            None
        }
    }

    /// Reset for next interval (returns true if repeating)
    pub fn reset_interval(&mut self, now: f64) -> bool {
        if let Some(interval) = self.interval {
            self.deadline = now + interval;
            self.state = TimerState::Pending;
            true
        } else {
            false
        }
    }

    /// Cancel the timer
    pub fn cancel(&mut self) {
        self.state = TimerState::Cancelled;
    }
}

/// Entry in the timer heap (for ordering)
#[derive(Debug)]
struct TimerEntry {
    deadline: f64,
    id: TimerId,
}

impl PartialEq for TimerEntry {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for TimerEntry {}

impl PartialOrd for TimerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TimerEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap (smallest deadline first)
        other.deadline.partial_cmp(&self.deadline).unwrap_or(Ordering::Equal)
    }
}

/// Timer queue - manages all active timers
#[derive(Debug)]
pub struct TimerQueue {
    /// Min-heap of timer entries
    heap: BinaryHeap<TimerEntry>,
    /// All timers by ID
    timers: std::collections::HashMap<TimerId, Timer>,
    /// Next timer ID
    next_id: u64,
}

impl TimerQueue {
    /// Create a new timer queue
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
            timers: std::collections::HashMap::new(),
            next_id: 1,
        }
    }

    /// Schedule a one-shot timer
    pub fn schedule(&mut self, delay_ms: f64, now: f64, wake_task: Option<TaskId>) -> TimerId {
        let id = TimerId(self.next_id);
        self.next_id += 1;

        let deadline = now + delay_ms;
        let timer = Timer::oneshot(id, deadline, wake_task);

        self.heap.push(TimerEntry { deadline, id });
        self.timers.insert(id, timer);

        id
    }

    /// Schedule a repeating interval timer
    pub fn schedule_interval(
        &mut self,
        interval_ms: f64,
        now: f64,
        wake_task: Option<TaskId>,
    ) -> TimerId {
        let id = TimerId(self.next_id);
        self.next_id += 1;

        let deadline = now + interval_ms;
        let timer = Timer::interval(id, deadline, interval_ms, wake_task);

        self.heap.push(TimerEntry { deadline, id });
        self.timers.insert(id, timer);

        id
    }

    /// Cancel a timer
    /// Returns true if the timer was pending and is now cancelled
    /// Returns false if the timer doesn't exist or was already cancelled/fired
    pub fn cancel(&mut self, id: TimerId) -> bool {
        if let Some(timer) = self.timers.get_mut(&id) {
            if timer.state == TimerState::Pending {
                timer.cancel();
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Process expired timers, returning tasks to wake
    pub fn tick(&mut self, now: f64) -> Vec<TaskId> {
        let mut tasks_to_wake = Vec::new();
        let mut to_reschedule = Vec::new();

        // Process all expired timers
        while let Some(entry) = self.heap.peek() {
            if entry.deadline > now {
                break;
            }

            let entry = self.heap.pop().unwrap();

            if let Some(timer) = self.timers.get_mut(&entry.id)
                && timer.state == TimerState::Pending && timer.is_expired(now) {
                    // Fire the timer
                    if let Some(task_id) = timer.fire() {
                        tasks_to_wake.push(task_id);
                    }

                    // Check if it's an interval timer
                    if timer.reset_interval(now) {
                        to_reschedule.push((timer.id, timer.deadline));
                    }
                }
        }

        // Reschedule interval timers
        for (id, deadline) in to_reschedule {
            self.heap.push(TimerEntry { deadline, id });
        }

        // Clean up fired/cancelled timers
        self.timers.retain(|_, t| t.state == TimerState::Pending);

        tasks_to_wake
    }

    /// Get time until next timer fires (for sleep optimization)
    pub fn time_until_next(&self, now: f64) -> Option<f64> {
        self.heap.peek().map(|entry| (entry.deadline - now).max(0.0))
    }

    /// Number of pending timers
    pub fn pending_count(&self) -> usize {
        self.timers.values().filter(|t| t.state == TimerState::Pending).count()
    }

    /// Get timer info
    pub fn get(&self, id: TimerId) -> Option<&Timer> {
        self.timers.get(&id)
    }

    /// Check if a timer exists and is pending
    pub fn is_pending(&self, id: TimerId) -> bool {
        self.timers.get(&id).map(|t| t.state == TimerState::Pending).unwrap_or(false)
    }
}

impl Default for TimerQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oneshot_timer() {
        let mut queue = TimerQueue::new();
        let task = TaskId(1);

        let timer_id = queue.schedule(100.0, 0.0, Some(task));
        assert!(queue.is_pending(timer_id));
        assert_eq!(queue.pending_count(), 1);

        // Not expired yet
        let woken = queue.tick(50.0);
        assert!(woken.is_empty());
        assert!(queue.is_pending(timer_id));

        // Now expired
        let woken = queue.tick(100.0);
        assert_eq!(woken, vec![task]);
        assert!(!queue.is_pending(timer_id));
    }

    #[test]
    fn test_interval_timer() {
        let mut queue = TimerQueue::new();
        let task = TaskId(1);

        let timer_id = queue.schedule_interval(100.0, 0.0, Some(task));

        // First fire
        let woken = queue.tick(100.0);
        assert_eq!(woken, vec![task]);
        assert!(queue.is_pending(timer_id)); // Still pending (repeating)

        // Second fire
        let woken = queue.tick(200.0);
        assert_eq!(woken, vec![task]);
        assert!(queue.is_pending(timer_id));

        // Cancel
        queue.cancel(timer_id);
        let woken = queue.tick(300.0);
        assert!(woken.is_empty());
    }

    #[test]
    fn test_multiple_timers() {
        let mut queue = TimerQueue::new();

        let t1 = queue.schedule(100.0, 0.0, Some(TaskId(1)));
        let t2 = queue.schedule(50.0, 0.0, Some(TaskId(2)));
        let t3 = queue.schedule(150.0, 0.0, Some(TaskId(3)));

        // First tick - t2 fires
        let woken = queue.tick(50.0);
        assert_eq!(woken, vec![TaskId(2)]);

        // Second tick - t1 fires
        let woken = queue.tick(100.0);
        assert_eq!(woken, vec![TaskId(1)]);

        // Third tick - t3 fires
        let woken = queue.tick(150.0);
        assert_eq!(woken, vec![TaskId(3)]);
    }

    #[test]
    fn test_cancel_timer() {
        let mut queue = TimerQueue::new();
        let task = TaskId(1);

        let timer_id = queue.schedule(100.0, 0.0, Some(task));
        assert!(queue.cancel(timer_id));

        let woken = queue.tick(100.0);
        assert!(woken.is_empty());
    }

    #[test]
    fn test_time_until_next() {
        let mut queue = TimerQueue::new();

        assert!(queue.time_until_next(0.0).is_none());

        queue.schedule(100.0, 0.0, None);
        assert_eq!(queue.time_until_next(0.0), Some(100.0));
        assert_eq!(queue.time_until_next(50.0), Some(50.0));
        assert_eq!(queue.time_until_next(100.0), Some(0.0));
    }

    #[test]
    fn test_timer_without_task() {
        let mut queue = TimerQueue::new();

        // Timer without associated task
        let timer_id = queue.schedule(100.0, 0.0, None);

        let woken = queue.tick(100.0);
        assert!(woken.is_empty()); // No task to wake
        assert!(!queue.is_pending(timer_id)); // But timer still fired
    }

    #[test]
    fn test_multiple_fires_same_tick() {
        let mut queue = TimerQueue::new();

        queue.schedule(50.0, 0.0, Some(TaskId(1)));
        queue.schedule(50.0, 0.0, Some(TaskId(2)));
        queue.schedule(50.0, 0.0, Some(TaskId(3)));

        let woken = queue.tick(50.0);
        assert_eq!(woken.len(), 3);
    }
}
