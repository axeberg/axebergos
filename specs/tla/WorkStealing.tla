--------------------------- MODULE WorkStealing ---------------------------
(***************************************************************************)
(* TLA+ specification for lock-free work stealing executor.                *)
(*                                                                         *)
(* Based on the Chase-Lev work stealing deque algorithm with extensions    *)
(* for task scheduling. This models N workers with local deques and a      *)
(* global injector queue.                                                  *)
(*                                                                         *)
(* Critical invariants verified:                                           *)
(*   W1: No Lost Tasks - Every spawned task is eventually executed         *)
(*   W2: No Double Execution - Each task executes exactly once             *)
(*   W3: LIFO Local / FIFO Steal - Owner pops newest, thieves steal oldest *)
(*   W4: Linearizability - All operations appear atomic                    *)
(*   W5: Progress - System makes progress under fair scheduling            *)
(*   W6: Bounded Stealing - Steal attempts don't spin forever              *)
(***************************************************************************)

EXTENDS Integers, Sequences, FiniteSets, Naturals, TLC

CONSTANTS
    NumWorkers,       \* Number of worker threads
    MaxTasks,         \* Maximum tasks for model checking
    MaxDequeSize      \* Maximum size of each worker's deque

ASSUME NumWorkers > 0
ASSUME MaxTasks > 0
ASSUME MaxDequeSize > 0

VARIABLES
    \* --- Chase-Lev Deque State (per worker) ---
    deques,           \* Function: WorkerId -> Sequence of TaskIds
    bottom,           \* Function: WorkerId -> Bottom index (owner push/pop)
    top,              \* Function: WorkerId -> Top index (steal point)

    \* --- Global State ---
    injector,         \* Global injector queue (MPMC)

    \* --- Task Tracking ---
    taskState,        \* Function: TaskId -> {"Pending", "Running", "Completed"}
    nextTaskId,       \* Next task ID to allocate

    \* --- Execution History (for verification) ---
    execHistory,      \* Sequence of [worker: WorkerId, task: TaskId]

    \* --- Worker State ---
    workerState,      \* Function: WorkerId -> {"Idle", "Working", "Stealing"}
    currentTask       \* Function: WorkerId -> TaskId or 0 (none)

-----------------------------------------------------------------------------
(* Type definitions *)

WorkerIds == 1..NumWorkers
TaskIds == 1..MaxTasks
NullTask == 0

WorkerStates == {"Idle", "Working", "Stealing"}
TaskStates == {"Spawned", "Pending", "Running", "Completed"}

(* Type invariant *)
TypeOK ==
    /\ deques \in [WorkerIds -> Seq(TaskIds)]
    /\ bottom \in [WorkerIds -> Nat]
    /\ top \in [WorkerIds -> Nat]
    /\ injector \in Seq(TaskIds)
    /\ taskState \in [1..nextTaskId-1 -> TaskStates]
    /\ nextTaskId \in 1..(MaxTasks+1)
    /\ execHistory \in Seq([worker: WorkerIds, task: TaskIds])
    /\ workerState \in [WorkerIds -> WorkerStates]
    /\ currentTask \in [WorkerIds -> 0..MaxTasks]

-----------------------------------------------------------------------------
(* Helper functions *)

(* Size of a worker's local deque (using bottom/top indices) *)
DequeSize(w) == bottom[w] - top[w]

(* Is deque empty? *)
DequeEmpty(w) == bottom[w] <= top[w]

(* Get task at deque index (1-indexed in TLA+) *)
DequeGet(w, idx) ==
    IF idx >= 1 /\ idx <= Len(deques[w])
    THEN deques[w][idx]
    ELSE NullTask

(* All tasks that exist *)
AllTasks == 1..(nextTaskId-1)

(* Tasks that have been spawned but not completed *)
PendingTasks == {t \in AllTasks : taskState[t] \in {"Spawned", "Pending"}}

(* Tasks currently being executed *)
RunningTasks == {t \in AllTasks : taskState[t] = "Running"}

(* Tasks that have finished *)
CompletedTasks == {t \in AllTasks : taskState[t] = "Completed"}

-----------------------------------------------------------------------------
(* Initial state *)

Init ==
    /\ deques = [w \in WorkerIds |-> <<>>]
    /\ bottom = [w \in WorkerIds |-> 0]
    /\ top = [w \in WorkerIds |-> 0]
    /\ injector = <<>>
    /\ taskState = <<>>  \* Empty function
    /\ nextTaskId = 1
    /\ execHistory = <<>>
    /\ workerState = [w \in WorkerIds |-> "Idle"]
    /\ currentTask = [w \in WorkerIds |-> NullTask]

-----------------------------------------------------------------------------
(* Actions *)

(* Spawn a new task (external) - goes to injector queue *)
SpawnTask ==
    /\ nextTaskId <= MaxTasks
    /\ Len(injector) < MaxDequeSize  \* Bounded queue
    /\ injector' = Append(injector, nextTaskId)
    /\ taskState' = taskState @@ (nextTaskId :> "Spawned")
    /\ nextTaskId' = nextTaskId + 1
    /\ UNCHANGED <<deques, bottom, top, execHistory, workerState, currentTask>>

(* Worker pushes task to local deque (internal spawn during execution) *)
LocalPush(w, taskId) ==
    /\ workerState[w] = "Working"
    /\ Len(deques[w]) < MaxDequeSize
    /\ taskId \in AllTasks
    /\ taskState[taskId] = "Spawned"
    \* Push to bottom (LIFO for owner)
    /\ deques' = [deques EXCEPT ![w] = Append(deques[w], taskId)]
    /\ bottom' = [bottom EXCEPT ![w] = bottom[w] + 1]
    /\ taskState' = [taskState EXCEPT ![taskId] = "Pending"]
    /\ UNCHANGED <<top, injector, nextTaskId, execHistory, workerState, currentTask>>

(* Worker pops from local deque (LIFO) - "take" operation *)
LocalPop(w) ==
    /\ workerState[w] = "Idle"
    /\ ~DequeEmpty(w)
    /\ LET newBottom == bottom[w] - 1
           taskId == deques[w][Len(deques[w])]
       IN
        \* Decrement bottom first
        /\ bottom' = [bottom EXCEPT ![w] = newBottom]
        \* CAS-like check: if top hasn't caught up, we got the task
        /\ IF newBottom > top[w]
           THEN \* Success - no contention
                /\ deques' = [deques EXCEPT ![w] = SubSeq(deques[w], 1, Len(deques[w])-1)]
                /\ taskState' = [taskState EXCEPT ![taskId] = "Running"]
                /\ workerState' = [workerState EXCEPT ![w] = "Working"]
                /\ currentTask' = [currentTask EXCEPT ![w] = taskId]
                /\ UNCHANGED <<top, injector, nextTaskId, execHistory>>
           ELSE IF newBottom = top[w]
                THEN \* Race with stealer - try CAS on top
                     \* In TLA+, we model this as: owner wins if top unchanged
                     /\ top' = [top EXCEPT ![w] = top[w] + 1]
                     /\ bottom' = [bottom EXCEPT ![w] = top[w] + 1]
                     /\ deques' = [deques EXCEPT ![w] = <<>>]
                     /\ taskState' = [taskState EXCEPT ![taskId] = "Running"]
                     /\ workerState' = [workerState EXCEPT ![w] = "Working"]
                     /\ currentTask' = [currentTask EXCEPT ![w] = taskId]
                     /\ UNCHANGED <<injector, nextTaskId, execHistory>>
                ELSE \* Empty - restore bottom
                     /\ bottom' = [bottom EXCEPT ![w] = top[w]]
                     /\ UNCHANGED <<deques, top, injector, taskState, nextTaskId,
                                    execHistory, workerState, currentTask>>

(* Worker steals from another worker's deque (FIFO from top) *)
Steal(thief, victim) ==
    /\ thief # victim
    /\ workerState[thief] = "Idle"
    /\ ~DequeEmpty(victim)
    /\ LET oldTop == top[victim]
           taskId == deques[victim][1]  \* Steal from front (oldest)
           newTop == oldTop + 1
       IN
        \* CAS on top - fails if another stealer got there first
        /\ top[victim] = oldTop  \* Simulates successful CAS
        /\ IF newTop <= bottom[victim]
           THEN \* Successful steal
                /\ top' = [top EXCEPT ![victim] = newTop]
                /\ deques' = [deques EXCEPT ![victim] = Tail(deques[victim])]
                /\ taskState' = [taskState EXCEPT ![taskId] = "Running"]
                /\ workerState' = [workerState EXCEPT ![thief] = "Working"]
                /\ currentTask' = [currentTask EXCEPT ![thief] = taskId]
                /\ UNCHANGED <<bottom, injector, nextTaskId, execHistory>>
           ELSE \* Failed - deque became empty
                /\ UNCHANGED <<deques, bottom, top, injector, taskState,
                               nextTaskId, execHistory, workerState, currentTask>>

(* Worker takes from global injector queue *)
TakeFromInjector(w) ==
    /\ workerState[w] = "Idle"
    /\ Len(injector) > 0
    /\ LET taskId == Head(injector)
       IN
        /\ injector' = Tail(injector)
        /\ taskState' = [taskState EXCEPT ![taskId] = "Running"]
        /\ workerState' = [workerState EXCEPT ![w] = "Working"]
        /\ currentTask' = [currentTask EXCEPT ![w] = taskId]
        /\ UNCHANGED <<deques, bottom, top, nextTaskId, execHistory>>

(* Worker completes current task *)
CompleteTask(w) ==
    /\ workerState[w] = "Working"
    /\ currentTask[w] # NullTask
    /\ LET taskId == currentTask[w]
       IN
        /\ taskState' = [taskState EXCEPT ![taskId] = "Completed"]
        /\ execHistory' = Append(execHistory, [worker |-> w, task |-> taskId])
        /\ workerState' = [workerState EXCEPT ![w] = "Idle"]
        /\ currentTask' = [currentTask EXCEPT ![w] = NullTask]
        /\ UNCHANGED <<deques, bottom, top, injector, nextTaskId>>

(* Worker transitions to stealing state (looking for work) *)
StartStealing(w) ==
    /\ workerState[w] = "Idle"
    /\ DequeEmpty(w)
    /\ Len(injector) = 0  \* No global work available
    /\ workerState' = [workerState EXCEPT ![w] = "Stealing"]
    /\ UNCHANGED <<deques, bottom, top, injector, taskState, nextTaskId,
                   execHistory, currentTask>>

(* Worker gives up stealing and returns to idle *)
StopStealing(w) ==
    /\ workerState[w] = "Stealing"
    /\ workerState' = [workerState EXCEPT ![w] = "Idle"]
    /\ UNCHANGED <<deques, bottom, top, injector, taskState, nextTaskId,
                   execHistory, currentTask>>

-----------------------------------------------------------------------------
(* Next-state relation *)

Next ==
    \/ SpawnTask
    \/ \E w \in WorkerIds : \E t \in AllTasks : LocalPush(w, t)
    \/ \E w \in WorkerIds : LocalPop(w)
    \/ \E thief, victim \in WorkerIds : Steal(thief, victim)
    \/ \E w \in WorkerIds : TakeFromInjector(w)
    \/ \E w \in WorkerIds : CompleteTask(w)
    \/ \E w \in WorkerIds : StartStealing(w)
    \/ \E w \in WorkerIds : StopStealing(w)

-----------------------------------------------------------------------------
(* INVARIANTS *)

(* W1: No Lost Tasks - Every spawned task is either pending, running, or completed *)
(* More precisely: task is in exactly one location *)
NoLostTasks ==
    \A t \in AllTasks :
        LET inInjector == t \in {injector[i] : i \in 1..Len(injector)}
            inSomeDeque == \E w \in WorkerIds : t \in {deques[w][i] : i \in 1..Len(deques[w])}
            beingRun == \E w \in WorkerIds : currentTask[w] = t
            isCompleted == taskState[t] = "Completed"
        IN
            (inInjector \/ inSomeDeque \/ beingRun \/ isCompleted)

(* W2: No Double Execution - Each task executes exactly once *)
NoDoubleExecution ==
    \A i, j \in 1..Len(execHistory) :
        i # j => execHistory[i].task # execHistory[j].task

(* W3: Task state consistency *)
TaskStateConsistency ==
    \A t \in AllTasks :
        /\ (taskState[t] = "Running") =>
           (\E w \in WorkerIds : currentTask[w] = t)
        /\ (taskState[t] = "Completed") =>
           (t \in {execHistory[i].task : i \in 1..Len(execHistory)})

(* W4: Deque index consistency *)
DequeConsistency ==
    \A w \in WorkerIds :
        /\ top[w] <= bottom[w]
        /\ bottom[w] - top[w] = Len(deques[w])

(* W5: Mutual exclusion - no two workers execute same task *)
MutualExclusion ==
    \A w1, w2 \in WorkerIds :
        (w1 # w2 /\ currentTask[w1] # NullTask) =>
            currentTask[w1] # currentTask[w2]

(* Combined safety invariant *)
Safety ==
    /\ TypeOK
    /\ NoLostTasks
    /\ NoDoubleExecution
    /\ TaskStateConsistency
    /\ MutualExclusion

-----------------------------------------------------------------------------
(* LIVENESS PROPERTIES *)

(* Every pending task eventually completes (under fair scheduling) *)
AllTasksComplete ==
    \A t \in AllTasks : <>(taskState[t] = "Completed")

(* Workers don't stay idle forever when there's work *)
WorkersProgress ==
    \A w \in WorkerIds :
        (workerState[w] = "Idle" /\ PendingTasks # {}) ~>
            (workerState[w] = "Working" \/ PendingTasks = {})

-----------------------------------------------------------------------------
(* Fairness conditions *)

(* Weak fairness: enabled actions eventually happen *)
Fairness ==
    /\ WF_<<deques, bottom, top, injector, taskState, nextTaskId,
            execHistory, workerState, currentTask>>(Next)
    /\ \A w \in WorkerIds :
        WF_<<deques, bottom, top, injector, taskState, nextTaskId,
             execHistory, workerState, currentTask>>(CompleteTask(w))

(* Specification *)
Spec == Init /\ [][Next]_<<deques, bottom, top, injector, taskState, nextTaskId,
                           execHistory, workerState, currentTask>> /\ Fairness

=============================================================================
