//! Package manager error types

use super::PackageId;
use std::fmt;

/// Package manager result type
pub type PkgResult<T> = Result<T, PkgError>;

/// Package manager errors
#[derive(Debug, Clone)]
pub enum PkgError {
    /// Invalid version string
    InvalidVersion(String),
    /// Invalid version requirement
    InvalidVersionReq(String),
    /// Invalid package ID
    InvalidPackageId(String),
    /// Invalid manifest format
    InvalidManifest(String),
    /// Package not found in registry
    PackageNotFound(String),
    /// No version matches the requirement
    NoMatchingVersion { name: String, requirement: String },
    /// Package already installed
    AlreadyInstalled(PackageId),
    /// Package not installed
    NotInstalled(String),
    /// Package has dependents that need it
    HasDependents {
        package: String,
        dependents: Vec<String>,
    },
    /// Dependency resolution failed
    DependencyConflict {
        package: String,
        requirement1: String,
        requirement2: String,
    },
    /// Circular dependency detected
    CircularDependency(Vec<String>),
    /// Checksum mismatch
    ChecksumMismatch { expected: String, actual: String },
    /// IO error
    IoError(String),
    /// Network error
    NetworkError(String),
    /// Registry error
    RegistryError(String),
    /// Feature not available (e.g., non-WASM build)
    NotAvailable(String),
    /// Invalid package archive
    InvalidArchive(String),
    /// Missing required file
    MissingFile(String),
    /// Permission denied
    PermissionDenied(String),
}

impl fmt::Display for PkgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PkgError::InvalidVersion(v) => write!(f, "invalid version: {}", v),
            PkgError::InvalidVersionReq(v) => write!(f, "invalid version requirement: {}", v),
            PkgError::InvalidPackageId(id) => write!(f, "invalid package ID: {}", id),
            PkgError::InvalidManifest(msg) => write!(f, "invalid manifest: {}", msg),
            PkgError::PackageNotFound(name) => write!(f, "package not found: {}", name),
            PkgError::NoMatchingVersion { name, requirement } => {
                write!(f, "no version of {} matches {}", name, requirement)
            }
            PkgError::AlreadyInstalled(id) => write!(f, "package already installed: {}", id),
            PkgError::NotInstalled(name) => write!(f, "package not installed: {}", name),
            PkgError::HasDependents {
                package,
                dependents,
            } => {
                write!(
                    f,
                    "cannot remove {}: required by {}",
                    package,
                    dependents.join(", ")
                )
            }
            PkgError::DependencyConflict {
                package,
                requirement1,
                requirement2,
            } => {
                write!(
                    f,
                    "dependency conflict for {}: {} vs {}",
                    package, requirement1, requirement2
                )
            }
            PkgError::CircularDependency(chain) => {
                write!(f, "circular dependency: {}", chain.join(" -> "))
            }
            PkgError::ChecksumMismatch { expected, actual } => {
                write!(
                    f,
                    "checksum mismatch: expected {}, got {}",
                    expected, actual
                )
            }
            PkgError::IoError(msg) => write!(f, "I/O error: {}", msg),
            PkgError::NetworkError(msg) => write!(f, "network error: {}", msg),
            PkgError::RegistryError(msg) => write!(f, "registry error: {}", msg),
            PkgError::NotAvailable(msg) => write!(f, "not available: {}", msg),
            PkgError::InvalidArchive(msg) => write!(f, "invalid archive: {}", msg),
            PkgError::MissingFile(file) => write!(f, "missing file: {}", file),
            PkgError::PermissionDenied(msg) => write!(f, "permission denied: {}", msg),
        }
    }
}

impl std::error::Error for PkgError {}

impl From<std::io::Error> for PkgError {
    fn from(e: std::io::Error) -> Self {
        PkgError::IoError(e.to_string())
    }
}
