wc(1)

# NAME

wc - word, line, and byte count

# SYNOPSIS

*wc* [*-lwc*] [_FILE_...]

# DESCRIPTION

Print line, word, and byte counts. If no files are given, reads from
standard input.

# OPTIONS

*-l*
	Print only line count.

*-w*
	Print only word count.

*-c*
	Print only byte count.

# EXAMPLES

Count all:

	wc file.txt

Count lines only:

	wc -l file.txt

Count words in pipeline:

	cat file.txt | wc -w

# SEE ALSO

*cat*(1)
