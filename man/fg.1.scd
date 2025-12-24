fg(1)

# NAME

fg - bring job to foreground

# SYNOPSIS

*fg* [_JOB_]

# DESCRIPTION

Move a background or stopped job to the foreground, making it the current
job. If no _JOB_ is specified, the current job (marked with + in jobs
output) is used.

# ARGUMENTS

_JOB_
	Job specification. Can be:
	- %N - Job number N
	- %+ or %% - Current job
	- %- - Previous job
	- %string - Job whose command begins with string

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Bring current job to foreground:

	fg

Bring job 1 to foreground:

	fg %1

# SEE ALSO

*bg*(1), *jobs*(1), *kill*(1)
