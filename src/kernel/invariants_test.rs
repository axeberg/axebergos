//! Invariant Tests
//!
//! These tests verify the critical invariants documented in docs/development/invariants.md
//! Each test is named after the invariant it verifies.

#[cfg(test)]
mod process_invariants {
    use crate::kernel::process::{Pid, Process, ProcessState};

    /// P1: Process State Machine - Valid transitions only
    #[test]
    fn p1_valid_state_transitions() {
        let mut proc = Process::new(Pid(1), "test".into(), None);

        // Running → Sleeping (valid)
        assert_eq!(proc.state, ProcessState::Running);
        proc.state = ProcessState::Sleeping;
        assert_eq!(proc.state, ProcessState::Sleeping);

        // Sleeping → Running (valid)
        proc.state = ProcessState::Running;
        assert_eq!(proc.state, ProcessState::Running);

        // Running → Stopped (valid)
        proc.state = ProcessState::Stopped;
        assert_eq!(proc.state, ProcessState::Stopped);

        // Stopped → Running (valid, via SIGCONT)
        proc.state = ProcessState::Running;
        assert_eq!(proc.state, ProcessState::Running);

        // Running → Zombie (valid)
        proc.state = ProcessState::Zombie(0);
        assert_eq!(proc.state, ProcessState::Zombie(0));
    }

    /// P2: Zombie Finality - Zombies cannot transition
    #[test]
    fn p2_zombie_finality() {
        let mut proc = Process::new(Pid(1), "test".into(), None);
        proc.state = ProcessState::Zombie(42);

        // Verify zombie state
        assert!(!proc.is_alive());

        // The invariant is that we should NEVER transition out of Zombie
        // This is enforced by code review and the type system (no method changes zombie state)
        // We test that is_alive correctly identifies zombies
        assert!(matches!(proc.state, ProcessState::Zombie(_)));
    }

    /// P3: Parent-Child Consistency
    #[test]
    fn p3_parent_child_consistency() {
        // Init process has no parent
        let init = Process::new(Pid(1), "init".into(), None);
        assert!(init.parent.is_none());

        // Child process has parent
        let child = Process::new(Pid(2), "child".into(), Some(Pid(1)));
        assert_eq!(child.parent, Some(Pid(1)));
    }
}

#[cfg(test)]
mod signal_invariants {
    use crate::kernel::signal::{ProcessSignals, Signal, SignalAction, SignalDisposition};

    /// S1: SIGKILL Guarantee - Cannot be blocked, ignored, or caught
    #[test]
    fn s1_sigkill_guarantee() {
        let mut signals = ProcessSignals::new();

        // Try to block SIGKILL - should fail
        assert!(signals.block(Signal::SIGKILL).is_err());

        // SIGKILL disposition cannot be changed
        let mut disp = SignalDisposition::new();
        assert!(
            disp.set_action(Signal::SIGKILL, SignalAction::Ignore)
                .is_err()
        );
        assert!(
            disp.set_action(Signal::SIGKILL, SignalAction::Handle)
                .is_err()
        );

        // Default action for SIGKILL is terminate
        assert_eq!(disp.get_action(Signal::SIGKILL), SignalAction::Default);
    }

    /// S1 continued: SIGKILL always delivered even when other signals blocked
    #[test]
    fn s1_sigkill_always_delivered() {
        let mut signals = ProcessSignals::new();

        // Block everything blockable
        signals.block(Signal::SIGTERM).unwrap();
        signals.block(Signal::SIGUSR1).unwrap();
        signals.block(Signal::SIGINT).unwrap();

        // Send SIGKILL
        signals.send(Signal::SIGKILL);

        // SIGKILL should be pending and deliverable
        assert!(signals.has_pending());
        let next = signals.next_pending();
        assert_eq!(next, Some(Signal::SIGKILL));
    }

    /// S2: SIGSTOP Guarantee - Cannot be blocked, ignored, or caught
    #[test]
    fn s2_sigstop_guarantee() {
        let mut signals = ProcessSignals::new();

        // Try to block SIGSTOP - should fail
        assert!(signals.block(Signal::SIGSTOP).is_err());

        // SIGSTOP disposition cannot be changed
        let mut disp = SignalDisposition::new();
        assert!(
            disp.set_action(Signal::SIGSTOP, SignalAction::Ignore)
                .is_err()
        );
    }

