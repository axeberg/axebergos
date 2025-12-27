//! Package installer
//!
//! Handles downloading, extracting, and installing packages.
//!
//! # Package Archive Format (.axepkg)
//!
//! The package archive is a simple concatenated format:
//!
//! ```text
//! [HEADER]
//! AXEPKG\x00\x01           # Magic + version (8 bytes)
//! manifest_size: u32       # Size of manifest (4 bytes, little-endian)
//! num_files: u32           # Number of files (4 bytes, little-endian)
//!
//! [MANIFEST]
//! package.toml content     # manifest_size bytes
//!
//! [FILE ENTRIES]
//! For each file:
//!   path_len: u16          # Path length (2 bytes)
//!   path: bytes            # Path (path_len bytes, UTF-8)
//!   content_len: u32       # Content length (4 bytes)
//!   content: bytes         # File content (content_len bytes)
//! ```

use super::PackageId;
use super::checksum::{Checksum, verify_checksum};
use super::database::{InstalledPackage, PackageDatabase};
use super::error::{PkgError, PkgResult};
use super::manifest::PackageManifest;
use super::paths;
use super::registry::PackageRegistry;
use super::resolver::ResolvedPackage;
use crate::kernel::syscall;

/// Package archive magic number
const AXEPKG_MAGIC: &[u8; 8] = b"AXEPKG\x00\x01";

/// Package installer
pub struct PackageInstaller {
    /// Whether to verify checksums
    verify_checksums: bool,
    /// Whether to keep cached archives
    keep_cache: bool,
}

impl PackageInstaller {
    /// Create a new installer
    pub fn new() -> Self {
        Self {
            verify_checksums: true,
            keep_cache: true,
        }
    }

    /// Set whether to verify checksums
    pub fn set_verify_checksums(&mut self, verify: bool) {
        self.verify_checksums = verify;
    }

    /// Set whether to keep cached archives
    pub fn set_keep_cache(&mut self, keep: bool) {
        self.keep_cache = keep;
    }

    /// Install a resolved package from the registry
    #[cfg(target_arch = "wasm32")]
    pub async fn install(
        &self,
        package: &ResolvedPackage,
        registry: &PackageRegistry,
    ) -> PkgResult<()> {
        // Download the package archive
        let archive_data = registry
            .download_package(&package.id.name, &package.id.version)
            .await?;

        // Cache the archive
        if self.keep_cache {
            self.cache_archive(&package.id, &archive_data)?;
        }

        // Extract and install
        self.install_from_archive(&archive_data)
    }

