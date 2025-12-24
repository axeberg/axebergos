mount(8)

# NAME

mount - mount a filesystem

# SYNOPSIS

*mount*
*mount* [-t TYPE] [-o OPTIONS] SOURCE TARGET

# DESCRIPTION

The mount command attaches the filesystem found on some device or
virtual source to the filesystem tree. With no arguments, it displays
all mounted filesystems.

# OPTIONS

*-t* TYPE
	Specify the filesystem type. Supported types:
	- proc - process information filesystem
	- sysfs - kernel object filesystem
	- devfs - device filesystem
	- tmpfs - temporary filesystem (RAM-backed)
	- memoryfs - main virtual filesystem

*-o* OPTIONS
	Comma-separated list of mount options:
	- ro, readonly - Mount read-only
	- rw, readwrite - Mount read-write (default)
	- noatime - Don't update access times
	- noexec - Don't allow execution
	- nosuid - Don't allow setuid/setgid
	- nodev - Don't interpret device files
	- size=SIZE - Size limit for tmpfs (e.g., 1G, 512M)

*--help*
	Display this help and exit.

# EXAMPLES

List all mounted filesystems:

	mount

Mount a tmpfs at /mnt/data:

	mount -t tmpfs tmpfs /mnt/data

Mount with options:

	mount -t tmpfs -o size=100M,noexec tmpfs /mnt/temp

Mount proc filesystem:

	mount -t proc proc /proc

# FILES

*/proc/mounts*
	Information about currently mounted filesystems.

*/etc/fstab*
	Static filesystem information (for automatic mounting).

# SEE ALSO

*umount*(8), *findmnt*(1), *fstab*(5)
