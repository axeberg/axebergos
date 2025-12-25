time(1)

# NAME

time - time command execution

# SYNOPSIS

*time* _COMMAND_ [_ARGS_...]

# DESCRIPTION

Run a command and report the time spent executing it. Displays real
(wall clock), user, and system time.

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# OUTPUT

*real*
	Elapsed wall clock time.

*user*
	Time spent in user mode.

*sys*
	Time spent in kernel mode.

# EXAMPLES

Time a command:

	time ls -la

Time a pipeline:

	time cat file.txt | sort | uniq

# NOTES

In axebergOS, user and system time are not separately tracked, so they
will always show as 0.

# SEE ALSO

*strace*(1), *ps*(1)
