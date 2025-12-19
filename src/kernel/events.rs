//! Event system for user input and system events
//!
//! Events are queued and dispatched to tasks that have subscribed to them.
//! The compositor gets first crack at all input events.

use super::TaskId;
use std::cell::RefCell;
use std::collections::VecDeque;

/// Input events from the browser
#[derive(Debug, Clone)]
pub enum InputEvent {
    /// Mouse moved to position
    MouseMove { x: f64, y: f64 },
    /// Mouse button pressed
    MouseDown { x: f64, y: f64, button: MouseButton },
    /// Mouse button released
    MouseUp { x: f64, y: f64, button: MouseButton },
    /// Key pressed
    KeyDown { key: String, code: String, modifiers: Modifiers },
    /// Key released
    KeyUp { key: String, code: String, modifiers: Modifiers },
    /// Window resized
    Resize { width: u32, height: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Other(u16),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

/// System events (internal)
#[derive(Debug, Clone)]
pub enum SystemEvent {
    /// A task completed
    TaskCompleted(TaskId),
    /// A task panicked
    TaskPanicked(TaskId, String),
    /// Frame tick (from requestAnimationFrame)
    Frame { timestamp: f64 },
}

/// All event types
#[derive(Debug, Clone)]
pub enum Event {
    Input(InputEvent),
    System(SystemEvent),
}

/// Event queue - collects events between ticks
pub struct EventQueue {
    queue: RefCell<VecDeque<Event>>,
}

impl EventQueue {
    pub fn new() -> Self {
        Self {
            queue: RefCell::new(VecDeque::new()),
        }
    }

    /// Push an event onto the queue
    pub fn push(&self, event: Event) {
        self.queue.borrow_mut().push_back(event);
    }

    /// Push an input event
    pub fn push_input(&self, event: InputEvent) {
        self.push(Event::Input(event));
    }

    /// Push a system event
    pub fn push_system(&self, event: SystemEvent) {
        self.push(Event::System(event));
    }

    /// Pop the next event
    pub fn pop(&self) -> Option<Event> {
        self.queue.borrow_mut().pop_front()
    }

    /// Drain all events into a Vec
    pub fn drain(&self) -> Vec<Event> {
        self.queue.borrow_mut().drain(..).collect()
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.queue.borrow().is_empty()
    }

    /// Number of pending events
    pub fn len(&self) -> usize {
        self.queue.borrow().len()
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new()
    }
}

// Global event queue
thread_local! {
    static EVENT_QUEUE: EventQueue = EventQueue::new();
}

/// Push an input event to the global queue
pub fn push_input(event: InputEvent) {
    EVENT_QUEUE.with(|q| q.push_input(event));
}

/// Push a system event to the global queue
pub fn push_system(event: SystemEvent) {
    EVENT_QUEUE.with(|q| q.push_system(event));
}

/// Pop the next event from the global queue
pub fn pop_event() -> Option<Event> {
    EVENT_QUEUE.with(|q| q.pop())
}

/// Drain all events from the global queue
pub fn drain_events() -> Vec<Event> {
    EVENT_QUEUE.with(|q| q.drain())
}

/// Check if there are pending events
pub fn has_events() -> bool {
    EVENT_QUEUE.with(|q| !q.is_empty())
}
