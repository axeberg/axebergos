autosave(1)

# NAME

autosave - configure automatic filesystem saving

# SYNOPSIS

*autosave* [on|off|status|interval _N_]

# DESCRIPTION

The *autosave* command configures automatic saving of the filesystem to
OPFS. When enabled, the filesystem is saved periodically after a specified
number of commands have been executed.

By default, auto-save is enabled and triggers every 10 commands.

# SUBCOMMANDS

*status*
	Show current auto-save settings (default if no argument given).

*on*
	Enable auto-save.

*off*
	Disable auto-save.

*interval* _N_
	Set the number of commands between saves. For example, *interval 5*
	saves after every 5 commands.

# OPTIONS

*--help*
	Display help and exit.

# EXAMPLES

Check current settings:

	autosave
	autosave status

Enable auto-save:

	autosave on

Disable auto-save:

	autosave off

Save every 5 commands:

	autosave interval 5

Save after every command:

	autosave interval 1

# EXIT STATUS

*0*
	Successful.

*1*
	Invalid option or missing argument.

# SEE ALSO

*save*(1), *fsload*(1), *fsreset*(1)

