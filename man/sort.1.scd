sort(1)

# NAME

sort - sort lines of text

# SYNOPSIS

*sort* [*-rnu*] [_FILE_...]

# DESCRIPTION

Sort lines of text. If no files are given, reads from standard input.

# OPTIONS

*-r*
	Reverse the result of comparisons.

*-n*
	Sort numerically. (Note: Not yet implemented)

*-u*
	Remove duplicate lines.

# EXAMPLES

Sort a file:

	sort file.txt

Reverse sort:

	sort -r file.txt

Numeric sort:

	sort -n numbers.txt

# SEE ALSO

*uniq*(1)
