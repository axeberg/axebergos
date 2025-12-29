//! File locking implementation
//!
//! Provides advisory file locking with two interfaces:
//! - flock(): Whole-file locks (BSD style)
//! - fcntl(): Byte-range locks (POSIX style)
//!
//! Lock types:
//! - Shared (read) locks: Multiple processes can hold
//! - Exclusive (write) locks: Only one process can hold
//!
//! All locks are advisory - they don't prevent actual I/O,
//! only coordinate between cooperating processes.

use super::process::Pid;
use std::collections::HashMap;

/// Lock type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockType {
    /// No lock
    Unlock,
    /// Shared (read) lock - multiple holders allowed
    Shared,
    /// Exclusive (write) lock - single holder only
    Exclusive,
}

/// A file lock (for flock-style whole-file locking)
#[derive(Debug, Clone)]
pub struct FileLock {
    /// Process holding the lock
    pub pid: Pid,
    /// Type of lock
    pub lock_type: LockType,
}

/// A byte-range lock (for fcntl-style locking)
#[derive(Debug, Clone)]
pub struct RangeLock {
    /// Process holding the lock
    pub pid: Pid,
    /// Type of lock
    pub lock_type: LockType,
    /// Start offset (0 for start of file)
    pub start: u64,
    /// Length (0 means to end of file)
    pub len: u64,
    /// Whence: 0=SEEK_SET, 1=SEEK_CUR, 2=SEEK_END
    pub whence: i32,
}

/// Error types for locking operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockError {
    /// Lock would block (EWOULDBLOCK)
    WouldBlock,
    /// Invalid argument
    InvalidArgument,
    /// Deadlock detected (EDEADLK)
    Deadlock,
}

/// File lock manager
///
/// Tracks all file locks in the system.
/// Uses inode-like identifiers (path hash) to identify files.
#[derive(Debug, Default)]
pub struct FileLockManager {
    /// Whole-file locks (flock style), keyed by path
    file_locks: HashMap<String, Vec<FileLock>>,
    /// Byte-range locks (fcntl style), keyed by path
    range_locks: HashMap<String, Vec<RangeLock>>,
}

impl FileLockManager {
    pub fn new() -> Self {
        Self {
            file_locks: HashMap::new(),
            range_locks: HashMap::new(),
        }
    }

    /// Apply a flock-style lock to a file
    ///
    /// # Arguments
    /// * `path` - File path (used as identifier)
    /// * `pid` - Process requesting the lock
    /// * `lock_type` - Type of lock (Unlock removes existing lock)
    /// * `blocking` - If false, return WouldBlock instead of waiting
    ///
    /// # Returns
    /// Ok(()) on success, Err on failure
    pub fn flock(
        &mut self,
        path: &str,
        pid: Pid,
        lock_type: LockType,
        _blocking: bool,
    ) -> Result<(), LockError> {
        // Handle unlock
        if lock_type == LockType::Unlock {
            if let Some(locks) = self.file_locks.get_mut(path) {
                locks.retain(|l| l.pid != pid);
                if locks.is_empty() {
                    self.file_locks.remove(path);
                }
            }
            return Ok(());
        }

        // Check for conflicts
        if let Some(locks) = self.file_locks.get(path) {
            for lock in locks {
                if lock.pid == pid {
                    // Same process - upgrade/downgrade allowed
                    continue;
                }

                // Check for conflict
                if lock.lock_type == LockType::Exclusive || lock_type == LockType::Exclusive {
                    // Exclusive conflicts with any lock
                    return Err(LockError::WouldBlock);
                }
            }
        }

        // Remove any existing lock from this process
        if let Some(locks) = self.file_locks.get_mut(path) {
            locks.retain(|l| l.pid != pid);
        }

        // Add the new lock
        let entry = self.file_locks.entry(path.to_string()).or_default();
        entry.push(FileLock { pid, lock_type });

        Ok(())
    }

