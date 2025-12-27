# RFD 0001: Package Registry for axeberg

## Metadata

- **Authors:** axeberg contributors
- **State:** ideation
- **Discussion:** (pending)
- **Created:** 2025-12-27

## Background

The axeberg package manager (`pkg`) provides a client-side implementation for installing, managing, and distributing WebAssembly command modules. However, the system currently lacks an actual registry server to host and serve packages.

The package manager client already supports:
- Semantic versioning with version constraints
- Dependency resolution with topological sort
- SHA-256 checksum verification
- Package archive format (`.axepkg`)
- HTTP-based registry protocol

What's missing is the server-side infrastructure to:
1. Host package archives
2. Serve package metadata
3. Provide search functionality
4. Enable community package publishing

## Problem Statement

Without a registry, users cannot:
- Discover available packages
- Install packages from a central repository
- Share their own packages with the community
- Benefit from a curated ecosystem of WASM commands

The registry needs to be:
- **Simple to operate** - Minimal infrastructure requirements
- **Reliable** - High availability for package downloads
- **Secure** - Protect against supply chain attacks
- **Cost-effective** - Sustainable for an open-source project
- **Decentralizable** - Support mirrors and self-hosting

## Proposed Solution

### Architecture: Static Registry

Rather than building a dynamic server with a database, we propose a **static registry** architecture. This approach:

1. Generates static JSON/binary files at publish time
2. Serves files via CDN or static hosting (GitHub Pages, S3, Cloudflare R2)
3. Requires no server-side compute
4. Scales automatically via CDN caching

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  Package Author │────>│  Publish Tool   │────>│  Static Storage │
│  (local)        │     │  (CI/CD)        │     │  (CDN)          │
└─────────────────┘     └─────────────────┘     └─────────────────┘
                                                        │
                              ┌─────────────────────────┤
                              │                         │
                              v                         v
                        ┌───────────┐            ┌───────────┐
                        │ pkg client│            │ pkg client│
                        │ (browser) │            │ (browser) │
                        └───────────┘            └───────────┘
```

### Registry Structure

```
pkg.axeberg.dev/
├── index.json                    # Full package index (compact)
├── index-full.json              # Full index with metadata
├── packages/
│   ├── hello/
│   │   ├── meta.json            # Package metadata
│   │   ├── 1.0.0.axepkg         # Version archive
│   │   ├── 1.0.0.json           # Version metadata
│   │   ├── 1.1.0.axepkg
│   │   └── 1.1.0.json
│   └── json-utils/
│       ├── meta.json
│       ├── 0.1.0.axepkg
│       └── 0.1.0.json
├── search/
│   ├── all.json                 # All packages (for client-side search)
│   └── keywords/
│       ├── json.json            # Packages tagged "json"
│       └── utils.json           # Packages tagged "utils"
└── api/
    └── v1/
        └── health.json          # Health check endpoint
