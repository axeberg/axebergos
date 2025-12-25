test(1)

# NAME

test - evaluate conditional expression

# SYNOPSIS

*test* _EXPRESSION_
*[* _EXPRESSION_ *]*

# DESCRIPTION

Evaluate conditional expressions. Returns exit status 0 (true) or 1
(false) based on the evaluation. The *[* command is an alias for
*test* but requires a closing *]*.

# STRING OPERATORS

*-n* _STRING_
	True if STRING is not empty.

*-z* _STRING_
	True if STRING is empty.

_STRING1_ *=* _STRING2_
	True if strings are equal.

_STRING1_ *!=* _STRING2_
	True if strings are not equal.

# NUMERIC OPERATORS

_NUM1_ *-eq* _NUM2_
	True if NUM1 equals NUM2.

_NUM1_ *-ne* _NUM2_
	True if NUM1 does not equal NUM2.

_NUM1_ *-lt* _NUM2_
	True if NUM1 is less than NUM2.

_NUM1_ *-le* _NUM2_
	True if NUM1 is less than or equal to NUM2.

_NUM1_ *-gt* _NUM2_
	True if NUM1 is greater than NUM2.

_NUM1_ *-ge* _NUM2_
	True if NUM1 is greater than or equal to NUM2.

# FILE OPERATORS

*-e* _FILE_
	True if FILE exists.

*-f* _FILE_
	True if FILE exists and is a regular file.

*-d* _FILE_
	True if FILE exists and is a directory.

*-r* _FILE_
	True if FILE exists and is readable.

*-w* _FILE_
	True if FILE exists and is writable.

*-x* _FILE_
	True if FILE exists and is executable.

*-s* _FILE_
	True if FILE exists and has size > 0.

*-L* _FILE_, *-h* _FILE_
	True if FILE is a symbolic link.

# LOGICAL OPERATORS

*!* _EXPRESSION_
	Negate the expression.

# EXAMPLES

Check if file exists:

	test -e /etc/passwd && echo "exists"

Compare numbers:

	[ 5 -gt 3 ] && echo "5 is greater"

Check empty string:

	test -z "$VAR" && echo "VAR is empty"

# EXIT STATUS

*0*
	Expression is true.

*1*
	Expression is false.

# SEE ALSO

*expr*(1)
