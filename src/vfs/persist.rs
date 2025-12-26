//! OPFS Persistence Layer
//!
//! Uses the Origin Private File System (OPFS) API to persist the VFS state.
//! OPFS provides a fast, sandboxed filesystem accessible from the browser.
//!
//! Key design decisions:
//! - Single JSON file for entire filesystem (simple, atomic)
//! - Async operations via wasm-bindgen-futures
//! - Graceful fallback if OPFS unavailable

use super::MemoryFs;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

/// Persistence manager for VFS
pub struct Persistence;

/// The filename we use in OPFS
const FS_FILENAME: &str = "axeberg_fs.json";

impl Persistence {
    /// Save filesystem to OPFS
    pub async fn save(fs: &MemoryFs) -> Result<(), String> {
        let data = fs
            .to_json()
            .map_err(|e| format!("Serialize error: {}", e))?;

        // Get OPFS root
        let root = Self::get_opfs_root().await?;

        // Create/open the file
        let file_opts = web_sys::FileSystemGetFileOptions::new();
        file_opts.set_create(true);

        let file_handle: web_sys::FileSystemFileHandle =
            JsFuture::from(root.get_file_handle_with_options(FS_FILENAME, &file_opts))
                .await
                .map_err(|e| format!("Failed to get file handle: {:?}", e))?
                .dyn_into()
                .map_err(|_| "Failed to cast to FileSystemFileHandle")?;

        // Create writable stream
        let writable: web_sys::FileSystemWritableFileStream =
            JsFuture::from(file_handle.create_writable())
                .await
                .map_err(|e| format!("Failed to create writable: {:?}", e))?
                .dyn_into()
                .map_err(|_| "Failed to cast to FileSystemWritableFileStream")?;

        // Write data
        let uint8_array = js_sys::Uint8Array::from(data.as_slice());
        let write_promise = writable
            .write_with_buffer_source(&uint8_array)
            .map_err(|e| format!("Failed to get write promise: {:?}", e))?;
        JsFuture::from(write_promise)
            .await
            .map_err(|e| format!("Failed to write: {:?}", e))?;

        // Close the stream
        JsFuture::from(writable.close())
            .await
            .map_err(|e| format!("Failed to close: {:?}", e))?;

        Ok(())
    }

    /// Load filesystem from OPFS
    pub async fn load() -> Result<Option<MemoryFs>, String> {
        // Get OPFS root
        let root = match Self::get_opfs_root().await {
            Ok(r) => r,
            Err(_) => return Ok(None), // OPFS not available
        };

        // Try to get the file
        let file_opts = web_sys::FileSystemGetFileOptions::new();
        file_opts.set_create(false);

        let file_handle: web_sys::FileSystemFileHandle = match JsFuture::from(
            root.get_file_handle_with_options(FS_FILENAME, &file_opts),
        )
        .await
        {
            Ok(handle) => handle
                .dyn_into()
                .map_err(|_| "Failed to cast to FileSystemFileHandle")?,
            Err(_) => return Ok(None), // File doesn't exist yet
        };

        // Get the file
        let file: web_sys::File = JsFuture::from(file_handle.get_file())
            .await
            .map_err(|e| format!("Failed to get file: {:?}", e))?
            .dyn_into()
            .map_err(|_| "Failed to cast to File")?;

        // Read the file contents
        let array_buffer = JsFuture::from(file.array_buffer())
            .await
            .map_err(|e| format!("Failed to read file: {:?}", e))?;

        let uint8_array = js_sys::Uint8Array::new(&array_buffer);
        let data = uint8_array.to_vec();

        // Deserialize
        let fs = MemoryFs::from_json(&data).map_err(|e| format!("Deserialize error: {}", e))?;

        Ok(Some(fs))
    }

    /// Check if OPFS is available
    pub async fn is_available() -> bool {
        Self::get_opfs_root().await.is_ok()
    }

    /// Get the OPFS root directory handle
    async fn get_opfs_root() -> Result<web_sys::FileSystemDirectoryHandle, String> {
        let window = web_sys::window().ok_or_else(|| "No window object".to_string())?;
        let navigator = window.navigator();

        // StorageManager access
        let storage = navigator.storage();

        // Get OPFS root
        let root: web_sys::FileSystemDirectoryHandle = JsFuture::from(storage.get_directory())
            .await
            .map_err(|e| format!("Failed to get OPFS root: {:?}", e))?
            .dyn_into()
            .map_err(|_| "Failed to cast to FileSystemDirectoryHandle")?;

        Ok(root)
    }

    /// Clear persisted data
    pub async fn clear() -> Result<(), String> {
        let root = Self::get_opfs_root().await?;

        // Remove the file if it exists
        JsFuture::from(root.remove_entry(FS_FILENAME)).await.ok(); // Ignore errors (file might not exist)

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::{FileSystem, OpenOptions};

    #[test]
    fn test_fs_snapshot_roundtrip() {
        let mut fs = MemoryFs::new();

        // Create some content
        fs.create_dir("/home").unwrap();
        fs.create_dir("/home/user").unwrap();

        let handle = fs
            .open(
                "/home/user/test.txt",
                OpenOptions::new().write(true).create(true),
            )
            .unwrap();
        fs.write(handle, b"hello persistence").unwrap();
        fs.close(handle).unwrap();

        // Serialize
        let json = fs.to_json().unwrap();

        // Deserialize
        let restored = MemoryFs::from_json(&json).unwrap();

        // Verify
        assert!(restored.exists("/home/user/test.txt"));
        let meta = restored.metadata("/home/user/test.txt").unwrap();
        assert_eq!(meta.size, 17); // "hello persistence"
    }

    #[test]
    fn test_empty_fs_roundtrip() {
        let fs = MemoryFs::new();
        let json = fs.to_json().unwrap();
        let restored = MemoryFs::from_json(&json).unwrap();
        assert!(restored.exists("/"));
    }
}
