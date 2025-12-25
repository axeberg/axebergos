echo(1)

# NAME

echo - display a line of text

# SYNOPSIS

*echo* [*-ne*] [_STRING_...]

# DESCRIPTION

Print arguments to standard output followed by a newline.

# OPTIONS

*-n*
	Do not print trailing newline.

*-e*
	Enable interpretation of backslash escapes: \\n (newline), \\t (tab),
	\\r (carriage return), \\a (alert), \\b (backspace), \\\\ (backslash).

# EXAMPLES

Print text:

	echo hello world

No trailing newline:

	echo -n "prompt: "

With escapes:

	echo -e "line1\\nline2"

# SEE ALSO

*cat*(1)
