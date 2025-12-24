useradd(8)

# NAME

useradd - create a new user

# SYNOPSIS

*useradd* [*-g* _GID_] _USERNAME_

# DESCRIPTION

Create a new user account. Only root can create new users.

A new UID is automatically assigned starting from 1000. If no
group is specified, a new group with the same name as the user
is created.

# OPTIONS

*-g* _GID_
	Specify the primary group ID or name for the new user.

*--help*
	Display usage information and exit.

# EXAMPLES

Create a new user:

	sudo useradd alice

Create a user in an existing group:

	sudo useradd -g users bob

# SEE ALSO

*groupadd*(8), *passwd*(1), *id*(1)
