//! File Browser - Visual file system navigator
//!
//! Provides:
//! - Directory listing with files and folders
//! - Keyboard navigation (arrows, enter, backspace)
//! - Current path display
//! - File operations (create, delete, rename, copy, move)

use crate::kernel::syscall::{self, OpenFlags};
use std::path::PathBuf;

/// Entry type in the file browser
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryType {
    Directory,
    File,
}

/// A file or directory entry
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub entry_type: EntryType,
    pub size: Option<u64>,
}

impl Entry {
    pub fn directory(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            entry_type: EntryType::Directory,
            size: None,
        }
    }

    pub fn file(name: impl Into<String>, size: u64) -> Self {
        Self {
            name: name.into(),
            entry_type: EntryType::File,
            size: Some(size),
        }
    }

    pub fn is_dir(&self) -> bool {
        self.entry_type == EntryType::Directory
    }
}

/// Input mode for text entry
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    /// Not in input mode
    None,
    /// Creating a new file
    NewFile,
    /// Creating a new directory
    NewDirectory,
    /// Renaming the selected entry
    Rename,
}

/// Clipboard operation
#[derive(Debug, Clone)]
pub struct ClipboardEntry {
    /// Full path of the item
    pub path: String,
    /// Whether this is a cut (move) operation
    pub is_cut: bool,
    /// Whether it's a directory
    pub is_dir: bool,
}

/// Status message with optional timeout
#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub text: String,
    pub is_error: bool,
}

/// The file browser state
pub struct FileBrowser {
    /// Current directory path
    cwd: PathBuf,
    /// Entries in current directory
    entries: Vec<Entry>,
    /// Currently selected index
    selected: usize,
    /// Scroll offset for display
    scroll_offset: usize,
    /// Error message to display (if any)
    error: Option<String>,
    /// Current input mode
    input_mode: InputMode,
    /// Input buffer for text entry
    input_buffer: String,
    /// Clipboard for copy/cut operations
    clipboard: Option<ClipboardEntry>,
    /// Status message
    status: Option<StatusMessage>,
}

impl FileBrowser {
    pub fn new() -> Self {
        let mut browser = Self {
            cwd: PathBuf::from("/"),
            entries: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            error: None,
            input_mode: InputMode::None,
            input_buffer: String::new(),
            clipboard: None,
            status: None,
        };
        browser.refresh();
        browser
    }

    /// Get current working directory
    pub fn cwd(&self) -> &PathBuf {
        &self.cwd
    }

    /// Get current entries
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Get selected index
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Get scroll offset
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Get error message if any
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Get current input mode
    pub fn input_mode(&self) -> &InputMode {
        &self.input_mode
    }

    /// Get input buffer
    pub fn input_buffer(&self) -> &str {
        &self.input_buffer
    }

    /// Get clipboard entry if any
    pub fn clipboard(&self) -> Option<&ClipboardEntry> {
        self.clipboard.as_ref()
    }

    /// Get status message if any
    pub fn status(&self) -> Option<&StatusMessage> {
        self.status.as_ref()
    }

    /// Clear status message
    pub fn clear_status(&mut self) {
        self.status = None;
    }

    /// Set a status message
    fn set_status(&mut self, text: impl Into<String>, is_error: bool) {
        self.status = Some(StatusMessage {
            text: text.into(),
            is_error,
        });
    }

    /// Refresh directory listing
    pub fn refresh(&mut self) {
        self.entries.clear();
        self.error = None;

        let path_str = self.cwd.display().to_string();

        // Add parent directory entry if not at root
        if self.cwd.as_os_str() != "/" {
            self.entries.push(Entry::directory(".."));
        }

        // Read directory entries
        match syscall::readdir(&path_str) {
            Ok(names) => {
                for name in names {
                    // Skip . and ..
                    if name == "." || name == ".." {
                        continue;
                    }

                    let full_path = format!("{}/{}", path_str.trim_end_matches('/'), name);

                    // Check if it's a directory or file
                    match syscall::metadata(&full_path) {
                        Ok(meta) => {
                            if meta.is_dir {
                                self.entries.push(Entry::directory(name));
                            } else {
                                self.entries.push(Entry::file(name, meta.size));
                            }
                        }
                        Err(_) => {
                            // Assume file if metadata fails
                            self.entries.push(Entry::file(name, 0));
                        }
                    }
                }

                // Sort: directories first, then alphabetically
                self.entries.sort_by(|a, b| {
                    match (&a.entry_type, &b.entry_type) {
                        (EntryType::Directory, EntryType::File) => std::cmp::Ordering::Less,
                        (EntryType::File, EntryType::Directory) => std::cmp::Ordering::Greater,
                        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    }
                });
            }
            Err(e) => {
                self.error = Some(format!("Cannot read directory: {}", e));
            }
        }

        // Reset selection if out of bounds
        if self.selected >= self.entries.len() {
            self.selected = self.entries.len().saturating_sub(1);
        }
    }

