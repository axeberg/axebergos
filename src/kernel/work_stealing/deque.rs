//! Lock-free Chase-Lev work stealing deque
//!
//! Based on "Dynamic Circular Work-Stealing Deque" by Chase and Lev (2005)
//! with simplifications from "Correct and Efficient Work-Stealing for
//! Weak Memory Models" by Le et al. (2013).
//!
//! Properties (verified in TLA+ spec `specs/tla/WorkStealing.tla`):
//! - Owner pushes/pops from bottom (LIFO) - O(1)
//! - Thieves steal from top (FIFO) - O(1)
//! - Lock-free: no thread can block another indefinitely
//! - ABA-safe: uses generation counters in top index
//!
//! Memory ordering rationale:
//! - bottom: only modified by owner, SeqCst for visibility to stealers
//! - top: modified by stealers via CAS, SeqCst for linearizability
//! - buffer: Relaxed loads/stores, correctness from index synchronization

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Result of a pop or steal operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StealResult<T> {
    /// Successfully retrieved a task
    Success(T),
    /// Queue was empty
    Empty,
    /// Lost race to another stealer (retry may succeed)
    Retry,
}

/// The fixed-size ring buffer backing the deque
///
/// We use a fixed size for simplicity. The Chase-Lev paper uses a
/// growable array, but for OS work we can bound task count.
struct Buffer<T> {
    /// Storage for tasks
    data: Box<[UnsafeCell<MaybeUninit<T>>]>,
    /// Capacity (power of 2 for fast modulo)
    capacity: usize,
    /// Mask for index wrapping (capacity - 1)
    mask: usize,
}

impl<T> Buffer<T> {
    fn new(capacity: usize) -> Self {
        assert!(capacity.is_power_of_two(), "Capacity must be power of 2");
        let data: Vec<_> = (0..capacity)
            .map(|_| UnsafeCell::new(MaybeUninit::uninit()))
            .collect();
        Self {
            data: data.into_boxed_slice(),
            capacity,
            mask: capacity - 1,
        }
    }

    /// Get the slot at index (wrapping)
    #[inline]
    fn slot(&self, index: usize) -> &UnsafeCell<MaybeUninit<T>> {
        // Safety: index is masked to be within bounds
        unsafe { self.data.get_unchecked(index & self.mask) }
    }

    /// Write a value at index
    ///
    /// # Safety
    /// Caller must ensure exclusive write access to this slot
    #[inline]
    unsafe fn write(&self, index: usize, value: T) {
        let slot = self.slot(index);
        // Safety: caller guarantees exclusive access
        unsafe {
            (*slot.get()).write(value);
        }
    }

    /// Read a value at index
    ///
    /// # Safety
    /// Caller must ensure the slot contains a valid value
    /// and no concurrent write is happening
    #[inline]
    unsafe fn read(&self, index: usize) -> T {
        let slot = self.slot(index);
        // Safety: caller guarantees slot is initialized and no concurrent write
        unsafe { (*slot.get()).assume_init_read() }
    }
}

// Buffer itself is not Send/Sync, but our deque manages thread safety
unsafe impl<T: Send> Send for Buffer<T> {}
unsafe impl<T: Send> Sync for Buffer<T> {}

/// Owner's handle to the deque - allows push and pop
pub struct Worker<T> {
    inner: Arc<Inner<T>>,
}

/// Thief's handle to the deque - allows steal only
pub struct Stealer<T> {
    inner: Arc<Inner<T>>,
}

/// Shared deque state
struct Inner<T> {
    /// Bottom index (only modified by owner)
    /// Packed as: [32-bit unused][32-bit bottom]
    bottom: AtomicU64,

    /// Top index with generation counter for ABA prevention
    /// Packed as: [32-bit generation][32-bit top]
    top: AtomicU64,

    /// The ring buffer
    buffer: Buffer<T>,
}

impl<T> Inner<T> {
    /// Extract bottom index from packed value
    #[inline]
    fn unpack_bottom(packed: u64) -> usize {
        packed as u32 as usize
    }

    /// Pack bottom index
    #[inline]
    fn pack_bottom(bottom: usize) -> u64 {
        bottom as u64
    }

    /// Extract (generation, top) from packed value
    #[inline]
    fn unpack_top(packed: u64) -> (u32, usize) {
        let generation = (packed >> 32) as u32;
        let top = packed as u32 as usize;
        (generation, top)
    }

    /// Pack (generation, top) into atomic value
    #[inline]
    fn pack_top(generation: u32, top: usize) -> u64 {
        ((generation as u64) << 32) | (top as u64)
    }
}

impl<T: Send> Worker<T> {
    /// Create a new deque with the given capacity (must be power of 2)
    pub fn new(capacity: usize) -> (Worker<T>, Stealer<T>) {
        let inner = Arc::new(Inner {
            bottom: AtomicU64::new(0),
            top: AtomicU64::new(0),
            buffer: Buffer::new(capacity),
        });

        (
            Worker {
                inner: inner.clone(),
            },
            Stealer { inner },
        )
    }

