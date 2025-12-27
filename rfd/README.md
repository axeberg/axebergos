# Requests for Discussion

This directory contains RFDs for axeberg.

RFDs are documents in the spirit of the IETF Request for Comments: a way to capture ideas while they're still forming, discuss them openly, and eventually converge on decisions. The philosophy is "timely rather than polished."

## Format

Each RFD lives in its own file: `NNNN-short-title.md`. The file begins with YAML metadata:

```
---
authors: Your Name <you@example.com>
state: predraft
discussion: https://github.com/axeberg/axebergos/issues/NNN
---
```

## States

- **predraft** - Work in progress, not ready for discussion
- **draft** - Ready for discussion, open for feedback
- **published** - Discussion converged, reflects reality
- **abandoned** - No longer being pursued

## RFD Index

| Number | Title | State |
|--------|-------|-------|
| [0001](0001-package-registry.md) | Package Registry | predraft |

## Contributing

Open a PR with your RFD. Discussion happens on the PR and in any linked issues.
