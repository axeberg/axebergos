sudo(1)

# NAME

sudo - execute a command as root

# SYNOPSIS

*sudo* _COMMAND_ [_ARG_]...

# DESCRIPTION

Run a command with root privileges. The current user must be in
the wheel group (sudoers) to use sudo.

Note: In the current implementation, sudo displays what would be executed
but does not actually run the command.

# OPTIONS

*--help*
	Display usage information and exit.

# EXAMPLES

Run a command as root:

	sudo useradd newuser

# SECURITY

Only users in the wheel group (gid 10) can use sudo. In a full
implementation, a password would be required.

# SEE ALSO

*su*(1), *id*(1), *groups*(1)
