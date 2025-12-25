intro(7)

# NAME

intro - introduction to AxebergOS commands and documentation

# DESCRIPTION

AxebergOS is a mini operating system implemented in Rust and compiled to
WebAssembly. It provides a Unix-like environment with a shell, virtual
filesystem, process management, and many familiar commands.

# COMMAND CATEGORIES

## File Operations

*cat*(1), *ls*(1), *mkdir*(1), *touch*(1), *rm*(1), *cp*(1), *mv*(1),
*ln*(1), *tree*(1), *find*(1)

## Text Processing

*head*(1), *tail*(1), *wc*(1), *grep*(1), *sort*(1), *uniq*(1), *cut*(1),
*tr*(1), *diff*(1), *comm*(1), *nl*(1), *fold*(1), *paste*(1), *strings*(1),
*rev*(1), *base64*(1), *xxd*(1)

## Utilities

*echo*(1), *cat*(1), *tee*(1), *date*(1), *cal*(1), *seq*(1), *yes*(1),
*basename*(1), *dirname*(1), *xargs*(1), *printf*(1), *test*(1), *expr*(1),
*which*(1), *type*(1)

## Process Management

*ps*(1), *kill*(1), *jobs*(1), *fg*(1), *bg*(1), *time*(1), *strace*(1)

## System Information

*uname*(1), *uptime*(1), *free*(1), *df*(1), *du*(1), *hostname*(1)

## User & Permission Management

*id*(1), *whoami*(1), *groups*(1), *su*(1), *sudo*(1), *useradd*(1),
*groupadd*(1), *passwd*(1), *chmod*(1), *chown*(1), *chgrp*(1)

## Service Management

*systemctl*(1)

## IPC (Inter-Process Communication)

*mkfifo*(1), *ipcs*(1), *ipcrm*(1)

## Mount Management

*mount*(8), *umount*(8), *findmnt*(1)

## Terminal

*tty*(1), *stty*(1)

## Shell Navigation

*cd*(1), *pwd*(1), *printenv*(1)

## Text Editor

*edit*(1)

## Persistence

*save*(1), *fsload*(1), *fsreset*(1), *autosave*(1)

## Help

*man*(1)

# FILESYSTEM DOCUMENTATION

*proc*(5) - Process information filesystem
*sysfs*(5) - System filesystem for kernel objects
*devfs*(5) - Device filesystem

# GETTING STARTED

Start by exploring the filesystem:

	ls /
	cat /etc/passwd
	ps

Get help on any command:

	man ls
	ls --help

# SEE ALSO

*man*(1), the individual command man pages
