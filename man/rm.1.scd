rm(1)

# NAME

rm - remove files and directories

# SYNOPSIS

*rm* [*-rf*] _FILE_...

# DESCRIPTION

Remove files and directories.

# OPTIONS

*-r*
	Remove directories and their contents recursively.

*-f*
	Force removal; ignore nonexistent files, never prompt.

# EXAMPLES

Remove a file:

	rm file.txt

Remove a directory recursively:

	rm -r mydir

Force remove without errors:

	rm -rf temp

# SEE ALSO

*mkdir*(1), *mv*(1), *cp*(1)
