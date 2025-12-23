cat(1)

# NAME

cat - concatenate and print files

# SYNOPSIS

*cat* [_FILE_...]

# DESCRIPTION

Concatenate files and print to standard output.

If no files are given or file is *-*, reads from standard input.

# EXAMPLES

Print a file:

	cat file.txt

Concatenate multiple files:

	cat file1.txt file2.txt

Use in pipeline:

	echo "hello" | cat

# SEE ALSO

*head*(1), *tail*(1), *tee*(1)
