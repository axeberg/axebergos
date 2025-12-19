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
