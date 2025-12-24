yes(1)

# NAME

yes - output a string repeatedly

# SYNOPSIS

*yes* [_STRING_]

# DESCRIPTION

Repeatedly output STRING (default: "y") until killed. In axebergOS,
limited to 100 iterations for safety.

# ARGUMENTS

_STRING_
	The string to output (default: "y").

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Output "y" repeatedly:

	yes

Output custom string:

	yes "I agree"

Pipe to command:

	yes | head -5

# NOTES

Limited to 100 lines in axebergOS to prevent runaway output.

# SEE ALSO

*seq*(1), *echo*(1)
