head(1)

# NAME

head - output the first part of files

# SYNOPSIS

*head* [*-n* _COUNT_] [_FILE_...]

# DESCRIPTION

Print the first _COUNT_ lines of each file. Default is 10 lines.

If no files are given, reads from standard input.

# OPTIONS

*-n* _COUNT_
	Print first _COUNT_ lines instead of 10.

# EXAMPLES

Show first 10 lines:

	head file.txt

Show first 5 lines:

	head -n 5 file.txt

Use in pipeline:

	cat log.txt | head -n 20

# SEE ALSO

*tail*(1), *cat*(1)
