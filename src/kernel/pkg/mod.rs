//! Package Manager for axeberg
//!
//! A full-featured package manager for installing, updating, and managing
//! WebAssembly command modules. Inspired by cargo, npm, and apt.
//!
//! # Features
//!
//! - **Semantic versioning**: Full semver support with version constraints
//! - **Dependency resolution**: Automatic resolution with conflict detection
//! - **Package registry**: Download packages from remote registries
//! - **Security**: SHA-256 checksums for integrity verification
//! - **Local database**: Track installed packages and their metadata
//!
//! # Package Format
//!
//! Packages are distributed as `.axepkg` files containing:
//! - `package.toml`: Package manifest with metadata and dependencies
//! - `bin/*.wasm`: WebAssembly command binaries
//! - `checksums.txt`: SHA-256 checksums for all files
//!
//! # Example Manifest (package.toml)
//!
//! ```toml
//! [package]
//! name = "hello"
//! version = "1.0.0"
//! description = "A hello world command"
//! authors = ["axeberg"]
//! license = "MIT"
//!
//! [[bin]]
//! name = "hello"
//! path = "bin/hello.wasm"
//!
//! [dependencies]
//! utils = "^1.0"
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use axeberg::kernel::pkg::{PackageManager, PackageId};
//!
//! let mut pm = PackageManager::new();
//!
//! // Install a package from registry
//! pm.install("hello", Some("1.0.0")).await?;
//!
//! // List installed packages
//! for pkg in pm.list_installed() {
//!     println!("{}: {}", pkg.name, pkg.version);
//! }
//!
//! // Remove a package
//! pm.remove("hello")?;
//! ```
//!
//! # Directory Structure
//!
//! ```text
//! /var/lib/pkg/
//! ├── db/                    # Package database
//! │   ├── installed.toml     # List of installed packages
//! │   └── packages/          # Package metadata cache
//! │       └── hello-1.0.0/
//! │           └── package.toml
//! ├── cache/                 # Downloaded package cache
//! │   └── hello-1.0.0.axepkg
//! └── registry/              # Registry index cache
//!     └── index.toml
//!
//! /bin/                      # Installed WASM binaries
//! └── hello.wasm
//! ```

mod checksum;
mod database;
mod error;
mod installer;
mod manifest;
mod registry;
mod resolver;
mod version;

pub use checksum::{Checksum, verify_checksum};
pub use database::{InstalledPackage, PackageDatabase};
pub use error::{PkgError, PkgResult};
pub use installer::PackageInstaller;
pub use manifest::{BinaryEntry, Dependency, PackageManifest};
pub use registry::{PackageRegistry, RegistryEntry};
pub use resolver::{DependencyResolver, ResolvedPackage};
pub use version::{Version, VersionReq};

use std::collections::HashMap;

/// Package manager paths
pub mod paths {
    /// Base directory for package manager data
    pub const PKG_BASE: &str = "/var/lib/pkg";
    /// Package database directory
    pub const PKG_DB: &str = "/var/lib/pkg/db";
    /// Installed packages list
    pub const PKG_INSTALLED: &str = "/var/lib/pkg/db/installed.toml";
    /// Package metadata cache
    pub const PKG_PACKAGES: &str = "/var/lib/pkg/db/packages";
    /// Downloaded package cache
    pub const PKG_CACHE: &str = "/var/lib/pkg/cache";
    /// Registry index cache
    pub const PKG_REGISTRY: &str = "/var/lib/pkg/registry";
    /// Default binary installation directory
    pub const BIN_DIR: &str = "/bin";
}

/// Package identifier (name + version)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageId {
    pub name: String,
    pub version: Version,
}

impl PackageId {
    pub fn new(name: impl Into<String>, version: Version) -> Self {
        Self {
            name: name.into(),
            version,
        }
    }

    /// Parse from string "name-version" format
    pub fn parse(s: &str) -> PkgResult<Self> {
        // Find the last hyphen that separates name from version
        if let Some(pos) = s.rfind('-') {
            let name = &s[..pos];
            let version_str = &s[pos + 1..];
            let version = Version::parse(version_str)?;
            Ok(Self::new(name, version))
        } else {
            Err(PkgError::InvalidPackageId(s.to_string()))
        }
    }

    /// Get the directory name for this package
    pub fn dir_name(&self) -> String {
        format!("{}-{}", self.name, self.version)
    }
}

impl std::fmt::Display for PackageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}", self.name, self.version)
    }
}

/// Main package manager interface
pub struct PackageManager {
    /// Local package database
    pub database: PackageDatabase,
    /// Package registry client
    pub registry: PackageRegistry,
    /// Package installer
    pub installer: PackageInstaller,
    /// Dependency resolver
    pub resolver: DependencyResolver,
}

impl PackageManager {
    /// Create a new package manager
    pub fn new() -> Self {
        Self {
            database: PackageDatabase::new(),
            registry: PackageRegistry::new(),
            installer: PackageInstaller::new(),
            resolver: DependencyResolver::new(),
        }
    }

    /// Initialize package manager directories
    pub fn init(&self) -> PkgResult<()> {
        self.database.init()
    }

