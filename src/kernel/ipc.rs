//! Inter-process communication via channels
//!
//! Simple MPSC channels for task communication. No fancy lock-free
//! algorithms - just a RefCell-wrapped VecDeque. Tractable > Clever.
//!
//! Provides both unbounded and bounded channels:
//! - `channel<T>()` - unbounded, grows without limit
//! - `bounded_channel<T>(cap)` - bounded, provides back-pressure

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

/// Create a new unbounded channel pair
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Rc::new(RefCell::new(ChannelInner {
        queue: VecDeque::new(),
        closed: false,
    }));

    (
        Sender {
            inner: inner.clone(),
        },
        Receiver { inner },
    )
}

/// Create a new bounded channel pair with specified capacity
///
/// When the channel is full, `try_send` returns `TrySendError::Full`,
/// and `send` yields until space is available.
pub fn bounded_channel<T>(capacity: usize) -> (BoundedSender<T>, BoundedReceiver<T>) {
    assert!(capacity > 0, "capacity must be at least 1");

    let inner = Rc::new(RefCell::new(BoundedChannelInner {
        queue: VecDeque::with_capacity(capacity),
        capacity,
        closed: false,
    }));

    (
        BoundedSender {
            inner: inner.clone(),
        },
        BoundedReceiver { inner },
    )
}

struct ChannelInner<T> {
    queue: VecDeque<T>,
    closed: bool,
}

struct BoundedChannelInner<T> {
    queue: VecDeque<T>,
    capacity: usize,
    closed: bool,
}

/// Sending half of a channel
pub struct Sender<T> {
    inner: Rc<RefCell<ChannelInner<T>>>,
}

impl<T> Sender<T> {
    /// Send a value into the channel
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        let mut inner = self.inner.borrow_mut();
        if inner.closed {
            return Err(SendError(value));
        }
        inner.queue.push_back(value);
        Ok(())
    }

    /// Close the sending side
    pub fn close(&self) {
        self.inner.borrow_mut().closed = true;
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// Receiving half of a channel
pub struct Receiver<T> {
    inner: Rc<RefCell<ChannelInner<T>>>,
}

impl<T> Receiver<T> {
    /// Try to receive a value without blocking
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        let mut inner = self.inner.borrow_mut();
        match inner.queue.pop_front() {
            Some(value) => Ok(value),
            None if inner.closed => Err(TryRecvError::Closed),
            None => Err(TryRecvError::Empty),
        }
    }

    /// Receive a value, yielding if none available
    pub fn recv(&self) -> RecvFuture<'_, T> {
        RecvFuture { receiver: self }
    }
}

/// Future for async receive
pub struct RecvFuture<'a, T> {
    receiver: &'a Receiver<T>,
}

impl<T> Future for RecvFuture<'_, T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.receiver.try_recv() {
            Ok(value) => Poll::Ready(Some(value)),
            Err(TryRecvError::Closed) => Poll::Ready(None),
            Err(TryRecvError::Empty) => {
                // In a real system, we'd register the waker here
                // For now, we yield and let the executor re-poll
                Poll::Pending
            }
        }
    }
}

/// Error when sending fails
#[derive(Debug)]
pub struct SendError<T>(pub T);

/// Error when try_recv fails
#[derive(Debug, PartialEq, Eq)]
pub enum TryRecvError {
    Empty,
    Closed,
}

// ============================================================================
// Bounded Channel Implementation
// ============================================================================

/// Sending half of a bounded channel
pub struct BoundedSender<T> {
    inner: Rc<RefCell<BoundedChannelInner<T>>>,
}

impl<T> BoundedSender<T> {
    /// Try to send a value without blocking
    ///
    /// Returns `TrySendError::Full` if the channel is at capacity.
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        let mut inner = self.inner.borrow_mut();
        if inner.closed {
            return Err(TrySendError::Closed(value));
        }
        if inner.queue.len() >= inner.capacity {
            return Err(TrySendError::Full(value));
        }
        inner.queue.push_back(value);
        Ok(())
    }

    /// Send a value, yielding if the channel is full
    ///
    /// This is an async method that will yield until space is available.
    pub fn send(&self, value: T) -> BoundedSendFuture<'_, T> {
        BoundedSendFuture {
            sender: self,
            value: Some(value),
        }
    }

    /// Close the sending side
    pub fn close(&self) {
        self.inner.borrow_mut().closed = true;
    }

    /// Check if the channel is full
    pub fn is_full(&self) -> bool {
        let inner = self.inner.borrow();
        inner.queue.len() >= inner.capacity
    }

    /// Get the number of items currently in the channel
    pub fn len(&self) -> usize {
        self.inner.borrow().queue.len()
    }

    /// Check if the channel is empty
    pub fn is_empty(&self) -> bool {
        self.inner.borrow().queue.is_empty()
    }

    /// Get the capacity of the channel
    pub fn capacity(&self) -> usize {
        self.inner.borrow().capacity
    }
}

