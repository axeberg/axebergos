basename(1)

# NAME

basename - strip directory and suffix from filename

# SYNOPSIS

*basename* _PATH_ [_SUFFIX_]

# DESCRIPTION

Print the final component of PATH, optionally removing SUFFIX.

# ARGUMENTS

_PATH_
	The pathname to process.

_SUFFIX_
	Optional suffix to remove from the result.

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Get filename:

	basename /home/user/file.txt

Result: file.txt

Remove suffix:

	basename /home/user/file.txt .txt

Result: file

# SEE ALSO

*dirname*(1), *pwd*(1)
