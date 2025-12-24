groups(1)

# NAME

groups - print group memberships

# SYNOPSIS

*groups* [_USER_]

# DESCRIPTION

Print the groups a user belongs to. Without arguments, prints the
groups of the current user.

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Show current user's groups:

	groups

Output:

	user wheel

Show groups for a specific user:

	groups root

# SEE ALSO

*id*(1), *whoami*(1)
