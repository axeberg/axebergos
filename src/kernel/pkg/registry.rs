//! Package registry client
//!
//! Handles fetching package information and downloading packages from
//! remote registries.
//!
//! # Registry Protocol
//!
//! The registry uses a simple HTTP-based protocol:
//!
//! ```text
//! GET /index.json              - Full registry index
//! GET /packages/{name}.json    - Package metadata
//! GET /packages/{name}/{version}.axepkg - Package archive
//! ```
//!
//! # Index Format (index.json)
//!
//! ```json
//! {
//!   "packages": {
//!     "hello": {
//!       "versions": ["1.0.0", "1.1.0", "2.0.0"],
//!       "latest": "2.0.0"
//!     }
//!   }
//! }
//! ```

use super::error::{PkgError, PkgResult};
use super::version::Version;
use std::collections::HashMap;

/// Default registry URL
pub const DEFAULT_REGISTRY: &str = "https://pkg.axeberg.dev";

/// A package entry in the registry
#[derive(Debug, Clone)]
pub struct RegistryEntry {
    /// Package name
    pub name: String,
    /// Available versions (sorted)
    pub versions: Vec<Version>,
    /// Latest version
    pub latest: Version,
    /// Short description
    pub description: Option<String>,
    /// Keywords for search
    pub keywords: Vec<String>,
    /// Download URL template
    pub download_url: Option<String>,
}

impl RegistryEntry {
    /// Get the download URL for a specific version
    pub fn download_url(&self, version: &Version, registry_url: &str) -> String {
        if let Some(ref template) = self.download_url {
            template
                .replace("{name}", &self.name)
                .replace("{version}", &version.to_string())
        } else {
            format!(
                "{}/packages/{}/{}.axepkg",
                registry_url, self.name, version
            )
        }
    }
}

/// Package registry client
pub struct PackageRegistry {
    /// Registry base URL
    registry_url: String,
    /// Per-package cache
    package_cache: HashMap<String, RegistryEntry>,
}

impl PackageRegistry {
    /// Create a new registry client with default URL
    pub fn new() -> Self {
        Self {
            registry_url: DEFAULT_REGISTRY.to_string(),
            package_cache: HashMap::new(),
        }
    }

    /// Create with a custom registry URL
    pub fn with_url(url: &str) -> Self {
        Self {
            registry_url: url.to_string(),
            package_cache: HashMap::new(),
        }
    }

    /// Get the registry URL
    pub fn url(&self) -> &str {
        &self.registry_url
    }

    /// Set the registry URL
    pub fn set_url(&mut self, url: &str) {
        self.registry_url = url.to_string();
        self.clear_cache();
    }

    /// Clear all caches
    pub fn clear_cache(&mut self) {
        self.package_cache.clear();
    }

    /// Update the registry index
    #[cfg(target_arch = "wasm32")]
    pub async fn update_index(&mut self) -> PkgResult<()> {
        use crate::kernel::network::HttpRequest;

        let url = format!("{}/index.json", self.registry_url);

        let response = HttpRequest::get(&url)
            .send()
            .await
            .map_err(|e| PkgError::NetworkError(e))?;

        if response.status != 200 {
            return Err(PkgError::RegistryError(format!(
                "HTTP {}: {}",
                response.status, response.status_text
            )));
        }

        let body = response
            .text()
            .map_err(|_| PkgError::RegistryError("invalid UTF-8 response".to_string()))?;

        let index = self.parse_index(&body)?;

        // Cache to disk
        let cache_path = format!("{}/index.json", paths::PKG_REGISTRY);
        let _ = write_file(&cache_path, &body);

        self.index_cache = Some(index);
        Ok(())
    }

