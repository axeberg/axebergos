//! Semaphore implementation
//!
//! System V-style semaphores for process synchronization.
//! Supports semaphore sets with multiple semaphores per set.

use std::collections::HashMap;

/// Semaphore ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SemId(pub u32);

/// A single semaphore
#[derive(Debug, Clone)]
pub struct Semaphore {
    /// Current value
    pub value: i32,
    /// Number of processes waiting for value to increase
    pub waiting_inc: u32,
    /// Number of processes waiting for value to become zero
    pub waiting_zero: u32,
    /// Last operation time
    pub otime: f64,
    /// PID of last operation
    pub pid: u32,
}

impl Semaphore {
    pub fn new(initial: i32) -> Self {
        Self {
            value: initial,
            waiting_inc: 0,
            waiting_zero: 0,
            otime: 0.0,
            pid: 0,
        }
    }
}

/// A semaphore set (array of semaphores)
#[derive(Debug)]
pub struct SemaphoreSet {
    /// Set ID
    pub id: SemId,
    /// Semaphores in this set
    semaphores: Vec<Semaphore>,
    /// Owner UID
    pub uid: u32,
    /// Owner GID
    pub gid: u32,
    /// Permission mode
    pub mode: u16,
    /// Creation time
    pub ctime: f64,
    /// Last operation time
    pub otime: f64,
}

impl SemaphoreSet {
    pub fn new(id: SemId, nsems: usize, uid: u32, gid: u32, now: f64) -> Self {
        Self {
            id,
            semaphores: vec![Semaphore::new(0); nsems],
            uid,
            gid,
            mode: 0o644,
            ctime: now,
            otime: 0.0,
        }
    }

    /// Get number of semaphores in set
    pub fn len(&self) -> usize {
        self.semaphores.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.semaphores.is_empty()
    }

    /// Get a semaphore's value
    pub fn getval(&self, sem_num: usize) -> Result<i32, SemError> {
        self.semaphores
            .get(sem_num)
            .map(|s| s.value)
            .ok_or(SemError::InvalidSemNum)
    }

    /// Set a semaphore's value
    pub fn setval(&mut self, sem_num: usize, value: i32, pid: u32, now: f64) -> Result<(), SemError> {
        let sem = self.semaphores
            .get_mut(sem_num)
            .ok_or(SemError::InvalidSemNum)?;
        sem.value = value;
        sem.pid = pid;
        sem.otime = now;
        self.otime = now;
        Ok(())
    }

    /// Get all values
    pub fn getall(&self) -> Vec<i32> {
        self.semaphores.iter().map(|s| s.value).collect()
    }

    /// Set all values
    pub fn setall(&mut self, values: &[i32], pid: u32, now: f64) -> Result<(), SemError> {
        if values.len() != self.semaphores.len() {
            return Err(SemError::InvalidArgument);
        }
        for (i, &val) in values.iter().enumerate() {
            self.semaphores[i].value = val;
            self.semaphores[i].pid = pid;
            self.semaphores[i].otime = now;
        }
        self.otime = now;
        Ok(())
    }

    /// Perform a semaphore operation
    ///
    /// - sem_op > 0: add to value (V operation / signal)
    /// - sem_op < 0: subtract from value (P operation / wait), blocks if would go negative
    /// - sem_op == 0: wait for value to become zero
    pub fn semop(&mut self, sem_num: usize, sem_op: i32, pid: u32, now: f64) -> Result<SemOpResult, SemError> {
        let sem = self.semaphores
            .get_mut(sem_num)
            .ok_or(SemError::InvalidSemNum)?;

        if sem_op > 0 {
            // V operation: increment
            sem.value += sem_op;
            sem.pid = pid;
            sem.otime = now;
            self.otime = now;
            Ok(SemOpResult::Completed)
        } else if sem_op < 0 {
            // P operation: decrement (or block)
            let abs_op = sem_op.abs();
            if sem.value >= abs_op {
                sem.value -= abs_op;
                sem.pid = pid;
                sem.otime = now;
                self.otime = now;
                Ok(SemOpResult::Completed)
            } else {
                // Would block
                sem.waiting_inc += 1;
                Ok(SemOpResult::WouldBlock)
            }
        } else {
            // Wait for zero
            if sem.value == 0 {
                sem.pid = pid;
                sem.otime = now;
                self.otime = now;
                Ok(SemOpResult::Completed)
            } else {
                sem.waiting_zero += 1;
                Ok(SemOpResult::WouldBlock)
            }
        }
    }

    /// Get PID of last operation on a semaphore
    pub fn getpid(&self, sem_num: usize) -> Result<u32, SemError> {
        self.semaphores
            .get(sem_num)
            .map(|s| s.pid)
            .ok_or(SemError::InvalidSemNum)
    }

