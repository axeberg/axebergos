chgrp(1)

# NAME

chgrp - change group ownership

# SYNOPSIS

*chgrp* _GROUP_ _FILE_...

# DESCRIPTION

Change the group ownership of each FILE to GROUP.

Non-root users can only change to a group they belong to.

# OPTIONS

*--help*
	Display usage information and exit.

# EXAMPLES

Change group to wheel:

	chgrp wheel file.txt

Change group by GID:

	chgrp 10 file.txt

# SEE ALSO

*chown*(1), *chmod*(1), *groups*(1)
