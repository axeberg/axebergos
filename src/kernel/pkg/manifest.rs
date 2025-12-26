//! Package manifest parsing
//!
//! Parses `package.toml` files that describe package metadata.
//!
//! # Format
//!
//! ```toml
//! [package]
//! name = "hello"
//! version = "1.0.0"
//! description = "A hello world command"
//! authors = ["axeberg"]
//! license = "MIT"
//! repository = "https://github.com/axeberg/hello"
//!
//! [[bin]]
//! name = "hello"
//! path = "bin/hello.wasm"
//!
//! [dependencies]
//! utils = "^1.0"
//! core = ">=2.0.0, <3.0.0"
//! ```

use super::checksum::Checksum;
use super::error::{PkgError, PkgResult};
use super::version::{Version, VersionReq};
use std::collections::HashMap;

/// Package manifest (parsed from package.toml)
#[derive(Debug, Clone)]
pub struct PackageManifest {
    /// Package name
    pub name: String,
    /// Package version
    pub version: Version,
    /// Short description
    pub description: Option<String>,
    /// List of authors
    pub authors: Vec<String>,
    /// License identifier (e.g., "MIT", "Apache-2.0")
    pub license: Option<String>,
    /// Repository URL
    pub repository: Option<String>,
    /// Homepage URL
    pub homepage: Option<String>,
    /// Keywords for search
    pub keywords: Vec<String>,
    /// Binary entries
    pub binaries: Vec<BinaryEntry>,
    /// Dependencies
    pub dependencies: Vec<Dependency>,
    /// Development dependencies (not installed by default)
    pub dev_dependencies: Vec<Dependency>,
}

/// A binary entry in the package
#[derive(Debug, Clone)]
pub struct BinaryEntry {
    /// Binary name (command name)
    pub name: String,
    /// Path to WASM file within package
    pub path: String,
    /// SHA-256 checksum
    pub checksum: Option<Checksum>,
}

/// A package dependency
#[derive(Debug, Clone)]
pub struct Dependency {
    /// Package name
    pub name: String,
    /// Version requirement
    pub version_req: VersionReq,
    /// Optional (not required for installation)
    pub optional: bool,
}

impl PackageManifest {
    /// Parse a manifest from TOML string
    pub fn parse(content: &str) -> PkgResult<Self> {
        let mut parser = TomlParser::new(content);
        parser.parse_manifest()
    }

    /// Serialize manifest to TOML string
    pub fn to_toml(&self) -> String {
        let mut output = String::new();

        // [package] section
        output.push_str("[package]\n");
        output.push_str(&format!("name = \"{}\"\n", self.name));
        output.push_str(&format!("version = \"{}\"\n", self.version));

        if let Some(ref desc) = self.description {
            output.push_str(&format!("description = \"{}\"\n", escape_toml_string(desc)));
        }

        if !self.authors.is_empty() {
            let authors: Vec<String> = self.authors.iter().map(|a| format!("\"{}\"", a)).collect();
            output.push_str(&format!("authors = [{}]\n", authors.join(", ")));
        }

        if let Some(ref license) = self.license {
            output.push_str(&format!("license = \"{}\"\n", license));
        }

        if let Some(ref repo) = self.repository {
            output.push_str(&format!("repository = \"{}\"\n", repo));
        }

        if let Some(ref homepage) = self.homepage {
            output.push_str(&format!("homepage = \"{}\"\n", homepage));
        }

        if !self.keywords.is_empty() {
            let keywords: Vec<String> =
                self.keywords.iter().map(|k| format!("\"{}\"", k)).collect();
            output.push_str(&format!("keywords = [{}]\n", keywords.join(", ")));
        }

        // [[bin]] sections
        for bin in &self.binaries {
            output.push_str("\n[[bin]]\n");
            output.push_str(&format!("name = \"{}\"\n", bin.name));
            output.push_str(&format!("path = \"{}\"\n", bin.path));
            if let Some(ref checksum) = bin.checksum {
                output.push_str(&format!("checksum = \"{}\"\n", checksum));
            }
        }

        // [dependencies] section
        if !self.dependencies.is_empty() {
            output.push_str("\n[dependencies]\n");
            for dep in &self.dependencies {
                let version_str = if dep.optional {
                    format!("{{ version = \"{}\", optional = true }}", dep.version_req)
                } else {
                    format!("\"{}\"", dep.version_req)
                };
                output.push_str(&format!("{} = {}\n", dep.name, version_str));
            }
        }

        // [dev-dependencies] section
        if !self.dev_dependencies.is_empty() {
            output.push_str("\n[dev-dependencies]\n");
            for dep in &self.dev_dependencies {
                output.push_str(&format!("{} = \"{}\"\n", dep.name, dep.version_req));
            }
        }

        output
    }
}