    /// Install from registry (non-WASM stub)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn install(
        &self,
        _package: &ResolvedPackage,
        _registry: &PackageRegistry,
    ) -> PkgResult<()> {
        Err(PkgError::NotAvailable("WASM required".to_string()))
    }

    /// Install a package from a local file
    pub fn install_local(
        &self,
        path: &str,
        database: &mut PackageDatabase,
    ) -> PkgResult<PackageId> {
        // Read the archive
        let archive_data = read_file_bytes(path)?;

        // Extract and install
        let manifest = self.install_from_archive(&archive_data)?;

        // Record in database
        let id = PackageId::new(&manifest.name, manifest.version.clone());
        database.record_installed(&id, &manifest)?;

        Ok(id)
    }

    /// Install from archive bytes
    fn install_from_archive(&self, data: &[u8]) -> PkgResult<PackageManifest> {
        // Parse the archive
        let archive = self.parse_archive(data)?;

        // Verify checksums if enabled
        if self.verify_checksums {
            for (bin_name, bin_data) in &archive.files {
                if bin_name.ends_with(".wasm") {
                    // Find matching binary entry in manifest
                    for bin_entry in &archive.manifest.binaries {
                        if bin_entry.path.ends_with(bin_name)
                            && let Some(ref expected) = bin_entry.checksum
                        {
                            verify_checksum(bin_data, expected)?;
                        }
                    }
                }
            }
        }

        // Install binaries to /bin
        for bin_entry in &archive.manifest.binaries {
            // Find the binary data
            let bin_data = archive
                .files
                .iter()
                .find(|(name, _)| name == &bin_entry.path || bin_entry.path.ends_with(name));

            if let Some((_, data)) = bin_data {
                let dest_path = format!("{}/{}.wasm", paths::BIN_DIR, bin_entry.name);
                write_file_bytes(&dest_path, data)?;

                // Make executable
                let _ = syscall::chmod(&dest_path, 0o755);
            }
        }

        Ok(archive.manifest)
    }

    /// Parse a package archive
    fn parse_archive(&self, data: &[u8]) -> PkgResult<PackageArchive> {
        // Check minimum size
        if data.len() < 16 {
            return Err(PkgError::InvalidArchive("archive too small".to_string()));
        }

        // Check magic
        if &data[0..8] != AXEPKG_MAGIC {
            // Try to parse as raw package.toml + WASM for simpler packages
            return self.parse_simple_archive(data);
        }

        // Read header
        let manifest_size = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
        let num_files = u32::from_le_bytes([data[12], data[13], data[14], data[15]]) as usize;

        // Check sizes
        if data.len() < 16 + manifest_size {
            return Err(PkgError::InvalidArchive(
                "manifest extends past archive".to_string(),
            ));
        }

        // Read manifest
        let manifest_data = &data[16..16 + manifest_size];
        let manifest_str = String::from_utf8(manifest_data.to_vec())
            .map_err(|_| PkgError::InvalidArchive("invalid manifest UTF-8".to_string()))?;
        let manifest = PackageManifest::parse(&manifest_str)?;

        // Read files
        let mut files = Vec::new();
        let mut offset = 16 + manifest_size;

        for _ in 0..num_files {
            if offset + 2 > data.len() {
                return Err(PkgError::InvalidArchive("truncated file entry".to_string()));
            }

            // Read path length
            let path_len = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
            offset += 2;

            if offset + path_len > data.len() {
                return Err(PkgError::InvalidArchive("truncated path".to_string()));
            }

            // Read path
            let path = String::from_utf8(data[offset..offset + path_len].to_vec())
                .map_err(|_| PkgError::InvalidArchive("invalid path UTF-8".to_string()))?;
            offset += path_len;

            if offset + 4 > data.len() {
                return Err(PkgError::InvalidArchive(
                    "truncated content length".to_string(),
                ));
            }

            // Read content length
            let content_len = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]) as usize;
            offset += 4;

            if offset + content_len > data.len() {
                return Err(PkgError::InvalidArchive("truncated content".to_string()));
            }

            // Read content
            let content = data[offset..offset + content_len].to_vec();
            offset += content_len;

            files.push((path, content));
        }

        Ok(PackageArchive { manifest, files })
    }

    /// Parse a simpler archive format (just WASM + manifest)
    fn parse_simple_archive(&self, data: &[u8]) -> PkgResult<PackageArchive> {
        // Try to interpret as raw WASM module
        // Check for WASM magic number
        if data.len() >= 4 && &data[0..4] == b"\x00asm" {
            // This is a raw WASM file
            // Create a minimal manifest
            let manifest = PackageManifest {
                name: "unknown".to_string(),
                version: super::version::Version::new(0, 0, 0),
                description: None,
                authors: vec![],
                license: None,
                repository: None,
                homepage: None,
                keywords: vec![],
                binaries: vec![super::manifest::BinaryEntry {
                    name: "unknown".to_string(),
                    path: "bin/unknown.wasm".to_string(),
                    checksum: Some(Checksum::compute(data)),
                }],
                dependencies: vec![],
                dev_dependencies: vec![],
            };

            return Ok(PackageArchive {
                manifest,
                files: vec![("bin/unknown.wasm".to_string(), data.to_vec())],
            });
        }

        // Try to parse as TOML manifest
        let content = String::from_utf8(data.to_vec())
            .map_err(|_| PkgError::InvalidArchive("not a valid archive format".to_string()))?;

        if content.contains("[package]") {
            let manifest = PackageManifest::parse(&content)?;
            return Ok(PackageArchive {
                manifest,
                files: vec![],
            });
        }

        Err(PkgError::InvalidArchive(
            "unrecognized archive format".to_string(),
        ))
    }

    /// Remove an installed package
    pub fn remove(&self, package: &InstalledPackage) -> PkgResult<()> {
        // Remove binary files
        for bin_path in &package.binaries {
            if path_exists(bin_path) {
                syscall::remove_file(bin_path)
                    .map_err(|e| PkgError::IoError(format!("{}: {}", bin_path, e)))?;
            }
        }

        // Remove package metadata directory
        let pkg_dir = format!("{}/{}", paths::PKG_PACKAGES, package.id().dir_name());
        if path_exists(&pkg_dir) {
            // Remove all files in the directory
            if let Ok(entries) = syscall::readdir(&pkg_dir) {
                for entry in entries {
                    let entry_path = format!("{}/{}", pkg_dir, entry);
                    let _ = syscall::remove_file(&entry_path);
                }
            }
            let _ = syscall::rmdir(&pkg_dir);
        }

        Ok(())
    }

    /// Verify an installed package's integrity
    pub fn verify(&self, package: &InstalledPackage) -> PkgResult<bool> {
        for bin_path in &package.binaries {
            if !path_exists(bin_path) {
                return Ok(false);
            }

            // If we have a manifest checksum, verify the manifest
            if let Some(ref expected) = package.manifest_checksum {
                let manifest_path = format!(
                    "{}/{}/package.toml",
                    paths::PKG_PACKAGES,
                    package.id().dir_name()
                );
                if let Ok(content) = read_file(&manifest_path) {
                    let actual = Checksum::compute(content.as_bytes());
                    if &actual != expected {
                        return Ok(false);
                    }
                }
            }
        }

        Ok(true)
    }

    /// Clean the package cache
    pub fn clean_cache(&self) -> PkgResult<()> {
        if !path_exists(paths::PKG_CACHE) {
            return Ok(());
        }

        let entries = syscall::readdir(paths::PKG_CACHE)
            .map_err(|e| PkgError::IoError(format!("{}: {}", paths::PKG_CACHE, e)))?;

        for entry in entries {
            let path = format!("{}/{}", paths::PKG_CACHE, entry);
            let _ = syscall::remove_file(&path);
        }

        Ok(())
    }
}

