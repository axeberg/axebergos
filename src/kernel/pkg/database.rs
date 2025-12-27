//! Local package database
//!
//! Tracks installed packages and their metadata.

use super::checksum::Checksum;
use super::error::{PkgError, PkgResult};
use super::manifest::PackageManifest;
use super::paths;
use super::version::Version;
use super::PackageId;
use crate::kernel::syscall;
use std::collections::HashMap;

/// An installed package record
#[derive(Debug, Clone)]
pub struct InstalledPackage {
    /// Package name
    pub name: String,
    /// Installed version
    pub version: Version,
    /// Installation timestamp (Unix epoch)
    pub installed_at: u64,
    /// List of installed binary paths
    pub binaries: Vec<String>,
    /// Dependencies that were installed with this package
    pub dependencies: Vec<String>,
    /// Checksum of the package manifest
    pub manifest_checksum: Option<Checksum>,
}

impl InstalledPackage {
    /// Create from a manifest after installation
    pub fn from_manifest(manifest: &PackageManifest, binaries: Vec<String>) -> Self {
        Self {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            installed_at: current_timestamp(),
            binaries,
            dependencies: manifest.dependencies.iter().map(|d| d.name.clone()).collect(),
            manifest_checksum: Some(Checksum::compute(manifest.to_toml().as_bytes())),
        }
    }

    /// Get the package ID
    pub fn id(&self) -> PackageId {
        PackageId::new(&self.name, self.version.clone())
    }
}

/// Local package database
pub struct PackageDatabase {
    /// Cached list of installed packages
    cache: Option<HashMap<String, InstalledPackage>>,
}

impl PackageDatabase {
    /// Create a new package database
    pub fn new() -> Self {
        Self { cache: None }
    }

    /// Initialize database directories
    pub fn init(&self) -> PkgResult<()> {
        // Create directory structure
        let dirs = [
            paths::PKG_BASE,
            paths::PKG_DB,
            paths::PKG_PACKAGES,
            paths::PKG_CACHE,
            paths::PKG_REGISTRY,
        ];

        for dir in dirs {
            if !path_exists(dir) {
                mkdir_recursive(dir)?;
            }
        }

        // Create empty installed.toml if it doesn't exist
        if !path_exists(paths::PKG_INSTALLED) {
            write_file(paths::PKG_INSTALLED, "# Installed packages\n")?;
        }

        Ok(())
    }