/// Escape special characters in TOML string
fn escape_toml_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Simple TOML parser for package manifests
struct TomlParser<'a> {
    content: &'a str,
    pos: usize,
}

impl<'a> TomlParser<'a> {
    fn new(content: &'a str) -> Self {
        Self { content, pos: 0 }
    }

    fn parse_manifest(&mut self) -> PkgResult<PackageManifest> {
        let mut name = None;
        let mut version = None;
        let mut description = None;
        let mut authors = Vec::new();
        let mut license = None;
        let mut repository = None;
        let mut homepage = None;
        let mut keywords = Vec::new();
        let mut binaries = Vec::new();
        let mut dependencies = Vec::new();
        let mut dev_dependencies = Vec::new();

        while self.pos < self.content.len() {
            self.skip_whitespace_and_comments();
            if self.pos >= self.content.len() {
                break;
            }

            let line = self.read_line();

            // Section header
            if line.starts_with('[') {
                let section = line.trim_start_matches('[').trim_end_matches(']').trim();

                if section == "[bin]" || section == "bin" {
                    // Array of tables [[bin]]
                    binaries.push(self.parse_bin_section()?);
                } else if section == "package" {
                    // Parse package section - need to handle arrays specially
                    loop {
                        self.skip_whitespace_and_comments();
                        if self.pos >= self.content.len() {
                            break;
                        }
                        let peek = self.peek_line();
                        if peek.starts_with('[') {
                            break;
                        }
                        let line = self.read_line();
                        if line.is_empty() {
                            continue;
                        }
                        if line.contains('=') {
                            let (key, value) = self.parse_key_value(&line)?;
                            match key.as_str() {
                                "name" => name = Some(value),
                                "version" => version = Some(value),
                                "description" => description = Some(value),
                                "license" => license = Some(value),
                                "repository" => repository = Some(value),
                                "homepage" => homepage = Some(value),
                                "authors" => authors = self.parse_array_value(&value),
                                "keywords" => keywords = self.parse_array_value(&value),
                                _ => {}
                            }
                        }
                    }
                } else if section == "dependencies" {
                    dependencies = self.parse_dependencies_section()?;
                } else if section == "dev-dependencies" {
                    dev_dependencies = self.parse_dependencies_section()?;
                }
            } else if line.starts_with("[[bin]]") {
                binaries.push(self.parse_bin_section()?);
            } else if !line.is_empty() && line.contains('=') {
                // Top-level key-value (treat as package section)
                let (key, value) = self.parse_key_value(&line)?;
                match key.as_str() {
                    "name" => name = Some(value),
                    "version" => version = Some(value),
                    "description" => description = Some(value),
                    "authors" => authors = self.parse_array_value(&value),
                    "license" => license = Some(value),
                    "repository" => repository = Some(value),
                    "homepage" => homepage = Some(value),
                    "keywords" => keywords = self.parse_array_value(&value),
                    _ => {}
                }
            }
        }

        let name =
            name.ok_or_else(|| PkgError::InvalidManifest("missing 'name' field".to_string()))?;
        let version_str = version
            .ok_or_else(|| PkgError::InvalidManifest("missing 'version' field".to_string()))?;
        let version = Version::parse(&version_str)?;

        Ok(PackageManifest {
            name,
            version,
            description,
            authors,
            license,
            repository,
            homepage,
            keywords,
            binaries,
            dependencies,
            dev_dependencies,
        })
    }

