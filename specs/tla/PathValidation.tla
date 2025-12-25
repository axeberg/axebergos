------------------------------ MODULE PathValidation ------------------------------
(***************************************************************************)
(* TLA+ specification for VFS path validation.                             *)
(*                                                                         *)
(* This models the path validation invariants:                             *)
(*   PV1: Paths cannot contain null bytes                                  *)
(*   PV2: Total path length cannot exceed MAX_PATH_LEN (4096)              *)
(*   PV3: Individual components cannot exceed MAX_NAME_LEN (255)           *)
(***************************************************************************)

EXTENDS Integers, Sequences, FiniteSets

CONSTANTS
    MAX_PATH_LEN,       \* Maximum total path length (4096)
    MAX_NAME_LEN,       \* Maximum component name length (255)
    Chars,              \* Set of valid characters (excluding null)
    NullChar            \* The null character

ASSUME MAX_PATH_LEN = 4096
ASSUME MAX_NAME_LEN = 255
ASSUME NullChar \notin Chars

VARIABLES
    path,               \* Current path being validated
    valid,              \* Boolean: is path valid?
    error               \* Error message if invalid

-----------------------------------------------------------------------------
(* Helper: Check if path contains null *)
ContainsNull(p) ==
    \E i \in 1..Len(p) : p[i] = NullChar

(* Helper: Get path components (split by '/') *)
(* Simplified: assume components is a sequence of strings *)

(* Type invariant *)
TypeOK ==
    /\ path \in Seq(Chars \cup {NullChar, "/"})
    /\ valid \in BOOLEAN
    /\ error \in STRING

-----------------------------------------------------------------------------
(* Validation predicate *)
IsValidPath(p) ==
    /\ ~ContainsNull(p)                    \* PV1: No null bytes
    /\ Len(p) <= MAX_PATH_LEN              \* PV2: Length limit
    \* PV3 would check each component <= MAX_NAME_LEN
    \* (simplified here as we don't model component extraction)

(* Path validation action *)
ValidatePath(p) ==
    /\ path' = p
    /\ IF ~ContainsNull(p) /\ Len(p) <= MAX_PATH_LEN
       THEN /\ valid' = TRUE
            /\ error' = ""
       ELSE /\ valid' = FALSE
            /\ error' = IF ContainsNull(p)
                        THEN "path contains null byte"
                        ELSE "path too long"

-----------------------------------------------------------------------------
(* Invariants *)

(* PV1: Null byte rejection - any path with null is invalid *)
NullByteRejection ==
    ContainsNull(path) => valid = FALSE

(* PV2: Length limit - any path over MAX_PATH_LEN is invalid *)
LengthLimitEnforced ==
    Len(path) > MAX_PATH_LEN => valid = FALSE

(* Safety: Valid paths have no null and proper length *)
ValidPathsAreSafe ==
    valid = TRUE => (~ContainsNull(path) /\ Len(path) <= MAX_PATH_LEN)

=============================================================================
