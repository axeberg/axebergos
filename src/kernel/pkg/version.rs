//! Semantic Versioning support
//!
//! Implements Semantic Versioning 2.0.0 (https://semver.org/)
//!
//! # Version Format
//!
//! A version consists of MAJOR.MINOR.PATCH with optional pre-release
//! and build metadata:
//!
//! ```text
//! MAJOR.MINOR.PATCH[-PRERELEASE][+BUILD]
//!
//! Examples:
//!   1.0.0
//!   2.1.0-alpha.1
//!   3.0.0-beta.2+build.123
//! ```
//!
//! # Version Requirements
//!
//! Supports npm/cargo-style version requirements:
//!
//! - `1.0.0` - Exact version
//! - `^1.0.0` - Compatible (same major, greater minor/patch)
//! - `~1.0.0` - Approximately (same major.minor, greater patch)
//! - `>=1.0.0` - Greater than or equal
//! - `>1.0.0` - Greater than
//! - `<=1.0.0` - Less than or equal
//! - `<1.0.0` - Less than
//! - `*` - Any version

use super::error::{PkgError, PkgResult};
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};

/// A semantic version (MAJOR.MINOR.PATCH)
#[derive(Debug, Clone, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub prerelease: Option<String>,
    pub build: Option<String>,
}

impl Version {
    /// Create a new version
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
            prerelease: None,
            build: None,
        }
    }

    /// Create a version with pre-release tag
    pub fn with_prerelease(mut self, prerelease: &str) -> Self {
        self.prerelease = Some(prerelease.to_string());
        self
    }

    /// Create a version with build metadata
    pub fn with_build(mut self, build: &str) -> Self {
        self.build = Some(build.to_string());
        self
    }

    /// Parse a version string
    pub fn parse(s: &str) -> PkgResult<Self> {
        let s = s.trim();
        if s.is_empty() {
            return Err(PkgError::InvalidVersion("empty version".to_string()));
        }

        // Split off build metadata first (after +)
        let (version_pre, build) = if let Some(pos) = s.find('+') {
            (&s[..pos], Some(s[pos + 1..].to_string()))
        } else {
            (s, None)
        };

        // Split off prerelease (after -)
        let (version, prerelease) = if let Some(pos) = version_pre.find('-') {
            (
                &version_pre[..pos],
                Some(version_pre[pos + 1..].to_string()),
            )
        } else {
            (version_pre, None)
        };

        // Parse major.minor.patch
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 {
            return Err(PkgError::InvalidVersion(format!(
                "expected MAJOR.MINOR.PATCH, got: {}",
                s
            )));
        }

        let major = parts[0].parse().map_err(|_| {
            PkgError::InvalidVersion(format!("invalid major version: {}", parts[0]))
        })?;
        let minor = parts[1].parse().map_err(|_| {
            PkgError::InvalidVersion(format!("invalid minor version: {}", parts[1]))
        })?;
        let patch = parts[2].parse().map_err(|_| {
            PkgError::InvalidVersion(format!("invalid patch version: {}", parts[2]))
        })?;

        Ok(Self {
            major,
            minor,
            patch,
            prerelease,
            build,
        })
    }

    /// Check if this is a prerelease version
    pub fn is_prerelease(&self) -> bool {
        self.prerelease.is_some()
    }

    /// Check if this version is compatible with another (same major version)
    pub fn is_compatible_with(&self, other: &Version) -> bool {
        self.major == other.major
    }

    /// Get the next major version
    pub fn next_major(&self) -> Version {
        Version::new(self.major + 1, 0, 0)
    }

    /// Get the next minor version
    pub fn next_minor(&self) -> Version {
        Version::new(self.major, self.minor + 1, 0)
    }

    /// Get the next patch version
    pub fn next_patch(&self) -> Version {
        Version::new(self.major, self.minor, self.patch + 1)
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        // Build metadata is ignored for equality (per semver spec)
        self.major == other.major
            && self.minor == other.minor
            && self.patch == other.patch
            && self.prerelease == other.prerelease
    }
}

