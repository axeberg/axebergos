//! Async executor for cooperative multitasking
//!
//! This is a minimal, single-threaded executor designed for WASM.
//! No work-stealing, no thread pools - just a simple task queue
//! that polls futures until completion.
//!
//! Tractability > Complexity

use super::task::BoxFuture;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::rc::Rc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

/// The executor - runs async tasks cooperatively
pub struct Executor {
    /// Queue of tasks ready to be polled
    ready_queue: Rc<RefCell<VecDeque<BoxFuture>>>,
}

impl Executor {
    pub fn new() -> Self {
        Self {
            ready_queue: Rc::new(RefCell::new(VecDeque::new())),
        }
    }

    /// Spawn a future onto the executor
    pub fn spawn<F>(&mut self, future: F)
    where
        F: Future<Output = ()> + 'static,
    {
        self.ready_queue.borrow_mut().push_back(Box::pin(future));
    }

    /// Run all tasks until the queue is empty
    ///
    /// In WASM, we can't block, so this runs synchronously until
    /// all currently-ready tasks have been polled. Tasks that are
    /// pending on async operations will be re-queued when woken.
    pub fn run(&mut self) {
        loop {
            let task = self.ready_queue.borrow_mut().pop_front();

            match task {
                Some(mut future) => {
                    // Create a waker that re-queues the task
                    let queue = self.ready_queue.clone();
                    let waker = create_waker(queue.clone());
                    let mut cx = Context::from_waker(&waker);

                    match future.as_mut().poll(&mut cx) {
                        Poll::Ready(()) => {
                            // Task completed, don't re-queue
                        }
                        Poll::Pending => {
                            // Task yielded, re-queue for later
                            // In a real system, it would only be re-queued
                            // when its waker is invoked. For now, we re-queue
                            // immediately (busy-wait style).
                            queue.borrow_mut().push_back(future);
                        }
                    }
                }
                None => {
                    // No more tasks
                    break;
                }
            }
        }
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

// Minimal waker implementation for our single-threaded executor
// The waker doesn't need to do anything fancy - tasks are re-queued
// immediately on Pending for now.

fn create_waker(queue: Rc<RefCell<VecDeque<BoxFuture>>>) -> Waker {
    // We leak the Rc here because wakers require 'static lifetime.
    // In a production system, we'd use a more sophisticated approach.
    let ptr = Rc::into_raw(queue) as *const ();
    let raw = RawWaker::new(ptr, &VTABLE);
    unsafe { Waker::from_raw(raw) }
}

const VTABLE: RawWakerVTable = RawWakerVTable::new(clone_fn, wake_fn, wake_by_ref_fn, drop_fn);

unsafe fn clone_fn(ptr: *const ()) -> RawWaker {
    // SAFETY: ptr was created from Rc::into_raw in create_waker
    unsafe {
        let rc = Rc::from_raw(ptr as *const RefCell<VecDeque<BoxFuture>>);
        let cloned = rc.clone();
        let _ = Rc::into_raw(rc); // Don't drop the original
        RawWaker::new(Rc::into_raw(cloned) as *const (), &VTABLE)
    }
}

unsafe fn wake_fn(ptr: *const ()) {
    // SAFETY: ptr was created from Rc::into_raw in create_waker
    // In our simple executor, waking is a no-op since we re-queue on Pending
    unsafe {
        drop(Rc::from_raw(
            ptr as *const RefCell<VecDeque<BoxFuture>>,
        ));
    }
}

unsafe fn wake_by_ref_fn(_ptr: *const ()) {
    // No-op for our simple executor
}

unsafe fn drop_fn(ptr: *const ()) {
    // SAFETY: ptr was created from Rc::into_raw in create_waker
    unsafe {
        drop(Rc::from_raw(
            ptr as *const RefCell<VecDeque<BoxFuture>>,
        ));
    }
}
