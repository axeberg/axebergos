tr(1)

# NAME

tr - translate characters

# SYNOPSIS

*tr* _SET1_ _SET2_

# DESCRIPTION

Translate characters from SET1 to corresponding characters in SET2.
Reads from stdin.

# ARGUMENTS

_SET1_
	Characters to replace.

_SET2_
	Replacement characters.

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Lowercase to uppercase:

	echo "hello" | tr abc ABC

Result: ABCllo

Replace spaces with newlines:

	echo "a b c" | tr " " "\n"

# SEE ALSO

*cut*(1), *sed*(1)
