//! Memory Management
//!
//! Provides memory accounting, allocation tracking, and shared memory.
//!
//! In WASM we don't have hardware MMU or page tables, but we can still:
//! - Track per-process memory usage
//! - Enforce memory limits/quotas
//! - Provide shared memory for efficient IPC
//! - Give visibility into system memory state
//!
//! Design principles:
//! - Every allocation is tracked
//! - Processes have memory budgets
//! - Shared memory enables zero-copy IPC
//! - All state is inspectable

use super::process::Pid;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Unique identifier for a memory region
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegionId(pub u64);

impl RegionId {
    pub const NULL: RegionId = RegionId(0);
}

/// Unique identifier for a shared memory segment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShmId(pub u64);

/// Memory protection flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Protection {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl Protection {
    pub const NONE: Protection = Protection {
        read: false,
        write: false,
        execute: false,
    };

    pub const READ: Protection = Protection {
        read: true,
        write: false,
        execute: false,
    };

    pub const READ_WRITE: Protection = Protection {
        read: true,
        write: true,
        execute: false,
    };

    pub const READ_EXEC: Protection = Protection {
        read: true,
        write: false,
        execute: true,
    };
}

impl Default for Protection {
    fn default() -> Self {
        Self::READ_WRITE
    }
}

/// A memory region - a tracked allocation
#[derive(Debug, Clone)]
pub struct MemoryRegion {
    /// Unique identifier
    pub id: RegionId,
    /// Size in bytes
    pub size: usize,
    /// Protection flags
    pub protection: Protection,
    /// The actual data
    data: Vec<u8>,
    /// Is this region part of shared memory?
    shared: Option<ShmId>,
}

impl MemoryRegion {
    /// Create a new private memory region
    pub fn new(id: RegionId, size: usize, protection: Protection) -> Self {
        Self {
            id,
            size,
            protection,
            data: vec![0u8; size],
            shared: None,
        }
    }

    /// Create a region backed by shared memory
    pub fn from_shared(id: RegionId, shm_id: ShmId, data: Vec<u8>, protection: Protection) -> Self {
        let size = data.len();
        Self {
            id,
            size,
            protection,
            data,
            shared: Some(shm_id),
        }
    }

    /// Read from the region
    pub fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, MemoryError> {
        if !self.protection.read {
            return Err(MemoryError::PermissionDenied);
        }

        if offset >= self.size {
            return Ok(0);
        }

        let available = self.size - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&self.data[offset..offset + to_read]);
        Ok(to_read)
    }

    /// Write to the region
    pub fn write(&mut self, offset: usize, buf: &[u8]) -> Result<usize, MemoryError> {
        if !self.protection.write {
            return Err(MemoryError::PermissionDenied);
        }

        if offset >= self.size {
            return Err(MemoryError::OutOfBounds);
        }

        let available = self.size - offset;
        let to_write = buf.len().min(available);
        self.data[offset..offset + to_write].copy_from_slice(&buf[..to_write]);
        Ok(to_write)
    }

    /// Get a slice of the data (for zero-copy access)
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    /// Get a mutable slice of the data
    pub fn as_mut_slice(&mut self) -> Result<&mut [u8], MemoryError> {
        if !self.protection.write {
            return Err(MemoryError::PermissionDenied);
        }
        Ok(&mut self.data)
    }

    /// Check if this region is shared
    pub fn is_shared(&self) -> bool {
        self.shared.is_some()
    }

    /// Get the shared memory ID if this is a shared region
    pub fn shm_id(&self) -> Option<ShmId> {
        self.shared
    }
}

/// Memory errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryError {
    /// Out of memory (quota exceeded or system limit)
    OutOfMemory,
    /// Invalid region ID
    InvalidRegion,
    /// Permission denied (protection violation)
    PermissionDenied,
    /// Access out of bounds
    OutOfBounds,
    /// Shared memory segment not found
    ShmNotFound,
    /// Already attached to this shared memory
    AlreadyAttached,
    /// Not attached to this shared memory
    NotAttached,
    /// Invalid size
    InvalidSize,
}

