sysfs(5)

# NAME

sysfs - kernel object filesystem

# DESCRIPTION

The sysfs filesystem provides a view into kernel object attributes.
It is commonly mounted at /sys and provides a structured hierarchy
of kernel objects and their properties.

# TOP-LEVEL DIRECTORIES

*/sys/block*
	Block devices (simulated/empty in WASM).

*/sys/bus*
	Bus types and attached devices (simulated/empty in WASM).

*/sys/class*
	Device classes organized by function:
	- tty - terminal devices
	- mem - memory devices (null, zero, random)

*/sys/devices*
	Physical and virtual devices:
	- system/cpu - CPU information
	- system/memory - memory information

*/sys/firmware*
	Firmware interfaces (simulated/empty in WASM).

*/sys/fs*
	Filesystem-specific information.

*/sys/kernel*
	Kernel parameters and information:
	- hostname - system hostname
	- ostype - operating system type
	- osrelease - kernel release version
	- version - kernel version string

*/sys/module*
	Loaded kernel modules (simulated/empty in WASM).

*/sys/power*
	Power management states.

# EXAMPLES

Read hostname:

	cat /sys/kernel/hostname

List kernel information:

	ls /sys/kernel

View OS type:

	cat /sys/kernel/ostype

# SEE ALSO

*proc*(5), *ls*(1)
