strings(1)

# NAME

strings - print the sequences of printable characters in files

# SYNOPSIS

*strings* [*-n* _MIN_] [_FILE_]

# DESCRIPTION

Print sequences of printable characters from FILE or standard input.
Useful for finding text in binary files.

# OPTIONS

*-n* _MIN_
	Minimum string length (default 4).

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Find strings in a file:

	strings binary.bin

Find strings with minimum length 8:

	strings -n 8 binary.bin

# SEE ALSO

*xxd*(1), *grep*(1)