impl std::fmt::Display for MemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfMemory => write!(f, "out of memory"),
            Self::InvalidRegion => write!(f, "invalid region"),
            Self::PermissionDenied => write!(f, "permission denied"),
            Self::OutOfBounds => write!(f, "out of bounds"),
            Self::ShmNotFound => write!(f, "shared memory not found"),
            Self::AlreadyAttached => write!(f, "already attached"),
            Self::NotAttached => write!(f, "not attached"),
            Self::InvalidSize => write!(f, "invalid size"),
        }
    }
}

impl std::error::Error for MemoryError {}

/// Per-process memory tracking
#[derive(Debug)]
pub struct ProcessMemory {
    /// Memory regions owned by this process
    regions: HashMap<RegionId, MemoryRegion>,
    /// Total bytes allocated
    allocated: usize,
    /// Memory limit (0 = unlimited)
    limit: usize,
    /// Peak memory usage
    peak: usize,
    /// Shared memory segments attached
    attached_shm: HashMap<ShmId, RegionId>,
}

impl ProcessMemory {
    /// Create new process memory tracker
    pub fn new() -> Self {
        Self {
            regions: HashMap::new(),
            allocated: 0,
            limit: 0, // unlimited by default
            peak: 0,
            attached_shm: HashMap::new(),
        }
    }

    /// Create with a memory limit
    pub fn with_limit(limit: usize) -> Self {
        Self {
            regions: HashMap::new(),
            allocated: 0,
            limit,
            peak: 0,
            attached_shm: HashMap::new(),
        }
    }

    /// Set memory limit (0 = unlimited)
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
    }

    /// Get current memory limit
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Allocate a new memory region
    pub fn allocate(&mut self, id: RegionId, size: usize, prot: Protection) -> Result<(), MemoryError> {
        if size == 0 {
            return Err(MemoryError::InvalidSize);
        }

        // Check limit
        if self.limit > 0 && self.allocated + size > self.limit {
            return Err(MemoryError::OutOfMemory);
        }

        let region = MemoryRegion::new(id, size, prot);
        self.regions.insert(id, region);
        self.allocated += size;
        self.peak = self.peak.max(self.allocated);

        Ok(())
    }

    /// Free a memory region
    pub fn free(&mut self, id: RegionId) -> Result<(), MemoryError> {
        let region = self.regions.remove(&id).ok_or(MemoryError::InvalidRegion)?;

        // If it's shared memory, just remove our mapping
        if let Some(shm_id) = region.shared {
            self.attached_shm.remove(&shm_id);
        }

        self.allocated = self.allocated.saturating_sub(region.size);
        Ok(())
    }

    /// Get a region by ID
    pub fn get(&self, id: RegionId) -> Option<&MemoryRegion> {
        self.regions.get(&id)
    }

    /// Get a mutable region by ID
    pub fn get_mut(&mut self, id: RegionId) -> Option<&mut MemoryRegion> {
        self.regions.get_mut(&id)
    }

    /// Current bytes allocated
    pub fn allocated(&self) -> usize {
        self.allocated
    }

    /// Peak memory usage
    pub fn peak(&self) -> usize {
        self.peak
    }

    /// Number of regions
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    /// Attach a shared memory segment
    pub fn attach_shm(&mut self, shm_id: ShmId, region: MemoryRegion) -> Result<RegionId, MemoryError> {
        if self.attached_shm.contains_key(&shm_id) {
            return Err(MemoryError::AlreadyAttached);
        }

        // Check limit (shared memory counts toward limit)
        if self.limit > 0 && self.allocated + region.size > self.limit {
            return Err(MemoryError::OutOfMemory);
        }

        let region_id = region.id;
        self.allocated += region.size;
        self.peak = self.peak.max(self.allocated);
        self.attached_shm.insert(shm_id, region_id);
        self.regions.insert(region_id, region);

        Ok(region_id)
    }

    /// Detach a shared memory segment
    pub fn detach_shm(&mut self, shm_id: ShmId) -> Result<(), MemoryError> {
        let region_id = self.attached_shm.remove(&shm_id).ok_or(MemoryError::NotAttached)?;

        if let Some(region) = self.regions.remove(&region_id) {
            self.allocated = self.allocated.saturating_sub(region.size);
        }

        Ok(())
    }

    /// Check if attached to a shared memory segment
    pub fn is_attached(&self, shm_id: ShmId) -> bool {
        self.attached_shm.contains_key(&shm_id)
    }

    /// Get region ID for attached shared memory
    pub fn shm_region(&self, shm_id: ShmId) -> Option<RegionId> {
        self.attached_shm.get(&shm_id).copied()
    }

    /// Iterate over all regions
    pub fn regions(&self) -> impl Iterator<Item = &MemoryRegion> {
        self.regions.values()
    }

    /// Get memory stats
    pub fn stats(&self) -> MemoryStats {
        MemoryStats {
            allocated: self.allocated,
            limit: self.limit,
            peak: self.peak,
            region_count: self.regions.len(),
            shm_count: self.attached_shm.len(),
        }
    }
}