    /// Load installed packages from disk
    fn load(&mut self) -> PkgResult<()> {
        if self.cache.is_some() {
            return Ok(());
        }

        let mut packages = HashMap::new();

        // Read installed.toml
        let content = match read_file(paths::PKG_INSTALLED) {
            Ok(c) => c,
            Err(_) => {
                self.cache = Some(packages);
                return Ok(());
            }
        };

        // Parse installed packages
        let mut current_pkg: Option<InstalledPackage> = None;

        for line in content.lines() {
            let line = line.trim();

            // Skip comments and empty lines
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Section header [[packages.name]]
            if line.starts_with("[[packages.") && line.ends_with("]]") {
                // Save previous package
                if let Some(pkg) = current_pkg.take() {
                    packages.insert(pkg.name.clone(), pkg);
                }

                let name = line
                    .trim_start_matches("[[packages.")
                    .trim_end_matches("]]");
                current_pkg = Some(InstalledPackage {
                    name: name.to_string(),
                    version: Version::new(0, 0, 0),
                    installed_at: 0,
                    binaries: Vec::new(),
                    dependencies: Vec::new(),
                    manifest_checksum: None,
                });
            } else if let Some(ref mut pkg) = current_pkg {
                // Parse key-value
                if let Some(pos) = line.find('=') {
                    let key = line[..pos].trim();
                    let value = line[pos + 1..].trim().trim_matches('"');

                    match key {
                        "version" => {
                            if let Ok(v) = Version::parse(value) {
                                pkg.version = v;
                            }
                        }
                        "installed_at" => {
                            if let Ok(t) = value.parse() {
                                pkg.installed_at = t;
                            }
                        }
                        "binaries" => {
                            pkg.binaries = parse_array(value);
                        }
                        "dependencies" => {
                            pkg.dependencies = parse_array(value);
                        }
                        "manifest_checksum" => {
                            if let Ok(c) = Checksum::from_hex(value) {
                                pkg.manifest_checksum = Some(c);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Save last package
        if let Some(pkg) = current_pkg {
            packages.insert(pkg.name.clone(), pkg);
        }

        self.cache = Some(packages);
        Ok(())
    }

    /// Save database to disk
    fn save(&self) -> PkgResult<()> {
        let packages = self.cache.as_ref().ok_or_else(|| {
            PkgError::IoError("database not loaded".to_string())
        })?;

        let mut content = String::new();
        content.push_str("# Installed packages\n");
        content.push_str("# This file is auto-generated. Do not edit manually.\n\n");

        for pkg in packages.values() {
            content.push_str(&format!("[[packages.{}]]\n", pkg.name));
            content.push_str(&format!("version = \"{}\"\n", pkg.version));
            content.push_str(&format!("installed_at = {}\n", pkg.installed_at));

            if !pkg.binaries.is_empty() {
                let bins: Vec<String> = pkg.binaries.iter().map(|b| format!("\"{}\"", b)).collect();
                content.push_str(&format!("binaries = [{}]\n", bins.join(", ")));
            }

            if !pkg.dependencies.is_empty() {
                let deps: Vec<String> =
                    pkg.dependencies.iter().map(|d| format!("\"{}\"", d)).collect();
                content.push_str(&format!("dependencies = [{}]\n", deps.join(", ")));
            }

            if let Some(ref checksum) = pkg.manifest_checksum {
                content.push_str(&format!("manifest_checksum = \"{}\"\n", checksum));
            }

            content.push('\n');
        }

        write_file(paths::PKG_INSTALLED, &content)?;
        Ok(())
    }

    /// Check if a package is installed
    pub fn is_installed(&mut self, name: &str, version: Option<&Version>) -> PkgResult<bool> {
        self.load()?;
        let packages = self.cache.as_ref().unwrap();

        match packages.get(name) {
            Some(pkg) => {
                if let Some(v) = version {
                    Ok(&pkg.version == v)
                } else {
                    Ok(true)
                }
            }
            None => Ok(false),
        }
    }

    /// Get an installed package by name
    pub fn get_installed(&mut self, name: &str) -> PkgResult<Option<InstalledPackage>> {
        self.load()?;
        let packages = self.cache.as_ref().unwrap();
        Ok(packages.get(name).cloned())
    }

    /// List all installed packages
    pub fn list_installed(&self) -> PkgResult<Vec<InstalledPackage>> {
        // Need to load if not cached
        let content = match read_file(paths::PKG_INSTALLED) {
            Ok(c) => c,
            Err(_) => return Ok(Vec::new()),
        };

        // Quick parse for listing
        let mut packages = Vec::new();
        let mut current_pkg: Option<InstalledPackage> = None;

        for line in content.lines() {
            let line = line.trim();

            if line.starts_with("[[packages.") && line.ends_with("]]") {
                if let Some(pkg) = current_pkg.take() {
                    packages.push(pkg);
                }

                let name = line
                    .trim_start_matches("[[packages.")
                    .trim_end_matches("]]");
                current_pkg = Some(InstalledPackage {
                    name: name.to_string(),
                    version: Version::new(0, 0, 0),
                    installed_at: 0,
                    binaries: Vec::new(),
                    dependencies: Vec::new(),
                    manifest_checksum: None,
                });
            } else if let Some(ref mut pkg) = current_pkg
                && let Some(pos) = line.find('=') {
                    let key = line[..pos].trim();
                    let value = line[pos + 1..].trim().trim_matches('"');

                    match key {
                        "version" => {
                            if let Ok(v) = Version::parse(value) {
                                pkg.version = v;
                            }
                        }
                        "installed_at" => {
                            if let Ok(t) = value.parse() {
                                pkg.installed_at = t;
                            }
                        }
                        "binaries" => {
                            pkg.binaries = parse_array(value);
                        }
                        "dependencies" => {
                            pkg.dependencies = parse_array(value);
                        }
                        _ => {}
                    }
                }
        }

        if let Some(pkg) = current_pkg {
            packages.push(pkg);
        }

        Ok(packages)
    }

    /// Record a newly installed package
    pub fn record_installed(&mut self, id: &PackageId, manifest: &PackageManifest) -> PkgResult<()> {
        self.load()?;

        let binaries: Vec<String> = manifest
            .binaries
            .iter()
            .map(|b| format!("{}/{}.wasm", paths::BIN_DIR, b.name))
            .collect();

        let pkg = InstalledPackage::from_manifest(manifest, binaries);

        if let Some(ref mut packages) = self.cache {
            packages.insert(id.name.clone(), pkg);
        }

        self.save()?;

        // Also save manifest to packages directory
        let manifest_dir = format!("{}/{}", paths::PKG_PACKAGES, id.dir_name());
        mkdir_recursive(&manifest_dir)?;
        write_file(&format!("{}/package.toml", manifest_dir), &manifest.to_toml())?;

        Ok(())
    }

    /// Remove an installed package from the database
    pub fn remove_installed(&mut self, name: &str) -> PkgResult<()> {
        self.load()?;

        if let Some(ref mut packages) = self.cache {
            packages.remove(name);
        }

        self.save()?;
        Ok(())
    }

    /// Get packages that depend on a given package
    pub fn get_dependents(&mut self, name: &str) -> PkgResult<Vec<String>> {
        self.load()?;
        let packages = self.cache.as_ref().unwrap();

        let dependents: Vec<String> = packages
            .values()
            .filter(|pkg| pkg.dependencies.contains(&name.to_string()))
            .map(|pkg| pkg.name.clone())
            .collect();

        Ok(dependents)
    }

    /// Get the manifest for an installed package
    pub fn get_manifest(&self, name: &str) -> PkgResult<Option<PackageManifest>> {
        // Find the package version first
        let installed = match self.list_installed()?.into_iter().find(|p| p.name == name) {
            Some(p) => p,
            None => return Ok(None),
        };

        let id = installed.id();
        let manifest_path = format!("{}/{}/package.toml", paths::PKG_PACKAGES, id.dir_name());

        match read_file(&manifest_path) {
            Ok(content) => Ok(Some(PackageManifest::parse(&content)?)),
            Err(_) => Ok(None),
        }
    }

    /// Clear the cache (force reload on next access)
    pub fn clear_cache(&mut self) {
        self.cache = None;
    }
}

impl Default for PackageDatabase {
    fn default() -> Self {
        Self::new()
    }
}

// Helper functions for filesystem operations

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
            syscall::mkdir(&current).map_err(|e| PkgError::IoError(format!("{}: {}", current, e)))?;
        }
    }

    Ok(())
}

fn read_file(path: &str) -> PkgResult<String> {
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
    String::from_utf8(content).map_err(|_| PkgError::IoError(format!("{}: invalid UTF-8", path)))
}

fn write_file(path: &str, content: &str) -> PkgResult<()> {
    let fd = syscall::open(path, syscall::OpenFlags::WRITE)
        .map_err(|e| PkgError::IoError(format!("{}: {}", path, e)))?;

    syscall::write(fd, content.as_bytes())
        .map_err(|e| PkgError::IoError(format!("{}: {}", path, e)))?;

    syscall::close(fd).map_err(|e| PkgError::IoError(format!("{}: {}", path, e)))?;

    Ok(())
}

fn parse_array(s: &str) -> Vec<String> {
    let s = s.trim();
    if !s.starts_with('[') || !s.ends_with(']') {
        return vec![];
    }

    let inner = &s[1..s.len() - 1];
    let mut values = Vec::new();
    let mut current = String::new();
    let mut in_string = false;

    for c in inner.chars() {
        match c {
            '"' => {
                if in_string {
                    values.push(current.clone());
                    current.clear();
                }
                in_string = !in_string;
            }
            ',' if !in_string => {}
            _ if in_string => current.push(c),
            _ => {}
        }
    }

    values
}

fn current_timestamp() -> u64 {
    // In WASM, we can use Date.now() via js_sys
    #[cfg(target_arch = "wasm32")]
    {
        (js_sys::Date::now() / 1000.0) as u64
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_array() {
        let arr = parse_array("[\"a\", \"b\", \"c\"]");
        assert_eq!(arr, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_empty_array() {
        let arr = parse_array("[]");
        assert!(arr.is_empty());
    }

    #[test]
    fn test_installed_package_from_manifest() {
        let manifest = PackageManifest {
            name: "test".to_string(),
            version: Version::new(1, 0, 0),
            description: None,
            authors: vec![],
            license: None,
            repository: None,
            homepage: None,
            keywords: vec![],
            binaries: vec![super::super::manifest::BinaryEntry {
                name: "test".to_string(),
                path: "bin/test.wasm".to_string(),
                checksum: None,
            }],
            dependencies: vec![],
            dev_dependencies: vec![],
        };

        let installed = InstalledPackage::from_manifest(&manifest, vec!["/bin/test.wasm".to_string()]);
        assert_eq!(installed.name, "test");
        assert_eq!(installed.version, Version::new(1, 0, 0));
        assert_eq!(installed.binaries, vec!["/bin/test.wasm"]);
    }

    #[test]
    fn test_package_database_new() {
        let db = PackageDatabase::new();
        assert!(db.cache.is_none());
    }
}
