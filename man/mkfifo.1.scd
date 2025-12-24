mkfifo(1)

# NAME

mkfifo - make FIFOs (named pipes)

# SYNOPSIS

*mkfifo* [OPTION]... NAME...

# DESCRIPTION

Create named pipes (FIFOs) with the given NAMEs.

# OPTIONS

*-m* MODE
	Set file permission mode (as in chmod), not a=rw - umask.

*--help*
	Display this help and exit.

# DESCRIPTION

A FIFO (First In First Out) is a special file type that permits
independent processes to communicate. One process opens the FIFO for
writing, and another for reading. Data written to the FIFO by the
writer can be read by the reader in the order it was written.

Unlike pipes, FIFOs persist in the filesystem and can be accessed
by unrelated processes.

# EXAMPLES

Create a named pipe:

	mkfifo /tmp/mypipe

Use a named pipe for communication:

	# Terminal 1: reader
	cat /tmp/mypipe

	# Terminal 2: writer
	echo "Hello" > /tmp/mypipe

# SEE ALSO

*ipcs*(1), *rm*(1), *pipe*(7)
