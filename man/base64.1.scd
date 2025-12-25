base64(1)

# NAME

base64 - encode/decode base64

# SYNOPSIS

*base64* [*-d*] [_FILE_]

# DESCRIPTION

Base64 encode or decode FILE, or standard input if no file is
specified.

# OPTIONS

*-d*, *--decode*
	Decode base64 input.

*-h*, *--help*
	Display usage information and exit.

# EXAMPLES

Encode a file:

	base64 file.txt

Encode from stdin:

	echo "hello" | base64

Decode:

	echo "aGVsbG8K" | base64 -d

# SEE ALSO

*xxd*(1)
