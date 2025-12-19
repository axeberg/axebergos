//! Inter-process communication via channels
//!
//! Simple MPSC channels for task communication. No fancy lock-free
//! algorithms - just a RefCell-wrapped VecDeque. Tractable > Clever.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

/// Create a new channel pair
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

struct ChannelInner<T> {
    queue: VecDeque<T>,
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
}
