---
authors: axeberg contributors
state: predraft
discussion: https://github.com/axeberg/axebergos/issues/TBD
---

# RFD 0001 Package Registry

## Introduction

The axeberg package manager exists, but the registry it talks to does not. This document proposes the design of a package registry that can serve WASM command packages to axeberg installations.

The package manager client already implements semantic versioning, dependency resolution, checksum verification, and an HTTP-based registry protocol. What remains is designing and operating the server-side infrastructure: hosting packages, serving metadata, enabling discovery, and allowing the community to publish.

## Problem Statement

Without a registry, axeberg is a closed system. Users cannot share packages. The ecosystem cannot grow. Every installation is limited to whatever commands ship with the OS itself.

We need infrastructure that:

1. Hosts package archives reliably
2. Serves package metadata efficiently
3. Resists supply chain attacks
4. Operates with minimal cost and complexity

The last point matters because axeberg is a small project. We cannot afford to run PostgreSQL clusters or pay for dedicated compute. Whatever we build must work with static hosting and free-tier services.

## Prior Art

Package registries are a solved problem, but the solutions have diverged based on scale and threat model.

**crates.io** started with a git-based index that clients cloned entirely. This worked until the index exceeded 200MB, at which point CI builds became painfully slow. They responded with the sparse index protocol (RFC 2789): clients fetch only the index files they need via HTTP, using conditional requests for cache efficiency. More recently, they adopted trusted publishing (RFC 3691), replacing long-lived API tokens with short-lived OIDC credentials from CI systems.

**npm** learned different lessons. With millions of packages came millions of attacks: typosquatting, dependency confusion, account takeovers. Their responses included scoped packages (`@scope/name`) to prevent namespace collisions, Levenshtein distance checks to catch typosquats, and mandatory 2FA for popular package maintainers. Despite this, Snyk documented 3,000+ malicious npm packages in 2024 alone.

**PyPI** pioneered trusted publishing, proving that OIDC-based authentication from GitHub Actions could replace API tokens entirely. Over 16,000 projects now use it.

The pattern is clear: start simple, get attacked, add defenses. We can skip the "get attacked" phase by learning from their mistakes.

## Design Principles

**Static over dynamic.** A registry that requires no running servers cannot go down because a server crashed. Static files on a CDN scale infinitely and cost almost nothing. The tradeoff is that publishing becomes a batch operation (commit to git, rebuild index, deploy) rather than an instant API call. This is acceptable.

**Scoped from day one.** npm added scopes after namespace conflicts became painful. We should require them immediately: `@axeberg/core`, `@user/mypkg`, `@org/internal`. This eliminates dependency confusion attacks entirely and makes ownership unambiguous.

**No long-lived secrets.** API tokens get leaked. They sit in CI configs, get committed to repos, get stolen in breaches. Trusted publishing with OIDC means the only credential is a cryptographic proof that "this request came from GitHub Actions running workflow X in repo Y." That proof is unforgeable and expires in minutes.

**Immutable versions.** Once `@axeberg/hello@1.0.0` exists, its checksum is forever. Yanking hides a version from default resolution but does not delete it. This makes builds reproducible and makes supply chain forensics possible.

## The Sparse Index Protocol

Following Cargo's design, the index is a collection of files, one per package. Each file contains one JSON object per line, one line per version:

```
{"name":"hello","vers":"1.0.0","deps":[],"cksum":"sha256:abc...","yanked":false}
{"name":"hello","vers":"1.1.0","deps":[{"name":"core","req":"^1.0","scope":"axeberg"}],"cksum":"sha256:def...","yanked":false}
```

Files are organized to keep directories small. A package named `hello` lives at `he/ll/hello`. Shorter names use simpler paths: `1/a`, `2/ab`, `3/a/abc`. Scoped packages nest under their scope: `@axeberg/co/re/core`.

Clients fetch `https://index.pkg.axeberg.dev/he/ll/hello` when resolving the `hello` package. HTTP/2 allows parallel requests. Conditional requests (`If-None-Match`, `If-Modified-Since`) avoid re-downloading unchanged files. A cold cache fetches only what it needs; a warm cache fetches almost nothing.

The root `config.json` tells clients where to find things:

```json
{
  "dl": "https://dl.pkg.axeberg.dev/{crate}/{version}.axepkg",
  "api": "https://pkg.axeberg.dev"
}
```

Downloads come from a separate CDN-optimized domain. The `{crate}` and `{version}` markers are client-substituted.

## Package Downloads

Versioned package URLs are immutable:

```
https://dl.pkg.axeberg.dev/@axeberg/hello/1.0.0.axepkg
```

The CDN caches these forever (`Cache-Control: immutable, max-age=31536000`). There is no invalidation problem because there is nothing to invalidate. A version, once published, never changes.

The index files have shorter cache times (`max-age=300, stale-while-revalidate=60`) because they do change as new versions are published. But even stale index data only means "you might not see the newest version for five minutes," which is acceptable.

## Trusted Publishing

When an author wants to publish `@octocat/hello`, their GitHub Actions workflow requests an OIDC token from GitHub. This token is a signed JWT asserting:

- Repository: `octocat/hello-pkg`
- Workflow: `.github/workflows/publish.yml`
- Ref: `refs/tags/v1.0.0`

The registry verifies this token against GitHub's public keys, then checks a trust database:

