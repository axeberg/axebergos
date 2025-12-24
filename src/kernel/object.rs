//! Kernel objects
//!
//! Every resource in the system is a kernel object. Processes access
//! them through handles (file descriptors). This provides isolation -
//! a process can only access objects it has handles to.

use super::process::Handle;
use std::collections::{HashMap, VecDeque};

/// Window identifier (stub - now using xterm.js instead of compositor)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub u64);
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

/// Unique object identifier (internal to kernel)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectId(pub u64);

/// A kernel object - any resource that can be accessed via a handle
pub enum KernelObject {
    /// A file in the VFS
    File(FileObject),

    /// A pipe for IPC
    Pipe(PipeObject),

    /// A console/terminal device
    Console(ConsoleObject),

    /// A window in the compositor
    Window(WindowObject),

    /// A directory (for readdir)
    Directory(DirectoryObject),
}

impl KernelObject {
    /// Read from this object
    pub fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            KernelObject::File(f) => f.read(buf),
            KernelObject::Pipe(p) => p.read(buf),
            KernelObject::Console(c) => c.read(buf),
            KernelObject::Window(_) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot read from window",
            )),
            KernelObject::Directory(_) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot read from directory",
            )),
        }
    }

    /// Write to this object
    pub fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            KernelObject::File(f) => f.write(buf),
            KernelObject::Pipe(p) => p.write(buf),
            KernelObject::Console(c) => c.write(buf),
            KernelObject::Window(w) => w.write(buf),
            KernelObject::Directory(_) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot write to directory",
            )),
        }
    }

    /// Seek within this object
    pub fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match self {
            KernelObject::File(f) => f.seek(pos),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "object does not support seeking",
            )),
        }
    }

    /// Get object type name
    pub fn type_name(&self) -> &'static str {
        match self {
            KernelObject::File(_) => "file",
            KernelObject::Pipe(_) => "pipe",
            KernelObject::Console(_) => "console",
            KernelObject::Window(_) => "window",
            KernelObject::Directory(_) => "directory",
        }
    }
}

/// A file object - represents an open file
pub struct FileObject {
    /// Path to the file
    pub path: PathBuf,
    /// Current position in the file
    pub position: u64,
    /// File contents (we store a copy for now - will be backed by VFS)
    pub data: Vec<u8>,
    /// Can we read?
    pub readable: bool,
    /// Can we write?
    pub writable: bool,
}

impl FileObject {
    pub fn new(path: PathBuf, data: Vec<u8>, readable: bool, writable: bool) -> Self {
        Self {
            path,
            position: 0,
            data,
            readable,
            writable,
        }
    }
}

impl Read for FileObject {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.readable {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "file not opened for reading",
            ));
        }

        let pos = self.position as usize;
        if pos >= self.data.len() {
            return Ok(0); // EOF
        }

        let available = &self.data[pos..];
        let to_read = buf.len().min(available.len());
        buf[..to_read].copy_from_slice(&available[..to_read]);
        self.position += to_read as u64;
        Ok(to_read)
    }
}

impl Write for FileObject {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if !self.writable {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "file not opened for writing",
            ));
        }

        let pos = self.position as usize;

        // Extend file if needed
        if pos + buf.len() > self.data.len() {
            self.data.resize(pos + buf.len(), 0);
        }

        self.data[pos..pos + buf.len()].copy_from_slice(buf);
        self.position += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(()) // No-op for memory-backed files
    }
}

impl Seek for FileObject {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let new_pos = match pos {
            SeekFrom::Start(n) => n as i64,
            SeekFrom::End(n) => self.data.len() as i64 + n,
            SeekFrom::Current(n) => self.position as i64 + n,
        };

        if new_pos < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek before start of file",
            ));
        }

        self.position = new_pos as u64;
        Ok(self.position)
    }
}

/// A pipe object - unidirectional byte stream
pub struct PipeObject {
    /// Buffer of bytes
    buffer: VecDeque<u8>,
    /// Has the write end been closed?
    write_closed: bool,
    /// Has the read end been closed?
    read_closed: bool,
    /// Maximum buffer size
    capacity: usize,
}

impl PipeObject {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            write_closed: false,
            read_closed: false,
            capacity,
        }
    }

    pub fn close_write(&mut self) {
        self.write_closed = true;
    }

    pub fn close_read(&mut self) {
        self.read_closed = true;
    }

    pub fn is_closed(&self) -> bool {
        self.write_closed && self.read_closed
    }
}

impl Read for PipeObject {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.buffer.is_empty() {
            if self.write_closed {
                return Ok(0); // EOF
            }
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "pipe empty",
            ));
        }

        let to_read = buf.len().min(self.buffer.len());
        for (i, byte) in self.buffer.drain(..to_read).enumerate() {
            buf[i] = byte;
        }
        Ok(to_read)
    }
}

