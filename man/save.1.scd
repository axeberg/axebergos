save(1)

# NAME

save - persist filesystem to OPFS storage

# SYNOPSIS

*save*

# DESCRIPTION

The *save* command persists the current state of the virtual filesystem to
the browser's Origin Private File System (OPFS). This allows your files and
directories to survive page reloads and browser restarts.

OPFS is a browser API that provides a sandboxed filesystem accessible only
to the current origin. Data saved to OPFS persists until explicitly deleted.

The save operation is asynchronous. The command returns immediately while
the save completes in the background. Check the browser console for the
result.

# OPTIONS

*--help*
	Display help and exit.

# EXAMPLES

Save the current filesystem:

	save

# FILES

_axeberg_fs.json_
	The serialized filesystem stored in OPFS.

# EXIT STATUS

*0*
	Save initiated successfully.

# SEE ALSO

*fsload*(1), *fsreset*(1), *autosave*(1)

