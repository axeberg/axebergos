------------------------------ MODULE SignalDelivery ------------------------------
(***************************************************************************)
(* TLA+ specification for Axeberg kernel signal delivery.                  *)
(*                                                                         *)
(* This models the signal system and verifies critical invariants:         *)
(*   S1: SIGKILL cannot be blocked, ignored, or caught                     *)
(*   S2: SIGSTOP cannot be blocked, ignored, or caught                     *)
(*   S3: Signals coalesce (except SIGKILL)                                 *)
(*   S4: Blocked signals are queued until unblocked                        *)
(*   S5: Priority ordering (SIGKILL > SIGSTOP > others)                    *)
(***************************************************************************)

EXTENDS Integers, Sequences, FiniteSets

CONSTANTS
    Signals,          \* Set of all signals
    SIGKILL,          \* The SIGKILL signal (cannot be blocked)
    SIGSTOP,          \* The SIGSTOP signal (cannot be blocked)
    SIGCONT           \* The SIGCONT signal

ASSUME SIGKILL \in Signals
ASSUME SIGSTOP \in Signals
ASSUME SIGCONT \in Signals

VARIABLES
    pending,          \* Set of pending signals
    blocked,          \* Set of blocked signals
    delivered,        \* Sequence of delivered signals (for checking order)
    processState      \* Current process state: "Running", "Stopped", "Zombie"

-----------------------------------------------------------------------------
(* Unblockable signals *)
UnblockableSignals == {SIGKILL, SIGSTOP}
BlockableSignals == Signals \ UnblockableSignals

(* Signal priorities for delivery order *)
Priority(sig) ==
    IF sig = SIGKILL THEN 0      \* Highest priority
    ELSE IF sig = SIGSTOP THEN 1
    ELSE 2                        \* All others equal

(* Type invariant *)
TypeOK ==
    /\ pending \subseteq Signals
    /\ blocked \subseteq BlockableSignals  \* S1 & S2: cannot block SIGKILL/SIGSTOP
    /\ delivered \in Seq(Signals)
    /\ processState \in {"Running", "Stopped", "Zombie"}

-----------------------------------------------------------------------------
(* Initial state *)
Init ==
    /\ pending = {}
    /\ blocked = {}
    /\ delivered = <<>>
    /\ processState = "Running"

-----------------------------------------------------------------------------
(* Actions *)

(* Send a signal to the process *)
Send(sig) ==
    /\ processState # "Zombie"    \* Cannot signal zombies
    /\ pending' = pending \cup {sig}  \* S3: Set semantics = coalescing
    /\ UNCHANGED <<blocked, delivered, processState>>

(* Block a signal (only blockable signals can be blocked) *)
Block(sig) ==
    /\ sig \in BlockableSignals   \* S1 & S2: SIGKILL/SIGSTOP cannot be blocked
    /\ blocked' = blocked \cup {sig}
    /\ UNCHANGED <<pending, delivered, processState>>

(* Unblock a signal *)
Unblock(sig) ==
    /\ sig \in blocked
    /\ blocked' = blocked \ {sig}
    /\ UNCHANGED <<pending, delivered, processState>>

(* Deliver a signal (process it) *)
(* S5: Must deliver in priority order *)
Deliver ==
    /\ pending # {}
    /\ processState # "Zombie"
    /\ LET deliverable == pending \ blocked
       IN /\ deliverable # {}
          /\ LET sig == CHOOSE s \in deliverable :
                          \A t \in deliverable : Priority(s) <= Priority(t)
             IN /\ pending' = pending \ {sig}
                /\ delivered' = Append(delivered, sig)
                /\ processState' =
                     IF sig = SIGKILL THEN "Zombie"
                     ELSE IF sig = SIGSTOP THEN "Stopped"
                     ELSE IF sig = SIGCONT /\ processState = "Stopped" THEN "Running"
                     ELSE processState
                /\ UNCHANGED blocked

(* Combined next-state relation *)
Next ==
    \/ \E s \in Signals : Send(s)
    \/ \E s \in Signals : Block(s)
    \/ \E s \in Signals : Unblock(s)
    \/ Deliver

-----------------------------------------------------------------------------
(* Invariants *)

(* S1: SIGKILL Guarantee - SIGKILL is never blocked *)
SigkillNeverBlocked ==
    SIGKILL \notin blocked

(* S2: SIGSTOP Guarantee - SIGSTOP is never blocked *)
SigstopNeverBlocked ==
    SIGSTOP \notin blocked

(* S3: Signal Coalescing - at most one of each signal pending *)
(* This is automatic from using a set for pending *)

(* S4: Blocked signals stay pending until unblocked *)
BlockedSignalsQueued ==
    \A s \in Signals :
        (s \in pending /\ s \in blocked) => s \in pending'

(* S5: Priority ordering - check delivered sequence *)
(* SIGKILL delivered before other signals in same batch *)
PriorityOrdering ==
    \A i, j \in 1..Len(delivered) :
        i < j => Priority(delivered[i]) <= Priority(delivered[j])

(* SIGKILL eventually terminates - if SIGKILL is pending, process becomes zombie *)
SigkillTerminates ==
    (SIGKILL \in pending /\ processState # "Zombie") =>
        <>(processState = "Zombie")

(* SIGSTOP eventually stops - if SIGSTOP is pending and not zombie *)
SigstopStops ==
    (SIGSTOP \in pending /\ processState = "Running") =>
        <>(processState \in {"Stopped", "Zombie"})

-----------------------------------------------------------------------------
(* Fairness - signals are eventually delivered *)
Fairness ==
    /\ WF_<<pending, blocked, delivered, processState>>(Deliver)

(* Specification *)
Spec == Init /\ [][Next]_<<pending, blocked, delivered, processState>> /\ Fairness

=============================================================================
