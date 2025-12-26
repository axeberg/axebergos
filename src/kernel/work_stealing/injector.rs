//! Lock-free multi-producer multi-consumer injector queue
//!
//! Used for external task submission to the work stealing executor.
//! Tasks injected here are distributed to worker threads.
//!
//! Implementation: Simple MPMC queue using a locked VecDeque for correctness,
//! with future optimization path to lock-free (e.g., Michael-Scott queue).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// A global injector queue for external task submission
///
/// Thread-safe queue that multiple producers can push to and multiple
/// workers can pull from. This is the entry point for tasks spawned
/// from outside any worker thread.
pub struct Injector<T> {
    inner: Arc<InjectorInner<T>>,
}

struct InjectorInner<T> {
    queue: Mutex<VecDeque<T>>,
}

/// Result of a steal operation from the injector
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectResult<T> {
    /// Successfully retrieved a task
    Success(T),
    /// Queue was empty
    Empty,
}

impl<T> Injector<T> {
    /// Create a new empty injector queue
    pub fn new() -> Self {
        Injector {
            inner: Arc::new(InjectorInner {
                queue: Mutex::new(VecDeque::new()),
            }),
        }
    }

    /// Push a task to the injector queue
    ///
    /// Thread-safe, can be called from any thread.
    pub fn push(&self, task: T) {
        let mut queue = self.inner.queue.lock().unwrap();
        queue.push_back(task);
    }

    /// Try to steal a task from the injector queue
    ///
    /// Returns `InjectResult::Empty` if the queue is empty.
    pub fn steal(&self) -> InjectResult<T> {
        let mut queue = self.inner.queue.lock().unwrap();
        match queue.pop_front() {
            Some(task) => InjectResult::Success(task),
            None => InjectResult::Empty,
        }
    }

    /// Try to steal a batch of tasks, pushing them to a local worker deque
    ///
    /// This is more efficient than stealing one at a time as it amortizes
    /// the lock acquisition cost. Returns the number of tasks stolen.
    pub fn steal_batch<F>(&self, max: usize, mut push_fn: F) -> usize
    where
        F: FnMut(T),
    {
        let mut queue = self.inner.queue.lock().unwrap();
        let count = queue.len().min(max);

        for _ in 0..count {
            if let Some(task) = queue.pop_front() {
                push_fn(task);
            }
        }

        count
    }

    /// Check if the injector is empty
    pub fn is_empty(&self) -> bool {
        let queue = self.inner.queue.lock().unwrap();
        queue.is_empty()
    }

    /// Get the current length
    pub fn len(&self) -> usize {
        let queue = self.inner.queue.lock().unwrap();
        queue.len()
    }
}

impl<T> Default for Injector<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Clone for Injector<T> {
    fn clone(&self) -> Self {
        Injector {
            inner: self.inner.clone(),
        }
    }
}

// Safety: Thread safety provided by Mutex
unsafe impl<T: Send> Send for Injector<T> {}
unsafe impl<T: Send> Sync for Injector<T> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_steal() {
        let injector = Injector::new();

        injector.push(1);
        injector.push(2);
        injector.push(3);

        assert_eq!(injector.steal(), InjectResult::Success(1));
        assert_eq!(injector.steal(), InjectResult::Success(2));
        assert_eq!(injector.steal(), InjectResult::Success(3));
        assert_eq!(injector.steal(), InjectResult::Empty);
    }

    #[test]
    fn test_steal_batch() {
        let injector = Injector::new();

        for i in 0..10 {
            injector.push(i);
        }

        let mut collected = Vec::new();
        let count = injector.steal_batch(5, |t| collected.push(t));

        assert_eq!(count, 5);
        assert_eq!(collected, vec![0, 1, 2, 3, 4]);
        assert_eq!(injector.len(), 5);
    }

    #[test]
    fn test_is_empty() {
        let injector: Injector<i32> = Injector::new();
        assert!(injector.is_empty());

        injector.push(42);
        assert!(!injector.is_empty());

        injector.steal();
        assert!(injector.is_empty());
    }

    #[test]
    fn test_clone_shares_state() {
        let injector1 = Injector::new();
        let injector2 = injector1.clone();

        injector1.push(1);
        injector2.push(2);

        assert_eq!(injector1.len(), 2);
        assert_eq!(injector1.steal(), InjectResult::Success(1));
        assert_eq!(injector2.steal(), InjectResult::Success(2));
    }
}
