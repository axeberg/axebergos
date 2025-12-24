dirname(1)

# NAME

dirname - strip last component from filename

# SYNOPSIS

*dirname* _PATH_

# DESCRIPTION

Print PATH with its last component removed.

# ARGUMENTS

_PATH_
	The pathname to process.

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Get directory:

	dirname /home/user/file.txt

Result: /home/user

Root directory:

	dirname /file.txt

Result: /

No directory:

	dirname file.txt

Result: .

# SEE ALSO

*basename*(1), *pwd*(1)
