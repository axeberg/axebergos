xxd(1)

# NAME

xxd - make a hexdump

# SYNOPSIS

*xxd* [_FILE_]

# DESCRIPTION

Create a hex dump of a file or standard input. Output shows offset,
hex bytes, and ASCII representation.

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# OUTPUT FORMAT

Each line contains:
- 8-character hex offset
- Up to 16 bytes in hexadecimal (grouped in pairs)
- ASCII representation (non-printable as '.')

# EXAMPLES

Hex dump a file:

	xxd file.txt

Hex dump from stdin:

	echo "Hello" | xxd

# SEE ALSO

*base64*(1), *strings*(1)
