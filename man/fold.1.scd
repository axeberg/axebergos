fold(1)

# NAME

fold - wrap each input line to fit in specified width

# SYNOPSIS

*fold* [*-w* _WIDTH_] [_FILE_]

# DESCRIPTION

Wrap lines at the specified width. Reads from FILE or standard
input if no file is specified.

# OPTIONS

*-w* _WIDTH_
	Wrap at WIDTH columns (default 80).

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Fold lines at 40 characters:

	fold -w 40 file.txt

Fold stdin:

	echo "very long line" | fold -w 10

# SEE ALSO

*cut*(1), *tr*(1)