impl Default for PackageInstaller {
    fn default() -> Self {
        Self::new()
    }
}

/// Parsed package archive
struct PackageArchive {
    manifest: PackageManifest,
    files: Vec<(String, Vec<u8>)>,
}

// Helper functions

fn path_exists(path: &str) -> bool {
    syscall::exists(path).unwrap_or(false)
}

fn mkdir_recursive(path: &str) -> PkgResult<()> {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let mut current = String::new();

    for part in parts {
        current.push('/');
        current.push_str(part);

        if !path_exists(&current) {
            syscall::mkdir(&current)
                .map_err(|e| PkgError::IoError(format!("{}: {}", current, e)))?;
        }
    }

    Ok(())
}

fn read_file(path: &str) -> PkgResult<String> {
    let bytes = read_file_bytes(path)?;
    String::from_utf8(bytes).map_err(|_| PkgError::IoError(format!("{}: invalid UTF-8", path)))
}

fn read_file_bytes(path: &str) -> PkgResult<Vec<u8>> {
    let fd = syscall::open(path, syscall::OpenFlags::READ)
        .map_err(|e| PkgError::IoError(format!("{}: {}", path, e)))?;

    let mut content = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match syscall::read(fd, &mut buf) {
            Ok(0) => break,
            Ok(n) => content.extend_from_slice(&buf[..n]),
            Err(e) => {
                let _ = syscall::close(fd);
                return Err(PkgError::IoError(format!("{}: {}", path, e)));
            }
        }
    }

    let _ = syscall::close(fd);
    Ok(content)
}

fn write_file_bytes(path: &str, data: &[u8]) -> PkgResult<()> {
    // Ensure parent directory exists
    if let Some(pos) = path.rfind('/') {
        let parent = &path[..pos];
        if !parent.is_empty() && !path_exists(parent) {
            mkdir_recursive(parent)?;
        }
    }

    let fd = syscall::open(path, syscall::OpenFlags::WRITE)
        .map_err(|e| PkgError::IoError(format!("{}: {}", path, e)))?;

    syscall::write(fd, data).map_err(|e| PkgError::IoError(format!("{}: {}", path, e)))?;

    syscall::close(fd).map_err(|e| PkgError::IoError(format!("{}: {}", path, e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::pkg::version::Version;

    #[test]
    fn test_installer_new() {
        let installer = PackageInstaller::new();
        assert!(installer.verify_checksums);
        assert!(installer.keep_cache);
    }

    #[test]
    fn test_parse_raw_wasm() {
        let wasm_data = b"\x00asm\x01\x00\x00\x00some wasm content";

        let installer = PackageInstaller::new();
        let parsed = installer.parse_archive(wasm_data).unwrap();

        assert_eq!(parsed.manifest.name, "unknown");
        assert_eq!(parsed.files.len(), 1);
    }

    #[test]
    fn test_parse_invalid_archive() {
        let invalid_data = b"not a valid archive";

        let installer = PackageInstaller::new();
        let result = installer.parse_archive(invalid_data);

        assert!(result.is_err());
    }
}