    /// Update index (non-WASM stub)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn update_index(&mut self) -> PkgResult<()> {
        Err(PkgError::NotAvailable("WASM required".to_string()))
    }

    /// Fetch package information
    #[cfg(target_arch = "wasm32")]
    pub async fn fetch_package(&self, name: &str) -> PkgResult<RegistryEntry> {
        // Check cache first
        if let Some(entry) = self.package_cache.get(name) {
            return Ok(entry.clone());
        }

        use crate::kernel::network::HttpRequest;

        let url = format!("{}/packages/{}.json", self.registry_url, name);

        let response = HttpRequest::get(&url)
            .send()
            .await
            .map_err(|e| PkgError::NetworkError(e))?;

        if response.status == 404 {
            return Err(PkgError::PackageNotFound(name.to_string()));
        }

        if response.status != 200 {
            return Err(PkgError::RegistryError(format!(
                "HTTP {}: {}",
                response.status, response.status_text
            )));
        }

        let body = response
            .text()
            .map_err(|_| PkgError::RegistryError("invalid UTF-8 response".to_string()))?;

        self.parse_package_entry(name, &body)
    }

    /// Fetch package (non-WASM stub)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn fetch_package(&self, name: &str) -> PkgResult<RegistryEntry> {
        let _ = name;
        Err(PkgError::NotAvailable("WASM required".to_string()))
    }

    /// Download a package archive
    #[cfg(target_arch = "wasm32")]
    pub async fn download_package(
        &self,
        name: &str,
        version: &Version,
    ) -> PkgResult<Vec<u8>> {
        use crate::kernel::network::HttpRequest;

        let url = format!(
            "{}/packages/{}/{}.axepkg",
            self.registry_url, name, version
        );

        let response = HttpRequest::get(&url)
            .send()
            .await
            .map_err(|e| PkgError::NetworkError(e))?;

        if response.status == 404 {
            return Err(PkgError::PackageNotFound(format!("{}-{}", name, version)));
        }

        if response.status != 200 {
            return Err(PkgError::RegistryError(format!(
                "HTTP {}: {}",
                response.status, response.status_text
            )));
        }

        Ok(response.body)
    }

    /// Download package (non-WASM stub)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn download_package(
        &self,
        name: &str,
        version: &Version,
    ) -> PkgResult<Vec<u8>> {
        let _ = (name, version);
        Err(PkgError::NotAvailable("WASM required".to_string()))
    }

    /// Search packages by query
    #[cfg(target_arch = "wasm32")]
    pub async fn search(&self, query: &str) -> PkgResult<Vec<RegistryEntry>> {
        use crate::kernel::network::HttpRequest;

        let url = format!(
            "{}/search?q={}",
            self.registry_url,
            urlencoding::encode(query)
        );

        let response = HttpRequest::get(&url)
            .send()
            .await
            .map_err(|e| PkgError::NetworkError(e))?;

        if response.status != 200 {
            return Err(PkgError::RegistryError(format!(
                "HTTP {}: {}",
                response.status, response.status_text
            )));
        }

        let body = response
            .text()
            .map_err(|_| PkgError::RegistryError("invalid UTF-8 response".to_string()))?;

        self.parse_search_results(&body)
    }

    /// Search packages (non-WASM stub)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn search(&self, _query: &str) -> PkgResult<Vec<RegistryEntry>> {
        Err(PkgError::NotAvailable("WASM required".to_string()))
    }
}

impl Default for PackageRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_new() {
        let reg = PackageRegistry::new();
        assert_eq!(reg.url(), DEFAULT_REGISTRY);
    }

    #[test]
    fn test_registry_with_url() {
        let reg = PackageRegistry::with_url("https://custom.registry.com");
        assert_eq!(reg.url(), "https://custom.registry.com");
    }

    #[test]
    fn test_registry_entry_download_url() {
        let entry = RegistryEntry {
            name: "hello".to_string(),
            versions: vec![Version::new(1, 0, 0)],
            latest: Version::new(1, 0, 0),
            description: None,
            keywords: vec![],
            download_url: None,
        };

        let url = entry.download_url(&Version::new(1, 0, 0), DEFAULT_REGISTRY);
        assert_eq!(url, format!("{}/packages/hello/1.0.0.axepkg", DEFAULT_REGISTRY));
    }

    #[test]
    fn test_registry_entry_custom_download_url() {
        let entry = RegistryEntry {
            name: "hello".to_string(),
            versions: vec![Version::new(1, 0, 0)],
            latest: Version::new(1, 0, 0),
            description: None,
            keywords: vec![],
            download_url: Some("https://cdn.example.com/{name}/{version}.tar.gz".to_string()),
        };

        let url = entry.download_url(&Version::new(1, 0, 0), DEFAULT_REGISTRY);
        assert_eq!(url, "https://cdn.example.com/hello/1.0.0.tar.gz");
    }

}
