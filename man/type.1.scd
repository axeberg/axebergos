type(1)

# NAME

type - describe how a command would be interpreted

# SYNOPSIS

*type* _COMMAND_...

# DESCRIPTION

Indicate how each COMMAND would be interpreted if used as a
command name. Shows whether a command is a shell builtin,
an external program, or not found.

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# OUTPUT

For shell builtins:

	_command_ is a shell builtin

For external programs:

	_command_ is /bin/_command_

For unknown commands:

	_command_: not found

# EXAMPLES

Describe cd (builtin):

	type cd
	# Output: cd is a shell builtin

Describe ls (external):

	type ls
	# Output: ls is /bin/ls

Multiple commands:

	type cd ls cat
	# Shows type of each

# EXIT STATUS

*0*
	All commands found.

*1*
	One or more commands not found.

# SEE ALSO

*which*(1)
