passwd(1)

# NAME

passwd - change user password

# SYNOPSIS

*passwd* [_USER_]

# DESCRIPTION

Change the password for a user account. Without arguments, changes
the password for the current user.

Only root can change other users' passwords.

Note: Passwords are stored and used for authentication in the system.

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Change your own password:

	passwd

Change another user's password (as root):

	sudo passwd alice

# SEE ALSO

*su*(1), *sudo*(1), *useradd*(8)
