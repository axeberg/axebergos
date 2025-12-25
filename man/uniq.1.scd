uniq(1)

# NAME

uniq - report or filter out repeated lines

# SYNOPSIS

*uniq* [*-c*] [_FILE_...]

# DESCRIPTION

Filter adjacent matching lines. Only adjacent duplicates are removed; use
*sort*(1) first for complete deduplication.

# OPTIONS

*-c*
	Prefix lines with occurrence count.

# EXAMPLES

Remove duplicates:

	sort file.txt | uniq

Count occurrences:

	sort file.txt | uniq -c

# SEE ALSO

*sort*(1)