```json
{
  "@octocat/hello": {
    "trusted_publishers": [{
      "provider": "github",
      "repository": "octocat/hello-pkg",
      "workflow": "publish.yml"
    }]
  }
}
```

If the token's claims match a trusted publisher entry, the registry accepts the upload. No API tokens exist. Nothing can be stolen.

The workflow is simple:

```yaml
name: Publish
on:
  release:
    types: [published]

permissions:
  id-token: write

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: axepkg build
      - uses: axeberg/publish-action@v1
```

## Typosquatting Defenses

Before accepting a new package name, the registry checks:

1. Levenshtein distance to existing popular packages. `hello-wrold` is rejected if `hello-world` exists.
2. Homoglyph normalization. `he11o` (with digit one) is rejected if `hello` exists.
3. Scope ownership. `@octocat/*` requires proof of control over the `octocat` GitHub organization.

These checks happen at publish time. The static index contains only packages that passed validation.

## Package Signing

Checksums prove integrity but not provenance. A SHA-256 hash tells you "this file hasn't been modified" but not "this file came from the author you trust."

We use Sigstore for keyless signing. When publishing, the author's OIDC identity (their GitHub account) is bound to a signature over the package contents. This signature is recorded in Rekor, a public transparency log. Anyone can verify that a package was signed by a specific GitHub identity at a specific time.

Verification on install:

1. Compute SHA-256 of downloaded package
2. Compare to checksum in index entry
3. If signature present, verify against Sigstore and check that the signer matches a trusted publisher

Packages from unknown signers trigger a warning. Packages with invalid signatures fail to install.

## Hosting

The registry requires three components:

1. **Index storage.** Static files served via CDN. Cloudflare R2 or GitHub Pages work. Cost is negligible.

2. **Package storage.** Larger static files (the `.axepkg` archives). Same infrastructure as the index, possibly a separate bucket for operational clarity.

3. **Publish API.** A small HTTP endpoint that verifies OIDC tokens, validates packages, and commits to the index git repository. Cloudflare Workers can handle this within their free tier.

The source of truth is a git repository containing the index files. Publishing means committing a new line to a package's index file and pushing the new archive to storage. A GitHub Action rebuilds any derived artifacts and deploys to the CDN.

## Scope Ownership

How does `@myorg` prove they control `myorg`?

The simplest answer: GitHub organization membership. If you can generate an OIDC token from a repository in the `myorg` organization, you can publish to `@myorg/*`. This piggybacks on GitHub's existing access control.

For organizations not on GitHub, DNS verification works: place a TXT record at `_axepkg.myorg.com` containing a verification code. This is how npm handles it.

## User Interaction

**Installing a package:**

```
$ pkg install @axeberg/json
Resolving dependencies...
  @axeberg/json@1.2.0
  @axeberg/core@2.0.0 (already installed)
Downloading @axeberg/json@1.2.0...
Verifying signature... ok (signed by github:axeberg)
Installing...
Done.
```

**Publishing a package:**

Authors configure trusted publishing once (via a web UI or CLI that updates the trust database), then every release is automatic: tag, push, GitHub Actions handles the rest.

**Searching:**

```
$ pkg search json
@axeberg/json    1.2.0    JSON parsing and serialization
@user/json-ld    0.3.0    JSON-LD processing
```

Search happens client-side against a small metadata file. For a registry with hundreds of packages, this file is under 100KB. Client-side fuzzy matching is fast enough.

## Security Considerations

**What untrusted input enters the system?**

Package archives submitted for publishing. These are WASM modules and metadata. The registry validates checksums and manifest structure but does not execute the WASM.

**What privileges does publishing require?**

An OIDC token proving the request originates from a trusted repository/workflow combination. No persistent credentials.

**How could an attacker escalate privileges?**

If an attacker compromises a GitHub repository, they can publish malicious versions of packages owned by that repository. Sigstore signatures create an audit trail, and yanking allows rapid response, but the damage window exists. This is the same threat model as every other package registry.

**What about the registry infrastructure itself?**

The publish API runs on Cloudflare Workers with no persistent state. The index is a git repository. Compromise of either requires compromising Cloudflare or GitHub, at which point the attacker has bigger targets than our package registry.

## Open Questions

**Should we allow unscoped packages?**

Scopes add friction. Typing `@axeberg/hello` is more annoying than `hello`. But unscoped packages invite namespace conflicts and dependency confusion. The current proposal reserves a small set of unscoped names (`core`, `std`) for official use and requires scopes for everything else.

**How do we handle abandoned packages?**

If `@user/foo` is abandoned and `user` deletes their GitHub account, the package becomes unpublishable. Do we allow transferring ownership? Do we allow re-claiming abandoned scopes? This needs policy, not just technology.

**Should we support private registries?**

Enterprises want to host internal packages without publishing to the public registry. The protocol supports this (just point `config.json` at different URLs), but the tooling and documentation would need work.

## References

- Cargo Sparse Index: https://rust-lang.github.io/rfcs/2789-sparse-index.html
- Cargo Trusted Publishing: https://rust-lang.github.io/rfcs/3691-trusted-publishing-cratesio.html
- npm Threats and Mitigations: https://docs.npmjs.com/threats-and-mitigations/
- Sigstore: https://www.sigstore.dev/
- Survey on npm/PyPI Threats: https://arxiv.org/abs/2108.09576
