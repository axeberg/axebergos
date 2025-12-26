# Architecture Decision Records

This directory contains Architecture Decision Records (ADRs) documenting significant design decisions in axeberg.

## What is an ADR?

An ADR captures an important architectural decision along with its context and consequences. ADRs help future developers (including yourself) understand:

- Why decisions were made
- What alternatives were considered
- What trade-offs were accepted

## ADR Index

| ADR | Title | Status |
|-----|-------|--------|
| [001](001-wasm-target.md) | WebAssembly as Primary Target | Accepted |
| [002](002-async-executor.md) | Custom Async Executor | Accepted |
| [003](003-memory-filesystem.md) | In-Memory Filesystem | Accepted |
| [004](004-unix-like-interface.md) | Unix-like Interface Design | Accepted |
| [005](005-single-binary.md) | Single WASM Binary Architecture | Accepted |
| [006](006-cooperative-multitasking.md) | Cooperative Multitasking | Accepted |

## ADR Template

```markdown
# ADR-NNN: Title

## Status
[Proposed | Accepted | Deprecated | Superseded by ADR-XXX]

## Context
What is the issue that we're seeing that motivates this decision?

## Decision
What is the decision that we're making?

## Consequences
What are the positive and negative results of this decision?

## Alternatives Considered
What other options were evaluated?
```

## Contributing

When making a significant architectural decision:

1. Copy the template above
2. Fill in the sections
3. Number it sequentially
4. Add to the index above
5. Get feedback before marking as Accepted

Not every code change needs an ADR. Use them for:
- Fundamental architectural choices
- Technology selections
- Patterns that will be used project-wide
- Decisions that are hard to reverse