    fn skip_whitespace_and_comments(&mut self) {
        let bytes = self.content.as_bytes();
        while self.pos < bytes.len() {
            match bytes[self.pos] {
                b' ' | b'\t' | b'\r' | b'\n' => self.pos += 1,
                b'#' => {
                    // Skip comment until end of line
                    while self.pos < bytes.len() && bytes[self.pos] != b'\n' {
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
    }

    fn read_line(&mut self) -> String {
        let start = self.pos;
        let bytes = self.content.as_bytes();
        while self.pos < bytes.len() && bytes[self.pos] != b'\n' {
            self.pos += 1;
        }
        let line = self.content[start..self.pos].trim().to_string();
        if self.pos < bytes.len() {
            self.pos += 1; // Skip newline
        }
        line
    }

    fn parse_key_value(&self, line: &str) -> PkgResult<(String, String)> {
        let parts: Vec<&str> = line.splitn(2, '=').collect();
        if parts.len() != 2 {
            return Err(PkgError::InvalidManifest(format!(
                "invalid key-value: {}",
                line
            )));
        }

        let key = parts[0].trim().to_string();
        let value = self.parse_value(parts[1].trim())?;

        Ok((key, value))
    }

    fn parse_value(&self, s: &str) -> PkgResult<String> {
        let s = s.trim();

        // String value
        if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
            return Ok(unescape_toml_string(&s[1..s.len() - 1]));
        }

        // Array value - return as-is for later parsing
        if s.starts_with('[') && s.ends_with(']') {
            return Ok(s.to_string());
        }

        // Inline table
        if s.starts_with('{') && s.ends_with('}') {
            return Ok(s.to_string());
        }

        // Bare value
        Ok(s.to_string())
    }

    fn parse_array_value(&self, s: &str) -> Vec<String> {
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
                '"' if !in_string => {
                    in_string = true;
                }
                '"' if in_string => {
                    in_string = false;
                    values.push(current.clone());
                    current.clear();
                }
                ',' if !in_string => {
                    // Skip comma
                }
                _ if in_string => {
                    current.push(c);
                }
                _ => {}
            }
        }

        values
    }

    fn parse_section<F>(&mut self, mut handler: F) -> PkgResult<()>
    where
        F: FnMut(&str, &str) -> PkgResult<()>,
    {
        loop {
            self.skip_whitespace_and_comments();
            if self.pos >= self.content.len() {
                break;
            }

            // Check if we've reached a new section
            let peek = self.peek_line();
            if peek.starts_with('[') {
                break;
            }

            let line = self.read_line();
            if line.is_empty() {
                continue;
            }

            if line.contains('=') {
                let (key, value) = self.parse_key_value(&line)?;
                handler(&key, &value)?;
            }
        }

        Ok(())
    }

    fn peek_line(&self) -> String {
        let bytes = self.content.as_bytes();
        let mut pos = self.pos;

        // Skip whitespace
        while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
            pos += 1;
        }

        let start = pos;
        while pos < bytes.len() && bytes[pos] != b'\n' {
            pos += 1;
        }

        self.content[start..pos].trim().to_string()
    }

    fn parse_bin_section(&mut self) -> PkgResult<BinaryEntry> {
        let mut name = None;
        let mut path = None;
        let mut checksum = None;

        self.parse_section(|key, value| {
            match key {
                "name" => name = Some(value.to_string()),
                "path" => path = Some(value.to_string()),
                "checksum" => checksum = Some(Checksum::from_hex(value)?),
                _ => {}
            }
            Ok(())
        })?;

        let name = name
            .ok_or_else(|| PkgError::InvalidManifest("bin entry missing 'name'".to_string()))?;
        let path = path
            .ok_or_else(|| PkgError::InvalidManifest("bin entry missing 'path'".to_string()))?;

        Ok(BinaryEntry {
            name,
            path,
            checksum,
        })
    }

    fn parse_dependencies_section(&mut self) -> PkgResult<Vec<Dependency>> {
        let mut deps = Vec::new();

        loop {
            self.skip_whitespace_and_comments();
            if self.pos >= self.content.len() {
                break;
            }

            let peek = self.peek_line();
            if peek.starts_with('[') {
                break;
            }

            let line = self.read_line();
            if line.is_empty() {
                continue;
            }

            if line.contains('=') {
                let (key, value) = self.parse_key_value(&line)?;
                let dep = self.parse_dependency(&key, &value)?;
                deps.push(dep);
            }
        }

        Ok(deps)
    }

    fn parse_dependency(&self, name: &str, value: &str) -> PkgResult<Dependency> {
        let value = value.trim();

        // Simple string version
        if !value.starts_with('{') {
            let version_req = VersionReq::parse(value)?;
            return Ok(Dependency {
                name: name.to_string(),
                version_req,
                optional: false,
            });
        }

        // Inline table { version = "...", optional = true }
        let inner = value.trim_start_matches('{').trim_end_matches('}');
        let mut version_req = VersionReq::any();
        let mut optional = false;

        for part in inner.split(',') {
            let part = part.trim();
            if let Some(pos) = part.find('=') {
                let k = part[..pos].trim();
                let v = part[pos + 1..].trim().trim_matches('"');
                match k {
                    "version" => version_req = VersionReq::parse(v)?,
                    "optional" => optional = v == "true",
                    _ => {}
                }
            }
        }

        Ok(Dependency {
            name: name.to_string(),
            version_req,
            optional,
        })
    }
}

