groupadd(8)

# NAME

groupadd - create a new group

# SYNOPSIS

*groupadd* _GROUPNAME_

# DESCRIPTION

Create a new group. Only root can create new groups.

A new GID is automatically assigned starting from 1000.

# OPTIONS

*--help*
	Display usage information and exit.

# EXAMPLES

Create a new group:

	sudo groupadd developers

# SEE ALSO

*useradd*(8), *groups*(1), *id*(1)
