df(1)

# NAME

df - report filesystem disk space usage

# SYNOPSIS

*df* [*-h*]

# DESCRIPTION

Display information about the filesystem disk space usage. Shows total
size, used space, available space, and usage percentage.

# OPTIONS

*-h*
	Print sizes in human readable format (e.g., 1K, 234M, 2G).

*--help*
	Display usage information and exit.

# OUTPUT COLUMNS

*Filesystem*
	Name of the filesystem (axeberg-vfs for the virtual filesystem).

*Size*
	Total size of the filesystem.

*Used*
	Amount of space used.

*Avail*
	Amount of space available.

*Use%*
	Percentage of space used.

*Mounted on*
	Mount point (always / for the root filesystem).

# EXAMPLES

Show disk usage:

	df

Human readable format:

	df -h

# NOTES

The axeberg virtual filesystem has a simulated size of 100MB.

# SEE ALSO

*du*(1), *ls*(1)
