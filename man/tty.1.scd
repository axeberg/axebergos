tty(1)

# NAME

tty - print file name of terminal connected to standard input

# SYNOPSIS

*tty* [-s]

# DESCRIPTION

Print the file name of the terminal connected to standard input.

# OPTIONS

*-s*
	Print nothing, only return an exit status.

*--help*
	Display this help and exit.

# EXIT STATUS

*0*
	Standard input is a terminal.

*1*
	Standard input is not a terminal.

# EXAMPLES

Print the current TTY:

	tty

Check if running in a terminal:

	if tty -s; then
	    echo "Running in a terminal"
	fi

# SEE ALSO

*stty*(1)
