hostname(1)

# NAME

hostname - show or set system hostname

# SYNOPSIS

*hostname* [_NAME_]

# DESCRIPTION

Display or set the system hostname. If no argument is given, print the
current hostname. If _NAME_ is provided, set the hostname to that value.

The hostname is stored in the HOSTNAME environment variable.

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Show current hostname:

	hostname

Set hostname:

	hostname axeberg

# ENVIRONMENT

*HOSTNAME*
	The system hostname.

# SEE ALSO

*uname*(1), *printenv*(1)
