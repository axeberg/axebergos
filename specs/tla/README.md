# TLA+ Specifications for Axeberg Kernel

This directory contains formal TLA+ specifications for critical kernel subsystems.
These specs allow us to verify invariants and find subtle bugs that are hard to
catch with traditional testing.

## Why TLA+?

Traditional unit tests verify specific scenarios, but can miss edge cases in
concurrent and stateful systems. TLA+ allows us to:

1. **Exhaustively explore all states** - The model checker tries all possible
   interleavings and sequences of operations
2. **Verify invariants hold globally** - Not just for specific test cases
3. **Find bugs before implementation** - Spec first, implement second
4. **Document system behavior precisely** - The spec IS the documentation

## Specifications

This directory contains 5 formal TLA+ specifications:

### ProcessStateMachine.tla

Models the process lifecycle state machine:
- States: Running, Sleeping, Stopped, Zombie
- Invariants verified:
  - P1: Only valid state transitions occur
  - P2: Zombie is terminal (finality)
  - P3: Parent-child relationships are consistent

### SignalDelivery.tla

Models the signal delivery system:
- Invariants verified:
  - S1: SIGKILL cannot be blocked
  - S2: SIGSTOP cannot be blocked
  - S3: Signals coalesce except SIGKILL (at most one pending)
  - S4: Blocked signals queue until unblocked
  - S5: Priority ordering (SIGKILL > SIGSTOP > others)

### TimerQueue.tla

Models the timer queue:
- Invariants verified:
  - T1: Monotonic ordering (fire in deadline order)
  - T2: No missed timers
  - T3: Interval timers reschedule correctly
  - T4: Cancelled timers never fire

### PathValidation.tla

Models the VFS path validation system:
- Invariants verified:
  - PV1: Paths cannot contain null bytes
  - PV2: Total path length cannot exceed MAX_PATH_LEN (4096)
  - PV3: Individual components cannot exceed MAX_NAME_LEN (255)

### HistoryBuffer.tla

Models the command history buffer with size limits:
- Invariants verified:
  - H1: History size never exceeds MAX_HISTORY_SIZE (1000)
  - H2: Oldest entries are evicted when limit reached (FIFO)
  - H3: Duplicate consecutive commands are not added

## Running the Model Checker

### Install TLC (TLA+ Model Checker)

```bash
# Option 1: VS Code extension (recommended)
# Install "TLA+ Extension" from VS Code marketplace

# Option 2: Command line
wget https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar
```

### Create a Config File

(Required - not included in repository)

For each spec, create a `.cfg` file. Example for `ProcessStateMachine.cfg`:

```
CONSTANTS
    MaxProcesses = 4
    MaxPid = 8

INIT Init
NEXT Next

INVARIANTS
    TypeOK
    InitProcessInvariant
```

### Run the Model Checker

```bash
# With tla2tools.jar
java -jar tla2tools.jar -config ProcessStateMachine.cfg ProcessStateMachine.tla

# Or use the VS Code extension's "Check Model" command
```

## Relationship to Rust Tests

The TLA+ specs and Rust tests are complementary:

| Aspect | TLA+ Specs | Rust Tests |
|--------|------------|------------|
| Scope | All possible states | Specific scenarios |
| Speed | Slow (exhaustive) | Fast |
| Bugs found | Subtle state machine bugs | Implementation bugs |
| When to run | Design phase, major changes | Every commit |

The invariants documented in `docs/development/invariants.md` are verified by:
1. **TLA+ specs** - Prove they hold for all states
2. **Rust tests** - Verify the implementation matches the spec

## Adding New Specifications

When adding kernel features:

1. Write the TLA+ spec FIRST
2. Run the model checker to verify invariants
3. Implement in Rust
4. Add Rust tests that mirror the TLA+ invariants

This "spec-first" approach catches design bugs early.

## References

- [TLA+ Home](https://lamport.azurewebsites.net/tla/tla.html)
- [Learn TLA+ (Hillel Wayne)](https://learntla.com/)
- [Oxide's use of TLA+](https://oxide.computer/blog/tla-simulation)
- [Amazon's experience with TLA+](https://lamport.azurewebsites.net/tla/amazon-excerpt.html)