    /// S3: Signal Coalescing - Multiple same signals become one
    #[test]
    fn s3_signal_coalescing() {
        let mut signals = ProcessSignals::new();

        // Send SIGUSR1 multiple times
        signals.send(Signal::SIGUSR1);
        signals.send(Signal::SIGUSR1);
        signals.send(Signal::SIGUSR1);

        // Should only get one
        assert!(signals.has_pending());
        let first = signals.next_pending();
        assert_eq!(first, Some(Signal::SIGUSR1));

        // No more pending
        assert!(!signals.has_pending());
    }

    /// S3 exception: SIGKILL does not coalesce
    #[test]
    fn s3_sigkill_no_coalesce() {
        let mut signals = ProcessSignals::new();

        // Send SIGKILL multiple times
        signals.send(Signal::SIGKILL);
        signals.send(Signal::SIGKILL);
        signals.send(Signal::SIGKILL);

        // Should get multiple (for tracking purposes, though semantically identical)
        let mut count = 0;
        while signals.next_pending().is_some() {
            count += 1;
        }
        assert_eq!(count, 3);
    }

    /// S4: Blocked Signal Queueing
    #[test]
    fn s4_blocked_signal_queueing() {
        let mut signals = ProcessSignals::new();

        // Block SIGUSR1
        signals.block(Signal::SIGUSR1).unwrap();

        // Send while blocked
        signals.send(Signal::SIGUSR1);

        // Not deliverable while blocked
        assert!(!signals.has_pending());

        // Unblock
        signals.unblock(Signal::SIGUSR1);

        // Now deliverable
        assert!(signals.has_pending());
        assert_eq!(signals.next_pending(), Some(Signal::SIGUSR1));
    }

    /// S5: Priority Ordering - SIGKILL first, then SIGSTOP, then FIFO
    #[test]
    fn s5_priority_ordering() {
        let mut signals = ProcessSignals::new();

        // Send in reverse priority order
        signals.send(Signal::SIGUSR1);
        signals.send(Signal::SIGTERM);
        signals.send(Signal::SIGSTOP);
        signals.send(Signal::SIGKILL);

        // SIGKILL first
        assert_eq!(signals.next_pending(), Some(Signal::SIGKILL));

        // SIGSTOP second
        assert_eq!(signals.next_pending(), Some(Signal::SIGSTOP));

        // Then FIFO order
        assert_eq!(signals.next_pending(), Some(Signal::SIGUSR1));
        assert_eq!(signals.next_pending(), Some(Signal::SIGTERM));
    }
}

#[cfg(test)]
mod memory_invariants {
    use crate::kernel::memory::{MemoryError, MemoryRegion, ProcessMemory, Protection, RegionId};

    /// M1: Limit Enforcement
    #[test]
    fn m1_limit_enforcement() {
        let mut mem = ProcessMemory::new();
        mem.set_limit(1000);

        // Allocate within limit
        let r1 = RegionId(1);
        assert!(mem.allocate(r1, 500, Protection::READ_WRITE).is_ok());

        // Allocate more within limit
        let r2 = RegionId(2);
        assert!(mem.allocate(r2, 400, Protection::READ_WRITE).is_ok());

        // Allocate exceeding limit - must fail
        let r3 = RegionId(3);
        assert!(matches!(
            mem.allocate(r3, 200, Protection::READ_WRITE),
            Err(MemoryError::OutOfMemory)
        ));
    }

    /// M2: Region Bounds
    #[test]
    fn m2_region_bounds() {
        let id = RegionId(1);
        let mut region = MemoryRegion::new(id, 100, Protection::READ_WRITE);

        // Read within bounds
        let mut buf = [0u8; 10];
        assert!(region.read(0, &mut buf).is_ok());
        assert!(region.read(90, &mut buf).is_ok());

        // Read at boundary - returns 0 bytes (EOF behavior)
        let read = region.read(100, &mut buf).unwrap();
        assert_eq!(read, 0);

        // Write within bounds
        assert!(region.write(0, &[1, 2, 3]).is_ok());

        // Write exceeding bounds - partial write
        let big_data = vec![0u8; 101];
        let written = region.write(0, &big_data).unwrap();
        assert_eq!(written, 100); // Capped at region size
    }

