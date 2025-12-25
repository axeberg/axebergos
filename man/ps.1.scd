ps(1)

# NAME

ps - report process status

# SYNOPSIS

*ps* [*-al*]

# DESCRIPTION

Display information about running processes.

# OPTIONS

*-a*
	Show all processes (same as default).

*-l*
	Long format. Show PPID and PGID columns.

*-h*, *--help*
	Display usage information and exit.

# OUTPUT COLUMNS

*PID*
	Process ID.

*PPID* (long format)
	Parent process ID.

*PGID* (long format)
	Process group ID.

*STATE*
	Process state:
	- *R* - Running
	- *S* - Sleeping
	- *T* - Stopped
	- *D* - Blocked (uninterruptible)
	- *Z* - Zombie

*COMMAND*
	Name of the process.

# EXAMPLES

Show all processes:

	ps

Long format:

	ps -l

# SEE ALSO

*jobs*(1), *kill*(1), *id*(1)
