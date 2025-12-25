chmod(1)

# NAME

chmod - change file mode bits

# SYNOPSIS

*chmod* _MODE_ _FILE_...

# DESCRIPTION

Change the file mode (permissions) of each FILE to MODE.

# MODE

MODE must be specified in octal:

- *755* - rwxr-xr-x (owner rwx, group rx, other rx)
- *644* - rw-r--r-- (owner rw, group r, other r)
- *700* - rwx------ (owner rwx only)
- *600* - rw------- (owner rw only)

# PERMISSION BITS

- *4* - read (r)
- *2* - write (w)
- *1* - execute (x)

Each octal digit represents owner, group, and other permissions.

# OPTIONS

*--help*
	Display usage information and exit.

# EXAMPLES

Make a file executable:

	chmod 755 script.sh

Remove all permissions except owner read/write:

	chmod 600 secret.txt

# SEE ALSO

*chown*(1), *chgrp*(1), *ls*(1)
