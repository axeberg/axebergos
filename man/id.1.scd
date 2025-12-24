id(1)

# NAME

id - print real and effective user and group IDs

# SYNOPSIS

*id* [_USER_]

# DESCRIPTION

Print user and group ID information for the current process or the
specified USER.

Without arguments, displays the real and effective user/group IDs of
the current process, along with supplementary group memberships.

# OUTPUT FORMAT

The output shows:

uid=_UID_(_USERNAME_) gid=_GID_(_GROUPNAME_) [groups=...]

If effective IDs differ from real IDs, they are also displayed as
euid and egid.

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Show current user's IDs:

	id

Example output:

	uid=1000(user) gid=1000(user) groups=1000(user),10(wheel)

Show IDs for user 'root':

	id root

# SEE ALSO

*whoami*(1), *groups*(1), *su*(1)
