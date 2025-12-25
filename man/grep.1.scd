grep(1)

# NAME

grep - search for patterns in files

# SYNOPSIS

*grep* [*-inv*] _PATTERN_ [_FILE_...]

# DESCRIPTION

Search for lines matching _PATTERN_ in files. If no files are given, reads
from standard input.

# OPTIONS

*-i*
	Case-insensitive matching.

*-n*
	Print line numbers with output.

*-v*
	Invert match; print lines that do not match.

# EXAMPLES

Search for "error" in a file:

	grep error log.txt

Case-insensitive search with line numbers:

	grep -in warning \*.log

Find lines not containing pattern:

	grep -v debug output.txt

Search in pipeline:

	cat file.txt | grep pattern

# SEE ALSO

*cat*(1), *head*(1), *tail*(1)
