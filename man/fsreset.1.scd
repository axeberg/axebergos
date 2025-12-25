fsreset(1)

# NAME

fsreset - clear OPFS storage and reset filesystem

# SYNOPSIS

*fsreset* *-f*

# DESCRIPTION

The *fsreset* command clears all saved filesystem data from the browser's
Origin Private File System (OPFS). This is a destructive operation that
cannot be undone.

After running this command, reload the page to start with a fresh
filesystem. The next page load will initialize a new filesystem instead
of restoring from saved state.

For safety, this command requires the *-f* flag to confirm the operation.

# OPTIONS

*-f*, *--force*
	Force the reset without confirmation. This flag is required.

*--help*
	Display help and exit.

# EXAMPLES

Clear all saved data (requires confirmation):

	fsreset -f

# EXIT STATUS

*0*
	Reset initiated successfully.

*1*
	Missing -f flag (confirmation required).

# SEE ALSO

*save*(1), *fsload*(1), *autosave*(1)