    /// Apply a fcntl-style byte-range lock
    ///
    /// # Arguments
    /// * `path` - File path
    /// * `pid` - Process requesting the lock
    /// * `lock` - Lock parameters
    /// * `blocking` - If false, return WouldBlock instead of waiting
    ///
    /// # Returns
    /// Ok(()) on success, Err on failure
    pub fn fcntl_lock(
        &mut self,
        path: &str,
        pid: Pid,
        lock: RangeLock,
        _blocking: bool,
    ) -> Result<(), LockError> {
        // Handle unlock
        if lock.lock_type == LockType::Unlock {
            if let Some(locks) = self.range_locks.get_mut(path) {
                // Remove overlapping locks from this process
                locks.retain(|l| l.pid != pid || !ranges_overlap(l, &lock));
                if locks.is_empty() {
                    self.range_locks.remove(path);
                }
            }
            return Ok(());
        }

        // Check for conflicts
        if let Some(locks) = self.range_locks.get(path) {
            for existing in locks {
                if existing.pid == pid {
                    // Same process - upgrade/downgrade allowed
                    continue;
                }

                // Check for overlap
                if !ranges_overlap(existing, &lock) {
                    continue;
                }

                // Check for conflict
                if existing.lock_type == LockType::Exclusive || lock.lock_type == LockType::Exclusive
                {
                    return Err(LockError::WouldBlock);
                }
            }
        }

        // Remove any overlapping locks from this process and add new one
        if let Some(locks) = self.range_locks.get_mut(path) {
            locks.retain(|l| l.pid != pid || !ranges_overlap(l, &lock));
        }

        let entry = self.range_locks.entry(path.to_string()).or_default();
        entry.push(lock);

        Ok(())
    }

    /// Get information about locks on a file (for F_GETLK)
    ///
    /// Returns the first conflicting lock, or None if the lock could be placed.
    pub fn get_lock(&self, path: &str, pid: Pid, lock: &RangeLock) -> Option<RangeLock> {
        if let Some(locks) = self.range_locks.get(path) {
            for existing in locks {
                if existing.pid == pid {
                    continue;
                }

                if !ranges_overlap(existing, lock) {
                    continue;
                }

                if existing.lock_type == LockType::Exclusive || lock.lock_type == LockType::Exclusive
                {
                    return Some(existing.clone());
                }
            }
        }
        None
    }

    /// Release all locks held by a process (called on process exit)
    pub fn release_all(&mut self, pid: Pid) {
        // Release file locks
        for locks in self.file_locks.values_mut() {
            locks.retain(|l| l.pid != pid);
        }
        self.file_locks.retain(|_, v| !v.is_empty());

        // Release range locks
        for locks in self.range_locks.values_mut() {
            locks.retain(|l| l.pid != pid);
        }
        self.range_locks.retain(|_, v| !v.is_empty());
    }

    /// Release all locks for a specific file/fd by a process
    pub fn release_file(&mut self, path: &str, pid: Pid) {
        if let Some(locks) = self.file_locks.get_mut(path) {
            locks.retain(|l| l.pid != pid);
            if locks.is_empty() {
                self.file_locks.remove(path);
            }
        }

        if let Some(locks) = self.range_locks.get_mut(path) {
            locks.retain(|l| l.pid != pid);
            if locks.is_empty() {
                self.range_locks.remove(path);
            }
        }
    }
}

