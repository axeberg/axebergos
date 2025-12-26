# Package Manager

The axeberg package manager (`pkg`) enables installation, management, and distribution of WebAssembly command modules.

## Features

- **Semantic Versioning**: Full SemVer 2.0.0 support with version constraints
- **Dependency Resolution**: Automatic dependency resolution with conflict detection
- **Package Registry**: Download packages from remote registries
- **Security**: SHA-256 checksums for integrity verification
- **Local Installation**: Install from local `.axepkg` files

## Usage

```bash
# Initialize package manager directories
pkg init

# Install a package from registry
pkg install hello
pkg install hello@1.0.0

# Install from local file
pkg install-local ./mypackage.axepkg

# Remove a package
pkg remove hello

# List installed packages
pkg list

# Show package information
pkg info hello

# Search for packages (WASM only)
pkg search json

# Update registry index (WASM only)
pkg update

# Upgrade all packages (WASM only)
pkg upgrade

# Verify installed packages
pkg verify

# Clean package cache
pkg clean
```

## Package Format

### Manifest (package.toml)

```toml
[package]
name = "hello"
version = "1.0.0"
description = "A hello world command"
authors = ["axeberg"]
license = "MIT"
repository = "https://github.com/axeberg/hello"

[[bin]]
name = "hello"
path = "bin/hello.wasm"
checksum = "2cf24dba5fb0a30e..."

[dependencies]
utils = "^1.0.0"
core = ">=2.0.0, <3.0.0"

[dev-dependencies]
test-utils = "^1.0.0"
```

### Version Requirements

| Format | Meaning |
|--------|---------|
| `1.0.0` | Exact version |
| `^1.0.0` | Compatible (same major version) |
| `~1.0.0` | Approximately equal (same minor) |
| `>=1.0.0` | Greater than or equal |
| `>1.0.0` | Greater than |
| `<2.0.0` | Less than |
| `>=1.0.0, <2.0.0` | Range |
| `*` | Any version |

### Archive Format (.axepkg)

Package archives use a simple binary format:

```
[HEADER]
AXEPKG\x00\x01       # Magic + version (8 bytes)
manifest_size: u32   # Size of manifest
num_files: u32       # Number of files

[MANIFEST]
package.toml content

[FILE ENTRIES]
For each file:
  path_len: u16      # Path length
  path: bytes        # Path (UTF-8)
  content_len: u32   # Content length
  content: bytes     # File content
```

## Directory Structure

```
/var/lib/pkg/
├── db/                    # Package database
│   ├── installed.toml     # Installed packages list
│   └── packages/          # Package metadata cache
│       └── hello-1.0.0/
│           └── package.toml
├── cache/                 # Downloaded package cache
│   └── hello-1.0.0.axepkg
└── registry/              # Registry index cache
    └── index.json

/bin/                      # Installed WASM binaries
└── hello.wasm
```

## Registry Protocol

The registry uses HTTP endpoints:

| Endpoint | Description |
|----------|-------------|
| `GET /index.json` | Full package index |
| `GET /packages/{name}.json` | Package metadata |
| `GET /packages/{name}/{version}.axepkg` | Package archive |
| `GET /search?q={query}` | Search packages |

### Index Format

```json
{
  "packages": {
    "hello": {
      "versions": ["1.0.0", "1.1.0"],
      "latest": "1.1.0"
    }
  }
}
```

### Package Metadata

```json
{
  "name": "hello",
  "versions": ["1.0.0", "1.1.0"],
  "latest": "1.1.0",
  "description": "A hello world command",
  "keywords": ["hello", "example"]
}
```

## API

```rust
use axeberg::kernel::pkg::{PackageManager, Version, VersionReq};

// Create package manager
let mut pm = PackageManager::new();
pm.init()?;

// Install a package
let id = pm.install("hello", Some("^1.0.0")).await?;

// List installed packages
for pkg in pm.list_installed()? {
    println!("{}: {}", pkg.name, pkg.version);
}

// Remove a package
pm.remove("hello")?;

// Search registry
let results = pm.search("json").await?;

// Verify integrity
let results = pm.verify()?;
```

## Implementation

The package manager consists of several modules:

| Module | Purpose |
|--------|---------|
| `version.rs` | Semantic versioning and version requirements |
| `checksum.rs` | SHA-256 checksum computation and verification |
| `manifest.rs` | Package manifest parsing (TOML-like) |
| `database.rs` | Local package database management |
| `registry.rs` | Remote registry client |
| `resolver.rs` | Dependency resolution with topological sort |
| `installer.rs` | Package extraction and installation |

## Security

- All package binaries are verified against SHA-256 checksums
- Checksums are stored in the package manifest
- The registry protocol uses HTTPS
- Packages are validated before installation

## Limitations

- Network operations require WASM build (browser environment)
- No package signing (planned for future)
- Single registry support (configurable URL)
