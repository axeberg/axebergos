uname(1)

# NAME

uname - print system information

# SYNOPSIS

*uname* [*-amnrsv*]

# DESCRIPTION

Print information about the operating system and hardware. With no
options, print the operating system name.

# OPTIONS

*-a*
	Print all information in the following order: kernel name, hostname,
	kernel release, kernel version, machine architecture.

*-m*
	Print the machine hardware name (architecture).

*-n*
	Print the network node hostname.

*-r*
	Print the kernel release.

*-s*
	Print the kernel name (default).

*-v*
	Print the kernel version.

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Print kernel name:

	uname

Print all information:

	uname -a

Output:

	axeberg axeberg 0.1.0 axebergOS wasm32

Print machine architecture:

	uname -m

Output:

	wasm32

# SEE ALSO

*hostname*(1), *id*(1)
