//! FIFO (named pipe) implementation
//!
//! FIFOs provide a way for unrelated processes to communicate via
//! a named entry in the filesystem.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

/// A named pipe (FIFO) buffer
#[derive(Debug)]
pub struct FifoBuffer {
    /// Data buffer
    data: VecDeque<u8>,
    /// Maximum buffer size
    capacity: usize,
    /// Number of readers
    readers: u32,
    /// Number of writers
    writers: u32,
}

impl FifoBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: VecDeque::with_capacity(capacity),
            capacity,
            readers: 0,
            writers: 0,
        }
    }

    /// Write data to the FIFO
    pub fn write(&mut self, data: &[u8]) -> Result<usize, FifoError> {
        if self.readers == 0 {
            return Err(FifoError::BrokenPipe);
        }

        let available = self.capacity - self.data.len();
        let to_write = data.len().min(available);

        for &byte in &data[..to_write] {
            self.data.push_back(byte);
        }

        if to_write == 0 && !data.is_empty() {
            Err(FifoError::WouldBlock)
        } else {
            Ok(to_write)
        }
    }

    /// Read data from the FIFO
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, FifoError> {
        if self.data.is_empty() {
            if self.writers == 0 {
                return Ok(0); // EOF
            }
            return Err(FifoError::WouldBlock);
        }

        let to_read = buf.len().min(self.data.len());
        for byte in buf.iter_mut().take(to_read) {
            *byte = self.data.pop_front().unwrap();
        }

        Ok(to_read)
    }

    /// Check if FIFO is readable (has data or no writers)
    pub fn is_readable(&self) -> bool {
        !self.data.is_empty() || self.writers == 0
    }

    /// Check if FIFO is writable (has space and readers)
    pub fn is_writable(&self) -> bool {
        self.data.len() < self.capacity && self.readers > 0
    }

    /// Add a reader
    pub fn add_reader(&mut self) {
        self.readers += 1;
    }

    /// Remove a reader
    pub fn remove_reader(&mut self) {
        self.readers = self.readers.saturating_sub(1);
    }

    /// Add a writer
    pub fn add_writer(&mut self) {
        self.writers += 1;
    }

    /// Remove a writer
    pub fn remove_writer(&mut self) {
        self.writers = self.writers.saturating_sub(1);
    }

    /// Get number of bytes available to read
    pub fn available(&self) -> usize {
        self.data.len()
    }
}

/// FIFO error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FifoError {
    /// Operation would block
    WouldBlock,
    /// No readers (SIGPIPE condition)
    BrokenPipe,
    /// FIFO not found
    NotFound,
    /// Already exists
    AlreadyExists,
}

/// FIFO registry - manages all named FIFOs
pub struct FifoRegistry {
    /// Map from path to FIFO buffer
    fifos: HashMap<String, Rc<RefCell<FifoBuffer>>>,
    /// Default buffer capacity
    default_capacity: usize,
}

impl FifoRegistry {
    pub fn new() -> Self {
        Self {
            fifos: HashMap::new(),
            default_capacity: 65536, // 64KB default
        }
    }

    /// Create a new FIFO at the given path
    pub fn mkfifo(&mut self, path: &str) -> Result<(), FifoError> {
        if self.fifos.contains_key(path) {
            return Err(FifoError::AlreadyExists);
        }

        let fifo = Rc::new(RefCell::new(FifoBuffer::new(self.default_capacity)));
        self.fifos.insert(path.to_string(), fifo);
        Ok(())
    }

    /// Remove a FIFO
    pub fn unlink(&mut self, path: &str) -> Result<(), FifoError> {
        self.fifos.remove(path).map(|_| ()).ok_or(FifoError::NotFound)
    }

    /// Get a FIFO by path
    pub fn get(&self, path: &str) -> Option<Rc<RefCell<FifoBuffer>>> {
        self.fifos.get(path).cloned()
    }

    /// Check if a path is a FIFO
    pub fn is_fifo(&self, path: &str) -> bool {
        self.fifos.contains_key(path)
    }

    /// List all FIFOs
    pub fn list(&self) -> Vec<String> {
        self.fifos.keys().cloned().collect()
    }
}

impl Default for FifoRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fifo_buffer_basic() {
        let mut fifo = FifoBuffer::new(1024);
        fifo.add_reader();
        fifo.add_writer();

        let data = b"Hello, FIFO!";
        let written = fifo.write(data).unwrap();
        assert_eq!(written, data.len());

        let mut buf = [0u8; 64];
        let read = fifo.read(&mut buf).unwrap();
        assert_eq!(read, data.len());
        assert_eq!(&buf[..read], data);
    }

    #[test]
    fn test_fifo_eof() {
        let mut fifo = FifoBuffer::new(1024);
        fifo.add_reader();
        fifo.add_writer();

        fifo.write(b"data").unwrap();
        fifo.remove_writer();

        let mut buf = [0u8; 64];
        let read = fifo.read(&mut buf).unwrap();
        assert_eq!(read, 4);

        // EOF when no writers and empty
        let read = fifo.read(&mut buf).unwrap();
        assert_eq!(read, 0);
    }

    #[test]
    fn test_fifo_broken_pipe() {
        let mut fifo = FifoBuffer::new(1024);
        fifo.add_writer();
        // No readers

        let result = fifo.write(b"data");
        assert_eq!(result, Err(FifoError::BrokenPipe));
    }

    #[test]
    fn test_fifo_registry() {
        let mut registry = FifoRegistry::new();

        registry.mkfifo("/tmp/myfifo").unwrap();
        assert!(registry.is_fifo("/tmp/myfifo"));

        // Can't create duplicate
        assert_eq!(registry.mkfifo("/tmp/myfifo"), Err(FifoError::AlreadyExists));

        registry.unlink("/tmp/myfifo").unwrap();
        assert!(!registry.is_fifo("/tmp/myfifo"));
    }
}
