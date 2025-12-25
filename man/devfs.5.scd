devfs(5)

# NAME

devfs - device filesystem

# DESCRIPTION

The /dev directory contains device files that provide access to
hardware and pseudo-devices. In this WASM-based environment, most
devices are simulated.

# STANDARD DEVICES

*/dev/console*
	The system console device for output.

*/dev/null*
	The null device. Writes are discarded, reads return EOF.

*/dev/zero*
	The zero device. Reads return an endless stream of zero bytes.
	Writes are discarded.

*/dev/random*
	Random number generator. Returns pseudo-random bytes.

*/dev/urandom*
	Non-blocking random number generator. Same as /dev/random
	in this implementation.

*/dev/stdin*
	Symbolic link to process standard input.

*/dev/stdout*
	Symbolic link to process standard output.

*/dev/stderr*
	Symbolic link to process standard error.

*/dev/tty*
	The controlling terminal of the current process.

*/dev/ptmx*
	Pseudo-terminal master multiplexer.

*/dev/fd*
	Directory containing symbolic links to open file descriptors.

# EXAMPLES

Discard output:

	command > /dev/null

Generate random bytes:

	cat /dev/random | head -c 16

Create a file of zeros:

	head -c 1024 /dev/zero > zeros.bin

# SEE ALSO

*proc*(5), *sysfs*(5), *cat*(1)
