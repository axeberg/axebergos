systemctl(1)

# NAME

systemctl - control the systemd system and service manager

# SYNOPSIS

*systemctl* _COMMAND_ [_NAME_...]

# DESCRIPTION

systemctl is used to introspect and control the state of the init
system and service manager.

# COMMANDS

*list-units*, *list*
	List all loaded units (services).

*status* _NAME_
	Show the status of a unit.

*start* _NAME_
	Start (activate) a unit.

*stop* _NAME_
	Stop (deactivate) a unit.

*restart* _NAME_
	Restart a unit.

*enable* _NAME_
	Enable a unit to start at boot.

*disable* _NAME_
	Disable a unit from starting at boot.

*get-default*
	Get the default target.

*set-default* _TARGET_
	Set the default target.

# TARGETS

- *rescue.target* - Single-user rescue mode
- *multi-user.target* - Multi-user mode (default)
- *graphical.target* - Graphical mode
- *reboot.target* - Reboot the system
- *poweroff.target* - Power off the system

# EXAMPLES

List all services:

	systemctl list-units

Start a service:

	systemctl start shell

Check service status:

	systemctl status tty

Enable a service to start at boot:

	systemctl enable shell

Get the default boot target:

	systemctl get-default

Set the default boot target:

	systemctl set-default graphical

# SEE ALSO

*reboot*(8), *poweroff*(8)
