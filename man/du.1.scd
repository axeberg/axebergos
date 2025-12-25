du(1)

# NAME

du - estimate file space usage

# SYNOPSIS

*du* [*-sh*] [_PATH_...]

# DESCRIPTION

Summarize disk usage of each file and directory recursively.

# OPTIONS

*-s*
	Display only a total for each argument (summary mode).

*-h*
	Print sizes in human readable format (e.g., 1K, 234M, 2G).

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Show usage of current directory:

	du

Show total size only:

	du -s

Human readable sizes:

	du -sh /home

# OUTPUT

Without *-s*, shows size of each subdirectory. With *-s*, shows only
the total. Sizes are in 1K blocks unless *-h* is specified.

# SEE ALSO

*df*(1), *ls*(1)