impl Hash for Version {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Build metadata is ignored for hashing (must match PartialEq)
        self.major.hash(state);
        self.minor.hash(state);
        self.patch.hash(state);
        self.prerelease.hash(state);
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        // Compare major.minor.patch first
        match self.major.cmp(&other.major) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.minor.cmp(&other.minor) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.patch.cmp(&other.patch) {
            Ordering::Equal => {}
            ord => return ord,
        }

        // Pre-release versions have lower precedence
        match (&self.prerelease, &other.prerelease) {
            (None, None) => Ordering::Equal,
            (Some(_), None) => Ordering::Less, // pre-release < release
            (None, Some(_)) => Ordering::Greater, // release > pre-release
            (Some(a), Some(b)) => compare_prerelease(a, b),
        }
    }
}

/// Compare pre-release identifiers according to semver spec
fn compare_prerelease(a: &str, b: &str) -> Ordering {
    let a_parts: Vec<&str> = a.split('.').collect();
    let b_parts: Vec<&str> = b.split('.').collect();

    for (a_part, b_part) in a_parts.iter().zip(b_parts.iter()) {
        // Try to parse as numbers
        match (a_part.parse::<u64>(), b_part.parse::<u64>()) {
            (Ok(a_num), Ok(b_num)) => {
                // Numeric comparison
                match a_num.cmp(&b_num) {
                    Ordering::Equal => continue,
                    ord => return ord,
                }
            }
            (Ok(_), Err(_)) => return Ordering::Less, // numeric < alphanumeric
            (Err(_), Ok(_)) => return Ordering::Greater, // alphanumeric > numeric
            (Err(_), Err(_)) => {
                // Alphanumeric comparison
                match a_part.cmp(b_part) {
                    Ordering::Equal => continue,
                    ord => return ord,
                }
            }
        }
    }

    // Longer pre-release has higher precedence
    a_parts.len().cmp(&b_parts.len())
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(ref pre) = self.prerelease {
            write!(f, "-{}", pre)?;
        }
        if let Some(ref build) = self.build {
            write!(f, "+{}", build)?;
        }
        Ok(())
    }
}

/// A version requirement (constraint)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionReq {
    comparators: Vec<Comparator>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Comparator {
    /// Exact version match
    Exact(Version),
    /// Greater than
    Greater(Version),
    /// Greater than or equal
    GreaterEq(Version),
    /// Less than
    Less(Version),
    /// Less than or equal
    LessEq(Version),
    /// Compatible with (^version)
    Caret(Version),
    /// Approximately equal (~version)
    Tilde(Version),
    /// Any version
    Any,
}

impl VersionReq {
    /// Create a requirement that matches any version
    pub fn any() -> Self {
        Self {
            comparators: vec![Comparator::Any],
        }
    }

    /// Create a requirement for exact version
    pub fn exact(version: Version) -> Self {
        Self {
            comparators: vec![Comparator::Exact(version)],
        }
    }

    /// Parse a version requirement string
    pub fn parse(s: &str) -> PkgResult<Self> {
        let s = s.trim();
        if s.is_empty() || s == "*" {
            return Ok(Self::any());
        }

        // Handle compound requirements (comma-separated)
        if s.contains(',') {
            let parts: Vec<&str> = s.split(',').map(|p| p.trim()).collect();
            let mut comparators = Vec::new();
            for part in parts {
                let req = Self::parse(part)?;
                comparators.extend(req.comparators);
            }
            return Ok(Self { comparators });
        }

        // Parse single requirement
        let comparator = if let Some(rest) = s.strip_prefix(">=") {
            Comparator::GreaterEq(Version::parse(rest.trim())?)
        } else if let Some(rest) = s.strip_prefix("<=") {
            Comparator::LessEq(Version::parse(rest.trim())?)
        } else if let Some(rest) = s.strip_prefix('>') {
            Comparator::Greater(Version::parse(rest.trim())?)
        } else if let Some(rest) = s.strip_prefix('<') {
            Comparator::Less(Version::parse(rest.trim())?)
        } else if let Some(rest) = s.strip_prefix('^') {
            Comparator::Caret(Version::parse(rest.trim())?)
        } else if let Some(rest) = s.strip_prefix('~') {
            Comparator::Tilde(Version::parse(rest.trim())?)
        } else if let Some(rest) = s.strip_prefix('=') {
            Comparator::Exact(Version::parse(rest.trim())?)
        } else {
            // No operator - treat as exact or caret depending on context
            // For simplicity, treat as exact match
            Comparator::Exact(Version::parse(s)?)
        };

        Ok(Self {
            comparators: vec![comparator],
        })
    }

