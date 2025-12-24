su(1)

# NAME

su - switch user

# SYNOPSIS

*su* [*-*] [_USER_]

# DESCRIPTION

Switch to a different user account. Without arguments, switches to
the root user.

To switch to root, the current user must be in the wheel group or
already be root.

# OPTIONS

*-*, *-l*, *--login*
	Start a login shell (also sets HOME, USER, SHELL variables).

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Switch to root:

	su

Switch to root with login shell:

	su -

Switch to another user:

	su admin

# SECURITY

Only root or users in the wheel group can switch to other users.
In a full implementation, a password would be required.

# SEE ALSO

*sudo*(1), *id*(1), *whoami*(1)
