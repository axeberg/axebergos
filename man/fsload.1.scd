fsload(1)

# NAME

fsload - reload filesystem from OPFS storage

# SYNOPSIS

*fsload*

# DESCRIPTION

The *fsload* command reloads the virtual filesystem from the browser's
Origin Private File System (OPFS). This restores the filesystem to the
state that was previously saved.

This command is useful if you want to discard current changes and restore
a previously saved state, or to manually trigger a reload.

The load operation is asynchronous. The command returns immediately while
the load completes in the background. Check the browser console for the
result.

Note: On page load, the filesystem is automatically restored from OPFS
if a saved state exists.

# OPTIONS

*--help*
	Display help and exit.

# EXAMPLES

Reload filesystem from saved state:

	fsload

# FILES

_axeberg_fs.json_
	The serialized filesystem stored in OPFS.

# EXIT STATUS

*0*
	Load initiated successfully.

# SEE ALSO

*save*(1), *fsreset*(1), *autosave*(1)

