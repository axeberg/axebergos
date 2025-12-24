id(1)

# NAME

id - print process and user identification

# SYNOPSIS

*id*

# DESCRIPTION

Print information about the current process, including:

- *pid* - Process ID
- *ppid* - Parent process ID
- *pgid* - Process group ID
- *user* - Current username (from USER environment variable)
- *home* - Home directory (from HOME environment variable)

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Show process information:

	id

Example output:

	pid=1 ppid=(none) pgid=1
	user=user
	home=/home/user

# SEE ALSO

*printenv*(1), *ps*(1)
