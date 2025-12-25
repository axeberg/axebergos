ipcs(1)

# NAME

ipcs - show information on IPC facilities

# SYNOPSIS

*ipcs* [OPTIONS]

# DESCRIPTION

ipcs shows information about the active System V IPC facilities:
message queues, semaphore sets, and shared memory segments.

# OPTIONS

*-a*
	Show all IPC facilities (default).

*-q*
	Show information about active message queues.

*-s*
	Show information about active semaphore sets.

*-m*
	Show information about active shared memory segments.

*--help*
	Display this help and exit.

# OUTPUT FORMAT

## Message Queues

	key        msqid      owner      perms      used-bytes   messages

- key: The key associated with the queue (or - for private)
- msqid: The identifier of the message queue
- owner: The owner of the queue
- perms: The permission mode
- used-bytes: Current number of bytes in the queue
- messages: Number of messages in the queue

## Semaphore Arrays

	key        semid      owner      perms      nsems

- key: The key associated with the semaphore set
- semid: The identifier of the semaphore set
- owner: The owner UID
- perms: The permission mode
- nsems: Number of semaphores in the set

## Shared Memory Segments

	key        shmid      creator    attached   bytes

- key: The key associated with the segment (or - for private)
- shmid: The identifier of the shared memory segment
- creator: The PID that created the segment
- attached: Number of processes attached
- bytes: Size of the segment in bytes

# EXAMPLES

Show all IPC facilities:

	ipcs

Show only message queues:

	ipcs -q

Show only semaphores:

	ipcs -s

# SEE ALSO

*ipcrm*(1), *mkfifo*(1)
