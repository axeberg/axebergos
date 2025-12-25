bg(1)

# NAME

bg - continue job in background

# SYNOPSIS

*bg* [_JOB_]

# DESCRIPTION

Resume a stopped job and run it in the background. If no _JOB_ is
specified, the current job (marked with + in jobs output) is used.

# ARGUMENTS

_JOB_
	Job specification. Can be:
	- %N - Job number N
	- %+ or %% - Current job
	- %- - Previous job

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Continue current job in background:

	bg

Continue job 2 in background:

	bg %2

# SEE ALSO

*fg*(1), *jobs*(1), *kill*(1)
