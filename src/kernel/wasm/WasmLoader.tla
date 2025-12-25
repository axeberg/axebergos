---------------------------- MODULE WasmLoader ----------------------------
(*
 * TLA+ Specification for the axeberg WASM Command Loader
 *
 * This specification formally describes the behavior and invariants of
 * the WASM command loader subsystem. It models:
 *
 *   1. Command lifecycle states
 *   2. Syscall semantics
 *   3. Memory safety properties
 *   4. File descriptor management
 *
 * To check this specification:
 *   - Install TLA+ Toolbox or use the VS Code extension
 *   - Create a model with the constants defined below
 *   - Run the model checker (TLC)
 *)

EXTENDS Integers, Sequences, FiniteSets, TLC

\* --- Constants ---

CONSTANTS
    MaxFd,          \* Maximum file descriptor number (e.g., 64)
    MaxMemory,      \* Maximum memory size in pages (e.g., 256 = 16MB)
    PageSize,       \* Memory page size (always 65536 for WASM)
    Commands        \* Set of command names (e.g., {"cat", "ls", "echo"})

\* --- Variables ---

VARIABLES
    state,          \* Current loader state: "init" | "loading" | "ready" | "running" | "terminated" | "error"
    memory,         \* Memory contents (abstracted as a function from address ranges to values)
    memorySize,     \* Current memory size in bytes
    fdTable,        \* File descriptor table: fd -> {path, mode, position} or NULL
    nextFd,         \* Next fd to allocate
    exitCode,       \* Exit code (set when terminated)
    argc,           \* Argument count
    argv            \* Argument vector pointer

\* --- Type Invariants ---

TypeInvariant ==
    /\ state \in {"init", "loading", "ready", "running", "terminated", "error"}
    /\ memorySize \in 0..MaxMemory * PageSize
    /\ nextFd \in 0..MaxFd
    /\ exitCode \in -128..127 \cup {-999}  \* -999 = unset
    /\ argc >= 0

\* --- State Transitions ---

(*
 * State machine:
 *
 *   INIT --> LOADING --> READY --> RUNNING --> TERMINATED
 *              |           |          |
 *              +-----------+----------+--> ERROR
 *)

\* Initial state
Init ==
    /\ state = "init"
    /\ memory = [addr \in {} |-> 0]
    /\ memorySize = 0
    /\ fdTable = [fd \in {} |-> <<>>]
    /\ nextFd = 3  \* 0=stdin, 1=stdout, 2=stderr are pre-allocated
    /\ exitCode = -999
    /\ argc = 0
    /\ argv = 0

\* Load a WASM module
Load(cmd) ==
    /\ state = "init"
    /\ cmd \in Commands
    /\ state' = "loading"
    /\ UNCHANGED <<memory, memorySize, fdTable, nextFd, exitCode, argc, argv>>

\* Module loaded successfully, initialize memory
InitMemory(pages) ==
    /\ state = "loading"
    /\ pages > 0
    /\ pages <= MaxMemory
    /\ state' = "ready"
    /\ memorySize' = pages * PageSize
    /\ UNCHANGED <<memory, fdTable, nextFd, exitCode, argc, argv>>

\* Setup arguments and start execution
Start(argCount, argvPtr) ==
    /\ state = "ready"
    /\ argCount >= 0
    /\ argvPtr >= 0
    /\ argvPtr < memorySize
    /\ state' = "running"
    /\ argc' = argCount
    /\ argv' = argvPtr
    /\ UNCHANGED <<memory, memorySize, fdTable, nextFd, exitCode>>

\* --- Syscall Actions ---

(*
 * File descriptor safety:
 * - Only operate on valid, open fds
 * - Never exceed MaxFd
 * - Standard fds (0, 1, 2) always valid
 *)

\* Open a file
SysOpen(path, flags) ==
    /\ state = "running"
    /\ nextFd < MaxFd
    /\ fdTable' = fdTable @@ (nextFd :> <<path, flags, 0>>)
    /\ nextFd' = nextFd + 1
    /\ UNCHANGED <<state, memory, memorySize, exitCode, argc, argv>>

\* Close a file descriptor
SysClose(fd) ==
    /\ state = "running"
    /\ fd >= 3  \* Can't close stdin/stdout/stderr
    /\ fd \in DOMAIN fdTable
    /\ fdTable' = [f \in (DOMAIN fdTable \ {fd}) |-> fdTable[f]]
    /\ UNCHANGED <<state, memory, memorySize, nextFd, exitCode, argc, argv>>

\* Read from a file (abstracted - just checks fd validity)
SysRead(fd, bufPtr, len) ==
    /\ state = "running"
    /\ (fd \in {0, 1, 2} \/ fd \in DOMAIN fdTable)
    /\ bufPtr >= 0
    /\ bufPtr + len <= memorySize
    /\ UNCHANGED <<state, memory, memorySize, fdTable, nextFd, exitCode, argc, argv>>