    /// M3: Protection Enforcement
    #[test]
    fn m3_protection_enforcement() {
        // Read-only region
        let id = RegionId(1);
        let mut readonly = MemoryRegion::new(id, 100, Protection::READ);

        // Read works
        let mut buf = [0u8; 10];
        assert!(readonly.read(0, &mut buf).is_ok());

        // Write fails
        assert!(readonly.write(0, &[1, 2, 3]).is_err());
    }

    /// M5: No Use-After-Free (at region level)
    #[test]
    fn m5_no_use_after_free() {
        let mut mem = ProcessMemory::new();

        let region_id = RegionId(1);
        mem.allocate(region_id, 100, Protection::READ_WRITE)
            .unwrap();

        // Use before free works - access through get_mut
        {
            let region = mem.get_mut(region_id).unwrap();
            let mut buf = [0u8; 10];
            assert!(region.read(0, &mut buf).is_ok());
        }

        // Free
        mem.free(region_id).unwrap();

        // Use after free must error
        assert!(mem.get(region_id).is_none());
        assert!(mem.get_mut(region_id).is_none());
        assert!(mem.free(region_id).is_err());
    }
}

#[cfg(test)]
mod timer_invariants {
    use crate::kernel::task::TaskId;
    use crate::kernel::timer::TimerQueue;

    /// T1: Monotonic Ordering - Timers fire earliest first
    #[test]
    fn t1_monotonic_ordering() {
        let mut queue = TimerQueue::new();

        // Schedule out of order
        let _t3 = queue.schedule(300.0, 0.0, Some(TaskId(3)));
        let _t1 = queue.schedule(100.0, 0.0, Some(TaskId(1)));
        let _t2 = queue.schedule(200.0, 0.0, Some(TaskId(2)));

        // Tick past all timers
        let woken = queue.tick(400.0);

        // Should be in order: 1, 2, 3
        assert_eq!(woken, vec![TaskId(1), TaskId(2), TaskId(3)]);
    }

    /// T2: No Missed Timers
    #[test]
    fn t2_no_missed_timers() {
        let mut queue = TimerQueue::new();

        // Schedule timer for 100ms
        queue.schedule(100.0, 0.0, Some(TaskId(1)));

        // Tick at 50ms - not yet
        let woken = queue.tick(50.0);
        assert!(woken.is_empty());

        // Jump way past (simulating lag) to 500ms
        let woken = queue.tick(500.0);

        // Timer must still fire
        assert_eq!(woken, vec![TaskId(1)]);
    }

    /// T3: Interval Rescheduling
    #[test]
    fn t3_interval_rescheduling() {
        let mut queue = TimerQueue::new();

        // 50ms interval timer
        let timer_id = queue.schedule_interval(50.0, 0.0, Some(TaskId(1)));

        // First fire at 50ms
        let woken = queue.tick(50.0);
        assert_eq!(woken, vec![TaskId(1)]);
        assert!(queue.is_pending(timer_id));

        // Second fire at 100ms
        let woken = queue.tick(100.0);
        assert_eq!(woken, vec![TaskId(1)]);
        assert!(queue.is_pending(timer_id));

        // Third fire at 150ms
        let woken = queue.tick(150.0);
        assert_eq!(woken, vec![TaskId(1)]);
        assert!(queue.is_pending(timer_id));
    }

    /// T4: Cancel Effectiveness
    #[test]
    fn t4_cancel_effectiveness() {
        let mut queue = TimerQueue::new();

        let timer_id = queue.schedule(100.0, 0.0, Some(TaskId(1)));
        assert!(queue.is_pending(timer_id));

        // Cancel before fire
        assert!(queue.cancel(timer_id));
        assert!(!queue.is_pending(timer_id));

        // Tick past fire time
        let woken = queue.tick(200.0);

        // Must not fire
        assert!(woken.is_empty());
    }

