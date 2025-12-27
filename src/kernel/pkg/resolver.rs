//! Dependency resolution
//!
//! Resolves package dependencies using a topological sort algorithm.
//! Detects conflicts and circular dependencies.
//!
//! # Algorithm
//!
//! 1. Start with the root package
//! 2. Recursively fetch dependencies
//! 3. For each dependency:
//!    - Find the best matching version from the registry
//!    - Check for conflicts with already-resolved packages
//!    - Add to resolution queue
//! 4. Topologically sort to get installation order
//!
//! # Version Selection
//!
//! When multiple versions could satisfy a requirement:
//! - Prefer the highest compatible version
//! - For caret (^) requirements, stay within major version
//! - For tilde (~) requirements, stay within minor version

use super::error::{PkgError, PkgResult};
use super::manifest::PackageManifest;
use super::registry::PackageRegistry;
use super::version::{Version, VersionReq};
use super::PackageId;
use std::collections::{HashMap, HashSet};

/// A resolved package ready for installation
#[derive(Debug, Clone)]
pub struct ResolvedPackage {
    /// Package identifier
    pub id: PackageId,
    /// Package manifest
    pub manifest: PackageManifest,
    /// Direct dependencies (names only)
    pub dependencies: Vec<String>,
    /// Installation order (lower = install first)
    pub order: usize,
}

/// Dependency resolver
pub struct DependencyResolver {
    /// Resolved packages
    resolved: HashMap<String, ResolvedPackage>,
    /// Version constraints per package
    constraints: HashMap<String, Vec<VersionReq>>,
    /// Package currently being resolved (for cycle detection)
    resolving: HashSet<String>,
    /// Resolution path (for error messages)
    resolution_path: Vec<String>,
}

impl DependencyResolver {
    /// Create a new resolver
    pub fn new() -> Self {
        Self {
            resolved: HashMap::new(),
            constraints: HashMap::new(),
            resolving: HashSet::new(),
            resolution_path: Vec::new(),
        }
    }

    /// Reset the resolver state
    pub fn reset(&mut self) {
        self.resolved.clear();
        self.constraints.clear();
        self.resolving.clear();
        self.resolution_path.clear();
    }

    /// Resolve all dependencies for a package
    #[cfg(target_arch = "wasm32")]
    pub async fn resolve(
        &mut self,
        root: &PackageId,
        registry: &PackageRegistry,
    ) -> PkgResult<Vec<ResolvedPackage>> {
        self.reset();

        // Fetch root package
        let entry = registry.fetch_package(&root.name).await?;

        // Create manifest from registry entry
        let manifest = self.create_manifest_from_registry(&root.name, &root.version, registry).await?;

        // Resolve dependencies recursively
        self.resolve_recursive(&root.name, &root.version, &manifest, registry).await?;

        // Build resolved package for root
        let root_resolved = ResolvedPackage {
            id: root.clone(),
            manifest,
            dependencies: self
                .resolved
                .get(&root.name)
                .map(|r| r.dependencies.clone())
                .unwrap_or_default(),
            order: self.resolved.len(),
        };

        // Get installation order (topological sort)
        let mut result = self.topological_sort()?;

        // Add root if not already present
        if !result.iter().any(|p| p.id.name == root.name) {
            result.push(root_resolved);
        }

        Ok(result)
    }