    /// Get count waiting for value increase
    pub fn getncnt(&self, sem_num: usize) -> Result<u32, SemError> {
        self.semaphores
            .get(sem_num)
            .map(|s| s.waiting_inc)
            .ok_or(SemError::InvalidSemNum)
    }

    /// Get count waiting for zero
    pub fn getzcnt(&self, sem_num: usize) -> Result<u32, SemError> {
        self.semaphores
            .get(sem_num)
            .map(|s| s.waiting_zero)
            .ok_or(SemError::InvalidSemNum)
    }
}

/// Result of semop operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemOpResult {
    /// Operation completed
    Completed,
    /// Operation would block
    WouldBlock,
}

/// Semaphore errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemError {
    /// Invalid semaphore number
    InvalidSemNum,
    /// Invalid argument
    InvalidArgument,
    /// Semaphore set not found
    NotFound,
    /// Permission denied
    PermissionDenied,
    /// Already exists
    AlreadyExists,
    /// Too many semaphores
    TooMany,
}

/// Semaphore set statistics
#[derive(Debug, Clone)]
pub struct SemSetStats {
    /// Number of semaphores in set
    pub nsems: usize,
    /// Owner UID
    pub uid: u32,
    /// Owner GID
    pub gid: u32,
    /// Permission mode
    pub mode: u16,
    /// Creation time
    pub ctime: f64,
    /// Last operation time
    pub otime: f64,
}

/// Semaphore manager
pub struct SemaphoreManager {
    /// All semaphore sets
    sets: HashMap<SemId, SemaphoreSet>,
    /// Key to ID mapping
    key_map: HashMap<i32, SemId>,
    /// Next set ID
    next_id: u32,
    /// Maximum semaphores per set
    max_sems_per_set: usize,
}

impl SemaphoreManager {
    pub fn new() -> Self {
        Self {
            sets: HashMap::new(),
            key_map: HashMap::new(),
            next_id: 1,
            max_sems_per_set: 250,
        }
    }

    /// Get or create a semaphore set
    ///
    /// key < 0: create private set
    /// key >= 0: get existing or create new
    pub fn semget(
        &mut self,
        key: i32,
        nsems: usize,
        uid: u32,
        gid: u32,
        create: bool,
        now: f64,
    ) -> Result<SemId, SemError> {
        if nsems > self.max_sems_per_set {
            return Err(SemError::TooMany);
        }

        if key < 0 {
            // Private set
            let id = SemId(self.next_id);
            self.next_id += 1;
            let set = SemaphoreSet::new(id, nsems, uid, gid, now);
            self.sets.insert(id, set);
            return Ok(id);
        }

        // Check if exists
        if let Some(&id) = self.key_map.get(&key) {
            return Ok(id);
        }

        if !create {
            return Err(SemError::NotFound);
        }

        // Create new
        let id = SemId(self.next_id);
        self.next_id += 1;
        let set = SemaphoreSet::new(id, nsems, uid, gid, now);
        self.sets.insert(id, set);
        self.key_map.insert(key, id);
        Ok(id)
    }

    /// Perform operations on a semaphore set
    pub fn semop(
        &mut self,
        id: SemId,
        sem_num: usize,
        sem_op: i32,
        pid: u32,
        now: f64,
    ) -> Result<SemOpResult, SemError> {
        let set = self.sets.get_mut(&id).ok_or(SemError::NotFound)?;
        set.semop(sem_num, sem_op, pid, now)
    }

    /// Get a semaphore value
    pub fn semctl_getval(&self, id: SemId, sem_num: usize) -> Result<i32, SemError> {
        let set = self.sets.get(&id).ok_or(SemError::NotFound)?;
        set.getval(sem_num)
    }

    /// Set a semaphore value
    pub fn semctl_setval(
        &mut self,
        id: SemId,
        sem_num: usize,
        value: i32,
        pid: u32,
        now: f64,
    ) -> Result<(), SemError> {
        let set = self.sets.get_mut(&id).ok_or(SemError::NotFound)?;
        set.setval(sem_num, value, pid, now)
    }

    /// Get all semaphore values
    pub fn semctl_getall(&self, id: SemId) -> Result<Vec<i32>, SemError> {
        let set = self.sets.get(&id).ok_or(SemError::NotFound)?;
        Ok(set.getall())
    }

    /// Set all semaphore values
    pub fn semctl_setall(
        &mut self,
        id: SemId,
        values: &[i32],
        pid: u32,
        now: f64,
    ) -> Result<(), SemError> {
        let set = self.sets.get_mut(&id).ok_or(SemError::NotFound)?;
        set.setall(values, pid, now)
    }

