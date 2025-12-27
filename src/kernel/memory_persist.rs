//! OPFS Persistence for Memory Regions
//!
//! Provides persistence of memory data to the Origin Private File System (OPFS).
//! This allows processes to save and restore named data blobs across sessions.
//!
//! Key features:
//! - Named data storage (key-value style)
//! - Async operations via wasm-bindgen-futures
//! - Simple binary format (no overhead for small data)
//! - Directory-based organization for isolation

/// Memory persistence manager
pub struct MemoryPersistence;

/// Statistics about persisted memory data
#[derive(Debug, Clone)]
pub struct MemoryPersistStats {
    /// Number of stored data entries
    pub count: usize,
    /// Total size of all stored data in bytes
    pub total_size: usize,
    /// Names of all stored entries
    pub names: Vec<String>,
}

// ============================================================================
// WASM implementation (browser with OPFS)
// ============================================================================

#[cfg(target_arch = "wasm32")]
mod wasm_impl {
    use super::{MemoryPersistStats, MemoryPersistence};
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;

    /// Directory in OPFS where memory data is stored
    const MEMORY_DIR: &str = "axeberg_memory";

    impl MemoryPersistence {
        /// Save data to OPFS with a given name
        ///
        /// The data is stored as raw bytes under the given name.
        /// Existing data with the same name is overwritten.
        pub async fn save(name: &str, data: &[u8]) -> Result<(), String> {
            let dir = Self::get_memory_dir().await?;

            // Create/open the file
            let file_opts = web_sys::FileSystemGetFileOptions::new();
            file_opts.set_create(true);

            let file_handle: web_sys::FileSystemFileHandle =
                JsFuture::from(dir.get_file_handle_with_options(name, &file_opts))
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
            let uint8_array = js_sys::Uint8Array::from(data);
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

        /// Load data from OPFS by name
        ///
        /// Returns `None` if the data doesn't exist.
        pub async fn load(name: &str) -> Result<Option<Vec<u8>>, String> {
            let dir = match Self::get_memory_dir().await {
                Ok(d) => d,
                Err(_) => return Ok(None), // Directory doesn't exist
            };

            // Try to get the file
            let file_opts = web_sys::FileSystemGetFileOptions::new();
            file_opts.set_create(false);

            let file_handle: web_sys::FileSystemFileHandle =
                match JsFuture::from(dir.get_file_handle_with_options(name, &file_opts)).await {
                    Ok(handle) => handle
                        .dyn_into()
                        .map_err(|_| "Failed to cast to FileSystemFileHandle")?,
                    Err(_) => return Ok(None), // File doesn't exist
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

            Ok(Some(data))
        }

        /// Check if named data exists
        pub async fn exists(name: &str) -> bool {
            let dir = match Self::get_memory_dir().await {
                Ok(d) => d,
                Err(_) => return false,
            };

            let file_opts = web_sys::FileSystemGetFileOptions::new();
            file_opts.set_create(false);

            JsFuture::from(dir.get_file_handle_with_options(name, &file_opts))
                .await
                .is_ok()
        }

        /// Delete named data from OPFS
        pub async fn delete(name: &str) -> Result<(), String> {
            let dir = Self::get_memory_dir().await?;

            // Remove the file if it exists
            JsFuture::from(dir.remove_entry(name)).await.ok(); // Ignore errors

            Ok(())
        }

        /// List all stored data names
        pub async fn list() -> Result<Vec<String>, String> {
            let dir = match Self::get_memory_dir().await {
                Ok(d) => d,
                Err(_) => return Ok(Vec::new()),
            };

            let mut names = Vec::new();

            // Iterate over directory entries
            let entries = dir.entries();

            loop {
                let next_result = JsFuture::from(entries.next())
                    .await
                    .map_err(|e| format!("Failed to iterate: {:?}", e))?;

                let iter_result: js_sys::Object = next_result
                    .dyn_into()
                    .map_err(|_| "Failed to cast iterator result")?;

                // Check if done
                let done = js_sys::Reflect::get(&iter_result, &JsValue::from_str("done"))
                    .map_err(|_| "Failed to get done property")?
                    .as_bool()
                    .unwrap_or(true);

                if done {
                    break;
                }

                // Get the value (array of [name, handle])
                let value = js_sys::Reflect::get(&iter_result, &JsValue::from_str("value"))
                    .map_err(|_| "Failed to get value property")?;

                if let Some(arr) = value.dyn_ref::<js_sys::Array>() {
                    if let Some(name) = arr.get(0).as_string() {
                        names.push(name);
                    }
                }
            }

            Ok(names)
        }

        /// Get the size of stored data without loading it
        pub async fn size(name: &str) -> Result<Option<usize>, String> {
            let dir = match Self::get_memory_dir().await {
                Ok(d) => d,
                Err(_) => return Ok(None),
            };

            let file_opts = web_sys::FileSystemGetFileOptions::new();
            file_opts.set_create(false);

            let file_handle: web_sys::FileSystemFileHandle =
                match JsFuture::from(dir.get_file_handle_with_options(name, &file_opts)).await {
                    Ok(handle) => handle
                        .dyn_into()
                        .map_err(|_| "Failed to cast to FileSystemFileHandle")?,
                    Err(_) => return Ok(None),
                };

            let file: web_sys::File = JsFuture::from(file_handle.get_file())
                .await
                .map_err(|e| format!("Failed to get file: {:?}", e))?
                .dyn_into()
                .map_err(|_| "Failed to cast to File")?;

            Ok(Some(file.size() as usize))
        }

        /// Clear all stored memory data
        pub async fn clear() -> Result<(), String> {
            let root = Self::get_opfs_root().await?;

            // Remove the entire memory directory
            let opts = web_sys::FileSystemRemoveOptions::new();
            opts.set_recursive(true);
            JsFuture::from(root.remove_entry_with_options(MEMORY_DIR, &opts))
                .await
                .ok(); // Ignore errors (directory might not exist)

            Ok(())
        }

        /// Check if OPFS memory persistence is available
        pub async fn is_available() -> bool {
            Self::get_opfs_root().await.is_ok()
        }

        /// Get statistics about stored data
        pub async fn stats() -> Result<MemoryPersistStats, String> {
            let names = Self::list().await?;
            let mut total_size = 0usize;

            for name in &names {
                if let Ok(Some(size)) = Self::size(name).await {
                    total_size += size;
                }
            }

            Ok(MemoryPersistStats {
                count: names.len(),
                total_size,
                names,
            })
        }

        /// Get the OPFS root directory handle
        async fn get_opfs_root() -> Result<web_sys::FileSystemDirectoryHandle, String> {
            let window = web_sys::window().ok_or_else(|| "No window object".to_string())?;
            let navigator = window.navigator();
            let storage = navigator.storage();

            let root: web_sys::FileSystemDirectoryHandle = JsFuture::from(storage.get_directory())
                .await
                .map_err(|e| format!("Failed to get OPFS root: {:?}", e))?
                .dyn_into()
                .map_err(|_| "Failed to cast to FileSystemDirectoryHandle")?;

            Ok(root)
        }

        /// Get or create the memory directory
        async fn get_memory_dir() -> Result<web_sys::FileSystemDirectoryHandle, String> {
            let root = Self::get_opfs_root().await?;

            let dir_opts = web_sys::FileSystemGetDirectoryOptions::new();
            dir_opts.set_create(true);

            let dir: web_sys::FileSystemDirectoryHandle =
                JsFuture::from(root.get_directory_handle_with_options(MEMORY_DIR, &dir_opts))
                    .await
                    .map_err(|e| format!("Failed to get memory directory: {:?}", e))?
                    .dyn_into()
                    .map_err(|_| "Failed to cast to FileSystemDirectoryHandle")?;

            Ok(dir)
        }
    }
}

// ============================================================================
// Non-WASM stub implementation (for tests/native builds)
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
impl MemoryPersistence {
    /// Save data (stub - OPFS not available outside browser)
    pub async fn save(_name: &str, _data: &[u8]) -> Result<(), String> {
        Err("OPFS not available outside browser".to_string())
    }

