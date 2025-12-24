whoami(1)

# NAME

whoami - print effective username

# SYNOPSIS

*whoami*

# DESCRIPTION

Print the username associated with the current process. This is obtained
from the USER environment variable.

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Print current username:

	whoami

Output:

	user

# ENVIRONMENT

*USER*
	The username to display.

# SEE ALSO

*id*(1), *printenv*(1)