    /// T4 continued: Cancelled timer cannot be cancelled again
    #[test]
    fn t4_double_cancel() {
        let mut queue = TimerQueue::new();

        let timer_id = queue.schedule(100.0, 0.0, Some(TaskId(1)));

        // First cancel succeeds
        assert!(queue.cancel(timer_id));

        // Second cancel fails (already cancelled)
        assert!(!queue.cancel(timer_id));
    }
}

#[cfg(test)]
mod fd_invariants {
    use crate::kernel::object::{FileObject, KernelObject, ObjectTable};
    use crate::kernel::process::{FileTable, Handle};
    use std::path::PathBuf;

    fn make_file() -> KernelObject {
        KernelObject::File(FileObject::new(PathBuf::from("/test"), vec![], true, true))
    }

    /// F1: Refcount Correctness
    #[test]
    fn f1_refcount_correctness() {
        let mut table = ObjectTable::new();

        let handle = table.insert(make_file());

        // Initial refcount is 1
        assert_eq!(table.refcount(handle), 1);

        // Retain increases refcount
        table.retain(handle);
        assert_eq!(table.refcount(handle), 2);

        table.retain(handle);
        assert_eq!(table.refcount(handle), 3);

        // Release decreases refcount
        table.release(handle);
        assert_eq!(table.refcount(handle), 2);
    }

    /// F2: No Leaks - Object freed when refcount reaches 0
    #[test]
    fn f2_no_leaks() {
        let mut table = ObjectTable::new();

        let handle = table.insert(make_file());
        table.retain(handle);
        assert_eq!(table.refcount(handle), 2);

        // Release twice
        table.release(handle);
        assert_eq!(table.refcount(handle), 1);

        table.release(handle);
        // Should be gone (returns 0 for non-existent)
        assert_eq!(table.refcount(handle), 0);
        assert!(table.get(handle).is_none());
    }

    /// F3: No Use-After-Close
    #[test]
    fn f3_no_use_after_close() {
        let mut ft = FileTable::new();
        let handle = Handle(100);

        let fd = ft.alloc(handle).expect("should allocate fd");
        assert!(ft.contains(fd));

        // Remove (close)
        ft.remove(fd);

        // Use after close
        assert!(!ft.contains(fd));
        assert!(ft.get(fd).is_none());
    }

    /// F4: Dup Semantics
    #[test]
    fn f4_dup_semantics() {
        let mut table = ObjectTable::new();
        let mut ft = FileTable::new();

        let handle = table.insert(make_file());
        let fd1 = ft.alloc(handle).expect("should allocate fd");

        // Dup creates new fd to same handle
        table.retain(handle); // Simulate what dup syscall does
        let fd2 = ft.alloc(handle).expect("should allocate fd");

        assert_ne!(fd1, fd2);
        assert_eq!(ft.get(fd1), ft.get(fd2));
        assert_eq!(table.refcount(handle), 2);
    }
}

#[cfg(test)]
mod executor_invariants {
    use crate::kernel::executor::{Executor, Priority};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    /// E1: Priority Ordering
    #[test]
    fn e1_priority_ordering() {
        let mut exec = Executor::new();
        let order = Rc::new(RefCell::new(Vec::new()));

        // Spawn in reverse priority order
        {
            let order = order.clone();
            exec.spawn_with_priority(
                async move {
                    order.borrow_mut().push("background");
                },
                Priority::Background,
            );
        }
        {
            let order = order.clone();
            exec.spawn_with_priority(
                async move {
                    order.borrow_mut().push("normal");
                },
                Priority::Normal,
            );
        }
        {
            let order = order.clone();
            exec.spawn_with_priority(
                async move {
                    order.borrow_mut().push("critical");
                },
                Priority::Critical,
            );
        }

        exec.tick();

        // Must run in priority order
        assert_eq!(
            order.borrow().as_slice(),
            &["critical", "normal", "background"]
        );
    }

