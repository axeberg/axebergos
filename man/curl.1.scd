curl(1)

# NAME

curl - transfer data from or to a server

# SYNOPSIS

*curl* [OPTIONS] URL

# DESCRIPTION

*curl* is a tool to transfer data from or to a server using the Fetch API.
It supports HTTP and HTTPS protocols.

Note: This command runs asynchronously in the browser. Results are displayed
in the browser console, not the terminal output.

# OPTIONS

*-i*
	Include HTTP response headers in the output.

*-s*
	Silent mode. Suppress progress meter.

*-X* _METHOD_
	Specify the request method to use: GET, POST, PUT, DELETE, HEAD, PATCH.

*-H* _HEADER_
	Add a custom header to the request. Format: "Name: Value".

*--help*
	Display help and exit.

# LIMITATIONS

- Subject to browser CORS restrictions
- Results appear in browser console (async operation)
- No client certificate support
- No cookie jar

# EXAMPLES

Simple GET request:

	curl https://api.example.com/data

POST request:

	curl -X POST https://api.example.com/submit

Include response headers:

	curl -i https://example.com

Add custom header:

	curl -H "Authorization: Bearer token" https://api.example.com

# EXIT STATUS

*0*
	Request initiated successfully.

*1*
	No URL specified or invalid options.

# SEE ALSO

*wget*(1)

