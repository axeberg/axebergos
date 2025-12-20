--------------------------- MODULE ProcessStateMachine ---------------------------
(***************************************************************************)
(* TLA+ specification for the Axeberg kernel process state machine.        *)
(*                                                                         *)
(* This models the valid state transitions for a process and verifies      *)
(* critical invariants:                                                    *)
(*   P1: Only valid state transitions occur                                *)
(*   P2: Zombie is a terminal state (finality)                             *)
(*   P3: Parent-child relationships are consistent                         *)
(***************************************************************************)

EXTENDS Integers, Sequences, FiniteSets

CONSTANTS
    MaxProcesses,     \* Maximum number of processes in the system
    MaxPid            \* Maximum PID value

VARIABLES
    processes,        \* Function: Pid -> ProcessState
    nextPid,          \* Next PID to allocate
    parentOf          \* Function: Pid -> Pid (parent relationship)

-----------------------------------------------------------------------------
(* Process states *)
ProcessStates == {"Running", "Sleeping", "Stopped", "Zombie"}

(* Type invariant *)
TypeOK ==
    /\ processes \in [1..MaxPid -> ProcessStates \cup {"None"}]
    /\ nextPid \in 1..MaxPid
    /\ parentOf \in [1..MaxPid -> 0..MaxPid]  \* 0 means no parent (init)

-----------------------------------------------------------------------------
(* Valid state transitions *)
(*
   Running  -> Sleeping (process blocks on I/O or sleep)
   Running  -> Stopped  (SIGSTOP received)
   Running  -> Zombie   (process exits or SIGKILL)
   Sleeping -> Running  (I/O complete or timer fires)
   Sleeping -> Stopped  (SIGSTOP while sleeping)
   Sleeping -> Zombie   (SIGKILL while sleeping)
   Stopped  -> Running  (SIGCONT received)
   Stopped  -> Zombie   (SIGKILL while stopped)
   Zombie   -> NONE     (can only be removed, never transitions)
*)

ValidTransition(from, to) ==
    \/ from = "Running"  /\ to \in {"Sleeping", "Stopped", "Zombie"}
    \/ from = "Sleeping" /\ to \in {"Running", "Stopped", "Zombie"}
    \/ from = "Stopped"  /\ to \in {"Running", "Zombie"}
    \* Zombie can NEVER transition - this is P2 (finality)

-----------------------------------------------------------------------------
(* Initial state *)
Init ==
    /\ processes = [p \in 1..MaxPid |-> IF p = 1 THEN "Running" ELSE "None"]
    /\ nextPid = 2
    /\ parentOf = [p \in 1..MaxPid |-> 0]  \* Process 1 (init) has no parent

-----------------------------------------------------------------------------
(* Actions *)

(* Spawn a new process *)
Spawn(parent) ==
    /\ parent \in 1..MaxPid
    /\ processes[parent] = "Running"  \* Parent must be running
    /\ nextPid <= MaxPid
    /\ processes' = [processes EXCEPT ![nextPid] = "Running"]
    /\ parentOf' = [parentOf EXCEPT ![nextPid] = parent]
    /\ nextPid' = nextPid + 1

(* Process goes to sleep *)
Sleep(pid) ==
    /\ pid \in 1..MaxPid
    /\ processes[pid] = "Running"
    /\ processes' = [processes EXCEPT ![pid] = "Sleeping"]
    /\ UNCHANGED <<nextPid, parentOf>>

(* Process wakes up *)
Wake(pid) ==
    /\ pid \in 1..MaxPid
    /\ processes[pid] = "Sleeping"
    /\ processes' = [processes EXCEPT ![pid] = "Running"]
    /\ UNCHANGED <<nextPid, parentOf>>

(* Process is stopped (SIGSTOP) *)
Stop(pid) ==
    /\ pid \in 1..MaxPid
    /\ processes[pid] \in {"Running", "Sleeping"}
    /\ processes' = [processes EXCEPT ![pid] = "Stopped"]
    /\ UNCHANGED <<nextPid, parentOf>>

(* Process continues (SIGCONT) *)
Continue(pid) ==
    /\ pid \in 1..MaxPid
    /\ processes[pid] = "Stopped"
    /\ processes' = [processes EXCEPT ![pid] = "Running"]
    /\ UNCHANGED <<nextPid, parentOf>>

(* Process exits or is killed - becomes zombie *)
Exit(pid) ==
    /\ pid \in 1..MaxPid
    /\ pid # 1  \* Init (PID 1) cannot exit
    /\ processes[pid] \in {"Running", "Sleeping", "Stopped"}
    /\ processes' = [processes EXCEPT ![pid] = "Zombie"]
    /\ UNCHANGED <<nextPid, parentOf>>

(* Parent reaps zombie child *)
Reap(pid) ==
    /\ pid \in 1..MaxPid
    /\ processes[pid] = "Zombie"
    /\ processes' = [processes EXCEPT ![pid] = "None"]
    /\ UNCHANGED <<nextPid, parentOf>>

(* Combined next-state relation *)
Next ==
    \/ \E p \in 1..MaxPid : Spawn(p)
    \/ \E p \in 1..MaxPid : Sleep(p)
    \/ \E p \in 1..MaxPid : Wake(p)
    \/ \E p \in 1..MaxPid : Stop(p)
    \/ \E p \in 1..MaxPid : Continue(p)
    \/ \E p \in 1..MaxPid : Exit(p)
    \/ \E p \in 1..MaxPid : Reap(p)

-----------------------------------------------------------------------------
(* Invariants *)

(* P1: Only valid transitions - encoded in the actions themselves *)

(* P2: Zombie Finality - a zombie process never changes state except to None *)
ZombieFinality ==
    \A p \in 1..MaxPid :
        processes[p] = "Zombie" =>
            (processes'[p] = "Zombie" \/ processes'[p] = "None")

(* P3: Parent-child consistency - parent exists when child exists *)
ParentChildConsistency ==
    \A p \in 1..MaxPid :
        (processes[p] # "None" /\ parentOf[p] # 0) =>
            processes[parentOf[p]] # "None"

(* No orphan zombies - zombies should eventually be reaped *)
(* (This is a liveness property, not checked here) *)

(* Init process (PID 1) always exists and is never a zombie *)
InitProcessInvariant ==
    /\ processes[1] # "None"
    /\ processes[1] # "Zombie"

-----------------------------------------------------------------------------
(* Specification *)
Spec == Init /\ [][Next]_<<processes, nextPid, parentOf>>

(* Temporal properties *)
(* Eventually all zombies are reaped (liveness) *)
ZombiesEventuallyReaped ==
    \A p \in 1..MaxPid :
        processes[p] = "Zombie" ~> processes[p] = "None"

=============================================================================
