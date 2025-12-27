//! Memory Management
//!
//! Provides memory accounting, allocation tracking, shared memory, and copy-on-write.
//!
//! In WASM we don't have hardware MMU or page tables, but we can still:
//! - Track per-process memory usage
//! - Enforce memory limits/quotas
//! - Provide shared memory for efficient IPC
//! - Give visibility into system memory state
//! - Implement copy-on-write (COW) for efficient fork
//!
//! Design principles:
//! - Every allocation is tracked
//! - Processes have memory budgets
//! - Shared memory enables zero-copy IPC
//! - All state is inspectable
//! - COW pages share data until written

use super::process::Pid;
use std::collections::HashMap;
use std::sync::Arc;
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

// ============================================================================
// Copy-on-Write (COW) Support
// ============================================================================

/// Page size for COW operations (4KB, matching typical page sizes)
pub const PAGE_SIZE: usize = 4096;

/// Unique identifier for a physical page
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageId(pub u64);

/// A physical page that can be shared between processes via COW
///
/// Pages are reference-counted using Arc. When a COW page is written to,
/// it is cloned and the writing process gets its own private copy.
#[derive(Debug, Clone)]
pub struct Page {
    /// The actual page data
    data: Arc<Vec<u8>>,
}

impl Page {
    /// Create a new page with zeroed data
    pub fn new() -> Self {
        Self {
            data: Arc::new(vec![0u8; PAGE_SIZE]),
        }
    }

    /// Create a page from existing data
    pub fn from_data(data: Vec<u8>) -> Self {
        debug_assert!(data.len() <= PAGE_SIZE);
        let mut padded = data;
        padded.resize(PAGE_SIZE, 0);
        Self {
            data: Arc::new(padded),
        }
    }

    /// Get the reference count (1 = private, >1 = shared/COW)
    pub fn ref_count(&self) -> usize {
        Arc::strong_count(&self.data)
    }

    /// Check if this page is shared (COW)
    pub fn is_shared(&self) -> bool {
        Arc::strong_count(&self.data) > 1
    }

    /// Read from the page
    pub fn read(&self, offset: usize, buf: &mut [u8]) -> usize {
        if offset >= PAGE_SIZE {
            return 0;
        }
        let available = PAGE_SIZE - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&self.data[offset..offset + to_read]);
        to_read
    }

    /// Write to the page, performing COW if necessary
    /// Returns true if COW was triggered (page was copied)
    pub fn write(&mut self, offset: usize, buf: &[u8]) -> (usize, bool) {
        if offset >= PAGE_SIZE {
            return (0, false);
        }

        let available = PAGE_SIZE - offset;
        let to_write = buf.len().min(available);

        // COW: if shared, we need to make a private copy
        let cow_triggered = self.is_shared();
        if cow_triggered {
            // Clone the data to get our own private copy
            let mut new_data = (*self.data).clone();
            new_data[offset..offset + to_write].copy_from_slice(&buf[..to_write]);
            self.data = Arc::new(new_data);
        } else {
            // We have exclusive access, modify in place
            Arc::make_mut(&mut self.data)[offset..offset + to_write]
                .copy_from_slice(&buf[..to_write]);
        }

        (to_write, cow_triggered)
    }

    /// Get the raw data slice
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }
}

impl Default for Page {
    fn default() -> Self {
        Self::new()
    }
}

/// COW-aware memory data storage
///
/// Manages a contiguous memory region using pages. Supports COW semantics
/// where pages are shared until written.
#[derive(Debug, Clone)]
pub struct CowMemory {
    /// Pages making up this memory region
    pages: Vec<Page>,
    /// Total size in bytes (may be less than pages.len() * PAGE_SIZE)
    size: usize,
}

impl CowMemory {
    /// Create a new COW memory region of the given size
    pub fn new(size: usize) -> Self {
        let num_pages = size.div_ceil(PAGE_SIZE);
        let pages = (0..num_pages).map(|_| Page::new()).collect();
        Self { pages, size }
    }

    /// Create from existing data
    pub fn from_data(data: Vec<u8>) -> Self {
        let size = data.len();
        let num_pages = size.div_ceil(PAGE_SIZE);
        let mut pages = Vec::with_capacity(num_pages);

        for i in 0..num_pages {
            let start = i * PAGE_SIZE;
            let end = (start + PAGE_SIZE).min(data.len());
            let page_data = data[start..end].to_vec();
            pages.push(Page::from_data(page_data));
        }

        Self { pages, size }
    }

    /// Get the size of the memory region
    pub fn size(&self) -> usize {
        self.size
    }

    /// Get the number of pages
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Count shared (COW) pages
    pub fn shared_page_count(&self) -> usize {
        self.pages.iter().filter(|p| p.is_shared()).count()
    }

    /// Count private (non-shared) pages
    pub fn private_page_count(&self) -> usize {
        self.pages.iter().filter(|p| !p.is_shared()).count()
    }

    /// Read from the memory region
    pub fn read(&self, offset: usize, buf: &mut [u8]) -> usize {
        if offset >= self.size {
            return 0;
        }

        let available = self.size - offset;
        let to_read = buf.len().min(available);
        let mut bytes_read = 0;

        while bytes_read < to_read {
            let current_offset = offset + bytes_read;
            let page_idx = current_offset / PAGE_SIZE;
            let page_offset = current_offset % PAGE_SIZE;

            if page_idx >= self.pages.len() {
                break;
            }

            let remaining = to_read - bytes_read;
            let page_remaining = PAGE_SIZE - page_offset;
            let chunk_size = remaining.min(page_remaining);

            let read = self.pages[page_idx]
                .read(page_offset, &mut buf[bytes_read..bytes_read + chunk_size]);
            bytes_read += read;

            if read < chunk_size {
                break;
            }
        }

        bytes_read
    }

    /// Write to the memory region, performing COW as needed
    /// Returns (bytes_written, cow_faults) where cow_faults is the number of pages copied
    pub fn write(&mut self, offset: usize, buf: &[u8]) -> (usize, usize) {
        if offset >= self.size {
            return (0, 0);
        }

        let available = self.size - offset;
        let to_write = buf.len().min(available);
        let mut bytes_written = 0;
        let mut cow_faults = 0;

        while bytes_written < to_write {
            let current_offset = offset + bytes_written;
            let page_idx = current_offset / PAGE_SIZE;
            let page_offset = current_offset % PAGE_SIZE;

            if page_idx >= self.pages.len() {
                break;
            }

            let remaining = to_write - bytes_written;
            let page_remaining = PAGE_SIZE - page_offset;
            let chunk_size = remaining.min(page_remaining);

            let (written, cow_triggered) = self.pages[page_idx]
                .write(page_offset, &buf[bytes_written..bytes_written + chunk_size]);
            bytes_written += written;
            if cow_triggered {
                cow_faults += 1;
            }

            if written < chunk_size {
                break;
            }
        }

        (bytes_written, cow_faults)
    }