impl Write for PipeObject {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.read_closed {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "pipe read end closed",
            ));
        }

        if self.write_closed {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "pipe write end closed",
            ));
        }

        let available = self.capacity - self.buffer.len();
        if available == 0 {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "pipe full",
            ));
        }

        let to_write = buf.len().min(available);
        self.buffer.extend(&buf[..to_write]);
        Ok(to_write)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A console device - /dev/console
/// Reads keyboard input, writes to terminal display
pub struct ConsoleObject {
    /// Input buffer (keyboard input)
    input: VecDeque<u8>,
    /// Output buffer (for display)
    output: Vec<u8>,
}

impl ConsoleObject {
    pub fn new() -> Self {
        Self {
            input: VecDeque::new(),
            output: Vec::new(),
        }
    }

    /// Push keyboard input
    pub fn push_input(&mut self, data: &[u8]) {
        self.input.extend(data);
    }

    /// Take output (for rendering)
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }

    /// Peek at output without consuming
    pub fn peek_output(&self) -> &[u8] {
        &self.output
    }

    /// Clear the input buffer (for TCFLUSH)
    pub fn clear_input(&mut self) {
        self.input.clear();
    }

    /// Clear the output buffer
    pub fn clear_output(&mut self) {
        self.output.clear();
    }
}

impl Default for ConsoleObject {
    fn default() -> Self {
        Self::new()
    }
}

impl Read for ConsoleObject {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.input.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "no input available",
            ));
        }

        let to_read = buf.len().min(self.input.len());
        for (i, byte) in self.input.drain(..to_read).enumerate() {
            buf[i] = byte;
        }
        Ok(to_read)
    }
}

impl Write for ConsoleObject {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.output.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A window object - represents an open window
pub struct WindowObject {
    /// Window ID in the compositor
    pub window_id: WindowId,
    /// Text content to display
    pub content: Vec<String>,
    /// Dirty flag (needs redraw)
    pub dirty: bool,
}

impl WindowObject {
    pub fn new(window_id: WindowId) -> Self {
        Self {
            window_id,
            content: Vec::new(),
            dirty: true,
        }
    }

    /// Append a line of text
    pub fn append_line(&mut self, line: String) {
        self.content.push(line);
        self.dirty = true;
    }
}

impl Write for WindowObject {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Convert to string and append
        let text = String::from_utf8_lossy(buf);
        for line in text.lines() {
            self.append_line(line.to_string());
        }
        self.dirty = true;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A directory object - for iterating directory contents
pub struct DirectoryObject {
    /// Path to directory
    pub path: PathBuf,
    /// Entries
    pub entries: Vec<String>,
    /// Current position
    pub position: usize,
}

impl DirectoryObject {
    pub fn new(path: PathBuf, entries: Vec<String>) -> Self {
        Self {
            path,
            entries,
            position: 0,
        }
    }

    /// Get next entry
    pub fn next_entry(&mut self) -> Option<&str> {
        if self.position < self.entries.len() {
            let entry = &self.entries[self.position];
            self.position += 1;
            Some(entry)
        } else {
            None
        }
    }
}

/// An entry in the object table with reference count
struct ObjectEntry {
    object: KernelObject,
    refcount: usize,
}

/// The object table - maps handles to reference-counted objects
///
/// Reference counting rules:
/// - insert() creates an object with refcount 1
/// - retain() increments refcount (when duplicating a handle)
/// - release() decrements refcount, removes object when it reaches 0
/// - Objects are only deallocated when refcount drops to 0
pub struct ObjectTable {
    next_id: u64,
    objects: HashMap<Handle, ObjectEntry>,
}

impl ObjectTable {
    pub fn new() -> Self {
        Self {
            next_id: 1, // 0 is Handle::NULL
            objects: HashMap::new(),
        }
    }

    /// Insert a new object and return its handle (refcount starts at 1)
    pub fn insert(&mut self, obj: KernelObject) -> Handle {
        let handle = Handle(self.next_id);
        self.next_id += 1;
        self.objects.insert(
            handle,
            ObjectEntry {
                object: obj,
                refcount: 1,
            },
        );
        handle
    }

    /// Increment the reference count for a handle
    /// Returns true if the handle exists, false otherwise
    pub fn retain(&mut self, handle: Handle) -> bool {
        if let Some(entry) = self.objects.get_mut(&handle) {
            entry.refcount += 1;
            true
        } else {
            false
        }
    }

    /// Decrement the reference count for a handle
    /// Returns the object if refcount drops to 0 (object is removed)
    /// Returns None if handle doesn't exist or refcount is still > 0
    pub fn release(&mut self, handle: Handle) -> Option<KernelObject> {
        let should_remove = {
            if let Some(entry) = self.objects.get_mut(&handle) {
                entry.refcount = entry.refcount.saturating_sub(1);
                entry.refcount == 0
            } else {
                return None;
            }
        };

        if should_remove {
            self.objects.remove(&handle).map(|e| e.object)
        } else {
            None
        }
    }

    /// Get the current reference count for a handle
    pub fn refcount(&self, handle: Handle) -> usize {
        self.objects
            .get(&handle)
            .map(|e| e.refcount)
            .unwrap_or(0)
    }