    /// Push a task onto the bottom of the deque
    ///
    /// Returns `Err(task)` if the deque is full
    pub fn push(&self, task: T) -> Result<(), T> {
        let bottom = Inner::<T>::unpack_bottom(self.inner.bottom.load(Ordering::Relaxed));
        let (_, top) = Inner::<T>::unpack_top(self.inner.top.load(Ordering::Acquire));

        // Check if full
        let size = bottom.wrapping_sub(top);
        if size >= self.inner.buffer.capacity {
            return Err(task);
        }

        // Write task to buffer
        // Safety: we're the only writer to bottom, and this slot is not
        // accessible to stealers until we increment bottom
        unsafe {
            self.inner.buffer.write(bottom, task);
        }

        // Make the task visible to stealers
        // SeqCst ensures the write above is visible before bottom is incremented
        self.inner
            .bottom
            .store(Inner::<T>::pack_bottom(bottom.wrapping_add(1)), Ordering::SeqCst);

        Ok(())
    }

    /// Pop a task from the bottom of the deque (LIFO)
    pub fn pop(&self) -> StealResult<T> {
        // Decrement bottom speculatively
        let old_bottom = Inner::<T>::unpack_bottom(self.inner.bottom.load(Ordering::Relaxed));
        let new_bottom = old_bottom.wrapping_sub(1);
        self.inner
            .bottom
            .store(Inner::<T>::pack_bottom(new_bottom), Ordering::SeqCst);

        // Load top with acquire to see stealer updates
        let packed_top = self.inner.top.load(Ordering::SeqCst);
        let (generation, top) = Inner::<T>::unpack_top(packed_top);

        let size = old_bottom.wrapping_sub(top) as isize;

        if size <= 0 {
            // Deque was empty, restore bottom
            self.inner
                .bottom
                .store(Inner::<T>::pack_bottom(top), Ordering::SeqCst);
            return StealResult::Empty;
        }

        // Read the task
        // Safety: we've decremented bottom, so this slot is ours
        let task = unsafe { self.inner.buffer.read(new_bottom) };

        if size == 1 {
            // Last element - race with stealers
            // Try to claim it by incrementing top
            let new_packed_top = Inner::<T>::pack_top(generation.wrapping_add(1), top.wrapping_add(1));

            if self
                .inner
                .top
                .compare_exchange(packed_top, new_packed_top, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                // We won the race
                self.inner
                    .bottom
                    .store(Inner::<T>::pack_bottom(top.wrapping_add(1)), Ordering::SeqCst);
                StealResult::Success(task)
            } else {
                // A stealer took it, restore bottom
                self.inner
                    .bottom
                    .store(Inner::<T>::pack_bottom(top.wrapping_add(1)), Ordering::SeqCst);
                // Note: we already read the task, but the stealer invalidated it
                // This shouldn't happen in practice as we'd have lost the CAS
                // The task value is already moved out, stealer got nothing
                StealResult::Empty
            }
        } else {
            // More than one element, no race possible
            StealResult::Success(task)
        }
    }

    /// Check if the deque is empty
    pub fn is_empty(&self) -> bool {
        let bottom = Inner::<T>::unpack_bottom(self.inner.bottom.load(Ordering::Relaxed));
        let (_, top) = Inner::<T>::unpack_top(self.inner.top.load(Ordering::Acquire));
        bottom.wrapping_sub(top) == 0
    }

    /// Get approximate length (may be stale)
    pub fn len(&self) -> usize {
        let bottom = Inner::<T>::unpack_bottom(self.inner.bottom.load(Ordering::Relaxed));
        let (_, top) = Inner::<T>::unpack_top(self.inner.top.load(Ordering::Acquire));
        bottom.wrapping_sub(top)
    }

    /// Get a stealer handle for this deque
    pub fn stealer(&self) -> Stealer<T> {
        Stealer {
            inner: self.inner.clone(),
        }
    }
}

impl<T: Send> Stealer<T> {
    /// Steal a task from the top of the deque (FIFO)
    pub fn steal(&self) -> StealResult<T> {
        // Load top first
        let packed_top = self.inner.top.load(Ordering::Acquire);
        let (generation, top) = Inner::<T>::unpack_top(packed_top);

        // Fence to ensure we see bottom after top
        std::sync::atomic::fence(Ordering::SeqCst);

        let bottom = Inner::<T>::unpack_bottom(self.inner.bottom.load(Ordering::Acquire));

        let size = bottom.wrapping_sub(top) as isize;

        if size <= 0 {
            return StealResult::Empty;
        }

        // Read the task at top
        // Safety: we've checked there's at least one element
        let task = unsafe { self.inner.buffer.read(top) };

        // Try to claim it by incrementing top
        let new_packed_top = Inner::<T>::pack_top(generation.wrapping_add(1), top.wrapping_add(1));

        match self.inner.top.compare_exchange(
            packed_top,
            new_packed_top,
            Ordering::SeqCst,
            Ordering::Relaxed,
        ) {
            Ok(_) => StealResult::Success(task),
            Err(_) => {
                // Lost the race to another stealer or the owner
                // The task we read is invalid (another thread took it)
                // We need to forget it without running Drop
                std::mem::forget(task);
                StealResult::Retry
            }
        }
    }