\* Write to a file (abstracted)
SysWrite(fd, bufPtr, len) ==
    /\ state = "running"
    /\ (fd \in {0, 1, 2} \/ fd \in DOMAIN fdTable)
    /\ bufPtr >= 0
    /\ bufPtr + len <= memorySize
    /\ UNCHANGED <<state, memory, memorySize, fdTable, nextFd, exitCode, argc, argv>>

\* Exit the command
SysExit(code) ==
    /\ state = "running"
    /\ state' = "terminated"
    /\ exitCode' = code
    /\ UNCHANGED <<memory, memorySize, fdTable, nextFd, argc, argv>>

\* --- Error Transitions ---

LoadError ==
    /\ state = "loading"
    /\ state' = "error"
    /\ UNCHANGED <<memory, memorySize, fdTable, nextFd, exitCode, argc, argv>>

RuntimeError ==
    /\ state = "running"
    /\ state' = "error"
    /\ UNCHANGED <<memory, memorySize, fdTable, nextFd, exitCode, argc, argv>>

\* --- Safety Properties ---

(*
 * Memory Safety:
 * All memory accesses must be within bounds
 *)
MemorySafety ==
    state = "running" =>
        /\ argv < memorySize
        /\ argc * 4 + argv <= memorySize  \* argv array fits in memory

(*
 * File Descriptor Safety:
 * - Next fd is always greater than any allocated fd
 * - No fd collisions
 *)
FdSafety ==
    \A fd \in DOMAIN fdTable : fd < nextFd /\ fd >= 3

(*
 * Termination Guarantee:
 * Once terminated, state never changes
 *)
TerminationFinal ==
    [](state = "terminated" => [](state = "terminated"))

(*
 * Exit Code Set:
 * Exit code is only set upon termination
 *)