    /// Install a package by name
    ///
    /// If version is None, installs the latest version.
    #[cfg(target_arch = "wasm32")]
    pub async fn install(&mut self, name: &str, version: Option<&str>) -> PkgResult<PackageId> {
        // Parse version requirement
        let version_req = match version {
            Some(v) => VersionReq::parse(v)?,
            None => VersionReq::any(),
        };

        // Fetch package info from registry
        let entry = self.registry.fetch_package(name).await?;

        // Find best matching version
        let best_version = entry
            .versions
            .iter()
            .filter(|v| version_req.matches(v))
            .max()
            .ok_or_else(|| PkgError::NoMatchingVersion {
                name: name.to_string(),
                requirement: version_req.to_string(),
            })?
            .clone();

        let pkg_id = PackageId::new(name, best_version);

        // Check if already installed
        if self
            .database
            .is_installed(&pkg_id.name, Some(&pkg_id.version))?
        {
            return Err(PkgError::AlreadyInstalled(pkg_id.clone()));
        }

        // Resolve dependencies
        let resolved = self.resolver.resolve(&pkg_id, &self.registry).await?;

        // Install all resolved packages
        for pkg in resolved {
            if !self
                .database
                .is_installed(&pkg.id.name, Some(&pkg.id.version))?
            {
                self.installer.install(&pkg, &self.registry).await?;
                self.database.record_installed(&pkg.id, &pkg.manifest)?;
            }
        }

        Ok(pkg_id)
    }

    /// Install a package (non-WASM stub)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn install(&mut self, name: &str, version: Option<&str>) -> PkgResult<PackageId> {
        let _ = (name, version);
        Err(PkgError::NotAvailable("WASM required".to_string()))
    }

    /// Install a package from local file
    pub fn install_local(&mut self, path: &str) -> PkgResult<PackageId> {
        self.installer.install_local(path, &mut self.database)
    }

    /// Remove an installed package
    pub fn remove(&mut self, name: &str) -> PkgResult<()> {
        // Find installed package
        let installed = self
            .database
            .get_installed(name)?
            .ok_or_else(|| PkgError::NotInstalled(name.to_string()))?;

        // Check for dependents
        let dependents = self.database.get_dependents(name)?;
        if !dependents.is_empty() {
            return Err(PkgError::HasDependents {
                package: name.to_string(),
                dependents,
            });
        }

        // Remove binary files
        self.installer.remove(&installed)?;

        // Remove from database
        self.database.remove_installed(name)?;

        Ok(())
    }

    /// List installed packages
    pub fn list_installed(&self) -> PkgResult<Vec<InstalledPackage>> {
        self.database.list_installed()
    }

    /// Search for packages in the registry
    #[cfg(target_arch = "wasm32")]
    pub async fn search(&self, query: &str) -> PkgResult<Vec<RegistryEntry>> {
        self.registry.search(query).await
    }

    /// Search packages (non-WASM stub)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn search(&self, _query: &str) -> PkgResult<Vec<RegistryEntry>> {
        Err(PkgError::NotAvailable("WASM required".to_string()))
    }

    /// Get information about a package
    #[cfg(target_arch = "wasm32")]
    pub async fn info(&self, name: &str) -> PkgResult<RegistryEntry> {
        self.registry.fetch_package(name).await
    }

    /// Get package info (non-WASM stub)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn info(&self, _name: &str) -> PkgResult<RegistryEntry> {
        Err(PkgError::NotAvailable("WASM required".to_string()))
    }

    /// Update the registry index
    #[cfg(target_arch = "wasm32")]
    pub async fn update_index(&mut self) -> PkgResult<()> {
        self.registry.update_index().await
    }

    /// Update registry (non-WASM stub)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn update_index(&mut self) -> PkgResult<()> {
        Err(PkgError::NotAvailable("WASM required".to_string()))
    }

    /// Upgrade all installed packages to latest versions
    #[cfg(target_arch = "wasm32")]
    pub async fn upgrade_all(&mut self) -> PkgResult<Vec<PackageId>> {
        let installed = self.list_installed()?;
        let mut upgraded = Vec::new();

        for pkg in installed {
            // Check for newer version
            if let Ok(entry) = self.registry.fetch_package(&pkg.name).await {
                if let Some(latest) = entry.versions.iter().max() {
                    if latest > &pkg.version {
                        // Remove old version
                        self.remove(&pkg.name)?;
                        // Install new version
                        let new_id = self.install(&pkg.name, None).await?;
                        upgraded.push(new_id);
                    }
                }
            }
        }

        Ok(upgraded)
    }

    /// Upgrade all (non-WASM stub)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn upgrade_all(&mut self) -> PkgResult<Vec<PackageId>> {
        Err(PkgError::NotAvailable("WASM required".to_string()))
    }

    /// Verify integrity of installed packages
    pub fn verify(&self) -> PkgResult<HashMap<String, bool>> {
        let installed = self.list_installed()?;
        let mut results = HashMap::new();

        for pkg in installed {
            let valid = self.installer.verify(&pkg)?;
            results.insert(pkg.name.clone(), valid);
        }

        Ok(results)
    }

    /// Clean package cache
    pub fn clean_cache(&self) -> PkgResult<()> {
        self.installer.clean_cache()
    }
}

impl Default for PackageManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_id_new() {
        let version = Version::new(1, 2, 3);
        let id = PackageId::new("hello", version.clone());
        assert_eq!(id.name, "hello");
        assert_eq!(id.version, version);
    }

    #[test]
    fn test_package_id_parse() {
        let id = PackageId::parse("hello-1.2.3").unwrap();
        assert_eq!(id.name, "hello");
        assert_eq!(id.version, Version::new(1, 2, 3));
    }

    #[test]
    fn test_package_id_parse_with_hyphen_in_name() {
        let id = PackageId::parse("my-package-2.0.0").unwrap();
        assert_eq!(id.name, "my-package");
        assert_eq!(id.version, Version::new(2, 0, 0));
    }

    #[test]
    fn test_package_id_dir_name() {
        let id = PackageId::new("hello", Version::new(1, 0, 0));
        assert_eq!(id.dir_name(), "hello-1.0.0");
    }

    #[test]
    fn test_package_id_display() {
        let id = PackageId::new("hello", Version::new(1, 2, 3));
        assert_eq!(format!("{}", id), "hello-1.2.3");
    }
}