    /// Navigate into a directory or open a file
    pub fn open_selected(&mut self) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }

        let entry = &self.entries[self.selected];

        if entry.is_dir() {
            if entry.name == ".." {
                // Go to parent directory
                if let Some(parent) = self.cwd.parent() {
                    self.cwd = parent.to_path_buf();
                    if self.cwd.as_os_str().is_empty() {
                        self.cwd = PathBuf::from("/");
                    }
                }
            } else {
                // Enter subdirectory
                self.cwd.push(&entry.name);
            }
            self.selected = 0;
            self.scroll_offset = 0;
            self.refresh();
            None
        } else {
            // Return file path for opening
            let mut path = self.cwd.clone();
            path.push(&entry.name);
            Some(path.display().to_string())
        }
    }

    /// Go to parent directory
    pub fn go_up(&mut self) {
        if let Some(parent) = self.cwd.parent() {
            self.cwd = parent.to_path_buf();
            if self.cwd.as_os_str().is_empty() {
                self.cwd = PathBuf::from("/");
            }
            self.selected = 0;
            self.scroll_offset = 0;
            self.refresh();
        }
    }

    /// Move selection up
    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            // Adjust scroll if needed
            if self.selected < self.scroll_offset {
                self.scroll_offset = self.selected;
            }
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
        }
    }

    /// Adjust scroll offset for visible rows
    pub fn adjust_scroll(&mut self, visible_rows: usize) {
        if visible_rows == 0 {
            return;
        }
        // Ensure selected item is visible
        if self.selected >= self.scroll_offset + visible_rows {
            self.scroll_offset = self.selected - visible_rows + 1;
        }
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        }
    }

    // ========== FILE OPERATIONS ==========

    /// Start creating a new file
    pub fn start_new_file(&mut self) {
        self.input_mode = InputMode::NewFile;
        self.input_buffer.clear();
        self.status = None;
    }

    /// Start creating a new directory
    pub fn start_new_directory(&mut self) {
        self.input_mode = InputMode::NewDirectory;
        self.input_buffer.clear();
        self.status = None;
    }

    /// Start renaming the selected entry
    pub fn start_rename(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let entry = &self.entries[self.selected];
        if entry.name == ".." {
            self.set_status("Cannot rename parent directory", true);
            return;
        }
        self.input_mode = InputMode::Rename;
        self.input_buffer = entry.name.clone();
        self.status = None;
    }

    /// Cancel the current input mode
    pub fn cancel_input(&mut self) {
        self.input_mode = InputMode::None;
        self.input_buffer.clear();
        self.set_status("Cancelled", false);
    }

    /// Confirm input and perform the action
    pub fn confirm_input(&mut self) {
        if self.input_buffer.is_empty() {
            self.set_status("Name cannot be empty", true);
            return;
        }

        // Validate name (no slashes)
        if self.input_buffer.contains('/') {
            self.set_status("Name cannot contain '/'", true);
            return;
        }

        let name = self.input_buffer.clone();
        let path = format!("{}/{}", self.cwd.display().to_string().trim_end_matches('/'), name);

        match &self.input_mode {
            InputMode::NewFile => {
                match syscall::open(&path, OpenFlags::WRITE) {
                    Ok(fd) => {
                        let _ = syscall::close(fd);
                        self.set_status(format!("Created file: {}", name), false);
                        self.refresh();
                        // Select the new file
                        if let Some(pos) = self.entries.iter().position(|e| e.name == name) {
                            self.selected = pos;
                        }
                    }
                    Err(e) => {
                        self.set_status(format!("Failed to create file: {}", e), true);
                    }
                }
            }
            InputMode::NewDirectory => {
                match syscall::mkdir(&path) {
                    Ok(()) => {
                        self.set_status(format!("Created directory: {}", name), false);
                        self.refresh();
                        // Select the new directory
                        if let Some(pos) = self.entries.iter().position(|e| e.name == name) {
                            self.selected = pos;
                        }
                    }
                    Err(e) => {
                        self.set_status(format!("Failed to create directory: {}", e), true);
                    }
                }
            }
            InputMode::Rename => {
                if !self.entries.is_empty() {
                    let old_name = &self.entries[self.selected].name;
                    let old_path = format!(
                        "{}/{}",
                        self.cwd.display().to_string().trim_end_matches('/'),
                        old_name
                    );
                    match syscall::rename(&old_path, &path) {
                        Ok(()) => {
                            self.set_status(format!("Renamed to: {}", name), false);
                            self.refresh();
                            // Select the renamed item
                            if let Some(pos) = self.entries.iter().position(|e| e.name == name) {
                                self.selected = pos;
                            }
                        }
                        Err(e) => {
                            self.set_status(format!("Failed to rename: {}", e), true);
                        }
                    }
                }
            }
            InputMode::None => {}
        }

        self.input_mode = InputMode::None;
        self.input_buffer.clear();
    }

    /// Delete the selected entry
    pub fn delete_selected(&mut self) {
        if self.entries.is_empty() {
            return;
        }

        let entry = &self.entries[self.selected];
        if entry.name == ".." {
            self.set_status("Cannot delete parent directory", true);
            return;
        }

        let path = format!(
            "{}/{}",
            self.cwd.display().to_string().trim_end_matches('/'),
            entry.name
        );

        let result = if entry.is_dir() {
            syscall::remove_dir(&path)
        } else {
            syscall::remove_file(&path)
        };

        match result {
            Ok(()) => {
                self.set_status(format!("Deleted: {}", entry.name), false);
                self.refresh();
            }
            Err(e) => {
                self.set_status(format!("Failed to delete: {}", e), true);
            }
        }
    }

    /// Copy selected entry to clipboard
    pub fn copy_selected(&mut self) {
        if self.entries.is_empty() {
            return;
        }

        let entry = &self.entries[self.selected];
        if entry.name == ".." {
            self.set_status("Cannot copy parent directory", true);
            return;
        }

        let path = format!(
            "{}/{}",
            self.cwd.display().to_string().trim_end_matches('/'),
            entry.name
        );

        self.clipboard = Some(ClipboardEntry {
            path,
            is_cut: false,
            is_dir: entry.is_dir(),
        });

        self.set_status(format!("Copied: {}", entry.name), false);
    }

    /// Cut selected entry to clipboard (for move)
    pub fn cut_selected(&mut self) {
        if self.entries.is_empty() {
            return;
        }

        let entry = &self.entries[self.selected];
        if entry.name == ".." {
            self.set_status("Cannot cut parent directory", true);
            return;
        }

        let path = format!(
            "{}/{}",
            self.cwd.display().to_string().trim_end_matches('/'),
            entry.name
        );

        self.clipboard = Some(ClipboardEntry {
            path,
            is_cut: true,
            is_dir: entry.is_dir(),
        });

        self.set_status(format!("Cut: {}", entry.name), false);
    }

    /// Paste from clipboard
    pub fn paste(&mut self) {
        let clip = match self.clipboard.take() {
            Some(c) => c,
            None => {
                self.set_status("Nothing to paste", true);
                return;
            }
        };

        // Extract filename from path
        let filename = clip.path.rsplit('/').next().unwrap_or(&clip.path);
        let dest_path = format!(
            "{}/{}",
            self.cwd.display().to_string().trim_end_matches('/'),
            filename
        );

        // Check if destination already exists
        if syscall::exists(&dest_path).unwrap_or(false) {
            self.set_status(format!("Destination already exists: {}", filename), true);
            // Put it back in clipboard
            self.clipboard = Some(clip);
            return;
        }

        if clip.is_cut {
            // Move operation
            match syscall::rename(&clip.path, &dest_path) {
                Ok(()) => {
                    self.set_status(format!("Moved: {}", filename), false);
                    self.refresh();
                    // Select the pasted item
                    if let Some(pos) = self.entries.iter().position(|e| e.name == filename) {
                        self.selected = pos;
                    }
                }
                Err(e) => {
                    self.set_status(format!("Failed to move: {}", e), true);
                    // Put it back in clipboard on failure
                    self.clipboard = Some(clip);
                }
            }
        } else {
            // Copy operation
            if clip.is_dir {
                self.set_status("Directory copy not supported yet", true);
                // Put it back in clipboard
                self.clipboard = Some(clip);
            } else {
                match syscall::copy_file(&clip.path, &dest_path) {
                    Ok(_) => {
                        self.set_status(format!("Copied: {}", filename), false);
                        self.refresh();
                        // Select the pasted item
                        if let Some(pos) = self.entries.iter().position(|e| e.name == filename) {
                            self.selected = pos;
                        }
                        // Keep original in clipboard for multiple pastes
                        self.clipboard = Some(clip);
                    }
                    Err(e) => {
                        self.set_status(format!("Failed to copy: {}", e), true);
                        // Put it back in clipboard on failure
                        self.clipboard = Some(clip);
                    }
                }
            }
        }
    }

    /// Handle input in text entry mode
    fn handle_input_key(&mut self, key: &str, _code: &str, ctrl: bool) -> bool {
        if ctrl {
            return false;
        }

        match key {
            "Enter" => {
                self.confirm_input();
                true
            }
            "Escape" => {
                self.cancel_input();
                true
            }
            "Backspace" => {
                self.input_buffer.pop();
                true
            }
            _ if key.len() == 1 => {
                // Single character input
                let c = key.chars().next().unwrap();
                if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' || c == ' ' {
                    self.input_buffer.push(c);
                }
                true
            }
            _ => false,
        }
    }

    /// Handle keyboard input
    /// Returns Some(path) if a file was selected for opening
    pub fn handle_key(&mut self, key: &str, code: &str, ctrl: bool, _alt: bool) -> Option<String> {
        // If in input mode, handle input keys first
        if self.input_mode != InputMode::None {
            self.handle_input_key(key, code, ctrl);
            return None;
        }

        match key {
            "ArrowUp" => {
                self.select_prev();
                None
            }
            "ArrowDown" => {
                self.select_next();
                None
            }
            "Enter" => self.open_selected(),
            "Backspace" => {
                self.go_up();
                None
            }
            "Delete" => {
                self.delete_selected();
                None
            }
            "Escape" => {
                self.clipboard = None;
                self.set_status("Clipboard cleared", false);
                None
            }
            // n = new file
            "n" if !ctrl => {
                self.start_new_file();
                None
            }
            // N = new directory (uppercase)
            "N" => {
                self.start_new_directory();
                None
            }
            // d = delete
            "d" if !ctrl => {
                self.delete_selected();
                None
            }
            // r = rename, F2 = rename
            "r" if !ctrl => {
                self.start_rename();
                None
            }
            "F2" => {
                self.start_rename();
                None
            }
            // c = copy
            "c" if !ctrl => {
                self.copy_selected();
                None
            }
            // x = cut
            "x" if !ctrl => {
                self.cut_selected();
                None
            }
            // p or v = paste
            "p" | "v" if !ctrl => {
                self.paste();
                None
            }
            // Ctrl+R to refresh
            _ if code == "KeyR" && ctrl => {
                self.refresh();
                self.set_status("Refreshed", false);
                None
            }
            _ => None,
        }
    }

    /// Get visible entries with scroll offset
    pub fn visible_entries(&self, count: usize) -> impl Iterator<Item = (usize, &Entry)> {
        self.entries
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .take(count)
    }
}