ExitCodeInvariant ==
    (exitCode # -999) => (state = "terminated")

\* --- Combined Invariant ---

Invariant ==
    /\ TypeInvariant
    /\ MemorySafety
    /\ FdSafety
    /\ ExitCodeInvariant

\* --- Next State Relation ---

Next ==
    \/ \E cmd \in Commands : Load(cmd)
    \/ \E pages \in 1..MaxMemory : InitMemory(pages)
    \/ \E ac \in 0..10, av \in 0..1000 : Start(ac, av)
    \/ \E path \in {"a", "b"}, flags \in {0, 1} : SysOpen(path, flags)
    \/ \E fd \in 3..MaxFd : SysClose(fd)
    \/ \E fd \in 0..MaxFd, ptr \in 0..1000, len \in 1..100 : SysRead(fd, ptr, len)
    \/ \E fd \in 0..MaxFd, ptr \in 0..1000, len \in 1..100 : SysWrite(fd, ptr, len)
    \/ \E code \in -128..127 : SysExit(code)
    \/ LoadError
    \/ RuntimeError

\* --- Specification ---

Spec ==
    /\ Init
    /\ [][Next]_<<state, memory, memorySize, fdTable, nextFd, exitCode, argc, argv>>
    /\ WF_<<state, memory, memorySize, fdTable, nextFd, exitCode, argc, argv>>(Next)

\* --- Liveness Properties ---

(*
 * Every loaded command eventually terminates or errors
 *)
EventualTermination ==
    (state = "running") ~> (state \in {"terminated", "error"})

\* --- Executor State Machine ---

(*
 * The WasmExecutor coordinates WASM module execution:
 *
 *   IDLE --> COMPILING --> INSTANTIATING --> EXECUTING --> COMPLETED
 *              |              |                  |
 *              +-------------+------------------+--> FAILED
 *
 * Execution phases:
 *   1. COMPILING: WebAssembly.compile() on module bytes
 *   2. INSTANTIATING: Create import object, instantiate module
 *   3. EXECUTING: Call main(argc, argv), handle syscalls
 *   4. COMPLETED: Capture stdout, stderr, exit_code
 *)

VARIABLES
    execState,      \* Executor state: "idle" | "compiling" | "instantiating" | "executing" | "completed" | "failed"
    stdout,         \* Captured stdout buffer
    stderr,         \* Captured stderr buffer
    runtimeExitCode \* Exit code from runtime

ExecutorTypeInvariant ==
    /\ execState \in {"idle", "compiling", "instantiating", "executing", "completed", "failed"}
    /\ runtimeExitCode \in -128..127 \cup {-999}

\* Executor starts idle
ExecutorInit ==
    /\ execState = "idle"
    /\ stdout = <<>>
    /\ stderr = <<>>
    /\ runtimeExitCode = -999

\* Begin compiling WASM bytes
BeginCompile(bytes) ==
    /\ execState = "idle"
    /\ execState' = "compiling"
    /\ UNCHANGED <<stdout, stderr, runtimeExitCode>>

\* Compilation succeeded, begin instantiation
BeginInstantiate ==
    /\ execState = "compiling"
    /\ execState' = "instantiating"
    /\ UNCHANGED <<stdout, stderr, runtimeExitCode>>

\* Instantiation succeeded, begin execution
BeginExecute(argCount, argvPtr) ==
    /\ execState = "instantiating"
    /\ execState' = "executing"
    /\ UNCHANGED <<stdout, stderr, runtimeExitCode>>

\* Execution completed normally
CompleteExecution(code) ==
    /\ execState = "executing"
    /\ execState' = "completed"
    /\ runtimeExitCode' = code
    /\ UNCHANGED <<stdout, stderr>>

\* Execution failed
FailExecution(reason) ==
    /\ execState \in {"compiling", "instantiating", "executing"}
    /\ execState' = "failed"
    /\ UNCHANGED <<stdout, stderr, runtimeExitCode>>

\* --- Syscall Host Function Semantics ---

(*
 * Host functions are JavaScript closures that implement syscalls.
 * Each syscall has access to SharedRuntime (Rc<RefCell<RuntimeState>>).
 *
 * Key invariants:
 *   1. Syscalls only execute when execState = "executing"
 *   2. Memory access checks bounds before read/write
 *   3. FD operations validate descriptor state
 *   4. exit() terminates execution immediately
 *)

\* Host function: write(fd, buf_ptr, len) -> bytes_written
HostWrite(fd, bufPtr, len) ==
    /\ execState = "executing"
    /\ fd \in {0, 1, 2} \/ fd \in DOMAIN fdTable
    /\ bufPtr >= 0 /\ bufPtr + len <= memorySize
    /\ IF fd = 1 THEN
           \* Append to stdout buffer
           stdout' = stdout \o <<len>>
       ELSE IF fd = 2 THEN
           \* Append to stderr buffer
           stderr' = stderr \o <<len>>
       ELSE
           UNCHANGED <<stdout, stderr>>
    /\ UNCHANGED <<execState, runtimeExitCode>>

\* Host function: read(fd, buf_ptr, len) -> bytes_read
HostRead(fd, bufPtr, len) ==
    /\ execState = "executing"
    /\ fd \in {0, 1, 2} \/ fd \in DOMAIN fdTable
    /\ bufPtr >= 0 /\ bufPtr + len <= memorySize
    /\ UNCHANGED <<execState, stdout, stderr, runtimeExitCode>>

\* Host function: exit(code) -> !
HostExit(code) ==
    /\ execState = "executing"
    /\ execState' = "completed"
    /\ runtimeExitCode' = code
    /\ UNCHANGED <<stdout, stderr>>

\* --- Command Runner Coordination ---

(*
 * WasmCommandRunner coordinates:
 *   1. Finding commands in /bin, /usr/bin, /usr/local/bin
 *   2. Loading and validating module bytes
 *   3. Creating WasmExecutor instance
 *   4. Returning CommandResult
 *
 * Key properties:
 *   - Commands are searched in PATH order
 *   - Module bytes are cached for repeated execution
 *   - Each execution gets fresh runtime state
 *)

VARIABLES
    commandCache,   \* Cache of loaded module bytes: path -> bytes
    currentCommand  \* Currently executing command name

CommandRunnerInit ==
    /\ commandCache = [path \in {} |-> <<>>]
    /\ currentCommand = ""

\* Find and load a command
LoadCommand(name, path) ==
    /\ currentCommand = ""
    /\ currentCommand' = name
    /\ IF path \in DOMAIN commandCache THEN
           \* Cache hit - use cached bytes
           UNCHANGED <<commandCache>>
       ELSE
           \* Cache miss - load from filesystem
           commandCache' = commandCache @@ (path :> <<1>>)

\* Execute loaded command
ExecuteCommand ==
    /\ currentCommand # ""
    /\ execState = "idle"
    /\ BeginCompile(<<>>)
    /\ UNCHANGED <<currentCommand, commandCache>>

\* Command finished
FinishCommand(result) ==
    /\ execState \in {"completed", "failed"}
    /\ currentCommand' = ""
    /\ UNCHANGED <<commandCache>>

\* --- Shell Integration ---

(*
 * Shell executor integration:
 *   - try_execute_sync: Returns None for WASM commands
 *   - execute_wasm_command: Async execution via WasmCommandRunner
 *   - execute_single_async: Unified async command execution
 *
 * Command resolution order:
 *   1. Built-in commands (cd, echo, export, etc.)
 *   2. Program registry (70+ built-in programs)
 *   3. WASM commands in /bin/*.wasm
 *)

\* --- Combined Executor Invariant ---

ExecutorInvariant ==
    /\ ExecutorTypeInvariant
    \* Exit code only set when completed or terminated
    /\ (runtimeExitCode # -999) => (execState = "completed")
    \* Stdout/stderr only modified during execution
    /\ (Len(stdout) > 0 \/ Len(stderr) > 0) =>
           (execState \in {"executing", "completed", "failed"})

\* --- Theorems ---

THEOREM Spec => []Invariant
THEOREM Spec => []TypeInvariant
THEOREM Spec => []ExecutorInvariant

=============================================================================
\* Modification History
\* Updated: Added executor state machine, host functions, command runner
\* Created for axeberg WASM loader specification