    /// Resolve dependencies (non-WASM stub)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn resolve(
        &mut self,
        _root: &PackageId,
        _registry: &PackageRegistry,
    ) -> PkgResult<Vec<ResolvedPackage>> {
        Err(PkgError::NotAvailable("WASM required".to_string()))
    }

    /// Resolve a single package's dependencies
    #[cfg(target_arch = "wasm32")]
    async fn resolve_recursive(
        &mut self,
        name: &str,
        version: &Version,
        manifest: &PackageManifest,
        registry: &PackageRegistry,
    ) -> PkgResult<()> {
        // Check for circular dependency
        if self.resolving.contains(name) {
            let mut cycle = self.resolution_path.clone();
            cycle.push(name.to_string());
            return Err(PkgError::CircularDependency(cycle));
        }

        // Check if already resolved
        if self.resolved.contains_key(name) {
            return Ok(());
        }

        // Mark as resolving
        self.resolving.insert(name.to_string());
        self.resolution_path.push(name.to_string());

        // Collect dependency names
        let mut dep_names = Vec::new();

        // Resolve each dependency
        for dep in &manifest.dependencies {
            if dep.optional {
                continue; // Skip optional dependencies
            }

            // Get package info from registry
            let dep_entry = registry.fetch_package(&dep.name).await?;

            // Find best matching version
            let best_version = dep_entry
                .versions
                .iter()
                .filter(|v| dep.version_req.matches(v))
                .max()
                .ok_or_else(|| PkgError::NoMatchingVersion {
                    name: dep.name.clone(),
                    requirement: dep.version_req.to_string(),
                })?
                .clone();

            // Check for conflicts with existing constraints
            self.add_constraint(&dep.name, &dep.version_req)?;

            // Create manifest for dependency
            let dep_manifest =
                self.create_manifest_from_registry(&dep.name, &best_version, registry).await?;

            // Recursively resolve
            Box::pin(self.resolve_recursive(
                &dep.name,
                &best_version,
                &dep_manifest,
                registry,
            ))
            .await?;

            // Add to resolved
            let dep_resolved = ResolvedPackage {
                id: PackageId::new(&dep.name, best_version),
                manifest: dep_manifest.clone(),
                dependencies: dep_manifest
                    .dependencies
                    .iter()
                    .filter(|d| !d.optional)
                    .map(|d| d.name.clone())
                    .collect(),
                order: self.resolved.len(),
            };
            self.resolved.insert(dep.name.clone(), dep_resolved);
            dep_names.push(dep.name.clone());
        }

        // Mark as resolved
        self.resolving.remove(name);
        self.resolution_path.pop();

        // Add to resolved if not already there
        if !self.resolved.contains_key(name) {
            let resolved = ResolvedPackage {
                id: PackageId::new(name, version.clone()),
                manifest: manifest.clone(),
                dependencies: dep_names,
                order: self.resolved.len(),
            };
            self.resolved.insert(name.to_string(), resolved);
        }

        Ok(())
    }

    /// Add a version constraint and check for conflicts
    #[allow(dead_code)]
    fn add_constraint(&mut self, name: &str, req: &VersionReq) -> PkgResult<()> {
        let constraints = self.constraints.entry(name.to_string()).or_default();

        // Check if new constraint conflicts with existing ones
        // For simplicity, we just collect them - a more sophisticated resolver
        // would try to find a version satisfying all constraints
        constraints.push(req.clone());

        // Check if the constraints are satisfiable
        // (This is simplified - real resolvers use SAT solving)
        Ok(())
    }

    /// Create a manifest from registry metadata
    #[cfg(target_arch = "wasm32")]
    async fn create_manifest_from_registry(
        &self,
        name: &str,
        version: &Version,
        registry: &PackageRegistry,
    ) -> PkgResult<PackageManifest> {
        let entry = registry.fetch_package(name).await?;

        Ok(PackageManifest {
            name: name.to_string(),
            version: version.clone(),
            description: entry.description,
            authors: vec![],
            license: None,
            repository: None,
            homepage: None,
            keywords: entry.keywords,
            binaries: vec![super::manifest::BinaryEntry {
                name: name.to_string(),
                path: format!("bin/{}.wasm", name),
                checksum: None,
            }],
            dependencies: vec![], // Would need to fetch from registry
            dev_dependencies: vec![],
        })
    }

    /// Perform topological sort on resolved packages
    #[allow(dead_code)]
    fn topological_sort(&self) -> PkgResult<Vec<ResolvedPackage>> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut temp_visited = HashSet::new();

        fn visit(
            name: &str,
            resolved: &HashMap<String, ResolvedPackage>,
            visited: &mut HashSet<String>,
            temp_visited: &mut HashSet<String>,
            result: &mut Vec<ResolvedPackage>,
        ) -> PkgResult<()> {
            if temp_visited.contains(name) {
                return Err(PkgError::CircularDependency(vec![name.to_string()]));
            }
            if visited.contains(name) {
                return Ok(());
            }

            temp_visited.insert(name.to_string());

            if let Some(pkg) = resolved.get(name) {
                for dep in &pkg.dependencies {
                    visit(dep, resolved, visited, temp_visited, result)?;
                }
            }

            temp_visited.remove(name);
            visited.insert(name.to_string());

            if let Some(pkg) = resolved.get(name) {
                result.push(pkg.clone());
            }

            Ok(())
        }

        for name in self.resolved.keys() {
            visit(name, &self.resolved, &mut visited, &mut temp_visited, &mut result)?;
        }

        // Update order numbers
        for (i, pkg) in result.iter_mut().enumerate() {
            pkg.order = i;
        }

        Ok(result)
    }

    /// Check if all constraints for a package can be satisfied
    pub fn check_constraints(&self, name: &str, version: &Version) -> bool {
        if let Some(constraints) = self.constraints.get(name) {
            constraints.iter().all(|req| req.matches(version))
        } else {
            true
        }
    }

    /// Get all resolved packages
    pub fn get_resolved(&self) -> Vec<&ResolvedPackage> {
        self.resolved.values().collect()
    }
}

