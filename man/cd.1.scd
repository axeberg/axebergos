cd(1)

# NAME

cd - change working directory

# SYNOPSIS

*cd* [_DIRECTORY_]

# DESCRIPTION

Change the current working directory to _DIRECTORY_.

If no directory is given, changes to *$HOME*.

If directory is *-*, changes to *$OLDPWD* (previous directory).

If directory starts with *~*, expands to *$HOME*.

# EXAMPLES

Change to home directory:

	cd

Change to parent directory:

	cd ..

Return to previous directory:

	cd -

Change to subdirectory of home:

	cd ~/documents

# SEE ALSO

*pwd*(1)