    /// Get raw slice of all data (for compatibility)
    pub fn as_slice(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(self.size);
        for (i, page) in self.pages.iter().enumerate() {
            let remaining = self.size.saturating_sub(i * PAGE_SIZE);
            let to_copy = remaining.min(PAGE_SIZE);
            data.extend_from_slice(&page.as_slice()[..to_copy]);
        }
        data
    }

    /// Clone this memory region for COW fork
    /// Pages are shared (not copied) until written
    pub fn cow_clone(&self) -> Self {
        Self {
            pages: self.pages.clone(), // Arc clone - shares the data
            size: self.size,
        }
    }
}

/// Statistics about COW memory usage
#[derive(Debug, Clone, Copy, Default)]
pub struct CowStats {
    /// Total pages in the region
    pub total_pages: usize,
    /// Pages that are shared (COW)
    pub shared_pages: usize,
    /// Pages that are private
    pub private_pages: usize,
    /// Total COW faults (copies) that have occurred
    pub cow_faults: usize,
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
    /// The actual data (COW-enabled)
    data: CowMemory,
    /// Is this region part of shared memory?
    shared: Option<ShmId>,
    /// Total COW faults for this region
    cow_faults: usize,
}

impl MemoryRegion {
    /// Create a new private memory region
    pub fn new(id: RegionId, size: usize, protection: Protection) -> Self {
        Self {
            id,
            size,
            protection,
            data: CowMemory::new(size),
            shared: None,
            cow_faults: 0,
        }
    }

    /// Create a region backed by shared memory
    pub fn from_shared(id: RegionId, shm_id: ShmId, data: Vec<u8>, protection: Protection) -> Self {
        let size = data.len();
        Self {
            id,
            size,
            protection,
            data: CowMemory::from_data(data),
            shared: Some(shm_id),
            cow_faults: 0,
        }
    }

    /// Create a COW clone of this region (for fork)
    pub fn cow_clone(&self, new_id: RegionId) -> Self {
        Self {
            id: new_id,
            size: self.size,
            protection: self.protection,
            data: self.data.cow_clone(),
            shared: self.shared,
            cow_faults: 0,
        }
    }

    pub fn read(&self, offset: usize, buf: &mut [u8]) -> Result<usize, MemoryError> {
        if !self.protection.read {
            return Err(MemoryError::PermissionDenied);
        }

        if offset >= self.size {
            return Ok(0);
        }

        Ok(self.data.read(offset, buf))
    }

    pub fn write(&mut self, offset: usize, buf: &[u8]) -> Result<usize, MemoryError> {
        if !self.protection.write {
            return Err(MemoryError::PermissionDenied);
        }

        if offset >= self.size {
            return Err(MemoryError::OutOfBounds);
        }

        let (written, faults) = self.data.write(offset, buf);
        self.cow_faults += faults;
        Ok(written)
    }

    /// Get data as a byte slice (copies from pages)
    pub fn as_slice(&self) -> Vec<u8> {
        self.data.as_slice()
    }

    /// Check if this region has shared memory backing
    pub fn is_shared(&self) -> bool {
        self.shared.is_some()
    }

    pub fn shm_id(&self) -> Option<ShmId> {
        self.shared
    }

    /// Get COW statistics for this region
    pub fn cow_stats(&self) -> CowStats {
        CowStats {
            total_pages: self.data.page_count(),
            shared_pages: self.data.shared_page_count(),
            private_pages: self.data.private_page_count(),
            cow_faults: self.cow_faults,
        }
    }

