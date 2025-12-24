rev(1)

# NAME

rev - reverse lines

# SYNOPSIS

*rev* [_FILE_]

# DESCRIPTION

Reverse the characters in each line of input.

# ARGUMENTS

_FILE_
	File to read (or stdin if omitted).

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Reverse a string:

	echo "hello" | rev

Result: olleh

Reverse file contents:

	rev file.txt

# SEE ALSO

*cat*(1), *tr*(1)
