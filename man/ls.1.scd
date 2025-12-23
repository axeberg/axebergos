ls(1)

# NAME

ls - list directory contents

# SYNOPSIS

*ls* [*-la*] [_PATH_...]

# DESCRIPTION

List information about files and directories. If no _PATH_ is given, lists
the current directory.

# OPTIONS

*-l*
	Use long listing format showing permissions, size, and modification date.

*-a*
	Show hidden files (those starting with .).

# EXAMPLES

List current directory:

	ls

Long listing with hidden files:

	ls -la

List specific directory:

	ls /home/user

# SEE ALSO

*cd*(1), *tree*(1)
