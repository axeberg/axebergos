chown(1)

# NAME

chown - change file owner and group

# SYNOPSIS

*chown* [_OWNER_][*:*_GROUP_] _FILE_...

# DESCRIPTION

Change the owner and/or group of each FILE.

Only root can change the owner. Non-root users can only change
the group to a group they belong to.

# OWNER:GROUP FORMAT

- *owner* - change only the owner
- *owner:group* - change owner and group
- *owner:* - change owner, set group to owner's login group
- *:group* - change only the group

# OPTIONS

*--help*
	Display usage information and exit.

# EXAMPLES

Change owner to root:

	chown root file.txt

Change owner and group:

	chown user:users file.txt

Change only group:

	chown :wheel file.txt

# SEE ALSO

*chgrp*(1), *chmod*(1), *id*(1)