    /// Get an object by handle
    pub fn get(&self, handle: Handle) -> Option<&KernelObject> {
        self.objects.get(&handle).map(|e| &e.object)
    }

    /// Get a mutable object by handle
    pub fn get_mut(&mut self, handle: Handle) -> Option<&mut KernelObject> {
        self.objects.get_mut(&handle).map(|e| &mut e.object)
    }

    /// Check if a handle exists
    pub fn contains(&self, handle: Handle) -> bool {
        self.objects.contains_key(&handle)
    }

    /// Get total number of objects
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Check if table is empty
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
}

impl Default for ObjectTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_read_write() {
        let mut file = FileObject::new(
            PathBuf::from("/test.txt"),
            Vec::new(),
            true,
            true,
        );

        // Write
        let written = file.write(b"Hello, World!").unwrap();
        assert_eq!(written, 13);

        // Seek back
        file.seek(SeekFrom::Start(0)).unwrap();

        // Read
        let mut buf = [0u8; 20];
        let read = file.read(&mut buf).unwrap();
        assert_eq!(read, 13);
        assert_eq!(&buf[..read], b"Hello, World!");
    }

    #[test]
    fn test_pipe() {
        let mut pipe = PipeObject::new(1024);

        // Write
        pipe.write(b"test data").unwrap();

        // Read
        let mut buf = [0u8; 20];
        let read = pipe.read(&mut buf).unwrap();
        assert_eq!(read, 9);
        assert_eq!(&buf[..read], b"test data");

        // Empty pipe blocks
        assert!(pipe.read(&mut buf).is_err());

        // Close write end, then read returns EOF
        pipe.close_write();
        assert_eq!(pipe.read(&mut buf).unwrap(), 0);
    }

    #[test]
    fn test_console() {
        let mut console = ConsoleObject::new();

        // Write output
        console.write(b"Hello\n").unwrap();
        assert_eq!(console.peek_output(), b"Hello\n");

        // Push input
        console.push_input(b"abc");
        let mut buf = [0u8; 10];
        let read = console.read(&mut buf).unwrap();
        assert_eq!(read, 3);
        assert_eq!(&buf[..read], b"abc");
    }

    #[test]
    fn test_object_table_basic() {
        let mut table = ObjectTable::new();

        let h1 = table.insert(KernelObject::Console(ConsoleObject::new()));
        let h2 = table.insert(KernelObject::Pipe(PipeObject::new(1024)));

        assert!(table.get(h1).is_some());
        assert!(table.get(h2).is_some());
        assert!(table.get(Handle::NULL).is_none());
        assert_eq!(table.len(), 2);

        // Release with refcount 1 should remove
        let removed = table.release(h1);
        assert!(removed.is_some());
        assert!(table.get(h1).is_none());
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn test_refcount_initial() {
        let mut table = ObjectTable::new();
        let h = table.insert(KernelObject::Console(ConsoleObject::new()));

        // Initial refcount is 1
        assert_eq!(table.refcount(h), 1);
    }

    #[test]
    fn test_refcount_retain() {
        let mut table = ObjectTable::new();
        let h = table.insert(KernelObject::Console(ConsoleObject::new()));

        // Retain increments refcount
        assert!(table.retain(h));
        assert_eq!(table.refcount(h), 2);

        assert!(table.retain(h));
        assert_eq!(table.refcount(h), 3);

        // Retain on invalid handle returns false
        assert!(!table.retain(Handle::NULL));
    }

    #[test]
    fn test_refcount_release() {
        let mut table = ObjectTable::new();
        let h = table.insert(KernelObject::Console(ConsoleObject::new()));

        // Add more refs
        table.retain(h);
        table.retain(h);
        assert_eq!(table.refcount(h), 3);

        // Release doesn't remove until refcount hits 0
        assert!(table.release(h).is_none());
        assert_eq!(table.refcount(h), 2);
        assert!(table.get(h).is_some());

        assert!(table.release(h).is_none());
        assert_eq!(table.refcount(h), 1);
        assert!(table.get(h).is_some());

        // Final release removes the object
        let removed = table.release(h);
        assert!(removed.is_some());
        assert_eq!(table.refcount(h), 0);
        assert!(table.get(h).is_none());
    }

    #[test]
    fn test_refcount_shared_handle() {
        let mut table = ObjectTable::new();
        let h = table.insert(KernelObject::Pipe(PipeObject::new(1024)));

        // Simulate sharing: two processes have the same handle
        table.retain(h); // Second process gets it
        assert_eq!(table.refcount(h), 2);

        // First process closes
        assert!(table.release(h).is_none());
        assert!(table.get(h).is_some()); // Still exists

        // Second process closes
        let removed = table.release(h);
        assert!(removed.is_some()); // Now gone
        assert!(table.get(h).is_none());
    }

    #[test]
    fn test_release_invalid_handle() {
        let mut table = ObjectTable::new();

        // Release on non-existent handle returns None
        assert!(table.release(Handle(999)).is_none());
        assert!(table.release(Handle::NULL).is_none());
    }
}
