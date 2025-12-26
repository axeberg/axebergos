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

use super::checksum::Checksum;
use super::error::{PkgError, PkgResult};
use super::paths;
use super::version::Version;
use crate::kernel::syscall;
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
    /// Cached index
    index_cache: Option<RegistryIndex>,
    /// Per-package cache
    package_cache: HashMap<String, RegistryEntry>,
}

/// Registry index (list of all packages)
#[derive(Debug, Clone)]
struct RegistryIndex {
    packages: HashMap<String, IndexEntry>,
}

#[derive(Debug, Clone)]
struct IndexEntry {
    versions: Vec<Version>,
    latest: Version,
}

impl PackageRegistry {
    /// Create a new registry client with default URL
    pub fn new() -> Self {
        Self {
            registry_url: DEFAULT_REGISTRY.to_string(),
            index_cache: None,
            package_cache: HashMap::new(),
        }
    }

    /// Create with a custom registry URL
    pub fn with_url(url: &str) -> Self {
        Self {
            registry_url: url.to_string(),
            index_cache: None,
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
        self.index_cache = None;
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

    /// Parse the registry index JSON
    fn parse_index(&self, json: &str) -> PkgResult<RegistryIndex> {
        // Simple JSON parser for index format
        let mut packages = HashMap::new();

        // Find "packages" object
        if let Some(start) = json.find("\"packages\"") {
            let rest = &json[start..];
            if let Some(obj_start) = rest.find('{') {
                let obj_rest = &rest[obj_start + 1..];

                // Parse each package entry
                let mut depth = 1;
                let mut current_name = String::new();
                let mut in_object = false;
                let mut object_content = String::new();

                for (i, c) in obj_rest.chars().enumerate() {
                    match c {
                        '{' => {
                            depth += 1;
                            if depth == 2 {
                                in_object = true;
                                object_content.clear();
                            } else if in_object {
                                object_content.push(c);
                            }
                        }
                        '}' => {
                            depth -= 1;
                            if depth == 1 && in_object {
                                // Parse the package entry
                                if !current_name.is_empty() {
                                    if let Some(entry) =
                                        self.parse_index_entry(&object_content)
                                    {
                                        packages.insert(current_name.clone(), entry);
                                    }
                                }
                                in_object = false;
                            } else if in_object {
                                object_content.push(c);
                            }
                            if depth == 0 {
                                break;
                            }
                        }
                        '"' if !in_object && depth == 1 => {
                            // Start of package name
                            let name_end = obj_rest[i + 1..].find('"').unwrap_or(0);
                            current_name = obj_rest[i + 1..i + 1 + name_end].to_string();
                        }
                        _ if in_object => {
                            object_content.push(c);
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(RegistryIndex { packages })
    }

    fn parse_index_entry(&self, content: &str) -> Option<IndexEntry> {
        let mut versions = Vec::new();
        let mut latest = None;

        // Parse "versions" array
        if let Some(ver_start) = content.find("\"versions\"") {
            let ver_rest = &content[ver_start..];
            if let Some(arr_start) = ver_rest.find('[') {
                if let Some(arr_end) = ver_rest[arr_start..].find(']') {
                    let arr_content = &ver_rest[arr_start + 1..arr_start + arr_end];
                    for part in arr_content.split(',') {
                        let v = part.trim().trim_matches('"');
                        if let Ok(ver) = Version::parse(v) {
                            versions.push(ver);
                        }
                    }
                }
            }
        }

        // Parse "latest"
        if let Some(lat_start) = content.find("\"latest\"") {
            let lat_rest = &content[lat_start..];
            if let Some(colon) = lat_rest.find(':') {
                let val_rest = lat_rest[colon + 1..].trim();
                if val_rest.starts_with('"') {
                    if let Some(end) = val_rest[1..].find('"') {
                        if let Ok(ver) = Version::parse(&val_rest[1..end + 1]) {
                            latest = Some(ver);
                        }
                    }
                }
            }
        }

        let latest = latest.or_else(|| versions.iter().max().cloned())?;

        Some(IndexEntry { versions, latest })
    }

    fn parse_package_entry(&self, name: &str, json: &str) -> PkgResult<RegistryEntry> {
        let mut versions = Vec::new();
        let mut latest = None;
        let mut description = None;
        let mut keywords = Vec::new();
        let mut download_url = None;

        // Parse versions
        if let Some(ver_start) = json.find("\"versions\"") {
            let ver_rest = &json[ver_start..];
            if let Some(arr_start) = ver_rest.find('[') {
                if let Some(arr_end) = ver_rest[arr_start..].find(']') {
                    let arr_content = &ver_rest[arr_start + 1..arr_start + arr_end];
                    for part in arr_content.split(',') {
                        let v = part.trim().trim_matches('"');
                        if let Ok(ver) = Version::parse(v) {
                            versions.push(ver);
                        }
                    }
                }
            }
        }

        // Parse latest
        if let Some(lat_start) = json.find("\"latest\"") {
            let lat_rest = &json[lat_start..];
            if let Some(colon) = lat_rest.find(':') {
                let val = extract_string_value(&lat_rest[colon + 1..]);
                if let Some(v) = val {
                    if let Ok(ver) = Version::parse(&v) {
                        latest = Some(ver);
                    }
                }
            }
        }

        // Parse description
        if let Some(desc_start) = json.find("\"description\"") {
            let desc_rest = &json[desc_start..];
            if let Some(colon) = desc_rest.find(':') {
                description = extract_string_value(&desc_rest[colon + 1..]);
            }
        }

        // Parse keywords
        if let Some(kw_start) = json.find("\"keywords\"") {
            let kw_rest = &json[kw_start..];
            if let Some(arr_start) = kw_rest.find('[') {
                if let Some(arr_end) = kw_rest[arr_start..].find(']') {
                    let arr_content = &kw_rest[arr_start + 1..arr_start + arr_end];
                    for part in arr_content.split(',') {
                        let kw = part.trim().trim_matches('"').to_string();
                        if !kw.is_empty() {
                            keywords.push(kw);
                        }
                    }
                }
            }
        }

        // Parse download_url
        if let Some(url_start) = json.find("\"download_url\"") {
            let url_rest = &json[url_start..];
            if let Some(colon) = url_rest.find(':') {
                download_url = extract_string_value(&url_rest[colon + 1..]);
            }
        }

        versions.sort();
        let latest = latest
            .or_else(|| versions.iter().max().cloned())
            .ok_or_else(|| PkgError::RegistryError("no versions available".to_string()))?;

        Ok(RegistryEntry {
            name: name.to_string(),
            versions,
            latest,
            description,
            keywords,
            download_url,
        })
    }

    fn parse_search_results(&self, json: &str) -> PkgResult<Vec<RegistryEntry>> {
        let mut results = Vec::new();

        // Find "results" array
        if let Some(arr_start) = json.find("\"results\"") {
            let arr_rest = &json[arr_start..];
            if let Some(bracket_start) = arr_rest.find('[') {
                // Parse each result object
                let mut depth = 0;
                let mut obj_start = None;

                for (i, c) in arr_rest[bracket_start..].chars().enumerate() {
                    match c {
                        '[' if depth == 0 => depth = 1,
                        '{' => {
                            if depth == 1 {
                                obj_start = Some(i);
                            }
                            depth += 1;
                        }
                        '}' => {
                            depth -= 1;
                            if depth == 1 {
                                if let Some(start) = obj_start {
                                    let obj_content =
                                        &arr_rest[bracket_start + start..bracket_start + i + 1];
                                    if let Some(name) = extract_name(obj_content) {
                                        if let Ok(entry) = self.parse_package_entry(&name, obj_content)
                                        {
                                            results.push(entry);
                                        }
                                    }
                                }
                                obj_start = None;
                            }
                        }
                        ']' if depth == 1 => break,
                        _ => {}
                    }
                }
            }
        }

        Ok(results)
    }
}

impl Default for PackageRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// Helper functions

fn extract_string_value(s: &str) -> Option<String> {
    let s = s.trim();
    if s.starts_with('"') {
        let end = s[1..].find('"')?;
        Some(s[1..end + 1].to_string())
    } else {
        None
    }
}

fn extract_name(json: &str) -> Option<String> {
    if let Some(name_start) = json.find("\"name\"") {
        let rest = &json[name_start..];
        if let Some(colon) = rest.find(':') {
            return extract_string_value(&rest[colon + 1..]);
        }
    }
    None
}

fn write_file(path: &str, content: &str) -> PkgResult<()> {
    // Ensure parent directory exists
    if let Some(pos) = path.rfind('/') {
        let parent = &path[..pos];
        if !syscall::exists(parent).unwrap_or(false) {
            let _ = syscall::mkdir(parent);
        }
    }

    let fd = syscall::open(path, syscall::OpenFlags::WRITE)
        .map_err(|e| PkgError::IoError(format!("{}: {}", path, e)))?;

    syscall::write(fd, content.as_bytes())
        .map_err(|e| PkgError::IoError(format!("{}: {}", path, e)))?;

    syscall::close(fd).map_err(|e| PkgError::IoError(format!("{}: {}", path, e)))?;

    Ok(())
}

// Simple URL encoding for search queries
mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::new();
        for c in s.chars() {
            match c {
                'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                    result.push(c);
                }
                ' ' => result.push('+'),
                _ => {
                    for byte in c.to_string().bytes() {
                        result.push('%');
                        result.push_str(&format!("{:02X}", byte));
                    }
                }
            }
        }
        result
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

    #[test]
    fn test_urlencoding() {
        assert_eq!(urlencoding::encode("hello"), "hello");
        assert_eq!(urlencoding::encode("hello world"), "hello+world");
        assert_eq!(urlencoding::encode("hello@world"), "hello%40world");
    }

    #[test]
    fn test_extract_string_value() {
        assert_eq!(extract_string_value("\"hello\""), Some("hello".to_string()));
        assert_eq!(extract_string_value("  \"hello\"  "), Some("hello".to_string()));
        assert_eq!(extract_string_value("hello"), None);
    }
}
