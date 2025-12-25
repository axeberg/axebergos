------------------------------ MODULE HistoryBuffer ------------------------------
(***************************************************************************)
(* TLA+ specification for command history buffer with size limit.          *)
(*                                                                         *)
(* This models the history management invariants:                          *)
(*   H1: History size never exceeds MAX_HISTORY_SIZE                       *)
(*   H2: Oldest entries are evicted when limit reached                     *)
(*   H3: Duplicate consecutive commands are not added                      *)
(***************************************************************************)

EXTENDS Integers, Sequences, FiniteSets

CONSTANTS
    MAX_HISTORY_SIZE,   \* Maximum number of commands in history (1000)
    Commands            \* Set of all possible commands

ASSUME MAX_HISTORY_SIZE = 1000
ASSUME MAX_HISTORY_SIZE > 0

VARIABLES
    history,            \* Sequence of commands
    lastCommand         \* Last command executed (for duplicate detection)

-----------------------------------------------------------------------------
(* Type invariant *)
TypeOK ==
    /\ history \in Seq(Commands)
    /\ Len(history) <= MAX_HISTORY_SIZE    \* H1: Size limit
    /\ lastCommand \in Commands \cup {""}

-----------------------------------------------------------------------------
(* Initial state *)
Init ==
    /\ history = <<>>
    /\ lastCommand = ""

-----------------------------------------------------------------------------
(* Actions *)

(* Add a command to history *)
(* H2: If at limit, remove oldest first *)
(* H3: Don't add if same as last command *)
AddCommand(cmd) ==
    /\ cmd # lastCommand                   \* H3: No duplicates
    /\ IF Len(history) >= MAX_HISTORY_SIZE
       THEN history' = Append(Tail(history), cmd)  \* H2: Remove oldest, add new
       ELSE history' = Append(history, cmd)        \* Just append
    /\ lastCommand' = cmd

(* Command that duplicates last - no change to history *)
DuplicateCommand(cmd) ==
    /\ cmd = lastCommand
    /\ UNCHANGED <<history, lastCommand>>

(* Combined next-state relation *)
Next ==
    \E cmd \in Commands : AddCommand(cmd) \/ DuplicateCommand(cmd)

-----------------------------------------------------------------------------
(* Invariants *)

(* H1: History Size Bound - history never exceeds MAX_HISTORY_SIZE *)
HistorySizeBound ==
    Len(history) <= MAX_HISTORY_SIZE

(* H2: FIFO Eviction - when full, oldest is removed *)
(* This is captured by the AddCommand action structure *)

(* H3: No Consecutive Duplicates - last two entries differ *)
NoConsecutiveDuplicates ==
    Len(history) >= 2 =>
        history[Len(history)] # history[Len(history) - 1]

(* Progress: History can grow until limit *)
CanGrowToLimit ==
    Len(history) < MAX_HISTORY_SIZE =>
        \E cmd \in Commands : ENABLED AddCommand(cmd)

-----------------------------------------------------------------------------
(* Specification *)
Spec == Init /\ [][Next]_<<history, lastCommand>>

=============================================================================
