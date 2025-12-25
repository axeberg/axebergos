tee(1)

# NAME

tee - duplicate standard input

# SYNOPSIS

*tee* [*-a*] _FILE_

# DESCRIPTION

Copy standard input to both standard output and _FILE_.

# OPTIONS

*-a*
	Append to file instead of overwriting.

# EXAMPLES

Save and display:

	echo hello | tee output.txt

Append to file:

	echo more | tee -a output.txt

Save intermediate pipeline output:

	cat data | grep pattern | tee matches.txt | wc -l

# SEE ALSO

*cat*(1)