    /// Check if a version matches this requirement
    pub fn matches(&self, version: &Version) -> bool {
        self.comparators.iter().all(|c| c.matches(version))
    }
}

impl Comparator {
    fn matches(&self, version: &Version) -> bool {
        match self {
            Comparator::Any => true,
            Comparator::Exact(v) => version == v,
            Comparator::Greater(v) => version > v,
            Comparator::GreaterEq(v) => version >= v,
            Comparator::Less(v) => version < v,
            Comparator::LessEq(v) => version <= v,
            Comparator::Caret(v) => {
                // ^1.2.3 matches >=1.2.3 and <2.0.0
                // ^0.2.3 matches >=0.2.3 and <0.3.0
                // ^0.0.3 matches >=0.0.3 and <0.0.4
                if version < v {
                    return false;
                }
                if v.major == 0 {
                    if v.minor == 0 {
                        // ^0.0.x - only patch changes allowed
                        version.major == 0 && version.minor == 0
                    } else {
                        // ^0.x.y - minor must match
                        version.major == 0 && version.minor == v.minor
                    }
                } else {
                    // ^x.y.z - major must match
                    version.major == v.major
                }
            }
            Comparator::Tilde(v) => {
                // ~1.2.3 matches >=1.2.3 and <1.3.0
                if version < v {
                    return false;
                }
                version.major == v.major && version.minor == v.minor
            }
        }
    }
}