impl<T> Clone for BoundedSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// Future for async bounded send
pub struct BoundedSendFuture<'a, T> {
    sender: &'a BoundedSender<T>,
    value: Option<T>,
}

impl<T> Unpin for BoundedSendFuture<'_, T> {}

impl<T> Future for BoundedSendFuture<'_, T> {
    type Output = Result<(), SendError<T>>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let value = this.value.take().expect("polled after completion");

        match this.sender.try_send(value) {
            Ok(()) => Poll::Ready(Ok(())),
            Err(TrySendError::Closed(v)) => Poll::Ready(Err(SendError(v))),
            Err(TrySendError::Full(v)) => {
                // Put the value back and yield
                this.value = Some(v);
                Poll::Pending
            }
        }
    }
}

/// Receiving half of a bounded channel
pub struct BoundedReceiver<T> {
    inner: Rc<RefCell<BoundedChannelInner<T>>>,
}

impl<T> BoundedReceiver<T> {
    /// Try to receive a value without blocking
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        let mut inner = self.inner.borrow_mut();
        match inner.queue.pop_front() {
            Some(value) => Ok(value),
            None if inner.closed => Err(TryRecvError::Closed),
            None => Err(TryRecvError::Empty),
        }
    }

    /// Receive a value, yielding if none available
    pub fn recv(&self) -> BoundedRecvFuture<'_, T> {
        BoundedRecvFuture { receiver: self }
    }

    /// Get the number of items currently in the channel
    pub fn len(&self) -> usize {
        self.inner.borrow().queue.len()
    }

    /// Check if the channel is empty
    pub fn is_empty(&self) -> bool {
        self.inner.borrow().queue.is_empty()
    }

    /// Get the capacity of the channel
    pub fn capacity(&self) -> usize {
        self.inner.borrow().capacity
    }
}

/// Future for async bounded receive
pub struct BoundedRecvFuture<'a, T> {
    receiver: &'a BoundedReceiver<T>,
}

impl<T> Future for BoundedRecvFuture<'_, T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.receiver.try_recv() {
            Ok(value) => Poll::Ready(Some(value)),
            Err(TryRecvError::Closed) => Poll::Ready(None),
            Err(TryRecvError::Empty) => Poll::Pending,
        }
    }
}

