nl(1)

# NAME

nl - number lines of files

# SYNOPSIS

*nl* [_FILE_]

# DESCRIPTION

Add line numbers to the beginning of each line. Reads from FILE
or standard input if no file is specified.

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# OUTPUT FORMAT

Lines are numbered starting from 1, right-justified in a 6-character
field, followed by a tab and the line content.

# EXAMPLES

Number lines of a file:

	nl file.txt

Number lines from stdin:

	cat file.txt | nl

# SEE ALSO

*cat*(1), *head*(1)
