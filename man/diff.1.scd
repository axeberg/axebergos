diff(1)

# NAME

diff - compare files line by line

# SYNOPSIS

*diff* _FILE1_ _FILE2_

# DESCRIPTION

Compare two files line by line and output the differences in a
simple format. Shows which lines differ between the files.

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# OUTPUT FORMAT

The output uses the following notation:

*NcN*
	Line N in FILE1 differs from line N in FILE2.

*NdN*
	Line N in FILE1 was deleted (not in FILE2).

*NaN*
	Line N was added in FILE2 (not in FILE1).

*< line*
	Line from FILE1.

*> line*
	Line from FILE2.

*---*
	Separator between FILE1 and FILE2 content.

# EXAMPLES

Compare two files:

	diff file1.txt file2.txt

# EXIT STATUS

*0*
	Files are identical.

*1*
	Files differ.

# SEE ALSO

*comm*(1)
