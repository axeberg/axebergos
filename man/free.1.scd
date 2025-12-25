free(1)

# NAME

free - display amount of free and used memory

# SYNOPSIS

*free* [*-h*]

# DESCRIPTION

Display the total amount of free and used physical memory in
the system, including shared memory usage.

# OPTIONS

*-h*, *--human*
	Print sizes in human readable format (e.g., 1K, 234M, 2G).

*--help*
	Display usage information and exit.

# OUTPUT COLUMNS

*total*
	Total amount of memory (system limit).

*used*
	Amount of memory currently allocated.

*free*
	Amount of memory available (total - used).

*shared*
	Memory used for shared memory segments.

# EXAMPLES

Show memory usage:

	free

Human readable output:

	free -h

# NOTES

In axebergOS, memory statistics reflect the virtual memory
management of the WASM runtime.

# SEE ALSO

*uptime*(1), *df*(1), *ps*(1)
