# RFD 0001: Package Registry for axeberg

## Metadata

- **Authors:** axeberg contributors
- **State:** ideation
- **Discussion:** (pending)
- **Created:** 2025-12-27
- **Updated:** 2025-12-27

## Background

The axeberg package manager (`pkg`) provides a client-side implementation for installing, managing, and distributing WebAssembly command modules. However, the system currently lacks an actual registry server to host and serve packages.

This RFD proposes a registry design heavily inspired by [Cargo/crates.io](https://github.com/rust-lang/crates.io), which has evolved through years of real-world use and security incidents. We adapt their proven patterns for axeberg's browser-based WASM environment.

### Prior Art Reviewed

| Registry | Lessons Learned |
|----------|-----------------|
| [crates.io](https://crates.io) | Sparse index protocol, trusted publishing, yank semantics |
| [npm](https://npmjs.com) | Scoped packages, typosquatting detection, dependency confusion |
| [PyPI](https://pypi.org) | Trusted publishing pioneer (16,000+ projects) |
| [Alexandrie](https://github.com/Hirevo/alexandrie) | Modular design, multiple storage backends |
| [cargo-http-registry](https://github.com/d-e-s-o/cargo-http-registry) | Simplicity, filesystem-first |

## Problem Statement

Without a registry, users cannot discover, install, or share packages. The registry must address:

1. **Availability** - Packages must be downloadable reliably
2. **Security** - Protection against supply chain attacks
3. **Scalability** - Handle growth without infrastructure changes
4. **Simplicity** - Minimal operational burden for maintainers

### Known Attack Vectors

Research from [Snyk](https://snyk.io/blog/malicious-packages-open-source-ecosystems/) and [academia](https://arxiv.org/abs/2108.09576) documents 3,600+ malicious packages detected in 2024 alone:

| Attack | Description | Mitigation |
|--------|-------------|------------|
| **Typosquatting** | `lod-ash` instead of `lodash` | Levenshtein distance checks, name reservation |
| **Dependency Confusion** | Public package shadows private name | Scoped namespaces from day 1 |
| **Account Takeover** | Stolen API tokens | OIDC trusted publishing, no long-lived tokens |
| **Malicious Updates** | Compromised maintainer pushes bad version | Immutable versions, signing |
| **AI Hallucination** | LLMs invent package names attackers register | Reserved name detection |

## Proposed Solution

### Architecture Overview

We adopt Cargo's **sparse index protocol** with **static hosting**:

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│ Author Repo  │────>│ GitHub OIDC  │────>│ Registry Repo│
│              │     │ (verify ID)  │     │ (git + CI)   │
└──────────────┘     └──────────────┘     └──────┬───────┘
                                                 │ deploy
                                                 v
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│ pkg client   │<────│ CDN (Fastly/ │<────│ Static Files │
│ (browser)    │     │ Cloudflare)  │     │ (R2/S3/GHP)  │
└──────────────┘     └──────────────┘     └──────────────┘
```

### Index Format (Cargo-Inspired)

Following [Cargo's sparse index RFC 2789](https://rust-lang.github.io/rfcs/2789-sparse-index.html), we use one file per package with one JSON line per version:

#### Directory Structure

Packages are organized to minimize directory size (like crates.io):

```
index/
├── config.json              # Registry configuration
├── 1/                       # 1-char names
│   └── a                    # package "a"
├── 2/                       # 2-char names
│   └── ab                   # package "ab"
├── 3/                       # 3-char names
│   └── a/
│       └── abc              # package "abc"
├── he/                      # 4+ char names: first 2 chars
│   └── ll/                  # next 2 chars
│       └── hello            # package "hello"
└── @axeberg/                # scoped packages
    └── co/
        └── re/
            └── core         # @axeberg/core
```

#### config.json

```json
{
  "dl": "https://dl.pkg.axeberg.dev/{crate}/{version}.axepkg",
  "api": "https://pkg.axeberg.dev",
  "auth-required": false
}
```

The `{crate}` and `{version}` markers are substituted by the client. This separation allows the download CDN to be different from the API/index server.

#### Package Index File

Each package file contains one JSON object per line (no array wrapper):

```
{"name":"hello","vers":"1.0.0","deps":[],"cksum":"sha256:abc...","features":{},"yanked":false}
{"name":"hello","vers":"1.1.0","deps":[{"name":"core","req":"^1.0","scope":"axeberg"}],"cksum":"sha256:def...","features":{"async":["dep:tokio"]},"yanked":false}
```

##### Index Entry Schema

```typescript
interface IndexEntry {
  // Package name (without scope)
  name: string;

  // Semantic version
  vers: string;

  // Dependencies
  deps: Dependency[];

  // SHA-256 of .axepkg file
  cksum: string;

  // Feature flags
  features: Record<string, string[]>;

  // Soft-deleted (still downloadable, not installed by default)
  yanked: boolean;

  // Minimum axeberg version required (optional)
  axeberg_version?: string;

  // Binary targets in package
  bins: string[];
}

interface Dependency {
  // Package name
  name: string;

  // Version requirement (e.g., "^1.0.0", ">=2.0,<3.0")
  req: string;

  // Scope (optional, for scoped deps like @axeberg/core)
  scope?: string;

  // Is this optional?
  optional?: boolean;

  // Required features
  features?: string[];

  // Default features enabled?
  default_features?: boolean;
}
```

### Scoped Packages (Namespaces)

Following [npm's recommendation](https://docs.npmjs.com/threats-and-mitigations/) and [Inedo's analysis](https://blog.inedo.com/npm/avoid-security-risks-in-npm-packages-with-scoping), we require scopes from day 1:

```
@axeberg/core      # Official packages
@user/mypackage    # User packages
@org/internal      # Organization packages
```

**Why mandatory scopes?**
- Prevents namespace squatting
- Eliminates dependency confusion attacks
- Clear ownership model
- Matches npm/Deno conventions

**Exception**: A curated set of "blessed" unscoped names for stdlib:
- `core`, `std`, `test` → reserved for `@axeberg/*`

### Sparse HTTP Protocol

Instead of downloading the full index, clients fetch only needed package files:

```rust
impl PackageRegistry {
    /// Fetch index entry for a package
    pub async fn fetch_index_entry(&self, scope: &str, name: &str) -> PkgResult<Vec<IndexEntry>> {
        // Compute path: @axeberg/hello -> index/@axeberg/he/ll/hello
        let path = self.index_path(scope, name);
        let url = format!("{}/{}", self.index_url, path);

        // HTTP/2 allows efficient parallel fetches
        let response = self.client.get(&url)
            .header("If-None-Match", self.cached_etag(scope, name))
            .send()
            .await?;

        match response.status() {
            304 => self.cached_entries(scope, name),  // Not modified
            200 => self.parse_and_cache(scope, name, response),
            404 => Ok(vec![]),  // Package doesn't exist
            _ => Err(PkgError::RegistryError),
        }
    }
}
```

**Key benefits** (from [Cargo's experience](https://blog.rust-lang.org/inside-rust/2023/01/30/cargo-sparse-protocol.html)):
- First fetch: download only what you need (not 215MB git clone)
- Subsequent fetches: HTTP conditional requests (ETag/If-Modified-Since)
- HTTP/2 pipelining for parallel dependency resolution

### Package Archives

Downloads are served from a separate CDN-optimized endpoint:

```
https://dl.pkg.axeberg.dev/@axeberg/hello/1.0.0.axepkg
```

**Caching Strategy** (based on [CDN best practices](https://systemdesignschool.io/fundamentals/cdn-cache-invalidation)):

| Resource | Cache-Control | Why |
|----------|---------------|-----|
| `/{pkg}/{version}.axepkg` | `immutable, max-age=31536000` | Versions never change |
| `/index/{path}` | `max-age=300, stale-while-revalidate=60` | Index updates, but not critical |
| `/config.json` | `max-age=3600` | Rarely changes |

Versioned URLs are **immutable** - once `hello@1.0.0` is published, that exact checksum is forever. This sidesteps CDN invalidation complexity entirely.

### Authentication: Trusted Publishing

Following [crates.io's implementation](https://crates.io/docs/trusted-publishing) (via [RFC 3691](https://rust-lang.github.io/rfcs/3691-trusted-publishing-cratesio.html)) and [PyPI's pioneering work](https://docs.pypi.org/trusted-publishers/), we use **OIDC trusted publishing** instead of long-lived API tokens:

```yaml
# .github/workflows/publish.yml
name: Publish to axeberg registry
on:
  release:
    types: [published]

permissions:
  id-token: write  # Required for OIDC

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Build package
        run: axepkg build

      - uses: axeberg/publish-action@v1
        # No secrets needed! OIDC proves identity
```

**How it works:**

1. GitHub Actions generates an OIDC token proving:
   - Repository: `octocat/hello-pkg`
   - Workflow: `publish.yml`
   - Environment: `production` (optional)

2. Registry verifies token against GitHub's JWKS endpoint

3. Registry checks if `octocat/hello-pkg` is authorized to publish `@octocat/hello`

4. If valid, issues 30-minute scoped token for this publish only

**Trust Configuration** (stored in registry):

```json
{
  "@octocat/hello": {
    "trusted_publishers": [{
      "provider": "github",
      "repository": "octocat/hello-pkg",
      "workflow": "publish.yml",
      "environment": "production"
    }]
  }
}
```

### Typosquatting Protection

Before accepting new package names, we check:

1. **Levenshtein Distance**: Reject names within edit distance 2 of popular packages
2. **Homoglyph Detection**: `he11o` vs `hello` (1 vs l, 0 vs o)
3. **Reserved Prefixes**: `axeberg-*`, `stdlib-*`, etc.
4. **Scope Verification**: `@octocat/*` requires GitHub org membership

```rust
fn validate_package_name(name: &str, scope: &str) -> Result<(), ValidationError> {
    // Check against top 1000 packages
    for popular in POPULAR_PACKAGES {
        if levenshtein(name, popular) <= 2 && name != popular {
            return Err(ValidationError::TooSimilar(popular.to_string()));
        }
    }

    // Check homoglyphs
    let normalized = name
        .replace('1', "l")
        .replace('0', "o")
        .replace('_', "-");
    if normalized != name && package_exists(&normalized, scope) {
        return Err(ValidationError::Homoglyph(normalized));
    }

    Ok(())
}
```

### Package Signing (Phase 1, not Phase 2)

Given the [severity of supply chain attacks](https://vulert.com/blog/npm-supply-chain-attack-20-packages-compromised/), signing is essential from launch:

We use [Sigstore](https://www.sigstore.dev/) for keyless signing:

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│ Author       │────>│ Sigstore     │────>│ Rekor        │
│ (GitHub ID)  │     │ (sign w/OIDC)│     │ (transparency│
└──────────────┘     └──────────────┘     │  log)        │
                                          └──────────────┘
```

**Why Sigstore?**
- No key management for authors
- Identity tied to GitHub/GitLab account
- Public transparency log (Rekor) for audit
- Same approach [npm is adopting](https://github.blog/security/supply-chain-security/introducing-npm-package-provenance/)

**Verification on install:**

```rust
pub async fn verify_package(pkg: &[u8], entry: &IndexEntry) -> PkgResult<()> {
    // 1. Verify checksum
    let actual_checksum = sha256(pkg);
    if actual_checksum != entry.cksum {
        return Err(PkgError::ChecksumMismatch);
    }

    // 2. Verify Sigstore signature (if present)
    if let Some(sig) = &entry.signature {
        let bundle = sigstore::verify(pkg, sig).await?;

        // Check signer identity matches trusted publisher
        if !is_trusted_identity(&bundle.signer, &entry.name) {
            return Err(PkgError::UntrustedSigner);
        }
    }

    Ok(())
}
```

### Registry Operations

#### Publishing Flow

```
1. Author: git tag v1.0.0 && git push --tags
           │
           v
2. GitHub Actions: Build, test, create .axepkg
           │
           v
3. publish-action: Request OIDC token from GitHub
           │
           v
4. Registry API: Verify OIDC token, check trust config
           │
           v
5. Registry API: Validate package (name, checksum, manifest)
           │
           v
6. Registry API: Sign with Sigstore, upload to storage
           │
           v
7. Registry API: Update index file, commit to git
           │
           v
8. GitHub Pages/Cloudflare: Deploy updated index
```

#### Yanking

Yanking marks a version as "do not install by default" but keeps it downloadable for reproducibility:

```bash
$ axepkg yank @myorg/broken@1.0.0 --reason "Security vulnerability CVE-2025-1234"
```

```json
{"name":"broken","vers":"1.0.0",...,"yanked":true,"yank_reason":"Security vulnerability CVE-2025-1234"}
```

Clients show warnings but still allow explicit install of yanked versions.

#### Auditing

Every change to the registry is a git commit:

```
commit abc123
Author: trusted-publisher-bot
Date:   2025-12-27

    Publish @octocat/hello@1.0.0

    Publisher: github:octocat/hello-pkg@refs/heads/main
    Workflow: .github/workflows/publish.yml
    Signature: sigstore:rekor.sigstore.dev/entry/abc123
```

### Client Implementation

Update `src/kernel/pkg/registry.rs`:

```rust
pub struct PackageRegistry {
    index_url: String,      // https://index.pkg.axeberg.dev
    download_url: String,   // https://dl.pkg.axeberg.dev
    api_url: String,        // https://pkg.axeberg.dev
    cache: IndexCache,
    http_client: HttpClient,
}

impl PackageRegistry {
    /// Resolve dependencies using sparse index
    pub async fn resolve(&self, root: &PackageId) -> PkgResult<Vec<ResolvedPackage>> {
        let mut to_fetch = vec![root.clone()];
        let mut resolved = HashMap::new();

        while let Some(pkg) = to_fetch.pop() {
            if resolved.contains_key(&pkg) {
                continue;
            }

            // Fetch index entry (HTTP/2 parallel)
            let entries = self.fetch_index_entry(&pkg.scope, &pkg.name).await?;

            // Find matching version
            let entry = self.select_version(&entries, &pkg.version_req)?;

            // Queue dependencies
            for dep in &entry.deps {
                to_fetch.push(PackageId::from_dep(dep));
            }

            resolved.insert(pkg, entry);
        }

        Ok(self.topological_sort(resolved)?)
    }

    /// Download and verify package
    pub async fn download(&self, pkg: &ResolvedPackage) -> PkgResult<Vec<u8>> {
        let url = format!(
            "{}/{}/{}/{}.axepkg",
            self.download_url, pkg.scope, pkg.name, pkg.version
        );

        let bytes = self.http_client.get(&url).send().await?.bytes()?;

        // Verify checksum
        let checksum = sha256(&bytes);
        if checksum != pkg.cksum {
            return Err(PkgError::ChecksumMismatch);
        }

        // Verify signature
        self.verify_signature(&bytes, pkg).await?;

        Ok(bytes)
    }
}
```

### Hosting Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    pkg.axeberg.dev                          │
├─────────────────────────────────────────────────────────────┤
│  Cloudflare (DNS + CDN + WAF)                               │
│  ├── index.pkg.axeberg.dev → R2 bucket (index files)        │
│  ├── dl.pkg.axeberg.dev → R2 bucket (packages)              │
│  └── pkg.axeberg.dev → Workers (API for publish/search)     │
└─────────────────────────────────────────────────────────────┘
                           │
                           v
┌─────────────────────────────────────────────────────────────┐
│  GitHub                                                      │
│  ├── axeberg/registry (index git repo, source of truth)     │
│  ├── axeberg/registry-api (Workers source)                  │
│  └── GitHub Actions (index rebuild on push)                 │
└─────────────────────────────────────────────────────────────┘
```

**Cost Estimate** (based on [Cloudflare R2 pricing](https://www.cloudflare.com/products/r2/)):
- Storage: $0.015/GB/month
- Operations: Free for reads from Workers
- Egress: Free (Cloudflare's key advantage)
- Workers: Free tier covers most use cases

### Search

Since index files are per-package, we maintain a separate search index:

```
search/
├── all.json           # All packages (< 1MB typically)
├── popular.json       # Top 100 by downloads
└── recent.json        # Last 50 published
```

Client-side search with fuzzy matching:

```rust
pub async fn search(&self, query: &str) -> PkgResult<Vec<SearchResult>> {
    let index = self.fetch_search_index().await?;

    let mut results: Vec<_> = index.packages
        .iter()
        .filter_map(|pkg| {
            let score = fuzzy_score(query, &pkg.name, &pkg.description);
            if score > 0.3 {
                Some((pkg.clone(), score))
            } else {
                None
            }
        })
        .collect();

    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    Ok(results.into_iter().map(|(pkg, _)| pkg).take(20).collect())
}
```

## Alternatives Considered

### Full crates.io Clone

**Pros**: Battle-tested, feature-complete
**Cons**: PostgreSQL + Heroku + significant ops burden
**Decision**: Too complex for our scale; static hosting is sufficient

### Git-Based Index (Original Cargo)

**Pros**: Atomic updates, familiar
**Cons**: 215MB+ index, requires git in browser
**Decision**: Sparse HTTP is strictly better for our use case

### npm-Style Dynamic API

**Pros**: Real-time, powerful queries
**Cons**: Requires always-on server, higher latency
**Decision**: Static + client-side search is sufficient

## Open Questions

1. **Scope Claim Process**: How does `@myorg` prove they own `myorg`?
   - Option A: GitHub org membership (automated)
   - Option B: DNS TXT record (like npm)
   - Option C: Manual review (doesn't scale)
   - **Recommendation**: Start with GitHub org, add DNS later

2. **Private Registries**: Should we support self-hosted registries?
   - Config similar to Cargo's `[registries]` in `.cargo/config.toml`
   - **Recommendation**: Yes, essential for enterprise adoption

3. **Mirroring**: How do we handle regional mirrors?
   - Cloudflare's global network may be sufficient initially
   - **Recommendation**: Design for it, implement if needed

## Implementation Plan

### Phase 1: Foundation (Weeks 1-2)

- [ ] Create `axeberg/registry` repo with index structure
- [ ] Implement index builder script (Rust CLI)
- [ ] Deploy to Cloudflare R2 + Pages
- [ ] Update `pkg` client for sparse protocol
- [ ] Publish `@axeberg/core`, `@axeberg/hello` as seed packages

### Phase 2: Authentication (Weeks 3-4)

- [ ] Implement OIDC verification endpoint (Cloudflare Workers)
- [ ] Create `axeberg/publish-action` GitHub Action
- [ ] Add trust configuration management
- [ ] Integrate Sigstore signing

### Phase 3: Protection (Weeks 5-6)

- [ ] Implement typosquatting checks
- [ ] Add package name validation rules
- [ ] Create moderation tooling for yanking
- [ ] Set up security advisory system

### Phase 4: Polish (Weeks 7-8)

- [ ] Build web UI for browsing packages
- [ ] Add download statistics
- [ ] Implement private registry support
- [ ] Documentation and tutorials

## References

### Registry Design
- [Cargo Registry Index](https://doc.rust-lang.org/cargo/reference/registry-index.html)
- [Cargo Sparse Index RFC 2789](https://rust-lang.github.io/rfcs/2789-sparse-index.html)
- [crates.io Architecture](https://github.com/rust-lang/crates.io/blob/main/docs/ARCHITECTURE.md)
- [Alexandrie Registry](https://github.com/Hirevo/alexandrie)

### Security
- [crates.io Trusted Publishing RFC 3691](https://rust-lang.github.io/rfcs/3691-trusted-publishing-cratesio.html)
- [PyPI Trusted Publishers](https://docs.pypi.org/trusted-publishers/)
- [npm Threats and Mitigations](https://docs.npmjs.com/threats-and-mitigations/)
- [Survey on npm/PyPI Threats](https://arxiv.org/abs/2108.09576)
- [Sigstore](https://www.sigstore.dev/)

### Caching
- [CDN Cache Invalidation Strategies](https://systemdesignschool.io/fundamentals/cdn-cache-invalidation)
- [Cloudflare Cache Best Practices](https://developers.cloudflare.com/cache/)

### Supply Chain Security
- [OSSF Malicious Packages Database](https://github.com/ossf/malicious-packages)
- [Snyk Malicious Package Report](https://snyk.io/blog/malicious-packages-open-source-ecosystems/)
- [Dependency Confusion Attacks](https://www.aquasec.com/cloud-native-academy/supply-chain-security/dependency-confusion/)

---

*This RFD is in the ideation state. Feedback welcome via GitHub issues.*
