cut(1)

# NAME

cut - remove sections from each line

# SYNOPSIS

*cut* *-d* _DELIM_ *-f* _FIELDS_ [_FILE_]

# DESCRIPTION

Print selected fields from each line, using DELIM as field separator.

# OPTIONS

*-d* _DELIM_
	Use DELIM as field delimiter (default: tab).

*-f* _FIELDS_
	Select only these fields. Format:
	- N - field N
	- N,M - fields N and M
	- N-M - fields N through M

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Extract first field (colon-separated):

	echo "a:b:c" | cut -d : -f 1

Result: a

Multiple fields:

	echo "a:b:c:d" | cut -d : -f 1,3

Result: a:c

Field range:

	echo "a:b:c:d" | cut -d : -f 2-4

Result: b:c:d

# SEE ALSO

*tr*(1), *grep*(1)
