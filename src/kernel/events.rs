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
    /// Key pressed
    KeyDown { key: String, code: String, modifiers: Modifiers },
    /// Key released
    KeyUp { key: String, code: String, modifiers: Modifiers },
    /// Window resized
    Resize { width: u32, height: u32 },
    /// Mouse button pressed
    MouseDown { x: f32, y: f32, button: u16 },
    /// Mouse moved
    MouseMove { x: f32, y: f32 },
    /// Mouse button released
    MouseUp { x: f32, y: f32, button: u16 },
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

    /// Peek at all events without consuming them
    pub fn peek_all(&self) -> Vec<Event> {
        self.queue.borrow().iter().cloned().collect()
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new()
    }
}

// Global event queue
thread_local! {
    pub static EVENT_QUEUE: EventQueue = EventQueue::new();
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

/// Peek at all events without consuming them
pub fn peek_events() -> Vec<Event> {
    EVENT_QUEUE.with(|q| q.peek_all())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_queue_push_pop() {
        let queue = EventQueue::new();

        queue.push_input(InputEvent::Resize { width: 800, height: 600 });
        queue.push_input(InputEvent::Resize { width: 1024, height: 768 });

        assert_eq!(queue.len(), 2);
        assert!(!queue.is_empty());

        let event = queue.pop().unwrap();
        match event {
            Event::Input(InputEvent::Resize { width, height }) => {
                assert_eq!(width, 800);
                assert_eq!(height, 600);
            }
            _ => panic!("Wrong event type"),
        }

        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn test_event_queue_drain() {
        let queue = EventQueue::new();

        queue.push_input(InputEvent::Resize { width: 1, height: 1 });
        queue.push_input(InputEvent::Resize { width: 2, height: 2 });
        queue.push_input(InputEvent::Resize { width: 3, height: 3 });

        let events = queue.drain();
        assert_eq!(events.len(), 3);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_event_queue_empty() {
        let queue = EventQueue::new();

        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
        assert!(queue.pop().is_none());
    }

    #[test]
    fn test_event_queue_fifo() {
        let queue = EventQueue::new();

        for i in 0..10 {
            queue.push_system(SystemEvent::Frame { timestamp: i as f64 });
        }

        for i in 0..10 {
            match queue.pop() {
                Some(Event::System(SystemEvent::Frame { timestamp })) => {
                    assert_eq!(timestamp, i as f64);
                }
                _ => panic!("Wrong event"),
            }
        }
    }

    #[test]
    fn test_input_event_types() {
        let queue = EventQueue::new();

        queue.push_input(InputEvent::KeyDown {
            key: "a".to_string(),
            code: "KeyA".to_string(),
            modifiers: Modifiers {
                shift: true,
                ctrl: false,
                alt: false,
                meta: false,
            },
        });
        queue.push_input(InputEvent::Resize {
            width: 1920,
            height: 1080,
        });

        assert_eq!(queue.len(), 2);

        // Verify key down
        match queue.pop() {
            Some(Event::Input(InputEvent::KeyDown { key, modifiers, .. })) => {
                assert_eq!(key, "a");
                assert!(modifiers.shift);
            }
            _ => panic!("Expected KeyDown"),
        }

        // Verify resize
        match queue.pop() {
            Some(Event::Input(InputEvent::Resize { width, height })) => {
                assert_eq!(width, 1920);
                assert_eq!(height, 1080);
            }
            _ => panic!("Expected Resize"),
        }
    }

    #[test]
    fn test_system_events() {
        let queue = EventQueue::new();

        queue.push_system(SystemEvent::TaskCompleted(TaskId(42)));
        queue.push_system(SystemEvent::TaskPanicked(TaskId(99), "oops".to_string()));

        match queue.pop() {
            Some(Event::System(SystemEvent::TaskCompleted(id))) => {
                assert_eq!(id, TaskId(42));
            }
            _ => panic!("Expected TaskCompleted"),
        }

        match queue.pop() {
            Some(Event::System(SystemEvent::TaskPanicked(id, msg))) => {
                assert_eq!(id, TaskId(99));
                assert_eq!(msg, "oops");
            }
            _ => panic!("Expected TaskPanicked"),
        }
    }

    #[test]
    fn test_modifiers_default() {
        let mods = Modifiers::default();
        assert!(!mods.shift);
        assert!(!mods.ctrl);
        assert!(!mods.alt);
        assert!(!mods.meta);
    }
}
