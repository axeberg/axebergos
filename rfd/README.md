# Requests for Discussion (RFD)

This directory contains RFDs for axeberg - design documents that describe significant changes to the project.

## What is an RFD?

An RFD is a document that describes a problem and proposes a solution. It's a way to:

1. **Think through problems** before writing code
2. **Get feedback** from the community
3. **Document decisions** for future reference

This process is inspired by [Oxide's RFD process](https://oxide.computer/blog/rfd-1-requests-for-discussion).

## RFD States

| State | Meaning |
|-------|---------|
| `ideation` | Early idea, seeking feedback |
| `discussion` | Active discussion, may change significantly |
| `published` | Accepted and ready for implementation |
| `implemented` | Implementation complete |
| `abandoned` | No longer being pursued |

## RFD Index

| Number | Title | State |
|--------|-------|-------|
| [0001](0001-package-registry.md) | Package Registry | ideation |

## Creating an RFD

1. Pick the next available number (e.g., `0002`)
2. Create `NNNN-short-title.md`
3. Use the template below
4. Submit a PR for discussion

## Template

```markdown
# RFD NNNN: Title

## Metadata

- **Authors:** Your Name
- **State:** ideation
- **Discussion:** (link to PR or issue)
- **Created:** YYYY-MM-DD

## Background

What context does the reader need?

## Problem Statement

What problem are we solving? Why does it matter?

## Proposed Solution

How do we solve it? Include diagrams, code examples, etc.

## Alternatives Considered

What else did we consider? Why did we reject it?

## Open Questions

What's still uncertain?

## Implementation Plan

How will we build this?

## References

Links to related work, prior art, etc.
```

## Discussion

RFDs are discussed via:

- GitHub Pull Requests (for document changes)
- GitHub Issues (for broader discussion)
- Comments in the RFD itself

All feedback is welcome!
