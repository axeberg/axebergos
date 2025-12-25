uptime(1)

# NAME

uptime - show how long system has been running

# SYNOPSIS

*uptime*

# DESCRIPTION

Display how long the system has been running since boot, along
with system statistics including syscall count and process
spawning information.

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# OUTPUT

The output includes:

- *up*: Time since system started (days, hours:minutes, seconds)
- *syscalls*: Total number of system calls made
- *processes*: Count of spawned/exited processes

# EXAMPLES

Show uptime:

	uptime
	# Output: up 0 min, 5 sec
	#         syscalls: 42, processes: 3/2

# NOTES

In axebergOS, uptime is measured from when the kernel was
initialized for the current session.

# SEE ALSO

*free*(1), *ps*(1)
