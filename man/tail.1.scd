tail(1)

# NAME

tail - output the last part of files

# SYNOPSIS

*tail* [*-n* _COUNT_] [_FILE_]

# DESCRIPTION

Print the last _COUNT_ lines of each file. Default is 10 lines.

If no files are given, reads from standard input.

# OPTIONS

*-n* _COUNT_
	Print last _COUNT_ lines instead of 10.

# EXAMPLES

Show last 10 lines:

	tail file.txt

Show last 5 lines:

	tail -n 5 file.txt

Use in pipeline:

	cat log.txt | tail -n 20

# SEE ALSO

*head*(1), *cat*(1)