impl fmt::Display for VersionReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self
            .comparators
            .iter()
            .map(|c| match c {
                Comparator::Any => "*".to_string(),
                Comparator::Exact(v) => format!("={}", v),
                Comparator::Greater(v) => format!(">{}", v),
                Comparator::GreaterEq(v) => format!(">={}", v),
                Comparator::Less(v) => format!("<{}", v),
                Comparator::LessEq(v) => format!("<={}", v),
                Comparator::Caret(v) => format!("^{}", v),
                Comparator::Tilde(v) => format!("~{}", v),
            })
            .collect();
        write!(f, "{}", parts.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_new() {
        let v = Version::new(1, 2, 3);
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert_eq!(v.prerelease, None);
        assert_eq!(v.build, None);
    }

    #[test]
    fn test_version_parse_simple() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v, Version::new(1, 2, 3));
    }

    #[test]
    fn test_version_parse_with_prerelease() {
        let v = Version::parse("1.0.0-alpha.1").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 0);
        assert_eq!(v.prerelease, Some("alpha.1".to_string()));
    }

    #[test]
    fn test_version_parse_with_build() {
        let v = Version::parse("1.0.0+build.123").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.build, Some("build.123".to_string()));
    }

    #[test]
    fn test_version_parse_full() {
        let v = Version::parse("2.1.3-beta.2+build.456").unwrap();
        assert_eq!(v.major, 2);
        assert_eq!(v.minor, 1);
        assert_eq!(v.patch, 3);
        assert_eq!(v.prerelease, Some("beta.2".to_string()));
        assert_eq!(v.build, Some("build.456".to_string()));
    }

    #[test]
    fn test_version_comparison() {
        assert!(Version::new(1, 0, 0) < Version::new(2, 0, 0));
        assert!(Version::new(1, 1, 0) < Version::new(1, 2, 0));
        assert!(Version::new(1, 0, 1) < Version::new(1, 0, 2));
    }

    #[test]
    fn test_version_prerelease_comparison() {
        let release = Version::new(1, 0, 0);
        let alpha = Version::new(1, 0, 0).with_prerelease("alpha");
        let beta = Version::new(1, 0, 0).with_prerelease("beta");

        assert!(alpha < beta);
        assert!(alpha < release);
        assert!(beta < release);
    }

    #[test]
    fn test_version_display() {
        assert_eq!(format!("{}", Version::new(1, 2, 3)), "1.2.3");
        assert_eq!(
            format!("{}", Version::new(1, 0, 0).with_prerelease("alpha")),
            "1.0.0-alpha"
        );
        assert_eq!(
            format!("{}", Version::new(1, 0, 0).with_build("123")),
            "1.0.0+123"
        );
    }

    #[test]
    fn test_version_req_any() {
        let req = VersionReq::any();
        assert!(req.matches(&Version::new(0, 0, 1)));
        assert!(req.matches(&Version::new(1, 0, 0)));
        assert!(req.matches(&Version::new(99, 99, 99)));
    }

    #[test]
    fn test_version_req_exact() {
        let req = VersionReq::parse("1.2.3").unwrap();
        assert!(req.matches(&Version::new(1, 2, 3)));
        assert!(!req.matches(&Version::new(1, 2, 4)));
        assert!(!req.matches(&Version::new(1, 3, 0)));
    }

    #[test]
    fn test_version_req_greater() {
        let req = VersionReq::parse(">1.0.0").unwrap();
        assert!(!req.matches(&Version::new(0, 9, 0)));
        assert!(!req.matches(&Version::new(1, 0, 0)));
        assert!(req.matches(&Version::new(1, 0, 1)));
        assert!(req.matches(&Version::new(2, 0, 0)));
    }

    #[test]
    fn test_version_req_greater_eq() {
        let req = VersionReq::parse(">=1.0.0").unwrap();
        assert!(!req.matches(&Version::new(0, 9, 0)));
        assert!(req.matches(&Version::new(1, 0, 0)));
        assert!(req.matches(&Version::new(1, 0, 1)));
    }

    #[test]
    fn test_version_req_less() {
        let req = VersionReq::parse("<2.0.0").unwrap();
        assert!(req.matches(&Version::new(1, 9, 9)));
        assert!(!req.matches(&Version::new(2, 0, 0)));
        assert!(!req.matches(&Version::new(2, 0, 1)));
    }

    #[test]
    fn test_version_req_caret() {
        // ^1.2.3 matches >=1.2.3 <2.0.0
        let req = VersionReq::parse("^1.2.3").unwrap();
        assert!(!req.matches(&Version::new(1, 2, 2)));
        assert!(req.matches(&Version::new(1, 2, 3)));
        assert!(req.matches(&Version::new(1, 3, 0)));
        assert!(req.matches(&Version::new(1, 9, 9)));
        assert!(!req.matches(&Version::new(2, 0, 0)));
    }

    #[test]
    fn test_version_req_caret_zero_minor() {
        // ^0.2.3 matches >=0.2.3 <0.3.0
        let req = VersionReq::parse("^0.2.3").unwrap();
        assert!(!req.matches(&Version::new(0, 2, 2)));
        assert!(req.matches(&Version::new(0, 2, 3)));
        assert!(req.matches(&Version::new(0, 2, 9)));
        assert!(!req.matches(&Version::new(0, 3, 0)));
    }

    #[test]
    fn test_version_req_tilde() {
        // ~1.2.3 matches >=1.2.3 <1.3.0
        let req = VersionReq::parse("~1.2.3").unwrap();
        assert!(!req.matches(&Version::new(1, 2, 2)));
        assert!(req.matches(&Version::new(1, 2, 3)));
        assert!(req.matches(&Version::new(1, 2, 9)));
        assert!(!req.matches(&Version::new(1, 3, 0)));
    }

    #[test]
    fn test_version_req_compound() {
        // >=1.0.0, <2.0.0
        let req = VersionReq::parse(">=1.0.0, <2.0.0").unwrap();
        assert!(!req.matches(&Version::new(0, 9, 0)));
        assert!(req.matches(&Version::new(1, 0, 0)));
        assert!(req.matches(&Version::new(1, 9, 9)));
        assert!(!req.matches(&Version::new(2, 0, 0)));
    }

    #[test]
    fn test_version_req_display() {
        let req = VersionReq::parse("^1.0.0").unwrap();
        assert_eq!(format!("{}", req), "^1.0.0");

        let req = VersionReq::parse(">=1.0.0, <2.0.0").unwrap();
        assert_eq!(format!("{}", req), ">=1.0.0, <2.0.0");
    }
}