impl Default for ProcessMemory {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory statistics
#[derive(Debug, Clone, Copy)]
pub struct MemoryStats {
    pub allocated: usize,
    pub limit: usize,
    pub peak: usize,
    pub region_count: usize,
    pub shm_count: usize,
}

/// A shared memory segment
#[derive(Debug)]
pub struct SharedMemory {
    /// Segment ID
    pub id: ShmId,
    /// Size in bytes
    pub size: usize,
    /// The actual data (shared between processes)
    data: Vec<u8>,
    /// Processes attached to this segment
    attached: Vec<Pid>,
    /// Creator process
    creator: Pid,
    /// Reference count
    refcount: usize,
}

impl SharedMemory {
    /// Create a new shared memory segment
    /// Note: Creator is NOT auto-attached; they must call shmat explicitly
    pub fn new(id: ShmId, size: usize, creator: Pid) -> Self {
        Self {
            id,
            size,
            data: vec![0u8; size],
            attached: Vec::new(), // Creator must call shmat to attach
            creator,
            refcount: 0, // No one attached yet
        }
    }

    /// Attach a process
    pub fn attach(&mut self, pid: Pid) {
        if !self.attached.contains(&pid) {
            self.attached.push(pid);
            self.refcount += 1;
        }
    }

    /// Detach a process
    pub fn detach(&mut self, pid: Pid) -> bool {
        if let Some(pos) = self.attached.iter().position(|&p| p == pid) {
            self.attached.remove(pos);
            self.refcount = self.refcount.saturating_sub(1);
        }
        self.refcount == 0
    }

    /// Check if a process is attached
    pub fn is_attached(&self, pid: Pid) -> bool {
        self.attached.contains(&pid)
    }

    /// Get the data slice
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get mutable data slice
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Clone data for a new attachment
    pub fn clone_data(&self) -> Vec<u8> {
        self.data.clone()
    }

    /// Get reference count
    pub fn refcount(&self) -> usize {
        self.refcount
    }

    /// Get attached process count
    pub fn attached_count(&self) -> usize {
        self.attached.len()
    }
}

/// Global memory manager
#[derive(Debug)]
pub struct MemoryManager {
    /// Next region ID
    next_region_id: AtomicU64,
    /// Next shared memory ID
    next_shm_id: AtomicU64,
    /// Shared memory segments
    shared_segments: HashMap<ShmId, SharedMemory>,
    /// System memory limit (0 = unlimited)
    system_limit: usize,
    /// Total memory allocated across all processes
    total_allocated: usize,
}

impl MemoryManager {
    /// Create a new memory manager
    pub fn new() -> Self {
        Self {
            next_region_id: AtomicU64::new(1),
            next_shm_id: AtomicU64::new(1),
            shared_segments: HashMap::new(),
            system_limit: 0,
            total_allocated: 0,
        }
    }

    /// Set system-wide memory limit
    pub fn set_system_limit(&mut self, limit: usize) {
        self.system_limit = limit;
    }

