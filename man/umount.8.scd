umount(8)

# NAME

umount - unmount filesystems

# SYNOPSIS

*umount* TARGET

# DESCRIPTION

The umount command detaches the filesystem(s) mentioned from the
file hierarchy.

# OPTIONS

*--help*
	Display this help and exit.

# NOTES

The root filesystem (/) cannot be unmounted.

Filesystems that are in use (have open files or are the working
directory of a process) may fail to unmount.

# EXAMPLES

Unmount /mnt/data:

	umount /mnt/data

# EXIT STATUS

*0*
	Success

*1*
	Failure (target not mounted, busy, or permission denied)

# SEE ALSO

*mount*(8), *findmnt*(1)
