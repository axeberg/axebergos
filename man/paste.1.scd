paste(1)

# NAME

paste - merge lines of files

# SYNOPSIS

*paste* _FILE_...

# DESCRIPTION

Merge corresponding lines of files, separated by tabs. Each output
line contains the corresponding line from each input file.

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Merge two files side by side:

	paste file1.txt file2.txt

Merge three files:

	paste names.txt ages.txt cities.txt

# SEE ALSO

*cut*(1), *comm*(1)