    /// Load data (stub - always returns None)
    pub async fn load(_name: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }

    /// Check if data exists (stub - always returns false)
    pub async fn exists(_name: &str) -> bool {
        false
    }

    /// Delete data (stub - no-op)
    pub async fn delete(_name: &str) -> Result<(), String> {
        Ok(())
    }

    /// List stored data (stub - returns empty list)
    pub async fn list() -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }

    /// Get data size (stub - returns None)
    pub async fn size(_name: &str) -> Result<Option<usize>, String> {
        Ok(None)
    }

    /// Clear all data (stub - no-op)
    pub async fn clear() -> Result<(), String> {
        Ok(())
    }

    /// Check availability (stub - returns false)
    pub async fn is_available() -> bool {
        false
    }

    /// Get statistics (stub - returns empty stats)
    pub async fn stats() -> Result<MemoryPersistStats, String> {
        Ok(MemoryPersistStats {
            count: 0,
            total_size: 0,
            names: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;

    #[test]
    fn test_stats_struct() {
        let stats = MemoryPersistStats {
            count: 2,
            total_size: 1024,
            names: vec!["region1".to_string(), "region2".to_string()],
        };

        assert_eq!(stats.count, 2);
        assert_eq!(stats.total_size, 1024);
        assert_eq!(stats.names.len(), 2);
    }

    #[test]
    fn test_stub_save_fails() {
        let result = block_on(MemoryPersistence::save("test", b"data"));
        // On non-WASM, this should fail
        #[cfg(not(target_arch = "wasm32"))]
        assert!(result.is_err());
    }

    #[test]
    fn test_stub_load_returns_none() {
        let result = block_on(MemoryPersistence::load("test"));
        assert!(result.is_ok());
        #[cfg(not(target_arch = "wasm32"))]
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_stub_exists_returns_false() {
        #[cfg(not(target_arch = "wasm32"))]
        assert!(!block_on(MemoryPersistence::exists("test")));
    }

    #[test]
    fn test_stub_is_available_returns_false() {
        #[cfg(not(target_arch = "wasm32"))]
        assert!(!block_on(MemoryPersistence::is_available()));
    }
}
