which(1)

# NAME

which - locate a command

# SYNOPSIS

*which* _COMMAND_...

# DESCRIPTION

Show the full path of commands. For each COMMAND, print the path
that would be executed, or indicate if it's a shell built-in.

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# OUTPUT

For external commands, prints the path (e.g., /bin/ls).
For built-in commands, prints "_command_: shell built-in command".
If not found, prints an error message.

# EXAMPLES

Locate ls:

	which ls
	# Output: /bin/ls

Locate shell built-in:

	which cd
	# Output: cd: shell built-in command

Multiple commands:

	which cat grep echo
	# Shows path for each

# EXIT STATUS

*0*
	All commands found.

*1*
	One or more commands not found.

# SEE ALSO

*type*(1)
