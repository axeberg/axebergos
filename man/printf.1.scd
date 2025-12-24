printf(1)

# NAME

printf - format and print data

# SYNOPSIS

*printf* _FORMAT_ [_ARG_...]

# DESCRIPTION

Format and print data according to the FORMAT string. Similar to
the C printf function.

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# FORMAT SPECIFIERS

*%s*
	String.

*%d*, *%i*
	Signed decimal integer.

*%x*
	Unsigned hexadecimal (lowercase).

*%X*
	Unsigned hexadecimal (uppercase).

*%o*
	Unsigned octal.

*%c*
	Single character.

*%%*
	Literal percent sign.

# ESCAPE SEQUENCES

*\\n*
	Newline.

*\\t*
	Tab.

*\\r*
	Carriage return.

*\\\\*
	Backslash.

*\\"*
	Double quote.

*\\0*
	Null character.

# EXAMPLES

Print formatted string:

	printf "Hello, %s!\\n" "World"

Print number in hex:

	printf "Value: 0x%X\\n" 255

Print multiple values:

	printf "%s = %d\\n" "count" 42

# SEE ALSO

*echo*(1)