impl Default for FileBrowser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_kernel() {
        syscall::KERNEL.with(|k| {
            use crate::kernel::syscall::Kernel;
            *k.borrow_mut() = Kernel::new();
            let pid = k.borrow_mut().spawn_process("test", None);
            k.borrow_mut().set_current(pid);
        });
    }

    #[test]
    fn test_filebrowser_new() {
        setup_test_kernel();
        let browser = FileBrowser::new();
        assert_eq!(browser.cwd(), &PathBuf::from("/"));
        assert!(browser.error().is_none());
    }

    #[test]
    fn test_filebrowser_root_has_entries() {
        setup_test_kernel();
        // Root should at least have /home and /etc (created by kernel)
        let browser = FileBrowser::new();
        // Root has no ".." entry
        assert!(browser.entries().iter().all(|e| e.name != ".."));
    }

    #[test]
    fn test_filebrowser_navigate_home() {
        setup_test_kernel();
        let mut browser = FileBrowser::new();
        browser.cwd = PathBuf::from("/home");
        browser.refresh();

        // Should have ".." entry
        assert!(browser.entries().iter().any(|e| e.name == ".."));
    }

    #[test]
    fn test_filebrowser_selection() {
        setup_test_kernel();
        let mut browser = FileBrowser::new();

        let initial = browser.selected();
        browser.select_next();

        if !browser.entries().is_empty() {
            assert!(browser.selected() <= browser.entries().len());
        }

        browser.select_prev();
        assert!(browser.selected() <= initial || browser.entries().is_empty());
    }

    #[test]
    fn test_filebrowser_go_up_from_root() {
        setup_test_kernel();
        let mut browser = FileBrowser::new();
        browser.go_up();
        // Should stay at root
        assert_eq!(browser.cwd(), &PathBuf::from("/"));
    }

    #[test]
    fn test_filebrowser_go_up() {
        setup_test_kernel();
        let mut browser = FileBrowser::new();
        browser.cwd = PathBuf::from("/home/user");
        browser.go_up();
        assert_eq!(browser.cwd(), &PathBuf::from("/home"));
    }

    #[test]
    fn test_filebrowser_handle_arrows() {
        setup_test_kernel();
        let mut browser = FileBrowser::new();

        // Down arrow
        let result = browser.handle_key("ArrowDown", "ArrowDown", false, false);
        assert!(result.is_none());

        // Up arrow
        let result = browser.handle_key("ArrowUp", "ArrowUp", false, false);
        assert!(result.is_none());
    }

    #[test]
    fn test_filebrowser_handle_backspace() {
        setup_test_kernel();
        let mut browser = FileBrowser::new();
        browser.cwd = PathBuf::from("/home");
        browser.refresh();

        let result = browser.handle_key("Backspace", "Backspace", false, false);
        assert!(result.is_none());
        assert_eq!(browser.cwd(), &PathBuf::from("/"));
    }

    #[test]
    fn test_filebrowser_handle_refresh() {
        setup_test_kernel();
        let mut browser = FileBrowser::new();

        let result = browser.handle_key("r", "KeyR", true, false);
        assert!(result.is_none());
    }

    #[test]
    fn test_filebrowser_entries_sorted() {
        setup_test_kernel();

        // Create some test files and directories
        syscall::mkdir("/test_sort").ok();
        syscall::mkdir("/test_sort/zdir").ok();
        syscall::mkdir("/test_sort/adir").ok();

        let fd = syscall::open("/test_sort/bfile.txt", syscall::OpenFlags::WRITE).unwrap();
        syscall::write(fd, b"test").unwrap();
        syscall::close(fd).unwrap();

        let mut browser = FileBrowser::new();
        browser.cwd = PathBuf::from("/test_sort");
        browser.refresh();

        // Should be sorted: directories first (adir, zdir), then files (bfile.txt)
        // Also has ".." at the beginning
        let names: Vec<&str> = browser.entries().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names[0], "..");
        assert_eq!(names[1], "adir");
        assert_eq!(names[2], "zdir");
        assert_eq!(names[3], "bfile.txt");
    }

    #[test]
    fn test_entry_types() {
        let dir = Entry::directory("docs");
        assert!(dir.is_dir());
        assert!(dir.size.is_none());

        let file = Entry::file("readme.txt", 1024);
        assert!(!file.is_dir());
        assert_eq!(file.size, Some(1024));
    }

    // ========== FILE OPERATION TESTS ==========

    #[test]
    fn test_input_mode_new_file() {
        setup_test_kernel();
        let mut browser = FileBrowser::new();

        assert_eq!(browser.input_mode(), &InputMode::None);
        browser.start_new_file();
        assert_eq!(browser.input_mode(), &InputMode::NewFile);
        assert!(browser.input_buffer().is_empty());

        browser.cancel_input();
        assert_eq!(browser.input_mode(), &InputMode::None);
    }

    #[test]
    fn test_input_mode_new_directory() {
        setup_test_kernel();
        let mut browser = FileBrowser::new();

        browser.start_new_directory();
        assert_eq!(browser.input_mode(), &InputMode::NewDirectory);
    }

    #[test]
    fn test_input_mode_rename() {
        setup_test_kernel();
        syscall::mkdir("/rename_test").ok();

        let mut browser = FileBrowser::new();
        browser.cwd = PathBuf::from("/rename_test");
        browser.refresh();

        // Can't rename ".." entry
        browser.start_rename();
        assert!(browser.status().is_some());
        assert!(browser.status().unwrap().is_error);
    }

    #[test]
    fn test_create_file_via_input() {
        setup_test_kernel();
        syscall::mkdir("/create_test").ok();

        let mut browser = FileBrowser::new();
        browser.cwd = PathBuf::from("/create_test");
        browser.refresh();

        // Enter new file mode
        browser.handle_key("n", "KeyN", false, false);
        assert_eq!(browser.input_mode(), &InputMode::NewFile);

        // Type filename
        browser.handle_key("t", "KeyT", false, false);
        browser.handle_key("e", "KeyE", false, false);
        browser.handle_key("s", "KeyS", false, false);
        browser.handle_key("t", "KeyT", false, false);
        browser.handle_key(".", "Period", false, false);
        browser.handle_key("t", "KeyT", false, false);
        browser.handle_key("x", "KeyX", false, false);
        browser.handle_key("t", "KeyT", false, false);
        assert_eq!(browser.input_buffer(), "test.txt");

        // Confirm
        browser.handle_key("Enter", "Enter", false, false);
        assert_eq!(browser.input_mode(), &InputMode::None);

        // File should exist
        assert!(syscall::exists("/create_test/test.txt").unwrap());
    }

    #[test]
    fn test_create_directory_via_input() {
        setup_test_kernel();
        syscall::mkdir("/create_dir_test").ok();

        let mut browser = FileBrowser::new();
        browser.cwd = PathBuf::from("/create_dir_test");
        browser.refresh();

        // Enter new directory mode (uppercase N)
        browser.handle_key("N", "KeyN", false, false);
        assert_eq!(browser.input_mode(), &InputMode::NewDirectory);

        // Type directory name
        browser.input_buffer = "mydir".to_string();

        // Confirm
        browser.confirm_input();
        assert_eq!(browser.input_mode(), &InputMode::None);

        // Directory should exist
        assert!(syscall::exists("/create_dir_test/mydir").unwrap());
    }

    #[test]
    fn test_delete_file() {
        setup_test_kernel();
        syscall::mkdir("/delete_test").ok();

        let fd = syscall::open("/delete_test/todelete.txt", OpenFlags::WRITE).unwrap();
        syscall::write(fd, b"delete me").unwrap();
        syscall::close(fd).unwrap();

        let mut browser = FileBrowser::new();
        browser.cwd = PathBuf::from("/delete_test");
        browser.refresh();

        // Select the file (skip "..")
        browser.select_next();
        assert_eq!(browser.entries()[browser.selected()].name, "todelete.txt");

        // Delete
        browser.handle_key("d", "KeyD", false, false);

        // File should be gone
        assert!(!syscall::exists("/delete_test/todelete.txt").unwrap());
    }

    #[test]
    fn test_delete_empty_directory() {
        setup_test_kernel();
        syscall::mkdir("/delete_dir_test").ok();
        syscall::mkdir("/delete_dir_test/emptydir").ok();

        let mut browser = FileBrowser::new();
        browser.cwd = PathBuf::from("/delete_dir_test");
        browser.refresh();

        // Select the directory
        browser.select_next();
        assert_eq!(browser.entries()[browser.selected()].name, "emptydir");

        browser.delete_selected();

        assert!(!syscall::exists("/delete_dir_test/emptydir").unwrap());
    }

    #[test]
    fn test_rename_file() {
        setup_test_kernel();
        syscall::mkdir("/rename_file_test").ok();

        let fd = syscall::open("/rename_file_test/old.txt", OpenFlags::WRITE).unwrap();
        syscall::write(fd, b"content").unwrap();
        syscall::close(fd).unwrap();

        let mut browser = FileBrowser::new();
        browser.cwd = PathBuf::from("/rename_file_test");
        browser.refresh();

        // Select the file
        browser.select_next();

        // Start rename
        browser.handle_key("r", "KeyR", false, false);
        assert_eq!(browser.input_mode(), &InputMode::Rename);
        assert_eq!(browser.input_buffer(), "old.txt");

        // Clear and type new name
        browser.input_buffer = "new.txt".to_string();
        browser.confirm_input();

        assert!(!syscall::exists("/rename_file_test/old.txt").unwrap());
        assert!(syscall::exists("/rename_file_test/new.txt").unwrap());
    }

    #[test]
    fn test_copy_paste_file() {
        setup_test_kernel();
        syscall::mkdir("/copy_test").ok();
        syscall::mkdir("/copy_test/src").ok();
        syscall::mkdir("/copy_test/dest").ok();

        let fd = syscall::open("/copy_test/src/file.txt", OpenFlags::WRITE).unwrap();
        syscall::write(fd, b"copy me").unwrap();
        syscall::close(fd).unwrap();

        let mut browser = FileBrowser::new();
        browser.cwd = PathBuf::from("/copy_test/src");
        browser.refresh();

        // Select and copy the file
        browser.select_next();
        browser.handle_key("c", "KeyC", false, false);
        assert!(browser.clipboard().is_some());
        assert!(!browser.clipboard().unwrap().is_cut);

        // Navigate to dest
        browser.cwd = PathBuf::from("/copy_test/dest");
        browser.refresh();

        // Paste
        browser.handle_key("p", "KeyP", false, false);

        // Both files should exist
        assert!(syscall::exists("/copy_test/src/file.txt").unwrap());
        assert!(syscall::exists("/copy_test/dest/file.txt").unwrap());
    }

    #[test]
    fn test_cut_paste_file() {
        setup_test_kernel();
        syscall::mkdir("/move_test").ok();
        syscall::mkdir("/move_test/src").ok();
        syscall::mkdir("/move_test/dest").ok();

        let fd = syscall::open("/move_test/src/file.txt", OpenFlags::WRITE).unwrap();
        syscall::write(fd, b"move me").unwrap();
        syscall::close(fd).unwrap();

        let mut browser = FileBrowser::new();
        browser.cwd = PathBuf::from("/move_test/src");
        browser.refresh();

        // Select and cut the file
        browser.select_next();
        browser.handle_key("x", "KeyX", false, false);
        assert!(browser.clipboard().is_some());
        assert!(browser.clipboard().unwrap().is_cut);

        // Navigate to dest
        browser.cwd = PathBuf::from("/move_test/dest");
        browser.refresh();

        // Paste
        browser.handle_key("v", "KeyV", false, false);

        // Source should be gone, dest should exist
        assert!(!syscall::exists("/move_test/src/file.txt").unwrap());
        assert!(syscall::exists("/move_test/dest/file.txt").unwrap());
    }

    #[test]
    fn test_paste_to_existing_fails() {
        setup_test_kernel();
        syscall::mkdir("/paste_exist_test").ok();

        let fd = syscall::open("/paste_exist_test/file.txt", OpenFlags::WRITE).unwrap();
        syscall::write(fd, b"original").unwrap();
        syscall::close(fd).unwrap();

        let mut browser = FileBrowser::new();
        browser.cwd = PathBuf::from("/paste_exist_test");
        browser.refresh();

        // Copy the file
        browser.select_next();
        browser.copy_selected();

        // Try to paste (destination already exists)
        browser.paste();

        // Should have error status
        assert!(browser.status().is_some());
        assert!(browser.status().unwrap().is_error);
    }

    #[test]
    fn test_cancel_input_with_escape() {
        setup_test_kernel();
        let mut browser = FileBrowser::new();

        browser.start_new_file();
        browser.input_buffer = "test".to_string();

        browser.handle_key("Escape", "Escape", false, false);
        assert_eq!(browser.input_mode(), &InputMode::None);
        assert!(browser.input_buffer().is_empty());
    }

    #[test]
    fn test_clear_clipboard_with_escape() {
        setup_test_kernel();
        syscall::mkdir("/clipboard_test").ok();

        let fd = syscall::open("/clipboard_test/file.txt", OpenFlags::WRITE).unwrap();
        syscall::close(fd).unwrap();

        let mut browser = FileBrowser::new();
        browser.cwd = PathBuf::from("/clipboard_test");
        browser.refresh();

        browser.select_next();
        browser.copy_selected();
        assert!(browser.clipboard().is_some());

        browser.handle_key("Escape", "Escape", false, false);
        assert!(browser.clipboard().is_none());
    }

    #[test]
    fn test_input_validation_empty_name() {
        setup_test_kernel();
        let mut browser = FileBrowser::new();

        browser.start_new_file();
        // Don't type anything, just confirm
        browser.confirm_input();

        // Should have error and still be in None mode (reset after error)
        assert!(browser.status().is_some());
        assert!(browser.status().unwrap().is_error);
    }

    #[test]
    fn test_input_validation_slash_in_name() {
        setup_test_kernel();
        let mut browser = FileBrowser::new();

        browser.start_new_file();
        browser.input_buffer = "bad/name".to_string();
        browser.confirm_input();

        assert!(browser.status().is_some());
        assert!(browser.status().unwrap().is_error);
    }

    #[test]
    fn test_cannot_delete_parent() {
        setup_test_kernel();
        syscall::mkdir("/parent_test").ok();

        let mut browser = FileBrowser::new();
        browser.cwd = PathBuf::from("/parent_test");
        browser.refresh();

        // First entry should be ".."
        assert_eq!(browser.entries()[0].name, "..");

        browser.delete_selected();

        // Should have error status
        assert!(browser.status().is_some());
        assert!(browser.status().unwrap().is_error);
    }

    #[test]
    fn test_keyboard_shortcuts() {
        setup_test_kernel();
        let mut browser = FileBrowser::new();

        // n = new file
        browser.handle_key("n", "KeyN", false, false);
        assert_eq!(browser.input_mode(), &InputMode::NewFile);
        browser.cancel_input();

        // N = new directory
        browser.handle_key("N", "KeyN", false, false);
        assert_eq!(browser.input_mode(), &InputMode::NewDirectory);
        browser.cancel_input();

        // F2 = rename
        syscall::mkdir("/shortcut_test").ok();
        let fd = syscall::open("/shortcut_test/file.txt", OpenFlags::WRITE).unwrap();
        syscall::close(fd).unwrap();

        browser.cwd = PathBuf::from("/shortcut_test");
        browser.refresh();
        browser.select_next();

        browser.handle_key("F2", "F2", false, false);
        assert_eq!(browser.input_mode(), &InputMode::Rename);
    }

    #[test]
    fn test_backspace_in_input_mode() {
        setup_test_kernel();
        let mut browser = FileBrowser::new();

        browser.start_new_file();
        browser.input_buffer = "test".to_string();

        browser.handle_key("Backspace", "Backspace", false, false);
        assert_eq!(browser.input_buffer(), "tes");

        browser.handle_key("Backspace", "Backspace", false, false);
        assert_eq!(browser.input_buffer(), "te");
    }

    #[test]
    fn test_status_messages() {
        setup_test_kernel();
        let mut browser = FileBrowser::new();

        assert!(browser.status().is_none());

        browser.set_status("Test message", false);
        assert!(browser.status().is_some());
        assert_eq!(browser.status().unwrap().text, "Test message");
        assert!(!browser.status().unwrap().is_error);

        browser.clear_status();
        assert!(browser.status().is_none());
    }
}
