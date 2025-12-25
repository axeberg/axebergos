seq(1)

# NAME

seq - print sequence of numbers

# SYNOPSIS

*seq* [_FIRST_ [_INCREMENT_]] _LAST_

# DESCRIPTION

Print numbers from FIRST to LAST, with INCREMENT step.

# ARGUMENTS

_FIRST_
	Starting number (default: 1).

_INCREMENT_
	Step between numbers (default: 1). Can be negative.

_LAST_
	Ending number (required).

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Print 1 to 5:

	seq 5

Print 3 to 7:

	seq 3 7

Count by 2:

	seq 1 2 10

Count down:

	seq 10 -1 1

# SEE ALSO

*yes*(1), *echo*(1)