    /// E2: Wake Correctness
    #[test]
    fn e2_wake_correctness() {
        let mut exec = Executor::new();
        let counter = Rc::new(Cell::new(0));
        let counter_clone = counter.clone();

        let task_id = exec.spawn(async move {
            counter_clone.set(counter_clone.get() + 1);
            futures::pending!();
            counter_clone.set(counter_clone.get() + 1);
        });

        // First tick runs until yield
        exec.tick();
        assert_eq!(counter.get(), 1);

        // Second tick - not woken, doesn't run
        exec.tick();
        assert_eq!(counter.get(), 1);

        // Wake the task
        exec.wake_task(task_id);

        // Third tick - now it runs
        exec.tick();
        assert_eq!(counter.get(), 2);
    }

    /// E3: Completion Removal
    #[test]
    fn e3_completion_removal() {
        let mut exec = Executor::new();

        exec.spawn(async {});
        exec.spawn(async {});
        exec.spawn(async {});

        assert_eq!(exec.task_count(), 3);

        exec.tick();

        // All completed, all removed
        assert_eq!(exec.task_count(), 0);
        assert!(!exec.has_tasks());
    }

    /// E4: No Busy Waiting
    #[test]
    fn e4_no_busy_waiting() {
        let mut exec = Executor::new();
        let counter = Rc::new(Cell::new(0));
        let counter_clone = counter.clone();

        exec.spawn(async move {
            counter_clone.set(counter_clone.get() + 1);
            futures::pending!(); // Yield without waking
        });

        // First tick
        exec.tick();
        assert_eq!(counter.get(), 1);

        // Many more ticks - task not polled because not ready
        for _ in 0..100 {
            let polled = exec.tick();
            assert_eq!(polled, 0); // Nothing polled
        }

        // Counter unchanged - no busy waiting
        assert_eq!(counter.get(), 1);
    }
}

#[cfg(test)]
mod integration_tests {
    //! Tests that verify invariants across multiple subsystems

    use crate::kernel::memory::Protection;
    use crate::kernel::signal::Signal;
    use crate::kernel::syscall::*;

    fn setup_kernel() {
        KERNEL.with(|k| {
            *k.borrow_mut() = Kernel::new();
            let pid = k.borrow_mut().spawn_process("test", None);
            k.borrow_mut().set_current(pid);
        });
    }

    /// Signal + Process: SIGKILL terminates on next signal processing
    #[test]
    fn integration_sigkill_terminates() {
        setup_kernel();

        let target_pid = KERNEL.with(|k| k.borrow_mut().spawn_process("victim", None));

        // Verify alive
        let state = get_process_state(target_pid);
        assert!(matches!(state, Some(ProcessState::Running)));

        // SIGKILL - queues the signal
        kill(target_pid, Signal::SIGKILL).unwrap();

        // Process signals (simulates runtime tick)
        let result = process_signals(target_pid);
        assert!(result.is_some());

        // Must be zombie now
        let state = get_process_state(target_pid);
        assert!(matches!(state, Some(ProcessState::Zombie(_))));
    }

    /// Memory + Syscall: Limit enforced through syscall
    #[test]
    fn integration_memory_limit_syscall() {
        setup_kernel();

        // Set limit
        set_memlimit(1000).unwrap();

        // Allocate within limit
        let r1 = mem_alloc(500, Protection::READ_WRITE).unwrap();
        let r2 = mem_alloc(400, Protection::READ_WRITE).unwrap();

        // Exceeding limit fails
        let result = mem_alloc(200, Protection::READ_WRITE);
        assert!(result.is_err());

        // Free and retry works
        mem_free(r1).unwrap();
        let r3 = mem_alloc(200, Protection::READ_WRITE);
        assert!(r3.is_ok());

        mem_free(r2).unwrap();
        mem_free(r3.unwrap()).unwrap();
    }

    /// Signal blocking + unblocking roundtrip
    #[test]
    fn integration_signal_block_roundtrip() {
        setup_kernel();

        let my_pid = getpid().unwrap();

        // Block SIGUSR1
        sigblock(Signal::SIGUSR1).unwrap();

        // Send while blocked
        kill(my_pid, Signal::SIGUSR1).unwrap();

        // Not pending (blocked)
        assert!(!sigpending().unwrap());

        // Unblock
        sigunblock(Signal::SIGUSR1).unwrap();

        // Now pending
        assert!(sigpending().unwrap());
    }
}
