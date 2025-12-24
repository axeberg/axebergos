ipcrm(1)

# NAME

ipcrm - remove IPC resources

# SYNOPSIS

*ipcrm* [OPTIONS]

# DESCRIPTION

ipcrm removes System V IPC resources: message queues and semaphore
sets. Shared memory segments are automatically cleaned up when all
processes detach.

# OPTIONS

*-q* ID
	Remove the message queue with the specified identifier.

*-s* ID
	Remove the semaphore set with the specified identifier.

*-m* ID
	Remove the shared memory segment (not fully supported - segments
	are automatically cleaned up when detached).

*-a*
	Remove all IPC resources (message queues and semaphores).

*--help*
	Display this help and exit.

# EXAMPLES

Remove a message queue with ID 1:

	ipcrm -q 1

Remove a semaphore set with ID 2:

	ipcrm -s 2

Remove all IPC resources:

	ipcrm -a

# NOTES

Use ipcs(1) to list current IPC resources and their identifiers.

Removing an IPC resource that is in use by other processes may
cause those processes to receive errors.

# SEE ALSO

*ipcs*(1), *mkfifo*(1)