    /// Get semaphore set stats
    pub fn semctl_stat(&self, id: SemId) -> Result<SemSetStats, SemError> {
        let set = self.sets.get(&id).ok_or(SemError::NotFound)?;
        Ok(SemSetStats {
            nsems: set.len(),
            uid: set.uid,
            gid: set.gid,
            mode: set.mode,
            ctime: set.ctime,
            otime: set.otime,
        })
    }

    /// Remove a semaphore set
    pub fn semctl_rmid(&mut self, id: SemId) -> Result<(), SemError> {
        self.sets.remove(&id).ok_or(SemError::NotFound)?;
        // Remove from key map too
        self.key_map.retain(|_, v| *v != id);
        Ok(())
    }

    /// List all semaphore set IDs
    pub fn list(&self) -> Vec<SemId> {
        self.sets.keys().copied().collect()
    }

    /// Get info about a set
    pub fn get_set(&self, id: SemId) -> Option<&SemaphoreSet> {
        self.sets.get(&id)
    }
}

impl Default for SemaphoreManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semaphore_basic() {
        let mut set = SemaphoreSet::new(SemId(1), 3, 1000, 1000, 1.0);
        assert_eq!(set.len(), 3);

        // All start at 0
        assert_eq!(set.getval(0).unwrap(), 0);
        assert_eq!(set.getval(1).unwrap(), 0);
        assert_eq!(set.getval(2).unwrap(), 0);

        // Set value
        set.setval(1, 5, 100, 2.0).unwrap();
        assert_eq!(set.getval(1).unwrap(), 5);
    }

    #[test]
    fn test_semop_increment() {
        let mut set = SemaphoreSet::new(SemId(1), 1, 1000, 1000, 1.0);

        // V operation: increment by 2
        let result = set.semop(0, 2, 100, 2.0).unwrap();
        assert_eq!(result, SemOpResult::Completed);
        assert_eq!(set.getval(0).unwrap(), 2);

        // Another increment
        set.semop(0, 3, 100, 3.0).unwrap();
        assert_eq!(set.getval(0).unwrap(), 5);
    }

    #[test]
    fn test_semop_decrement() {
        let mut set = SemaphoreSet::new(SemId(1), 1, 1000, 1000, 1.0);
        set.setval(0, 5, 100, 1.0).unwrap();

        // P operation: decrement by 2
        let result = set.semop(0, -2, 100, 2.0).unwrap();
        assert_eq!(result, SemOpResult::Completed);
        assert_eq!(set.getval(0).unwrap(), 3);

        // Try to decrement more than available
        let result = set.semop(0, -5, 100, 3.0).unwrap();
        assert_eq!(result, SemOpResult::WouldBlock);
        assert_eq!(set.getval(0).unwrap(), 3); // Unchanged
    }

    #[test]
    fn test_semop_wait_zero() {
        let mut set = SemaphoreSet::new(SemId(1), 1, 1000, 1000, 1.0);

        // Value is 0, wait-for-zero succeeds
        let result = set.semop(0, 0, 100, 2.0).unwrap();
        assert_eq!(result, SemOpResult::Completed);

        // Set value to non-zero
        set.setval(0, 1, 100, 3.0).unwrap();

        // Wait-for-zero would block
        let result = set.semop(0, 0, 100, 4.0).unwrap();
        assert_eq!(result, SemOpResult::WouldBlock);
    }

    #[test]
    fn test_manager() {
        let mut mgr = SemaphoreManager::new();

        let id1 = mgr.semget(100, 3, 1000, 1000, true, 1.0).unwrap();
        let id2 = mgr.semget(100, 3, 1000, 1000, true, 2.0).unwrap();
        assert_eq!(id1, id2); // Same key, same ID

        // Operate on semaphore
        mgr.semctl_setval(id1, 0, 10, 100, 3.0).unwrap();
        assert_eq!(mgr.semctl_getval(id1, 0).unwrap(), 10);

        // Decrement
        mgr.semop(id1, 0, -3, 100, 4.0).unwrap();
        assert_eq!(mgr.semctl_getval(id1, 0).unwrap(), 7);
    }

    #[test]
    fn test_private_sets() {
        let mut mgr = SemaphoreManager::new();

        let id1 = mgr.semget(-1, 2, 1000, 1000, true, 1.0).unwrap();
        let id2 = mgr.semget(-1, 2, 1000, 1000, true, 2.0).unwrap();
        assert_ne!(id1, id2); // Private sets get unique IDs
    }

    #[test]
    fn test_getall_setall() {
        let mut set = SemaphoreSet::new(SemId(1), 4, 1000, 1000, 1.0);

        set.setall(&[1, 2, 3, 4], 100, 2.0).unwrap();
        assert_eq!(set.getall(), vec![1, 2, 3, 4]);

        // Wrong length fails
        let result = set.setall(&[1, 2], 100, 3.0);
        assert_eq!(result, Err(SemError::InvalidArgument));
    }
}
