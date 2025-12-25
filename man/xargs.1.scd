xargs(1)

# NAME

xargs - build and execute command lines from stdin

# SYNOPSIS

*xargs* [_COMMAND_ [_ARGS_...]]

# DESCRIPTION

Build command lines by appending arguments read from stdin. If no
COMMAND is specified, echo is used.

# ARGUMENTS

_COMMAND_
	Command to execute (default: echo).

_ARGS_
	Initial arguments to COMMAND.

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Echo items:

	echo "a b c" | xargs

Execute with command:

	echo "file1 file2" | xargs rm

With initial args:

	echo "arg1 arg2" | xargs echo prefix

# NOTES

In axebergOS, xargs shows the command that would be executed but does
not actually run it.

# SEE ALSO

*find*(1), *echo*(1)
