# AI Development Process

This directory documents how axeberg was built using AI assistance.

## Overview

axeberg was developed entirely through conversations with Claude (Anthropic's AI assistant, Opus 4.5). This transparency serves two purposes:

1. **Learning resource**: See how AI can be used to build real software
2. **Reproducibility**: Others can adapt these patterns for their projects

## Development Philosophy

### Prompt Engineering Principles

1. **Start with clear goals**: Define what you want before asking
2. **Iterate on designs**: Let the AI propose, then refine together
3. **Test-driven development**: Request tests alongside implementation
4. **Documentation as you go**: Ask for docs with each feature

### Effective Patterns We Used

- **"Implement X, but make it simple enough to understand"** - Explicitly requesting tractability
- **"Add tests that demonstrate the expected behavior"** - Getting tests with code
- **"What are the trade-offs of this approach?"** - Leveraging AI for design analysis
- **"Make this more Unix-like"** - Using familiar paradigms as anchors

## Conversation Structure

Each major component followed a similar pattern:

```
1. High-level design discussion
2. Implementation request
3. Test request
4. Iteration/refinement
5. Documentation
```

## Prompt Categories

| Category | Purpose | Example |
|----------|---------|---------|
| [Architecture](architecture/) | System design decisions | "Design a process model for a WASM OS" |
| [Implementation](implementation/) | Writing code | "Implement pipe support in the shell" |
| [Testing](testing/) | Verification | "Add tests for signal delivery" |
| [Debugging](debugging/) | Problem solving | "This test fails with..." |
| [Documentation](documentation/) | Explaining code | "Document the syscall interface" |

## Key Prompts by Feature

### Initial Bootstrap

The project started with a prompt like:
> "Build a mini operating system in Rust that compiles to WebAssembly and runs in the browser. Make it tractable - small enough that one person can understand the entire codebase."

### Shell Implementation

> "Implement a Unix-like shell with support for:
> - Pipes (cmd1 | cmd2)
> - Redirects (>, >>, <)
> - Background jobs (&)
> - Quoting ('single' and "double")
> Make it feel familiar to Unix users."

### Multi-User System

> "Add a multi-user system with:
> - /etc/passwd and /etc/shadow
> - login/logout commands
> - Session isolation
> - sudo support"

## Lessons Learned

### What Worked Well

1. **Incremental development**: Building features one at a time
2. **Test-first approach**: Asking for tests revealed edge cases
3. **Explicit simplicity requests**: "Keep it simple" produced better code
4. **Unix as reference**: Familiar patterns reduced design debates

### What Required Iteration

1. **Async executor**: Took several attempts to get right
2. **Signal delivery**: Complex semantics needed refinement
3. **Memory management**: Balance between realism and simplicity

### Tips for Others

1. **Be specific about constraints**: "Under 500 lines" or "No external deps"
2. **Request examples**: "Show me how this would be used"
3. **Ask for alternatives**: "What other approaches could work?"
4. **Verify understanding**: "Explain what this code does"

## Directory Structure

```
prompts/
├── README.md                 # This file
├── architecture/             # Design discussions
│   ├── initial-design.md     # Original system design
│   └── kernel-structure.md   # How we structured the kernel
├── implementation/           # Feature implementation
│   ├── shell.md              # Shell implementation
│   ├── vfs.md                # Filesystem
│   └── multi-user.md         # User system
└── lessons-learned.md        # Retrospective
```

## Contributing

If you build something using AI assistance, consider documenting your process too. It helps others learn and improves the collective understanding of AI-assisted development.
