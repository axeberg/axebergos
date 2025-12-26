# Lessons Learned

Reflections on building axeberg with AI assistance.

## What Worked Well

### 1. Incremental Development

Building feature by feature, testing each before moving on:

```
Kernel basics → VFS → Shell → Pipes → Users → Sessions
```

Each step was small enough to verify correctness before proceeding.

### 2. Test-First Conversations

Asking for tests alongside code revealed edge cases early:

> "Implement X and add tests that cover the edge cases"

The AI would often identify edge cases I hadn't considered.

### 3. Unix as Reference Architecture

Using Unix concepts as anchors reduced ambiguity:

> "Make it work like Linux's /proc filesystem"

Clear reference points meant less back-and-forth on design.

### 4. Explicit Simplicity Requests

> "Keep it under 500 lines"
> "Make it simple enough for one person to understand"

These constraints produced cleaner code than open-ended requests.

### 5. Asking for Trade-offs

> "What are the alternatives? What are the trade-offs?"

This led to better-informed decisions and documented rationale.

## What Required Iteration

### 1. Async Executor

The custom async executor took several attempts:

- **Attempt 1**: Too simple, didn't handle nested spawns
- **Attempt 2**: Added complexity but had subtle bugs
- **Attempt 3**: Simplified with better task tracking
- **Final**: Priority-based scheduler with proper wakeup

Key lesson: Complex concurrent code needs careful iteration.

### 2. Signal Delivery

Signals seem simple but have complex semantics:

- When exactly should signals be delivered?
- What about signals to stopped processes?
- Signal coalescing for multiple same-type signals?

Required TLA+ specification to get the semantics right.

### 3. Parser Edge Cases

The shell parser went through iterations:

- Initial: Didn't handle nested quotes
- Next: Didn't handle escaped characters
- Next: Didn't handle empty strings
- Final: Proper state machine

Lesson: Parsers are deceptively complex. Start simple, test extensively.

### 4. Memory Management API

Balancing realism vs. simplicity:

- **Too simple**: Just track total bytes
- **Too complex**: Full virtual memory with pages
- **Right balance**: Per-process limits with peak tracking

## Tips for AI-Assisted Development

### Do:

1. **Be specific about constraints**
   ```
   "Under 500 lines, no external deps, must compile to WASM"
   ```

2. **Request examples**
   ```
   "Show me how this would be used in practice"
   ```

3. **Ask for alternatives**
   ```
   "What other approaches could work? What are the trade-offs?"
   ```

4. **Verify understanding**
   ```
   "Explain what this code does in your own words"
   ```

5. **Test immediately**
   ```
   "Let me run these tests... this one fails because..."
   ```

### Don't:

1. **Accept without understanding**
   - Read the generated code
   - Ask questions if unclear
   - Run the tests

2. **Request everything at once**
   - Break large features into parts
   - Verify each part before continuing

3. **Over-specify implementation**
   - Describe what you want, not how to do it
   - Let the AI propose approaches

4. **Ignore warnings**
   - If the AI says "this might be a problem...", investigate

## Quantitative Results

| Metric | Value |
|--------|-------|
| Total Rust lines | ~21,400 |
| Test count | 674 |
| Documentation lines | ~6,400 |
| Major refactors | 3 |
| Features scrapped | 2 (compositor, bare-metal boot) |
| Conversation turns | ~200 (estimate) |

## Advice for Similar Projects

### Start Small
Don't try to build a full OS. Start with:
- Kernel struct with one syscall
- One simple command
- One test

### Keep a Learning Mindset
You're not just getting code, you're learning:
- OS design patterns
- Rust idioms
- Testing strategies

### Document Decisions
Future you (or others) will want to know:
- Why this approach?
- What alternatives were considered?
- What trade-offs were made?

### Know When to Stop
The hardest part is not adding more features. Ask:
- Does this serve the core purpose?
- Is this worth the complexity?
- Can I explain this to someone in 5 minutes?

## What I'd Do Differently

1. **Start with TLA+ specs earlier** - Formal specs caught bugs that tests missed
2. **More integration tests** - Unit tests don't catch system-level issues
3. **Better error messages** - Would have helped debugging
4. **Consistent naming** - Some inconsistency crept in over time

## Conclusion

AI-assisted development is a skill. It's not about getting the AI to write code for you - it's about:

1. **Clear communication** of requirements and constraints
2. **Iterative refinement** based on testing and review
3. **Understanding** what you're building
4. **Knowing when to push back** on suggestions

The result: a tractable mini-OS that actually works, built in a fraction of the time it would take solo, but with full understanding of every component.
