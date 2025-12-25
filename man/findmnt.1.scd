findmnt(1)

# NAME

findmnt - find a filesystem

# SYNOPSIS

*findmnt* [TARGET]

# DESCRIPTION

findmnt will list all mounted filesystems or search for a filesystem.
When called without arguments, it displays a table of all mounts.

# OPTIONS

*--help*
	Display this help and exit.

# OUTPUT

The output table has the following columns:

*TARGET*
	Mount point

*SOURCE*
	Device or source name

*FSTYPE*
	Filesystem type

*OPTIONS*
	Mount options

# EXAMPLES

List all mounts:

	findmnt

Find information about /proc:

	findmnt /proc

Find the mount containing /home/user/file:

	findmnt /home/user/file

# SEE ALSO

*mount*(8), *umount*(8)
