# Kernel Invariants

This document defines the critical invariants that must hold for kernel correctness. These guide our testing strategy and formal verification efforts.

## Process Invariants

### P1: Process State Machine
```
Valid transitions:
  Running → Sleeping     (waiting for I/O)
  Running → Blocked(pid) (waiting for process)
  Running → Stopped      (SIGSTOP received)
  Running → Zombie(code) (exit or fatal signal)
  Sleeping → Running     (I/O complete)
  Blocked → Running      (waited process exited)
  Stopped → Running      (SIGCONT received)
  Stopped → Zombie       (SIGKILL while stopped)

Invalid transitions:
  Zombie → anything      (zombies are terminal)
  Sleeping → Zombie      (must wake first, or SIGKILL)
```

### P2: Zombie Finality
Once a process enters `Zombie(code)`, it cannot transition to any other state.

### P3: Parent-Child Consistency
If process P has parent Q, then Q must exist (or be None for init).

## Signal Invariants

### S1: SIGKILL Guarantee
SIGKILL must always terminate the target process. It cannot be:
- Blocked
- Ignored
- Caught
- Delayed indefinitely

### S2: SIGSTOP Guarantee
SIGSTOP must always stop the target process. It cannot be:
- Blocked
- Ignored
- Caught

### S3: Signal Coalescing
Multiple instances of the same signal (except SIGKILL) coalesce to one.

### S4: Blocked Signal Queueing
Blocked signals are queued (not dropped) and delivered when unblocked.

### S5: Priority Ordering
When multiple signals are pending:
1. SIGKILL is always delivered first
2. SIGSTOP is delivered second
3. Other signals in FIFO order

## Memory Invariants

### M1: Limit Enforcement
A process cannot allocate memory exceeding its limit (if set).

### M2: Region Bounds
All reads/writes must be within region bounds.

### M3: Protection Enforcement
- READ regions cannot be written
- No region can be executed (WASM limitation)

### M4: Shared Memory Consistency
After `shm_sync()`, other attached processes can see the data via `shm_refresh()`.

### M5: No Use-After-Free
Accessing a freed region must return an error, not memory corruption.

## Timer Invariants

### T1: Monotonic Ordering
Timers fire in order of their fire times (earliest first).

### T2: No Missed Timers
If time advances past a timer's fire time, it must fire on the next tick.

### T3: Interval Rescheduling
Interval timers reschedule themselves after firing.

### T4: Cancel Effectiveness
A cancelled timer never fires.

## File Descriptor Invariants

### F1: Refcount Correctness
Object refcount = number of handles pointing to it.

### F2: No Leaks
When refcount reaches 0, object is freed.

### F3: No Use-After-Close
Accessing a closed fd returns BadFd, not undefined behavior.

### F4: Dup Semantics
`dup(fd)` creates new fd pointing to same object, increments refcount.

## Executor Invariants

### E1: Priority Ordering
Within a tick, Critical tasks run before Normal, Normal before Background.

### E2: Wake Correctness
A woken task is polled on the next tick.

### E3: Completion Removal
Completed tasks are removed from the executor.

### E4: No Busy Waiting
Tasks not in ready set are not polled.

---

## Verification Strategy

### Unit Tests
Each invariant should have at least one test that:
1. Sets up the precondition
2. Performs the operation
3. Asserts the invariant holds

### Property-Based Tests
Use proptest/quickcheck for:
- Random signal sequences
- Random allocation patterns
- Random timer schedules

### TLA+ Specifications
Formal specifications in `specs/tla/`:

1. **ProcessStateMachine.tla** - Process lifecycle (P1, P2, P3)
   - Exhaustively verifies state transitions
   - Proves zombie finality
   - Checks parent-child consistency

2. **SignalDelivery.tla** - Signal system (S1-S5)
   - Verifies SIGKILL/SIGSTOP cannot be blocked
   - Proves priority ordering
   - Models signal coalescing

3. **TimerQueue.tla** - Timer queue (T1-T4)
   - Verifies monotonic ordering
   - Proves no missed timers
   - Checks cancel effectiveness

Run with TLC model checker to find bugs before implementation.
See `specs/tla/README.md` for usage instructions.

### Fuzzing
Fuzz the syscall interface with random sequences to find:
- Crashes
- Invariant violations
- Resource leaks