```

### File Formats

#### index.json (Compact Index)

```json
{
  "version": 1,
  "updated": "2025-12-27T00:00:00Z",
  "packages": {
    "hello": {
      "latest": "1.1.0",
      "versions": ["1.0.0", "1.1.0"]
    },
    "json-utils": {
      "latest": "0.1.0",
      "versions": ["0.1.0"]
    }
  }
}
```

#### packages/{name}/meta.json

```json
{
  "name": "hello",
  "description": "A hello world command for axeberg",
  "repository": "https://github.com/axeberg/pkg-hello",
  "license": "MIT",
  "authors": ["axeberg"],
  "keywords": ["hello", "example", "tutorial"],
  "latest": "1.1.0",
  "versions": {
    "1.0.0": {
      "published": "2025-12-01T00:00:00Z",
      "checksum": "sha256:abc123...",
      "size": 1024,
      "dependencies": {}
    },
    "1.1.0": {
      "published": "2025-12-15T00:00:00Z",
      "checksum": "sha256:def456...",
      "size": 1156,
      "dependencies": {
        "core": "^1.0.0"
      }
    }
  }
}
```

#### packages/{name}/{version}.json

```json
{
  "name": "hello",
  "version": "1.1.0",
  "published": "2025-12-15T00:00:00Z",
  "checksum": "sha256:def456...",
  "size": 1156,
  "manifest": {
    "description": "A hello world command",
    "authors": ["axeberg"],
    "license": "MIT",
    "repository": "https://github.com/axeberg/pkg-hello",
    "binaries": [
      {
        "name": "hello",
        "path": "bin/hello.wasm",
        "checksum": "sha256:..."
      }
    ],
    "dependencies": {
      "core": "^1.0.0"
    }
  }
}
```

### Publishing Workflow

#### Option A: GitHub-Based (Recommended for MVP)

1. **Package Repository**: Each package lives in its own GitHub repo
2. **Release Trigger**: Author creates a GitHub release with tag `v1.0.0`
3. **CI Builds Package**: GitHub Actions builds `.axepkg` archive
4. **Registry Update**: CI pushes to registry repo, triggering index rebuild
5. **CDN Invalidation**: Registry repo deploys to GitHub Pages/Cloudflare

```yaml
# .github/workflows/publish.yml (in package repo)
name: Publish Package
on:
  release:
    types: [published]

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Build package
        run: |
          # Build WASM binaries
          cargo build --release --target wasm32-unknown-unknown
          # Create .axepkg archive
          axepkg pack --output dist/

      - name: Submit to registry
        uses: axeberg/publish-action@v1
        with:
          package: dist/*.axepkg
          registry-token: ${{ secrets.REGISTRY_TOKEN }}
```

```yaml
# .github/workflows/update-index.yml (in registry repo)
name: Update Index
on:
  push:
    paths:
      - 'packages/**'
  workflow_dispatch:

jobs:
  rebuild:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Rebuild index
        run: ./scripts/rebuild-index.sh

      - name: Deploy
        uses: peaceiris/actions-gh-pages@v3
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          publish_dir: ./public
```

#### Option B: Pull Request Based

1. Authors fork the registry repository
2. Add their package to `packages/{name}/`
3. Submit PR with package archive and metadata
4. Automated checks verify:
   - Manifest is valid
   - Checksums match
   - No malicious code patterns
   - Version doesn't already exist
5. Maintainers review and merge
6. CI rebuilds index and deploys

### Security Model

#### Package Integrity

1. **Checksum Verification**: Every package has SHA-256 checksum
2. **Manifest Checksums**: Individual binaries have checksums in manifest
3. **Reproducible Builds**: Encourage deterministic WASM compilation

#### Supply Chain Protection

1. **Immutable Versions**: Once published, versions cannot be changed
2. **Yanking**: Packages can be "yanked" (hidden) but not deleted
3. **Audit Trail**: Git history provides complete audit trail

#### Future: Package Signing (Phase 2)

```
┌─────────────────┐
│ Author Keypair  │
│ (ed25519)       │
└────────┬────────┘
         │ signs
         v
┌─────────────────┐     ┌─────────────────┐
│ package.axepkg  │────>│ package.sig     │
└─────────────────┘     └─────────────────┘

Verification:
1. Fetch author's public key from registry
2. Verify signature matches package
3. Optionally verify key is in trusted set
```

### Hosting Options

| Option | Cost | Complexity | Reliability | Latency |
|--------|------|------------|-------------|---------|
| GitHub Pages | Free | Low | High | Good |
| Cloudflare Pages | Free | Low | Very High | Excellent |
| Cloudflare R2 | ~$0.015/GB | Medium | Very High | Excellent |
| AWS S3 + CloudFront | ~$0.023/GB | Medium | Very High | Excellent |
| Self-hosted | Varies | High | Varies | Varies |

**Recommendation**: Start with GitHub Pages for simplicity, migrate to Cloudflare R2 if bandwidth becomes significant.

### Client Updates Required

The existing `PackageRegistry` client needs minor updates:

```rust
impl PackageRegistry {
    /// Fetch package metadata
    pub async fn fetch_metadata(&self, name: &str) -> PkgResult<PackageMetadata> {
        let url = format!("{}/packages/{}/meta.json", self.registry_url, name);
        // ... fetch and parse
    }

    /// Download specific version
    pub async fn download(&self, name: &str, version: &Version) -> PkgResult<Vec<u8>> {
        let url = format!(
            "{}/packages/{}/{}.axepkg",
            self.registry_url, name, version
        );
        // ... fetch binary
    }

    /// Get compact index for dependency resolution
    pub async fn fetch_index(&self) -> PkgResult<RegistryIndex> {
        let url = format!("{}/index.json", self.registry_url);
        // ... fetch and parse
    }
}
```

### Search Implementation

For a static registry, search is implemented client-side:

1. **Full Index Download**: Client downloads `search/all.json` (typically < 100KB)
2. **Local Filtering**: Filter by name, description, keywords
3. **Keyword Index**: Pre-computed keyword indices for common searches

```rust
pub async fn search(&self, query: &str) -> PkgResult<Vec<SearchResult>> {
    // Try keyword index first
    let keyword_url = format!("{}/search/keywords/{}.json", self.registry_url, query);
    if let Ok(results) = self.fetch_json(&keyword_url).await {
        return Ok(results);
    }

    // Fall back to full-text search on all.json
    let all = self.fetch_json(&format!("{}/search/all.json", self.registry_url)).await?;
    Ok(all.iter()
        .filter(|p| p.matches(query))
        .cloned()
        .collect())
}
```

### Operations

#### Initial Setup

1. Create GitHub organization `axeberg-registry`
2. Create repository `registry` with structure above
3. Configure GitHub Pages or Cloudflare Pages
4. Set up domain `pkg.axeberg.dev`
5. Create `axeberg/publish-action` for easy publishing

#### Publishing a New Package

```bash
# In package repository
$ axepkg login                    # Store token locally
$ axepkg publish                  # Build, verify, submit
Publishing hello@1.0.0...
  Building package... done
  Computing checksums... done
  Submitting to registry... done

Package hello@1.0.0 published successfully!
View at: https://pkg.axeberg.dev/packages/hello
```

#### Monitoring

- GitHub Actions logs for publish failures
- Uptime monitoring on `pkg.axeberg.dev/api/v1/health.json`
- CDN analytics for download counts

#### Incident Response

1. **Bad Package Published**:
   - Yank the version: `axepkg yank hello@1.0.0`
   - Investigate and communicate with author
   - Consider revoking author's publish token

2. **Security Vulnerability**:
   - Add advisory to package metadata
   - Notify dependent package authors
   - Coordinate fix and new release

### Migration Path

#### Phase 1: Bootstrap (MVP)

- Manual package submission via PRs
- Index rebuilt on merge
- Basic CI validation
- Host on GitHub Pages

#### Phase 2: Automation

- `axepkg publish` command
- GitHub Action for package repos
- Automated validation suite
- Add package signing

#### Phase 3: Scale

- Migrate to Cloudflare R2 for storage
- Add download statistics
- Add security scanning
- Support private registries

## Alternatives Considered

### Dynamic Server (crates.io style)

**Pros:**
- Real-time publishing
- Database queries for search
- Rate limiting, auth built-in

**Cons:**
- Requires compute infrastructure
- More complex to operate
- Higher cost
- Single point of failure

**Decision:** Static is simpler and sufficient for our scale.

### Git-Based Registry (Cargo sparse index)

**Pros:**
- Git provides versioning
- Familiar workflow
- Efficient incremental updates

**Cons:**
- Requires git client in browser (complex)
- Larger initial clone

**Decision:** JSON over HTTP is simpler for WASM environment.

### IPFS/Decentralized

**Pros:**
- Truly decentralized
- Content-addressed (immutable)
- Censorship resistant

**Cons:**
- Adds IPFS dependency
- Variable latency
- Less familiar to users

**Decision:** Consider for Phase 3, but not MVP.

## Open Questions

1. **Namespace Policy**: Should packages be globally unique or namespaced (`@user/package`)?
   - Recommendation: Start global, add namespaces if conflicts arise

2. **Verification Level**: How much automated scanning should we do?
   - Recommendation: Start with manifest validation, add WASM analysis later

3. **Governance**: Who can yank packages? Publish under any name?
   - Recommendation: Start with maintainer-only, add self-service later

4. **Breaking Changes**: How to handle packages that break the ecosystem?
   - Recommendation: Semantic versioning is the contract; educate authors

## Implementation Plan

1. **Week 1**: Create registry repository structure, implement index builder
2. **Week 2**: Set up GitHub Pages hosting, create first packages (hello, core)
3. **Week 3**: Update `pkg` client to work with new registry format
4. **Week 4**: Create `axepkg` CLI tool and publish GitHub Action
5. **Ongoing**: Accept community packages, iterate on process

## References

- [Cargo Registry](https://doc.rust-lang.org/cargo/reference/registries.html)
- [npm Registry API](https://github.com/npm/registry/blob/master/docs/REGISTRY-API.md)
- [Oxide RFD Process](https://oxide.computer/blog/rfd-1-requests-for-discussion)
- [Static Package Registries](https://research.swtch.com/vgo-module)

---

*This RFD is in the ideation state. Feedback welcome via GitHub issues.*