    /// Allocate a region ID
    pub fn alloc_region_id(&self) -> RegionId {
        RegionId(self.next_region_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Create a shared memory segment
    pub fn shmget(&mut self, size: usize, creator: Pid) -> Result<ShmId, MemoryError> {
        if size == 0 {
            return Err(MemoryError::InvalidSize);
        }

        // Check system limit
        if self.system_limit > 0 && self.total_allocated + size > self.system_limit {
            return Err(MemoryError::OutOfMemory);
        }

        let id = ShmId(self.next_shm_id.fetch_add(1, Ordering::Relaxed));
        let shm = SharedMemory::new(id, size, creator);
        self.shared_segments.insert(id, shm);
        self.total_allocated += size;

        Ok(id)
    }

    /// Attach to a shared memory segment
    pub fn shmat(
        &mut self,
        shm_id: ShmId,
        pid: Pid,
        prot: Protection,
    ) -> Result<MemoryRegion, MemoryError> {
        // Allocate region ID first to avoid borrow conflict
        let region_id = self.alloc_region_id();

        let shm = self.shared_segments.get_mut(&shm_id).ok_or(MemoryError::ShmNotFound)?;

        // Attach the process
        shm.attach(pid);

        // Create a region with a copy of the shared data
        // Note: In a real implementation, this would share memory.
        // Here we simulate it by copying and syncing on access.
        let data = shm.clone_data();
        let region = MemoryRegion::from_shared(region_id, shm_id, data, prot);

        Ok(region)
    }

    /// Detach from a shared memory segment
    pub fn shmdt(&mut self, shm_id: ShmId, pid: Pid) -> Result<bool, MemoryError> {
        let shm = self.shared_segments.get_mut(&shm_id).ok_or(MemoryError::ShmNotFound)?;

        let should_remove = shm.detach(pid);

        if should_remove {
            if let Some(removed) = self.shared_segments.remove(&shm_id) {
                self.total_allocated = self.total_allocated.saturating_sub(removed.size);
            }
        }

        Ok(should_remove)
    }

    /// Sync shared memory (write local changes back to shared segment)
    pub fn shm_sync(&mut self, shm_id: ShmId, data: &[u8]) -> Result<(), MemoryError> {
        let shm = self.shared_segments.get_mut(&shm_id).ok_or(MemoryError::ShmNotFound)?;

        if data.len() != shm.size {
            return Err(MemoryError::InvalidSize);
        }

        shm.data_mut().copy_from_slice(data);
        Ok(())
    }

    /// Get shared memory data (for reading latest)
    pub fn shm_read(&self, shm_id: ShmId) -> Result<&[u8], MemoryError> {
        let shm = self.shared_segments.get(&shm_id).ok_or(MemoryError::ShmNotFound)?;
        Ok(shm.data())
    }

    /// Get shared memory info
    pub fn shm_info(&self, shm_id: ShmId) -> Result<ShmInfo, MemoryError> {
        let shm = self.shared_segments.get(&shm_id).ok_or(MemoryError::ShmNotFound)?;
        Ok(ShmInfo {
            id: shm_id,
            size: shm.size,
            attached_count: shm.attached_count(),
            creator: shm.creator,
        })
    }

    /// List all shared memory segments
    pub fn shm_list(&self) -> Vec<ShmInfo> {
        self.shared_segments
            .values()
            .map(|shm| ShmInfo {
                id: shm.id,
                size: shm.size,
                attached_count: shm.attached_count(),
                creator: shm.creator,
            })
            .collect()
    }

    /// Get total allocated memory
    pub fn total_allocated(&self) -> usize {
        self.total_allocated
    }

    /// Get system stats
    pub fn system_stats(&self) -> SystemMemoryStats {
        SystemMemoryStats {
            total_allocated: self.total_allocated,
            system_limit: self.system_limit,
            shm_count: self.shared_segments.len(),
            shm_total_size: self.shared_segments.values().map(|s| s.size).sum(),
        }
    }
}

impl Default for MemoryManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared memory info
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShmInfo {
    pub id: ShmId,
    pub size: usize,
    pub attached_count: usize,
    pub creator: Pid,
}

/// System-wide memory stats
#[derive(Debug, Clone, Copy)]
pub struct SystemMemoryStats {
    pub total_allocated: usize,
    pub system_limit: usize,
    pub shm_count: usize,
    pub shm_total_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_region_basic() {
        let id = RegionId(1);
        let mut region = MemoryRegion::new(id, 1024, Protection::READ_WRITE);

        assert_eq!(region.id, id);
        assert_eq!(region.size, 1024);
        assert!(!region.is_shared());

        // Write and read
        let data = b"hello";
        assert_eq!(region.write(0, data).unwrap(), 5);

        let mut buf = [0u8; 10];
        assert_eq!(region.read(0, &mut buf).unwrap(), 10);
        assert_eq!(&buf[..5], b"hello");
    }

    #[test]
    fn test_region_protection() {
        let id = RegionId(1);
        let mut region = MemoryRegion::new(id, 1024, Protection::READ);

        // Can read
        let mut buf = [0u8; 10];
        assert!(region.read(0, &mut buf).is_ok());

        // Cannot write
        assert_eq!(
            region.write(0, b"test"),
            Err(MemoryError::PermissionDenied)
        );
    }

    #[test]
    fn test_region_bounds() {
        let id = RegionId(1);
        let mut region = MemoryRegion::new(id, 10, Protection::READ_WRITE);

        // Write at end
        assert_eq!(region.write(8, b"ab").unwrap(), 2);

        // Write past end (partial)
        assert_eq!(region.write(8, b"abcd").unwrap(), 2);

        // Write completely past end
        assert_eq!(region.write(100, b"test"), Err(MemoryError::OutOfBounds));
    }

    #[test]
    fn test_process_memory_basic() {
        let mut mem = ProcessMemory::new();
        let id = RegionId(1);

        // Allocate
        mem.allocate(id, 1024, Protection::READ_WRITE).unwrap();
        assert_eq!(mem.allocated(), 1024);
        assert_eq!(mem.region_count(), 1);

        // Access region
        let region = mem.get_mut(id).unwrap();
        region.write(0, b"test").unwrap();

        // Free
        mem.free(id).unwrap();
        assert_eq!(mem.allocated(), 0);
        assert_eq!(mem.region_count(), 0);
    }

    #[test]
    fn test_process_memory_limit() {
        let mut mem = ProcessMemory::with_limit(1000);

        // Allocate within limit
        mem.allocate(RegionId(1), 500, Protection::READ_WRITE).unwrap();
        assert_eq!(mem.allocated(), 500);

        // Allocate more within limit
        mem.allocate(RegionId(2), 400, Protection::READ_WRITE).unwrap();
        assert_eq!(mem.allocated(), 900);

        // Exceed limit
        assert_eq!(
            mem.allocate(RegionId(3), 200, Protection::READ_WRITE),
            Err(MemoryError::OutOfMemory)
        );

        // Free and allocate again
        mem.free(RegionId(1)).unwrap();
        mem.allocate(RegionId(3), 200, Protection::READ_WRITE).unwrap();
        assert_eq!(mem.allocated(), 600);
    }

    #[test]
    fn test_process_memory_peak() {
        let mut mem = ProcessMemory::new();

        mem.allocate(RegionId(1), 1000, Protection::READ_WRITE).unwrap();
        mem.allocate(RegionId(2), 500, Protection::READ_WRITE).unwrap();
        assert_eq!(mem.peak(), 1500);

        mem.free(RegionId(1)).unwrap();
        assert_eq!(mem.allocated(), 500);
        assert_eq!(mem.peak(), 1500); // Peak unchanged
    }

    #[test]
    fn test_process_memory_stats() {
        let mut mem = ProcessMemory::with_limit(5000);
        mem.allocate(RegionId(1), 1000, Protection::READ_WRITE).unwrap();
        mem.allocate(RegionId(2), 2000, Protection::READ_WRITE).unwrap();

        let stats = mem.stats();
        assert_eq!(stats.allocated, 3000);
        assert_eq!(stats.limit, 5000);
        assert_eq!(stats.peak, 3000);
        assert_eq!(stats.region_count, 2);
    }

    #[test]
    fn test_memory_manager_shmget() {
        let mut mgr = MemoryManager::new();
        let pid = Pid(1);

        let shm_id = mgr.shmget(1024, pid).unwrap();
        assert!(shm_id.0 > 0);

        let info = mgr.shm_info(shm_id).unwrap();
        assert_eq!(info.size, 1024);
        assert_eq!(info.creator, pid);
        assert_eq!(info.attached_count, 0); // Not attached until shmat
    }

    #[test]
    fn test_memory_manager_shmat() {
        let mut mgr = MemoryManager::new();
        let pid1 = Pid(1);
        let pid2 = Pid(2);

        let shm_id = mgr.shmget(1024, pid1).unwrap();

        // Attach first process (creator must also call shmat)
        let _region1 = mgr.shmat(shm_id, pid1, Protection::READ_WRITE).unwrap();
        let info = mgr.shm_info(shm_id).unwrap();
        assert_eq!(info.attached_count, 1);

        // Attach second process
        let region = mgr.shmat(shm_id, pid2, Protection::READ_WRITE).unwrap();
        assert!(region.is_shared());
        assert_eq!(region.shm_id(), Some(shm_id));
        assert_eq!(region.size, 1024);

        let info = mgr.shm_info(shm_id).unwrap();
        assert_eq!(info.attached_count, 2);
    }

    #[test]
    fn test_memory_manager_shmdt() {
        let mut mgr = MemoryManager::new();
        let pid1 = Pid(1);
        let pid2 = Pid(2);

        let shm_id = mgr.shmget(1024, pid1).unwrap();

        // Both processes attach
        let _region1 = mgr.shmat(shm_id, pid1, Protection::READ_WRITE).unwrap();
        let _region2 = mgr.shmat(shm_id, pid2, Protection::READ_WRITE).unwrap();

        // Detach pid2
        let removed = mgr.shmdt(shm_id, pid2).unwrap();
        assert!(!removed); // Still one attached

        // Detach pid1 - should remove segment
        let removed = mgr.shmdt(shm_id, pid1).unwrap();
        assert!(removed);

        // Segment gone
        assert_eq!(mgr.shm_info(shm_id), Err(MemoryError::ShmNotFound));
    }

    #[test]
    fn test_memory_manager_shm_sync() {
        let mut mgr = MemoryManager::new();
        let pid1 = Pid(1);
        let pid2 = Pid(2);

        let shm_id = mgr.shmget(10, pid1).unwrap();

        // Process 1 writes
        mgr.shm_sync(shm_id, b"hello12345").unwrap();

        // Process 2 attaches and reads
        let region = mgr.shmat(shm_id, pid2, Protection::READ).unwrap();
        assert_eq!(region.as_slice(), b"hello12345");
    }

    #[test]
    fn test_process_memory_with_shm() {
        let mut mgr = MemoryManager::new();
        let mut mem = ProcessMemory::new();
        let pid = Pid(1);

        // Create shared memory
        let shm_id = mgr.shmget(1024, pid).unwrap();

        // Attach to process memory
        let region = mgr.shmat(shm_id, pid, Protection::READ_WRITE).unwrap();
        let region_id = mem.attach_shm(shm_id, region).unwrap();

        assert!(mem.is_attached(shm_id));
        assert_eq!(mem.shm_region(shm_id), Some(region_id));
        assert_eq!(mem.allocated(), 1024);

        // Detach
        mem.detach_shm(shm_id).unwrap();
        assert!(!mem.is_attached(shm_id));
        assert_eq!(mem.allocated(), 0);
    }

    #[test]
    fn test_memory_manager_system_stats() {
        let mut mgr = MemoryManager::new();

        mgr.shmget(1000, Pid(1)).unwrap();
        mgr.shmget(2000, Pid(2)).unwrap();

        let stats = mgr.system_stats();
        assert_eq!(stats.shm_count, 2);
        assert_eq!(stats.shm_total_size, 3000);
        assert_eq!(stats.total_allocated, 3000);
    }

    #[test]
    fn test_shm_list() {
        let mut mgr = MemoryManager::new();

        let id1 = mgr.shmget(1000, Pid(1)).unwrap();
        let id2 = mgr.shmget(2000, Pid(2)).unwrap();

        let list = mgr.shm_list();
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|i| i.id == id1));
        assert!(list.iter().any(|i| i.id == id2));
    }

    #[test]
    fn test_invalid_size() {
        let mut mgr = MemoryManager::new();
        let mut mem = ProcessMemory::new();

        assert_eq!(mgr.shmget(0, Pid(1)), Err(MemoryError::InvalidSize));
        assert_eq!(
            mem.allocate(RegionId(1), 0, Protection::READ_WRITE),
            Err(MemoryError::InvalidSize)
        );
    }

    #[test]
    fn test_double_attach() {
        let mut mgr = MemoryManager::new();
        let mut mem = ProcessMemory::new();
        let pid = Pid(1);

        let shm_id = mgr.shmget(1024, pid).unwrap();
        let region = mgr.shmat(shm_id, pid, Protection::READ_WRITE).unwrap();
        mem.attach_shm(shm_id, region).unwrap();

        // Try to attach again
        let region2 = mgr.shmat(shm_id, pid, Protection::READ_WRITE).unwrap();
        assert_eq!(mem.attach_shm(shm_id, region2), Err(MemoryError::AlreadyAttached));
    }
}
