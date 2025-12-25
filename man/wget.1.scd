wget(1)

# NAME

wget - download files from the web

# SYNOPSIS

*wget* [OPTIONS] URL

# DESCRIPTION

*wget* downloads files from the web and saves them to the local filesystem.
It uses the browser's Fetch API to retrieve content.

The output filename is determined from the URL unless overridden with *-O*.

Note: This command runs asynchronously in the browser. Results are displayed
in the browser console, not the terminal output.

# OPTIONS

*-O* _FILE_
	Save to FILE instead of the filename in the URL.

*-q*
	Quiet mode. Suppress output.

*--help*
	Display help and exit.

# LIMITATIONS

- Subject to browser CORS restrictions
- Results appear in browser console (async operation)
- No recursive download
- No resume support

# EXAMPLES

Download a file:

	wget https://example.com/file.txt

Save with different name:

	wget -O myfile.txt https://example.com/data

# EXIT STATUS

*0*
	Download initiated successfully.

*1*
	No URL specified.

# SEE ALSO

*curl*(1)

