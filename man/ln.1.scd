ln(1)

# NAME

ln - create links

# SYNOPSIS

*ln* *-s* [*-f*] _TARGET_ _LINK_NAME_

# DESCRIPTION

Create a symbolic link to _TARGET_ with the name _LINK_NAME_.

Only symbolic links are supported; the *-s* flag is required.

# OPTIONS

*-s*
	Create symbolic link (required).

*-f*
	Force; remove existing destination file.

# EXAMPLES

Create a symbolic link:

	ln -s /path/to/target linkname

Replace existing link:

	ln -sf newtarget existinglink

# SEE ALSO

*readlink*(1)
