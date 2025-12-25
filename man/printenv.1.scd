printenv(1)

# NAME

printenv - print environment variables

# SYNOPSIS

*printenv* [_NAME_...]

# DESCRIPTION

Print the values of environment variables from the kernel process. If no
_NAME_ arguments are given, print all environment variables.

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Print all environment variables:

	printenv

Print specific variable:

	printenv HOME

Print multiple variables:

	printenv USER HOME SHELL

# ENVIRONMENT

The following variables are set by default:

*HOME*
	The user's home directory (/home/user).

*USER*
	The current username (user).

*SHELL*
	The default shell (/bin/sh).

*PATH*
	The command search path (/bin:/usr/bin).

*TERM*
	The terminal type (xterm-256color).

# SEE ALSO

*env*(1), *export*(1), *id*(1)