    /// Check if any pages in this region are shared (COW)
    pub fn has_cow_pages(&self) -> bool {
        self.data.shared_page_count() > 0
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
    pub fn new() -> Self {
        Self {
            regions: HashMap::new(),
            allocated: 0,
            limit: 0, // unlimited by default
            peak: 0,
            attached_shm: HashMap::new(),
        }
    }

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

    pub fn limit(&self) -> usize {
        self.limit
    }

    pub fn allocate(
        &mut self,
        id: RegionId,
        size: usize,
        prot: Protection,
    ) -> Result<(), MemoryError> {
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

    pub fn free(&mut self, id: RegionId) -> Result<(), MemoryError> {
        let region = self.regions.remove(&id).ok_or(MemoryError::InvalidRegion)?;

        // If it's shared memory, just remove our mapping
        if let Some(shm_id) = region.shared {
            self.attached_shm.remove(&shm_id);
        }

        self.allocated = self.allocated.saturating_sub(region.size);
        Ok(())
    }

    pub fn get(&self, id: RegionId) -> Option<&MemoryRegion> {
        self.regions.get(&id)
    }

    pub fn get_mut(&mut self, id: RegionId) -> Option<&mut MemoryRegion> {
        self.regions.get_mut(&id)
    }

    pub fn allocated(&self) -> usize {
        self.allocated
    }

    pub fn peak(&self) -> usize {
        self.peak
    }

    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    pub fn attach_shm(
        &mut self,
        shm_id: ShmId,
        region: MemoryRegion,
    ) -> Result<RegionId, MemoryError> {
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

    pub fn detach_shm(&mut self, shm_id: ShmId) -> Result<(), MemoryError> {
        let region_id = self
            .attached_shm
            .remove(&shm_id)
            .ok_or(MemoryError::NotAttached)?;

        if let Some(region) = self.regions.remove(&region_id) {
            self.allocated = self.allocated.saturating_sub(region.size);
        }

        Ok(())
    }

    pub fn is_attached(&self, shm_id: ShmId) -> bool {
        self.attached_shm.contains_key(&shm_id)
    }

    pub fn shm_region(&self, shm_id: ShmId) -> Option<RegionId> {
        self.attached_shm.get(&shm_id).copied()
    }

    pub fn regions(&self) -> impl Iterator<Item = &MemoryRegion> {
        self.regions.values()
    }

    /// Clone this process memory for fork with COW semantics
    ///
    /// Returns a new ProcessMemory where all regions are COW clones.
    /// The region_id_generator is used to assign new IDs to the cloned regions.
    /// Returns (new_memory, old_to_new_id_mapping).
    pub fn cow_fork<F>(&self, mut region_id_generator: F) -> (Self, HashMap<RegionId, RegionId>)
    where
        F: FnMut() -> RegionId,
    {
        let mut new_memory = ProcessMemory {
            regions: HashMap::new(),
            allocated: self.allocated,
            limit: self.limit,
            peak: self.peak,
            attached_shm: HashMap::new(), // Shared memory not inherited in fork
        };

        let mut id_mapping = HashMap::new();

        for (old_id, region) in &self.regions {
            // Skip shared memory regions - they're not COW cloned
            if region.is_shared() {
                continue;
            }

            let new_id = region_id_generator();
            let new_region = region.cow_clone(new_id);
            id_mapping.insert(*old_id, new_id);
            new_memory.regions.insert(new_id, new_region);
        }

        // Recalculate allocated based on actual regions
        new_memory.allocated = new_memory.regions.values().map(|r| r.size).sum();

        (new_memory, id_mapping)
    }

    /// Get COW statistics for all regions
    pub fn cow_stats(&self) -> ProcessCowStats {
        let mut total_pages = 0;
        let mut shared_pages = 0;
        let mut private_pages = 0;
        let mut total_cow_faults = 0;

        for region in self.regions.values() {
            let stats = region.cow_stats();
            total_pages += stats.total_pages;
            shared_pages += stats.shared_pages;
            private_pages += stats.private_pages;
            total_cow_faults += stats.cow_faults;
        }

        ProcessCowStats {
            total_pages,
            shared_pages,
            private_pages,
            total_cow_faults,
            regions_with_cow: self.regions.values().filter(|r| r.has_cow_pages()).count(),
        }
    }

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

/// COW statistics for a process
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessCowStats {
    /// Total pages across all regions
    pub total_pages: usize,
    /// Pages that are shared (COW)
    pub shared_pages: usize,
    /// Pages that are private
    pub private_pages: usize,
    /// Total COW faults (page copies)
    pub total_cow_faults: usize,
    /// Number of regions with COW pages
    pub regions_with_cow: usize,
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
    /// Creator is NOT auto-attached; they must call shmat explicitly
    pub fn new(id: ShmId, size: usize, creator: Pid) -> Self {
        Self {
            id,
            size,
            data: vec![0u8; size],
            attached: Vec::new(),
            creator,
            refcount: 0,
        }
    }

    pub fn attach(&mut self, pid: Pid) {
        if !self.attached.contains(&pid) {
            self.attached.push(pid);
            self.refcount += 1;
        }
    }

    /// Returns true if refcount dropped to 0 (segment should be removed)
    pub fn detach(&mut self, pid: Pid) -> bool {
        if let Some(pos) = self.attached.iter().position(|&p| p == pid) {
            self.attached.remove(pos);
            self.refcount = self.refcount.saturating_sub(1);
        }
        self.refcount == 0
    }

    pub fn is_attached(&self, pid: Pid) -> bool {
        self.attached.contains(&pid)
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    pub fn clone_data(&self) -> Vec<u8> {
        self.data.clone()
    }

    pub fn refcount(&self) -> usize {
        self.refcount
    }

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
    pub fn new() -> Self {
        Self {
            next_region_id: AtomicU64::new(1),
            next_shm_id: AtomicU64::new(1),
            shared_segments: HashMap::new(),
            system_limit: 0,
            total_allocated: 0,
        }
    }

    pub fn set_system_limit(&mut self, limit: usize) {
        self.system_limit = limit;
    }

    pub fn alloc_region_id(&self) -> RegionId {
        RegionId(self.next_region_id.fetch_add(1, Ordering::Relaxed))
    }

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

    /// Attach to a shared memory segment. Creates a region with a copy of the
    /// shared data - in a real implementation this would share memory, but here
    /// we simulate it by copying and syncing on access.
    pub fn shmat(
        &mut self,
        shm_id: ShmId,
        pid: Pid,
        prot: Protection,
    ) -> Result<MemoryRegion, MemoryError> {
        let region_id = self.alloc_region_id();

        let shm = self
            .shared_segments
            .get_mut(&shm_id)
            .ok_or(MemoryError::ShmNotFound)?;

        shm.attach(pid);

        let data = shm.clone_data();
        let region = MemoryRegion::from_shared(region_id, shm_id, data, prot);

        Ok(region)
    }

    pub fn shmdt(&mut self, shm_id: ShmId, pid: Pid) -> Result<bool, MemoryError> {
        let shm = self
            .shared_segments
            .get_mut(&shm_id)
            .ok_or(MemoryError::ShmNotFound)?;

        let should_remove = shm.detach(pid);

        if should_remove && let Some(removed) = self.shared_segments.remove(&shm_id) {
            self.total_allocated = self.total_allocated.saturating_sub(removed.size);
        }

        Ok(should_remove)
    }

    /// Write local changes back to shared segment
    pub fn shm_sync(&mut self, shm_id: ShmId, data: &[u8]) -> Result<(), MemoryError> {
        let shm = self
            .shared_segments
            .get_mut(&shm_id)
            .ok_or(MemoryError::ShmNotFound)?;

        if data.len() != shm.size {
            return Err(MemoryError::InvalidSize);
        }

        shm.data_mut().copy_from_slice(data);
        Ok(())
    }

    pub fn shm_read(&self, shm_id: ShmId) -> Result<&[u8], MemoryError> {
        let shm = self
            .shared_segments
            .get(&shm_id)
            .ok_or(MemoryError::ShmNotFound)?;
        Ok(shm.data())
    }

    pub fn shm_info(&self, shm_id: ShmId) -> Result<ShmInfo, MemoryError> {
        let shm = self
            .shared_segments
            .get(&shm_id)
            .ok_or(MemoryError::ShmNotFound)?;
        Ok(ShmInfo {
            id: shm_id,
            size: shm.size,
            attached_count: shm.attached_count(),
            creator: shm.creator,
        })
    }

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

    pub fn total_allocated(&self) -> usize {
        self.total_allocated
    }

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
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemMemoryStats {
    pub total_allocated: usize,
    pub system_limit: usize,
    pub shm_count: usize,
    pub shm_total_size: usize,
}

// ============================================================================
// Memory-Mapped Files
// ============================================================================

/// Unique identifier for a memory-mapped file region
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MmapId(pub u64);

/// Flags for memory-mapped files
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MmapFlags {
    /// Map is shared - writes are visible to other processes
    pub shared: bool,
    /// Map is private (copy-on-write) - writes are private
    pub private: bool,
    /// Map is anonymous (not backed by a file)
    pub anonymous: bool,
    /// Map is fixed at the specified address (hint only in our implementation)
    pub fixed: bool,
}

impl Default for MmapFlags {
    fn default() -> Self {
        Self {
            shared: false,
            private: true,
            anonymous: false,
            fixed: false,
        }
    }
}

impl MmapFlags {
    /// Create flags for a shared file mapping
    pub fn shared() -> Self {
        Self {
            shared: true,
            private: false,
            anonymous: false,
            fixed: false,
        }
    }

    /// Create flags for a private (COW) file mapping
    pub fn private() -> Self {
        Self {
            shared: false,
            private: true,
            anonymous: false,
            fixed: false,
        }
    }

    /// Create flags for an anonymous mapping (not backed by file)
    pub fn anonymous() -> Self {
        Self {
            shared: false,
            private: true,
            anonymous: true,
            fixed: false,
        }
    }
}

/// A memory-mapped file region
#[derive(Debug)]
pub struct MmapRegion {
    /// Unique identifier
    pub id: MmapId,
    /// The memory region containing the mapped data
    pub region_id: RegionId,
    /// The file path this is mapped from (None for anonymous)
    pub file_path: Option<String>,
    /// Offset within the file
    pub file_offset: usize,
    /// Size of the mapping
    pub size: usize,
    /// Mapping flags
    pub flags: MmapFlags,
    /// Protection flags
    pub protection: Protection,
    /// Whether the mapping has been modified (dirty)
    dirty: bool,
}

impl MmapRegion {
    /// Check if this mapping is dirty (modified)
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark the mapping as dirty
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Clear the dirty flag (after sync)
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Check if this is an anonymous mapping
    pub fn is_anonymous(&self) -> bool {
        self.flags.anonymous
    }

    /// Check if this is a shared mapping
    pub fn is_shared(&self) -> bool {
        self.flags.shared
    }

    /// Check if this is a private (COW) mapping
    pub fn is_private(&self) -> bool {
        self.flags.private
    }
}

/// Memory-mapped file manager
///
/// Manages mappings between files and memory regions. Supports:
/// - Private (COW) mappings: changes are not written back to the file
/// - Shared mappings: changes can be synced back to the file
/// - Anonymous mappings: memory not backed by any file
#[derive(Debug, Default)]
pub struct MmapManager {
    /// Next mmap ID
    next_id: u64,
    /// Active mappings
    mappings: HashMap<MmapId, MmapRegion>,
    /// Mapping from region ID to mmap ID for quick lookup
    region_to_mmap: HashMap<RegionId, MmapId>,
}

impl MmapManager {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            mappings: HashMap::new(),
            region_to_mmap: HashMap::new(),
        }
    }

    /// Create a memory mapping from file data
    ///
    /// The caller is responsible for reading the file data and passing it here.
    /// This allows the mmap manager to be decoupled from the VFS.
    pub fn mmap(
        &mut self,
        region_id: RegionId,
        file_path: Option<String>,
        file_offset: usize,
        size: usize,
        protection: Protection,
        flags: MmapFlags,
    ) -> MmapId {
        let id = MmapId(self.next_id);
        self.next_id += 1;

        let region = MmapRegion {
            id,
            region_id,
            file_path,
            file_offset,
            size,
            flags,
            protection,
            dirty: false,
        };

        self.mappings.insert(id, region);
        self.region_to_mmap.insert(region_id, id);

        id
    }

    /// Remove a memory mapping
    pub fn munmap(&mut self, id: MmapId) -> Option<MmapRegion> {
        if let Some(region) = self.mappings.remove(&id) {
            self.region_to_mmap.remove(&region.region_id);
            Some(region)
        } else {
            None
        }
    }

    /// Get a mapping by ID
    pub fn get(&self, id: MmapId) -> Option<&MmapRegion> {
        self.mappings.get(&id)
    }

    /// Get a mutable mapping by ID
    pub fn get_mut(&mut self, id: MmapId) -> Option<&mut MmapRegion> {
        self.mappings.get_mut(&id)
    }

    /// Get a mapping by its memory region ID
    pub fn get_by_region(&self, region_id: RegionId) -> Option<&MmapRegion> {
        self.region_to_mmap
            .get(&region_id)
            .and_then(|id| self.mappings.get(id))
    }

    /// Get a mutable mapping by its memory region ID
    pub fn get_by_region_mut(&mut self, region_id: RegionId) -> Option<&mut MmapRegion> {
        if let Some(&id) = self.region_to_mmap.get(&region_id) {
            self.mappings.get_mut(&id)
        } else {
            None
        }
    }

    /// Mark a mapping as dirty (modified)
    pub fn mark_dirty(&mut self, id: MmapId) -> bool {
        if let Some(region) = self.mappings.get_mut(&id) {
            region.mark_dirty();
            true
        } else {
            false
        }
    }

    /// Get all dirty shared mappings (for msync)
    pub fn dirty_shared_mappings(&self) -> Vec<&MmapRegion> {
        self.mappings
            .values()
            .filter(|r| r.is_dirty() && r.is_shared())
            .collect()
    }

    /// List all mappings
    pub fn list(&self) -> Vec<MmapInfo> {
        self.mappings
            .values()
            .map(|r| MmapInfo {
                id: r.id,
                region_id: r.region_id,
                file_path: r.file_path.clone(),
                file_offset: r.file_offset,
                size: r.size,
                is_shared: r.is_shared(),
                is_anonymous: r.is_anonymous(),
                is_dirty: r.is_dirty(),
            })
            .collect()
    }

    /// Get the count of active mappings
    pub fn count(&self) -> usize {
        self.mappings.len()
    }
}

/// Information about a memory mapping
#[derive(Debug, Clone)]
pub struct MmapInfo {
    pub id: MmapId,
    pub region_id: RegionId,
    pub file_path: Option<String>,
    pub file_offset: usize,
    pub size: usize,
    pub is_shared: bool,
    pub is_anonymous: bool,
    pub is_dirty: bool,
}

// ============================================================================
// Memory Pools (Arena Allocation)
// ============================================================================

/// Unique identifier for a memory pool
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PoolId(pub u64);

/// A memory pool for arena-style allocation
///
/// Memory pools provide fast allocation for many small objects of the same size.
/// Objects are allocated from a contiguous chunk and freed all at once.
#[derive(Debug)]
pub struct MemoryPool {
    /// Pool identifier
    pub id: PoolId,
    /// Size of each object in the pool
    pub object_size: usize,
    /// Maximum number of objects
    pub capacity: usize,
    /// The backing memory
    data: Vec<u8>,
    /// Bitmap of allocated slots (true = allocated)
    allocated: Vec<bool>,
    /// Number of allocated objects
    alloc_count: usize,
    /// Free list for O(1) allocation
    free_list: Vec<usize>,
}

impl MemoryPool {
    /// Create a new memory pool
    ///
    /// # Arguments
    /// * `id` - Unique identifier for this pool
    /// * `object_size` - Size of each object in bytes
    /// * `capacity` - Maximum number of objects
    pub fn new(id: PoolId, object_size: usize, capacity: usize) -> Self {
        assert!(object_size > 0, "object_size must be > 0");
        assert!(capacity > 0, "capacity must be > 0");

        let total_size = object_size * capacity;
        let data = vec![0u8; total_size];
        let allocated = vec![false; capacity];
        let free_list = (0..capacity).rev().collect(); // Stack of free indices

        Self {
            id,
            object_size,
            capacity,
            data,
            allocated,
            alloc_count: 0,
            free_list,
        }
    }

    /// Allocate an object from the pool
    ///
    /// Returns the offset within the pool where the object is allocated,
    /// or None if the pool is full.
    pub fn alloc(&mut self) -> Option<usize> {
        if let Some(slot) = self.free_list.pop() {
            self.allocated[slot] = true;
            self.alloc_count += 1;
            Some(slot * self.object_size)
        } else {
            None // Pool is full
        }
    }

    /// Free an object at the given offset
    ///
    /// Returns true if the object was freed, false if the offset was invalid.
    pub fn free(&mut self, offset: usize) -> bool {
        if !offset.is_multiple_of(self.object_size) {
            return false; // Not aligned to object boundary
        }

        let slot = offset / self.object_size;
        if slot >= self.capacity || !self.allocated[slot] {
            return false; // Out of bounds or not allocated
        }

        // Clear the memory (optional, but good for security/debugging)
        let start = slot * self.object_size;
        let end = start + self.object_size;
        self.data[start..end].fill(0);

        self.allocated[slot] = false;
        self.alloc_count -= 1;
        self.free_list.push(slot);

        true
    }

    /// Read from an allocated object
    pub fn read(&self, offset: usize, buf: &mut [u8]) -> Option<usize> {
        let slot = offset / self.object_size;
        if slot >= self.capacity || !self.allocated[slot] {
            return None;
        }

        let start = offset;
        let slot_end = (slot + 1) * self.object_size;
        let available = slot_end - start;
        let to_read = buf.len().min(available);

        buf[..to_read].copy_from_slice(&self.data[start..start + to_read]);
        Some(to_read)
    }

    /// Write to an allocated object
    pub fn write(&mut self, offset: usize, buf: &[u8]) -> Option<usize> {
        let slot = offset / self.object_size;
        if slot >= self.capacity || !self.allocated[slot] {
            return None;
        }

        let start = offset;
        let slot_end = (slot + 1) * self.object_size;
        let available = slot_end - start;
        let to_write = buf.len().min(available);

        self.data[start..start + to_write].copy_from_slice(&buf[..to_write]);
        Some(to_write)
    }

    /// Get the number of allocated objects
    pub fn alloc_count(&self) -> usize {
        self.alloc_count
    }

    /// Get the number of free slots
    pub fn free_count(&self) -> usize {
        self.capacity - self.alloc_count
    }

    /// Check if the pool is full
    pub fn is_full(&self) -> bool {
        self.alloc_count == self.capacity
    }

    /// Check if the pool is empty
    pub fn is_empty(&self) -> bool {
        self.alloc_count == 0
    }

    /// Get the total memory used by this pool
    pub fn total_size(&self) -> usize {
        self.data.len()
    }

    /// Get the memory in use (allocated objects)
    pub fn used_size(&self) -> usize {
        self.alloc_count * self.object_size
    }

    /// Reset the pool, freeing all allocations
    pub fn reset(&mut self) {
        self.data.fill(0);
        self.allocated.fill(false);
        self.alloc_count = 0;
        self.free_list = (0..self.capacity).rev().collect();
    }
}

/// Memory pool manager
///
/// Manages multiple memory pools for different object sizes.
#[derive(Debug, Default)]
pub struct PoolManager {
    /// Next pool ID
    next_id: u64,
    /// Active pools
    pools: HashMap<PoolId, MemoryPool>,
}

impl PoolManager {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            pools: HashMap::new(),
        }
    }

    /// Create a new memory pool
    pub fn create_pool(&mut self, object_size: usize, capacity: usize) -> PoolId {
        let id = PoolId(self.next_id);
        self.next_id += 1;

        let pool = MemoryPool::new(id, object_size, capacity);
        self.pools.insert(id, pool);

        id
    }

    /// Delete a memory pool
    pub fn delete_pool(&mut self, id: PoolId) -> Option<MemoryPool> {
        self.pools.remove(&id)
    }

    /// Get a pool by ID
    pub fn get(&self, id: PoolId) -> Option<&MemoryPool> {
        self.pools.get(&id)
    }

    /// Get a mutable pool by ID
    pub fn get_mut(&mut self, id: PoolId) -> Option<&mut MemoryPool> {
        self.pools.get_mut(&id)
    }

    /// Allocate from a pool
    pub fn alloc(&mut self, id: PoolId) -> Option<usize> {
        self.pools.get_mut(&id).and_then(|p| p.alloc())
    }

    /// Free in a pool
    pub fn free(&mut self, id: PoolId, offset: usize) -> bool {
        self.pools
            .get_mut(&id)
            .map(|p| p.free(offset))
            .unwrap_or(false)
    }

    /// Get statistics for all pools
    pub fn stats(&self) -> PoolStats {
        let mut total_pools = 0;
        let mut total_capacity = 0;
        let mut total_allocated = 0;
        let mut total_memory = 0;

        for pool in self.pools.values() {
            total_pools += 1;
            total_capacity += pool.capacity;
            total_allocated += pool.alloc_count();
            total_memory += pool.total_size();
        }

        PoolStats {
            total_pools,
            total_capacity,
            total_allocated,
            total_memory,
        }
    }

    /// List all pools
    pub fn list(&self) -> Vec<PoolInfo> {
        self.pools
            .values()
            .map(|p| PoolInfo {
                id: p.id,
                object_size: p.object_size,
                capacity: p.capacity,
                alloc_count: p.alloc_count(),
                total_size: p.total_size(),
            })
            .collect()
    }
}

/// Statistics for all memory pools
#[derive(Debug, Clone, Copy, Default)]
pub struct PoolStats {
    pub total_pools: usize,
    pub total_capacity: usize,
    pub total_allocated: usize,
    pub total_memory: usize,
}

/// Information about a single pool
#[derive(Debug, Clone)]
pub struct PoolInfo {
    pub id: PoolId,
    pub object_size: usize,
    pub capacity: usize,
    pub alloc_count: usize,
    pub total_size: usize,
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
        assert_eq!(region.write(0, b"test"), Err(MemoryError::PermissionDenied));
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
        mem.allocate(RegionId(1), 500, Protection::READ_WRITE)
            .unwrap();
        assert_eq!(mem.allocated(), 500);

        // Allocate more within limit
        mem.allocate(RegionId(2), 400, Protection::READ_WRITE)
            .unwrap();
        assert_eq!(mem.allocated(), 900);

        // Exceed limit
        assert_eq!(
            mem.allocate(RegionId(3), 200, Protection::READ_WRITE),
            Err(MemoryError::OutOfMemory)
        );

        // Free and allocate again
        mem.free(RegionId(1)).unwrap();
        mem.allocate(RegionId(3), 200, Protection::READ_WRITE)
            .unwrap();
        assert_eq!(mem.allocated(), 600);
    }

    #[test]
    fn test_process_memory_peak() {
        let mut mem = ProcessMemory::new();

        mem.allocate(RegionId(1), 1000, Protection::READ_WRITE)
            .unwrap();
        mem.allocate(RegionId(2), 500, Protection::READ_WRITE)
            .unwrap();
        assert_eq!(mem.peak(), 1500);

        mem.free(RegionId(1)).unwrap();
        assert_eq!(mem.allocated(), 500);
        assert_eq!(mem.peak(), 1500); // Peak unchanged
    }

    #[test]
    fn test_process_memory_stats() {
        let mut mem = ProcessMemory::with_limit(5000);
        mem.allocate(RegionId(1), 1000, Protection::READ_WRITE)
            .unwrap();
        mem.allocate(RegionId(2), 2000, Protection::READ_WRITE)
            .unwrap();

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
        assert_eq!(
            mem.attach_shm(shm_id, region2),
            Err(MemoryError::AlreadyAttached)
        );
    }

    // ========================================================================
    // Copy-on-Write (COW) Tests
    // ========================================================================

    #[test]
    fn test_page_basic() {
        let mut page = Page::new();

        // Initially all zeros
        let mut buf = [0u8; 10];
        page.read(0, &mut buf);
        assert!(buf.iter().all(|&b| b == 0));

        // Write some data
        let (written, cow) = page.write(0, b"hello");
        assert_eq!(written, 5);
        assert!(!cow); // No COW on fresh page

        // Read it back
        page.read(0, &mut buf);
        assert_eq!(&buf[..5], b"hello");
    }

    #[test]
    fn test_page_cow_on_clone() {
        let page1 = Page::from_data(b"hello world".to_vec());

        // Clone the page (simulating COW)
        let mut page2 = page1.clone();

        // Both pages share the data
        assert!(page1.is_shared());
        assert!(page2.is_shared());
        assert_eq!(page1.ref_count(), 2);
        assert_eq!(page2.ref_count(), 2);

        // Read from both - should see same data
        let mut buf1 = [0u8; 11];
        let mut buf2 = [0u8; 11];
        page1.read(0, &mut buf1);
        page2.read(0, &mut buf2);
        assert_eq!(&buf1[..11], b"hello world");
        assert_eq!(&buf2[..11], b"hello world");

        // Write to page2 - should trigger COW
        let (written, cow_triggered) = page2.write(0, b"HELLO");
        assert_eq!(written, 5);
        assert!(cow_triggered);

        // page2 now has its own data
        assert!(!page2.is_shared());
        assert_eq!(page2.ref_count(), 1);

        // page1 is no longer shared (only ref remaining)
        assert!(!page1.is_shared());
        assert_eq!(page1.ref_count(), 1);

        // Data is different
        page1.read(0, &mut buf1);
        page2.read(0, &mut buf2);
        assert_eq!(&buf1[..11], b"hello world");
        assert_eq!(&buf2[..11], b"HELLO world");
    }

    #[test]
    fn test_cow_memory_basic() {
        let mut mem = CowMemory::new(PAGE_SIZE * 2 + 100);

        assert_eq!(mem.size(), PAGE_SIZE * 2 + 100);
        assert_eq!(mem.page_count(), 3); // 3 pages for the size

        // Write across page boundary
        let data = vec![0xAA; PAGE_SIZE + 10];
        let (written, faults) = mem.write(PAGE_SIZE - 5, &data);
        assert_eq!(written, PAGE_SIZE + 10);
        assert_eq!(faults, 0); // No COW on fresh memory

        // Read it back
        let mut buf = vec![0u8; PAGE_SIZE + 10];
        let read = mem.read(PAGE_SIZE - 5, &mut buf);
        assert_eq!(read, PAGE_SIZE + 10);
        assert!(buf.iter().all(|&b| b == 0xAA));
    }

    #[test]
    fn test_cow_memory_clone() {
        let mut mem1 = CowMemory::new(PAGE_SIZE * 2);

        // Write some data
        mem1.write(0, b"page1 data");
        mem1.write(PAGE_SIZE, b"page2 data");

        // COW clone
        let mut mem2 = mem1.cow_clone();

        // Both have shared pages
        assert_eq!(mem1.shared_page_count(), 2);
        assert_eq!(mem2.shared_page_count(), 2);

        // Read from both - same data
        let mut buf1 = [0u8; 10];
        let mut buf2 = [0u8; 10];
        mem1.read(0, &mut buf1);
        mem2.read(0, &mut buf2);
        assert_eq!(&buf1, &buf2);

        // Write to mem2 - COW
        let (_, faults) = mem2.write(0, b"MODIFIED!!");
        assert_eq!(faults, 1); // One page was COW copied

        // mem2's first page is now private
        assert_eq!(mem2.shared_page_count(), 1);
        assert_eq!(mem2.private_page_count(), 1);

        // mem1's first page is also now private (only 1 ref)
        assert_eq!(mem1.shared_page_count(), 1);
        assert_eq!(mem1.private_page_count(), 1);

        // Data is different
        mem1.read(0, &mut buf1);
        mem2.read(0, &mut buf2);
        assert_eq!(&buf1, b"page1 data");
        assert_eq!(&buf2, b"MODIFIED!!");
    }

    #[test]
    fn test_memory_region_cow_clone() {
        let id1 = RegionId(1);
        let id2 = RegionId(2);

        let mut region1 = MemoryRegion::new(id1, PAGE_SIZE * 2, Protection::READ_WRITE);
        region1.write(0, b"test data").unwrap();

        // COW clone
        let mut region2 = region1.cow_clone(id2);

        assert_eq!(region2.id, id2);
        assert_eq!(region2.size, region1.size);
        assert!(region1.has_cow_pages());
        assert!(region2.has_cow_pages());

        // Read from both
        let mut buf = [0u8; 9];
        region1.read(0, &mut buf).unwrap();
        assert_eq!(&buf, b"test data");
        region2.read(0, &mut buf).unwrap();
        assert_eq!(&buf, b"test data");

        // Write to region2 triggers COW
        region2.write(0, b"MODIFIED!").unwrap();

        let stats = region2.cow_stats();
        assert!(stats.cow_faults > 0);

        // Data is different
        region1.read(0, &mut buf).unwrap();
        assert_eq!(&buf, b"test data");
        region2.read(0, &mut buf).unwrap();
        assert_eq!(&buf, b"MODIFIED!");
    }

    #[test]
    fn test_process_memory_cow_fork() {
        let mut mem1 = ProcessMemory::new();

        // Allocate some regions
        mem1.allocate(RegionId(1), PAGE_SIZE, Protection::READ_WRITE)
            .unwrap();
        mem1.allocate(RegionId(2), PAGE_SIZE * 2, Protection::READ_WRITE)
            .unwrap();

        // Write data
        mem1.get_mut(RegionId(1))
            .unwrap()
            .write(0, b"region1")
            .unwrap();
        mem1.get_mut(RegionId(2))
            .unwrap()
            .write(0, b"region2")
            .unwrap();

        // COW fork
        let mut next_id = 10u64;
        let (mut mem2, mapping) = mem1.cow_fork(|| {
            let id = RegionId(next_id);
            next_id += 1;
            id
        });

        // Check mapping
        assert_eq!(mapping.len(), 2);
        assert!(mapping.contains_key(&RegionId(1)));
        assert!(mapping.contains_key(&RegionId(2)));

        // Both have same allocated size
        assert_eq!(mem2.allocated(), mem1.allocated());

        // Get COW stats
        let stats1 = mem1.cow_stats();
        let stats2 = mem2.cow_stats();
        assert!(stats1.shared_pages > 0);
        assert!(stats2.shared_pages > 0);

        // Write to child - triggers COW
        let child_region1_id = mapping[&RegionId(1)];
        mem2.get_mut(child_region1_id)
            .unwrap()
            .write(0, b"CHILD!!")
            .unwrap();

        // Parent unchanged
        let mut buf = [0u8; 7];
        mem1.get(RegionId(1)).unwrap().read(0, &mut buf).unwrap();
        assert_eq!(&buf, b"region1");

        // Child has new data
        mem2.get(child_region1_id)
            .unwrap()
            .read(0, &mut buf)
            .unwrap();
        assert_eq!(&buf, b"CHILD!!");
    }

    #[test]
    fn test_cow_multiple_clones() {
        // Test that multiple COW clones work correctly
        let page = Page::from_data(b"original".to_vec());

        let page2 = page.clone();
        let page3 = page.clone();

        // All three share
        assert_eq!(page.ref_count(), 3);
        assert_eq!(page2.ref_count(), 3);
        assert_eq!(page3.ref_count(), 3);

        // Write to one
        let mut page2 = page2;
        page2.write(0, b"page2!!!");

        // page2 is now private, others still share
        assert_eq!(page2.ref_count(), 1);
        assert_eq!(page.ref_count(), 2);
        assert_eq!(page3.ref_count(), 2);
    }

    #[test]
    fn test_cow_large_region() {
        // Test COW with a larger region spanning many pages
        let size = PAGE_SIZE * 10;
        let mut mem1 = CowMemory::new(size);

        // Write to every page
        for i in 0..10 {
            let offset = i * PAGE_SIZE;
            let data = format!("page{:02}", i);
            mem1.write(offset, data.as_bytes());
        }

        // COW clone
        let mut mem2 = mem1.cow_clone();
        assert_eq!(mem2.shared_page_count(), 10);

        // Write to pages 0, 5, 9 in child
        mem2.write(0, b"ZERO");
        mem2.write(PAGE_SIZE * 5, b"FIVE");
        mem2.write(PAGE_SIZE * 9, b"NINE");

        // Check COW happened correctly
        assert_eq!(mem2.private_page_count(), 3);
        assert_eq!(mem2.shared_page_count(), 7);

        // Verify data
        let mut buf = [0u8; 6];
        mem1.read(0, &mut buf);
        assert_eq!(&buf, b"page00");
        mem2.read(0, &mut buf);
        assert_eq!(&buf[..4], b"ZERO");

        mem1.read(PAGE_SIZE * 5, &mut buf);
        assert_eq!(&buf, b"page05");
        mem2.read(PAGE_SIZE * 5, &mut buf);
        assert_eq!(&buf[..4], b"FIVE");
    }

    #[test]
    fn test_cow_stats_tracking() {
        let mut region = MemoryRegion::new(RegionId(1), PAGE_SIZE * 3, Protection::READ_WRITE);
        region.write(0, b"test").unwrap();

        let mut clone = region.cow_clone(RegionId(2));

        let stats = clone.cow_stats();
        assert_eq!(stats.total_pages, 3);
        assert_eq!(stats.shared_pages, 3);
        assert_eq!(stats.cow_faults, 0);

        // Trigger COW
        clone.write(0, b"new!").unwrap();

        let stats = clone.cow_stats();
        assert_eq!(stats.cow_faults, 1);
        assert_eq!(stats.private_pages, 1);
        assert_eq!(stats.shared_pages, 2);
    }

    // ========================================================================
    // Memory-Mapped File Tests
    // ========================================================================

    #[test]
    fn test_mmap_flags() {
        let shared = MmapFlags::shared();
        assert!(shared.shared);
        assert!(!shared.private);
        assert!(!shared.anonymous);

        let private = MmapFlags::private();
        assert!(!private.shared);
        assert!(private.private);
        assert!(!private.anonymous);

        let anon = MmapFlags::anonymous();
        assert!(!anon.shared);
        assert!(anon.private);
        assert!(anon.anonymous);
    }

    #[test]
    fn test_mmap_manager_basic() {
        let mut mgr = MmapManager::new();
        let region_id = RegionId(1);

        let mmap_id = mgr.mmap(
            region_id,
            Some("/path/to/file".to_string()),
            0,
            4096,
            Protection::READ_WRITE,
            MmapFlags::private(),
        );

        assert_eq!(mgr.count(), 1);

        let mapping = mgr.get(mmap_id).unwrap();
        assert_eq!(mapping.region_id, region_id);
        assert_eq!(mapping.file_path, Some("/path/to/file".to_string()));
        assert_eq!(mapping.size, 4096);
        assert!(!mapping.is_dirty());
        assert!(mapping.is_private());
        assert!(!mapping.is_shared());
        assert!(!mapping.is_anonymous());
    }

    #[test]
    fn test_mmap_manager_anonymous() {
        let mut mgr = MmapManager::new();
        let region_id = RegionId(1);

        let mmap_id = mgr.mmap(
            region_id,
            None,
            0,
            4096,
            Protection::READ_WRITE,
            MmapFlags::anonymous(),
        );

        let mapping = mgr.get(mmap_id).unwrap();
        assert!(mapping.is_anonymous());
        assert!(mapping.file_path.is_none());
    }

    #[test]
    fn test_mmap_dirty_tracking() {
        let mut mgr = MmapManager::new();
        let region_id = RegionId(1);

        let mmap_id = mgr.mmap(
            region_id,
            Some("/file".to_string()),
            0,
            4096,
            Protection::READ_WRITE,
            MmapFlags::shared(),
        );

        // Initially not dirty
        assert!(!mgr.get(mmap_id).unwrap().is_dirty());
        assert!(mgr.dirty_shared_mappings().is_empty());

        // Mark dirty
        mgr.mark_dirty(mmap_id);
        assert!(mgr.get(mmap_id).unwrap().is_dirty());
        assert_eq!(mgr.dirty_shared_mappings().len(), 1);

        // Clear dirty
        mgr.get_mut(mmap_id).unwrap().clear_dirty();
        assert!(!mgr.get(mmap_id).unwrap().is_dirty());
        assert!(mgr.dirty_shared_mappings().is_empty());
    }

    #[test]
    fn test_mmap_munmap() {
        let mut mgr = MmapManager::new();
        let region_id = RegionId(1);

        let mmap_id = mgr.mmap(
            region_id,
            Some("/file".to_string()),
            0,
            4096,
            Protection::READ_WRITE,
            MmapFlags::private(),
        );

        assert_eq!(mgr.count(), 1);

        let removed = mgr.munmap(mmap_id);
        assert!(removed.is_some());
        assert_eq!(mgr.count(), 0);
        assert!(mgr.get(mmap_id).is_none());
    }

    #[test]
    fn test_mmap_get_by_region() {
        let mut mgr = MmapManager::new();
        let region_id = RegionId(42);

        let mmap_id = mgr.mmap(
            region_id,
            Some("/file".to_string()),
            0,
            4096,
            Protection::READ,
            MmapFlags::shared(),
        );

        let mapping = mgr.get_by_region(region_id).unwrap();
        assert_eq!(mapping.id, mmap_id);

        assert!(mgr.get_by_region(RegionId(999)).is_none());
    }

    #[test]
    fn test_mmap_list() {
        let mut mgr = MmapManager::new();

        mgr.mmap(
            RegionId(1),
            Some("/file1".to_string()),
            0,
            4096,
            Protection::READ,
            MmapFlags::shared(),
        );
        mgr.mmap(
            RegionId(2),
            Some("/file2".to_string()),
            0,
            8192,
            Protection::READ_WRITE,
            MmapFlags::private(),
        );

        let list = mgr.list();
        assert_eq!(list.len(), 2);
    }

    // ========================================================================
    // Memory Pool Tests
    // ========================================================================

    #[test]
    fn test_memory_pool_basic() {
        let pool = MemoryPool::new(PoolId(1), 64, 10);

        assert_eq!(pool.object_size, 64);
        assert_eq!(pool.capacity, 10);
        assert_eq!(pool.alloc_count(), 0);
        assert_eq!(pool.free_count(), 10);
        assert!(pool.is_empty());
        assert!(!pool.is_full());
    }

    #[test]
    fn test_memory_pool_alloc_free() {
        let mut pool = MemoryPool::new(PoolId(1), 64, 3);

        // Allocate
        let off1 = pool.alloc().unwrap();
        let off2 = pool.alloc().unwrap();
        let off3 = pool.alloc().unwrap();

        assert_eq!(pool.alloc_count(), 3);
        assert!(pool.is_full());
        assert!(pool.alloc().is_none()); // Pool is full

        // Free one
        assert!(pool.free(off2));
        assert_eq!(pool.alloc_count(), 2);
        assert!(!pool.is_full());

        // Allocate again
        let off4 = pool.alloc().unwrap();
        assert_eq!(off4, off2); // Reuses freed slot

        // Free all
        assert!(pool.free(off1));
        assert!(pool.free(off3));
        assert!(pool.free(off4));
        assert!(pool.is_empty());
    }

    #[test]
    fn test_memory_pool_read_write() {
        let mut pool = MemoryPool::new(PoolId(1), 32, 2);

        let off = pool.alloc().unwrap();

        // Write data
        let data = b"hello, pool!";
        assert_eq!(pool.write(off, data), Some(12));

        // Read it back
        let mut buf = [0u8; 12];
        assert_eq!(pool.read(off, &mut buf), Some(12));
        assert_eq!(&buf, data);

        // Can't read/write unallocated
        let off2 = pool.object_size; // Second slot (not allocated)
        assert!(pool.read(off2, &mut buf).is_none());
        assert!(pool.write(off2, data).is_none());
    }

    #[test]
    fn test_memory_pool_invalid_free() {
        let mut pool = MemoryPool::new(PoolId(1), 64, 10);

        // Free unallocated slot
        assert!(!pool.free(0));

        // Free out of bounds
        assert!(!pool.free(1000));

        // Free misaligned
        let off = pool.alloc().unwrap();
        assert!(!pool.free(off + 1)); // Not aligned

        // Double free
        pool.free(off);
        assert!(!pool.free(off));
    }

    #[test]
    fn test_memory_pool_reset() {
        let mut pool = MemoryPool::new(PoolId(1), 64, 5);

        // Allocate all
        for _ in 0..5 {
            pool.alloc().unwrap();
        }
        assert!(pool.is_full());

        // Reset
        pool.reset();
        assert!(pool.is_empty());
        assert_eq!(pool.free_count(), 5);

        // Can allocate again
        assert!(pool.alloc().is_some());
    }

    #[test]
    fn test_memory_pool_sizes() {
        let pool = MemoryPool::new(PoolId(1), 100, 10);

        assert_eq!(pool.total_size(), 1000);
        assert_eq!(pool.used_size(), 0);
    }

    #[test]
    fn test_pool_manager_basic() {
        let mut mgr = PoolManager::new();

        let id1 = mgr.create_pool(64, 100);
        let id2 = mgr.create_pool(128, 50);

        let stats = mgr.stats();
        assert_eq!(stats.total_pools, 2);
        assert_eq!(stats.total_capacity, 150);
        assert_eq!(stats.total_allocated, 0);
        assert_eq!(stats.total_memory, 64 * 100 + 128 * 50);

        // Allocate from pools
        mgr.alloc(id1).unwrap();
        mgr.alloc(id1).unwrap();
        mgr.alloc(id2).unwrap();

        let stats = mgr.stats();
        assert_eq!(stats.total_allocated, 3);

        // Free
        mgr.free(id1, 0);
        let stats = mgr.stats();
        assert_eq!(stats.total_allocated, 2);
    }

    #[test]
    fn test_pool_manager_delete() {
        let mut mgr = PoolManager::new();

        let id = mgr.create_pool(64, 10);
        assert!(mgr.get(id).is_some());

        let deleted = mgr.delete_pool(id);
        assert!(deleted.is_some());
        assert!(mgr.get(id).is_none());
    }

    #[test]
    fn test_pool_manager_list() {
        let mut mgr = PoolManager::new();

        mgr.create_pool(32, 10);
        mgr.create_pool(64, 20);
        mgr.create_pool(128, 30);

        let list = mgr.list();
        assert_eq!(list.len(), 3);

        // Check one of them
        let info = list.iter().find(|p| p.object_size == 64).unwrap();
        assert_eq!(info.capacity, 20);
    }

    #[test]
    #[should_panic(expected = "object_size must be > 0")]
    fn test_pool_zero_object_size() {
        MemoryPool::new(PoolId(1), 0, 10);
    }

    #[test]
    #[should_panic(expected = "capacity must be > 0")]
    fn test_pool_zero_capacity() {
        MemoryPool::new(PoolId(1), 64, 0);
    }
}