/// Check if two byte ranges overlap
fn ranges_overlap(a: &RangeLock, b: &RangeLock) -> bool {
    // For simplicity, assume whence is always SEEK_SET (0)
    // and len=0 means "to end of file" (infinite length)

    let a_end = if a.len == 0 {
        u64::MAX
    } else {
        a.start + a.len
    };
    let b_end = if b.len == 0 {
        u64::MAX
    } else {
        b.start + b.len
    };

    // Ranges overlap if start < other_end and other_start < end
    a.start < b_end && b.start < a_end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flock_shared() {
        let mut mgr = FileLockManager::new();

        // Two processes can hold shared locks
        assert!(mgr.flock("/test", Pid(1), LockType::Shared, false).is_ok());
        assert!(mgr.flock("/test", Pid(2), LockType::Shared, false).is_ok());
    }

    #[test]
    fn test_flock_exclusive_conflict() {
        let mut mgr = FileLockManager::new();

        // First exclusive lock succeeds
        assert!(mgr
            .flock("/test", Pid(1), LockType::Exclusive, false)
            .is_ok());

        // Second exclusive lock fails
        assert_eq!(
            mgr.flock("/test", Pid(2), LockType::Exclusive, false),
            Err(LockError::WouldBlock)
        );
    }

    #[test]
    fn test_flock_shared_exclusive_conflict() {
        let mut mgr = FileLockManager::new();

        // Shared lock first
        assert!(mgr.flock("/test", Pid(1), LockType::Shared, false).is_ok());

        // Exclusive lock blocked
        assert_eq!(
            mgr.flock("/test", Pid(2), LockType::Exclusive, false),
            Err(LockError::WouldBlock)
        );
    }

    #[test]
    fn test_flock_unlock() {
        let mut mgr = FileLockManager::new();

        // Lock and unlock
        assert!(mgr
            .flock("/test", Pid(1), LockType::Exclusive, false)
            .is_ok());
        assert!(mgr.flock("/test", Pid(1), LockType::Unlock, false).is_ok());

        // Now another process can lock
        assert!(mgr
            .flock("/test", Pid(2), LockType::Exclusive, false)
            .is_ok());
    }

    #[test]
    fn test_flock_upgrade() {
        let mut mgr = FileLockManager::new();

        // Shared then exclusive by same process
        assert!(mgr.flock("/test", Pid(1), LockType::Shared, false).is_ok());
        assert!(mgr
            .flock("/test", Pid(1), LockType::Exclusive, false)
            .is_ok());
    }

    #[test]
    fn test_fcntl_range_no_overlap() {
        let mut mgr = FileLockManager::new();

        // Non-overlapping ranges should not conflict
        let lock1 = RangeLock {
            pid: Pid(1),
            lock_type: LockType::Exclusive,
            start: 0,
            len: 100,
            whence: 0,
        };
        let lock2 = RangeLock {
            pid: Pid(2),
            lock_type: LockType::Exclusive,
            start: 200,
            len: 100,
            whence: 0,
        };

        assert!(mgr.fcntl_lock("/test", Pid(1), lock1, false).is_ok());
        assert!(mgr.fcntl_lock("/test", Pid(2), lock2, false).is_ok());
    }

    #[test]
    fn test_fcntl_range_overlap() {
        let mut mgr = FileLockManager::new();

        // Overlapping exclusive ranges should conflict
        let lock1 = RangeLock {
            pid: Pid(1),
            lock_type: LockType::Exclusive,
            start: 0,
            len: 100,
            whence: 0,
        };
        let lock2 = RangeLock {
            pid: Pid(2),
            lock_type: LockType::Exclusive,
            start: 50,
            len: 100,
            whence: 0,
        };

        assert!(mgr.fcntl_lock("/test", Pid(1), lock1, false).is_ok());
        assert_eq!(
            mgr.fcntl_lock("/test", Pid(2), lock2, false),
            Err(LockError::WouldBlock)
        );
    }

    #[test]
    fn test_release_all() {
        let mut mgr = FileLockManager::new();

        assert!(mgr
            .flock("/test1", Pid(1), LockType::Exclusive, false)
            .is_ok());
        assert!(mgr
            .flock("/test2", Pid(1), LockType::Exclusive, false)
            .is_ok());

        mgr.release_all(Pid(1));

        // Now other processes can lock
        assert!(mgr
            .flock("/test1", Pid(2), LockType::Exclusive, false)
            .is_ok());
        assert!(mgr
            .flock("/test2", Pid(2), LockType::Exclusive, false)
            .is_ok());
    }
}