/// Error when try_send fails on a bounded channel
#[derive(Debug, PartialEq, Eq)]
pub enum TrySendError<T> {
    /// Channel is at capacity
    Full(T),
    /// Channel is closed
    Closed(T),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_receive() {
        let (tx, rx) = channel::<i32>();

        tx.send(42).unwrap();
        tx.send(43).unwrap();

        assert_eq!(rx.try_recv(), Ok(42));
        assert_eq!(rx.try_recv(), Ok(43));
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn test_empty_channel() {
        let (_tx, rx) = channel::<i32>();
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn test_closed_channel() {
        let (tx, rx) = channel::<i32>();

        tx.send(1).unwrap();
        tx.close();

        // Can still receive what was sent before close
        assert_eq!(rx.try_recv(), Ok(1));
        // Then get Closed
        assert_eq!(rx.try_recv(), Err(TryRecvError::Closed));
    }

    #[test]
    fn test_send_after_close_fails() {
        let (tx, _rx) = channel::<i32>();

        tx.close();
        let result = tx.send(42);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().0, 42); // Get the value back
    }

    #[test]
    fn test_clone_sender() {
        let (tx1, rx) = channel::<i32>();
        let tx2 = tx1.clone();

        tx1.send(1).unwrap();
        tx2.send(2).unwrap();

        assert_eq!(rx.try_recv(), Ok(1));
        assert_eq!(rx.try_recv(), Ok(2));
    }

    #[test]
    fn test_fifo_order() {
        let (tx, rx) = channel::<i32>();

        for i in 0..100 {
            tx.send(i).unwrap();
        }

        for i in 0..100 {
            assert_eq!(rx.try_recv(), Ok(i));
        }
    }

    #[test]
    fn test_channel_with_strings() {
        let (tx, rx) = channel::<String>();

        tx.send("hello".to_string()).unwrap();
        tx.send("world".to_string()).unwrap();

        assert_eq!(rx.try_recv(), Ok("hello".to_string()));
        assert_eq!(rx.try_recv(), Ok("world".to_string()));
    }

    // ========================================================================
    // Bounded Channel Tests
    // ========================================================================

    #[test]
    fn test_bounded_send_receive() {
        let (tx, rx) = bounded_channel::<i32>(10);

        tx.try_send(42).unwrap();
        tx.try_send(43).unwrap();

        assert_eq!(rx.try_recv(), Ok(42));
        assert_eq!(rx.try_recv(), Ok(43));
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn test_bounded_capacity() {
        let (tx, rx) = bounded_channel::<i32>(3);

        assert_eq!(tx.capacity(), 3);
        assert_eq!(rx.capacity(), 3);
        assert!(!tx.is_full());
        assert!(tx.is_empty());
    }

    #[test]
    fn test_bounded_full() {
        let (tx, _rx) = bounded_channel::<i32>(2);

        // Fill the channel
        tx.try_send(1).unwrap();
        tx.try_send(2).unwrap();

        assert!(tx.is_full());
        assert_eq!(tx.len(), 2);

        // Third send should fail with Full
        let result = tx.try_send(3);
        assert!(matches!(result, Err(TrySendError::Full(3))));
    }

    #[test]
    fn test_bounded_full_then_recv_makes_space() {
        let (tx, rx) = bounded_channel::<i32>(2);

        tx.try_send(1).unwrap();
        tx.try_send(2).unwrap();
        assert!(tx.is_full());

        // Receive one - makes space
        assert_eq!(rx.try_recv(), Ok(1));
        assert!(!tx.is_full());
        assert_eq!(tx.len(), 1);

        // Now we can send again
        tx.try_send(3).unwrap();
        assert!(tx.is_full());

        assert_eq!(rx.try_recv(), Ok(2));
        assert_eq!(rx.try_recv(), Ok(3));
    }

    #[test]
    fn test_bounded_closed() {
        let (tx, rx) = bounded_channel::<i32>(10);

        tx.try_send(1).unwrap();
        tx.close();

        // Can still receive what was sent before close
        assert_eq!(rx.try_recv(), Ok(1));
        // Then get Closed
        assert_eq!(rx.try_recv(), Err(TryRecvError::Closed));
    }

    #[test]
    fn test_bounded_send_after_close() {
        let (tx, _rx) = bounded_channel::<i32>(10);

        tx.close();
        let result = tx.try_send(42);

        assert!(matches!(result, Err(TrySendError::Closed(42))));
    }

    #[test]
    fn test_bounded_clone_sender() {
        let (tx1, rx) = bounded_channel::<i32>(10);
        let tx2 = tx1.clone();

        tx1.try_send(1).unwrap();
        tx2.try_send(2).unwrap();

        assert_eq!(rx.try_recv(), Ok(1));
        assert_eq!(rx.try_recv(), Ok(2));
    }

    #[test]
    fn test_bounded_fifo_order() {
        let (tx, rx) = bounded_channel::<i32>(100);

        for i in 0..100 {
            tx.try_send(i).unwrap();
        }

        for i in 0..100 {
            assert_eq!(rx.try_recv(), Ok(i));
        }
    }

    #[test]
    #[should_panic(expected = "capacity must be at least 1")]
    fn test_bounded_zero_capacity_panics() {
        let _ = bounded_channel::<i32>(0);
    }

    #[test]
    fn test_bounded_capacity_one() {
        let (tx, rx) = bounded_channel::<i32>(1);

        tx.try_send(1).unwrap();
        assert!(tx.is_full());

        assert!(matches!(tx.try_send(2), Err(TrySendError::Full(2))));

        assert_eq!(rx.try_recv(), Ok(1));
        assert!(!tx.is_full());

        tx.try_send(2).unwrap();
        assert_eq!(rx.try_recv(), Ok(2));
    }
}