    /// Check if the deque is empty
    pub fn is_empty(&self) -> bool {
        let (_, top) = Inner::<T>::unpack_top(self.inner.top.load(Ordering::Acquire));
        let bottom = Inner::<T>::unpack_bottom(self.inner.bottom.load(Ordering::Acquire));
        bottom.wrapping_sub(top) == 0
    }

    /// Clone this stealer handle
    pub fn clone(&self) -> Stealer<T> {
        Stealer {
            inner: self.inner.clone(),
        }
    }
}

// Safety: We ensure thread safety through atomics
unsafe impl<T: Send> Send for Worker<T> {}
unsafe impl<T: Send> Send for Stealer<T> {}
unsafe impl<T: Send> Sync for Stealer<T> {}

// Note: Worker is intentionally NOT Sync - only one thread should own it

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_pop_single() {
        let (worker, _stealer) = Worker::new(16);

        assert!(worker.is_empty());

        worker.push(42).unwrap();
        assert!(!worker.is_empty());
        assert_eq!(worker.len(), 1);

        match worker.pop() {
            StealResult::Success(v) => assert_eq!(v, 42),
            _ => panic!("Expected success"),
        }

        assert!(worker.is_empty());
    }

    #[test]
    fn test_push_pop_multiple() {
        let (worker, _stealer) = Worker::new(16);

        for i in 0..10 {
            worker.push(i).unwrap();
        }

        assert_eq!(worker.len(), 10);

        // LIFO order
        for i in (0..10).rev() {
            match worker.pop() {
                StealResult::Success(v) => assert_eq!(v, i),
                _ => panic!("Expected success"),
            }
        }
    }

    #[test]
    fn test_steal_single() {
        let (worker, stealer) = Worker::new(16);

        worker.push(42).unwrap();

        match stealer.steal() {
            StealResult::Success(v) => assert_eq!(v, 42),
            _ => panic!("Expected success"),
        }

        assert!(worker.is_empty());
    }

    #[test]
    fn test_steal_order() {
        let (worker, stealer) = Worker::new(16);

        for i in 0..5 {
            worker.push(i).unwrap();
        }

        // Steal is FIFO (from top)
        for i in 0..5 {
            match stealer.steal() {
                StealResult::Success(v) => assert_eq!(v, i),
                _ => panic!("Expected success"),
            }
        }
    }

    #[test]
    fn test_pop_empty() {
        let (worker, _stealer): (Worker<i32>, _) = Worker::new(16);
        match worker.pop() {
            StealResult::Empty => (),
            _ => panic!("Expected empty"),
        }
    }

    #[test]
    fn test_steal_empty() {
        let (_, stealer): (Worker<i32>, _) = Worker::new(16);
        match stealer.steal() {
            StealResult::Empty => (),
            _ => panic!("Expected empty"),
        }
    }

    #[test]
    fn test_full_deque() {
        let (worker, _stealer) = Worker::new(4);

        for i in 0..4 {
            worker.push(i).unwrap();
        }

        // Should fail when full
        assert!(worker.push(4).is_err());
    }

    #[test]
    fn test_mixed_pop_steal() {
        let (worker, stealer) = Worker::new(16);

        // Push 1, 2, 3, 4, 5
        for i in 1..=5 {
            worker.push(i).unwrap();
        }

        // Stealer takes oldest (1)
        assert_eq!(stealer.steal(), StealResult::Success(1));

        // Owner takes newest (5)
        assert_eq!(worker.pop(), StealResult::Success(5));

        // Stealer takes next oldest (2)
        assert_eq!(stealer.steal(), StealResult::Success(2));

        // Owner takes next newest (4)
        assert_eq!(worker.pop(), StealResult::Success(4));

        // Only 3 remains
        assert_eq!(worker.len(), 1);
        assert_eq!(worker.pop(), StealResult::Success(3));
    }

    #[test]
    fn test_stealer_clone() {
        let (worker, stealer1) = Worker::new(16);
        let stealer2 = stealer1.clone();

        worker.push(1).unwrap();
        worker.push(2).unwrap();

        // Both stealers can try to steal
        let r1 = stealer1.steal();
        let r2 = stealer2.steal();

        // One should succeed, one should get empty or retry
        match (r1, r2) {
            (StealResult::Success(a), StealResult::Success(b)) => {
                assert!(a != b);
            }
            (StealResult::Success(_), StealResult::Empty) => (),
            (StealResult::Empty, StealResult::Success(_)) => (),
            (StealResult::Success(_), StealResult::Retry) => (),
            (StealResult::Retry, StealResult::Success(_)) => (),
            _ => panic!("Unexpected result combination"),
        }
    }
}
