find(1)

# NAME

find - search for files in a directory hierarchy

# SYNOPSIS

*find* [_PATH_] [*-name* _PATTERN_] [*-type* _TYPE_]

# DESCRIPTION

Search for files in the directory tree rooted at _PATH_. If no _PATH_ is
given, search from the current directory.

# OPTIONS

*-name* _PATTERN_
	Match files whose name matches _PATTERN_. Supports \* as wildcard:
	- \*.txt matches files ending in .txt
	- test\* matches files starting with test
	- \*log\* matches files containing log

*-type* _TYPE_
	Match files of specified type:
	- *f* - regular file
	- *d* - directory
	- *l* - symbolic link

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Find all files:

	find /home

Find text files:

	find . -name "\*.txt"

Find directories:

	find /home -type d

Find files named config:

	find / -name config -type f

# SEE ALSO

*ls*(1), *grep*(1), *tree*(1)