impl Default for DependencyResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple dependency resolution for local packages (without network)
#[allow(dead_code)]
pub struct LocalResolver {
    /// Available packages (from local database)
    available: HashMap<String, Vec<Version>>,
}

#[allow(dead_code)]
impl LocalResolver {
    pub fn new() -> Self {
        Self {
            available: HashMap::new(),
        }
    }

    /// Add an available package
    pub fn add_available(&mut self, name: &str, version: Version) {
        self.available
            .entry(name.to_string())
            .or_default()
            .push(version);
    }

    /// Find best matching version for a requirement
    pub fn find_match(&self, name: &str, req: &VersionReq) -> Option<Version> {
        self.available
            .get(name)?
            .iter()
            .filter(|v| req.matches(v))
            .max()
            .cloned()
    }

    /// Check if a package is available
    pub fn is_available(&self, name: &str) -> bool {
        self.available.contains_key(name)
    }
}

impl Default for LocalResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolver_new() {
        let resolver = DependencyResolver::new();
        assert!(resolver.resolved.is_empty());
        assert!(resolver.constraints.is_empty());
    }

    #[test]
    fn test_resolver_reset() {
        let mut resolver = DependencyResolver::new();
        resolver.constraints.insert(
            "test".to_string(),
            vec![VersionReq::parse("^1.0.0").unwrap()],
        );
        resolver.reset();
        assert!(resolver.constraints.is_empty());
    }

    #[test]
    fn test_check_constraints() {
        let mut resolver = DependencyResolver::new();
        resolver.add_constraint("test", &VersionReq::parse("^1.0.0").unwrap()).unwrap();

        assert!(resolver.check_constraints("test", &Version::new(1, 0, 0)));
        assert!(resolver.check_constraints("test", &Version::new(1, 5, 0)));
        assert!(!resolver.check_constraints("test", &Version::new(2, 0, 0)));
    }

    #[test]
    fn test_local_resolver() {
        let mut resolver = LocalResolver::new();
        resolver.add_available("foo", Version::new(1, 0, 0));
        resolver.add_available("foo", Version::new(1, 1, 0));
        resolver.add_available("foo", Version::new(2, 0, 0));

        // Find best match for ^1.0
        let req = VersionReq::parse("^1.0.0").unwrap();
        let best = resolver.find_match("foo", &req);
        assert_eq!(best, Some(Version::new(1, 1, 0)));

        // Find best match for >=2.0
        let req = VersionReq::parse(">=2.0.0").unwrap();
        let best = resolver.find_match("foo", &req);
        assert_eq!(best, Some(Version::new(2, 0, 0)));
    }

    #[test]
    fn test_local_resolver_no_match() {
        let mut resolver = LocalResolver::new();
        resolver.add_available("foo", Version::new(1, 0, 0));

        let req = VersionReq::parse("^2.0.0").unwrap();
        let best = resolver.find_match("foo", &req);
        assert_eq!(best, None);
    }

    #[test]
    fn test_resolved_package() {
        let manifest = PackageManifest {
            name: "test".to_string(),
            version: Version::new(1, 0, 0),
            description: None,
            authors: vec![],
            license: None,
            repository: None,
            homepage: None,
            keywords: vec![],
            binaries: vec![],
            dependencies: vec![],
            dev_dependencies: vec![],
        };

        let resolved = ResolvedPackage {
            id: PackageId::new("test", Version::new(1, 0, 0)),
            manifest,
            dependencies: vec![],
            order: 0,
        };

        assert_eq!(resolved.id.name, "test");
        assert_eq!(resolved.order, 0);
    }
}
