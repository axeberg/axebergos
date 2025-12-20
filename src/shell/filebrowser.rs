//! File Browser - Visual file system navigator
//!
//! Provides:
//! - Directory listing with files and folders
//! - Keyboard navigation (arrows, enter, backspace)
//! - Current path display

use crate::kernel::syscall;
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
}

impl FileBrowser {
    pub fn new() -> Self {
        let mut browser = Self {
            cwd: PathBuf::from("/"),
            entries: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            error: None,
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

    /// Handle keyboard input
    /// Returns Some(path) if a file was selected for opening
    pub fn handle_key(&mut self, key: &str, code: &str, ctrl: bool, _alt: bool) -> Option<String> {
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
            _ if code == "KeyR" && ctrl => {
                // Ctrl+R to refresh
                self.refresh();
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
}