/// Unescape TOML string escape sequences
fn unescape_toml_string(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('\\') => result.push('\\'),
                Some('"') => result.push('"'),
                Some(c) => {
                    result.push('\\');
                    result.push(c);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_manifest() {
        let toml = r#"
[package]
name = "hello"
version = "1.0.0"
description = "A hello world command"
"#;

        let manifest = PackageManifest::parse(toml).unwrap();
        assert_eq!(manifest.name, "hello");
        assert_eq!(manifest.version, Version::new(1, 0, 0));
        assert_eq!(manifest.description, Some("A hello world command".to_string()));
    }

    #[test]
    fn test_parse_manifest_with_bin() {
        let toml = r#"
[package]
name = "hello"
version = "1.0.0"

[[bin]]
name = "hello"
path = "bin/hello.wasm"
"#;

        let manifest = PackageManifest::parse(toml).unwrap();
        assert_eq!(manifest.binaries.len(), 1);
        assert_eq!(manifest.binaries[0].name, "hello");
        assert_eq!(manifest.binaries[0].path, "bin/hello.wasm");
    }

    #[test]
    fn test_parse_manifest_with_dependencies() {
        let toml = r#"
[package]
name = "myapp"
version = "2.0.0"

[dependencies]
utils = "^1.0.0"
core = ">=2.0.0"
"#;

        let manifest = PackageManifest::parse(toml).unwrap();
        assert_eq!(manifest.dependencies.len(), 2);

        let utils_dep = manifest.dependencies.iter().find(|d| d.name == "utils").unwrap();
        assert!(utils_dep.version_req.matches(&Version::new(1, 0, 0)));
        assert!(utils_dep.version_req.matches(&Version::new(1, 5, 0)));
        assert!(!utils_dep.version_req.matches(&Version::new(2, 0, 0)));
    }

    #[test]
    fn test_parse_manifest_with_optional_dependency() {
        let toml = r#"
[package]
name = "myapp"
version = "1.0.0"

[dependencies]
optional-dep = { version = "^1.0.0", optional = true }
"#;

        let manifest = PackageManifest::parse(toml).unwrap();
        assert_eq!(manifest.dependencies.len(), 1);
        assert!(manifest.dependencies[0].optional);
    }

    #[test]
    fn test_manifest_to_toml() {
        let manifest = PackageManifest {
            name: "test".to_string(),
            version: Version::new(1, 2, 3),
            description: Some("Test package".to_string()),
            authors: vec!["Author 1".to_string()],
            license: Some("MIT".to_string()),
            repository: None,
            homepage: None,
            keywords: vec!["test".to_string(), "example".to_string()],
            binaries: vec![BinaryEntry {
                name: "test".to_string(),
                path: "bin/test.wasm".to_string(),
                checksum: None,
            }],
            dependencies: vec![Dependency {
                name: "dep1".to_string(),
                version_req: VersionReq::parse("^1.0.0").unwrap(),
                optional: false,
            }],
            dev_dependencies: vec![],
        };

        let toml = manifest.to_toml();
        assert!(toml.contains("name = \"test\""));
        assert!(toml.contains("version = \"1.2.3\""));
        assert!(toml.contains("[[bin]]"));
        assert!(toml.contains("[dependencies]"));
    }

    #[test]
    fn test_parse_authors_array() {
        let toml = r#"
[package]
name = "test"
version = "1.0.0"
authors = ["Author 1", "Author 2"]
"#;

        let manifest = PackageManifest::parse(toml).unwrap();
        assert_eq!(manifest.authors.len(), 2);
        assert_eq!(manifest.authors[0], "Author 1");
        assert_eq!(manifest.authors[1], "Author 2");
    }

    #[test]
    fn test_parse_checksum() {
        let toml = r#"
[package]
name = "test"
version = "1.0.0"

[[bin]]
name = "test"
path = "bin/test.wasm"
checksum = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
"#;

        let manifest = PackageManifest::parse(toml).unwrap();
        assert!(manifest.binaries[0].checksum.is_some());
    }

    #[test]
    fn test_escape_toml_string() {
        assert_eq!(escape_toml_string("hello"), "hello");
        assert_eq!(escape_toml_string("hello\nworld"), "hello\\nworld");
        assert_eq!(escape_toml_string("say \"hi\""), "say \\\"hi\\\"");
    }

    #[test]
    fn test_unescape_toml_string() {
        assert_eq!(unescape_toml_string("hello"), "hello");
        assert_eq!(unescape_toml_string("hello\\nworld"), "hello\nworld");
        assert_eq!(unescape_toml_string("say \\\"hi\\\""), "say \"hi\"");
    }
}
