comm(1)

# NAME

comm - compare two sorted files line by line

# SYNOPSIS

*comm* [*-123*] _FILE1_ _FILE2_

# DESCRIPTION

Compare two sorted files line by line. Output consists of three
columns:
- Column 1: lines unique to FILE1
- Column 2: lines unique to FILE2
- Column 3: lines common to both

Files must be sorted for correct operation.

# OPTIONS

*-1*
	Suppress column 1 (lines unique to FILE1).

*-2*
	Suppress column 2 (lines unique to FILE2).

*-3*
	Suppress column 3 (common lines).

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Show all three columns:

	comm file1.txt file2.txt

Show only common lines:

	comm -12 file1.txt file2.txt

Show lines unique to file1:

	comm -23 file1.txt file2.txt

# SEE ALSO

*diff*(1), *sort*(1), *uniq*(1)
