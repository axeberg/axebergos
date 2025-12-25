grep(1)

# NAME

grep - search for patterns in files

# SYNOPSIS

*grep* _PATTERN_ [_FILE_...]

# DESCRIPTION

Search for lines matching _PATTERN_ in files using basic pattern matching.
If no files are given, reads from standard input.

# EXAMPLES

Search for "error" in a file:

	grep error log.txt

Search in multiple files:

	grep pattern \*.txt

Search in pipeline:

	cat file.txt | grep pattern

# SEE ALSO

*cat*(1), *head*(1), *tail*(1)
