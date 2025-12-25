expr(1)

# NAME

expr - evaluate expressions

# SYNOPSIS

*expr* _EXPRESSION_

# DESCRIPTION

Evaluate an expression and print the result to standard output.
Supports arithmetic, string, and logical operations.

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# ARITHMETIC OPERATORS

_NUM1_ *+* _NUM2_
	Addition.

_NUM1_ *-* _NUM2_
	Subtraction.

_NUM1_ *\\** _NUM2_
	Multiplication (escape the asterisk in shell).

_NUM1_ */* _NUM2_
	Integer division.

_NUM1_ *%* _NUM2_
	Modulo (remainder).

# COMPARISON OPERATORS

_NUM1_ *<* _NUM2_
	Less than (returns 1 if true, 0 if false).

_NUM1_ *<=* _NUM2_
	Less than or equal.

_NUM1_ *>* _NUM2_
	Greater than.

_NUM1_ *>=* _NUM2_
	Greater than or equal.

_NUM1_ *=* _NUM2_
	Equal.

_NUM1_ *!=* _NUM2_
	Not equal.

# STRING OPERATIONS

*length* _STRING_
	Length of STRING.

*substr* _STRING_ _POS_ _LEN_
	Substring starting at POS (1-indexed) with length LEN.

_STRING_ *:* _PATTERN_
	Pattern match (returns length of match).

# LOGICAL OPERATORS

_EXPR1_ *&* _EXPR2_
	AND: returns EXPR1 if both are non-zero, else 0.

_EXPR1_ *|* _EXPR2_
	OR: returns EXPR1 if non-zero, else EXPR2.

# EXAMPLES

Arithmetic:

	expr 5 + 3
	# Output: 8

Multiplication (note escaping):

	expr 4 \* 5
	# Output: 20

String length:

	expr length "hello"
	# Output: 5

Substring:

	expr substr "hello" 2 3
	# Output: ell

# EXIT STATUS

*0*
	Expression evaluated to non-zero/non-null.

*1*
	Expression evaluated to zero/null.

*2*
	Invalid expression.

# SEE ALSO

*test*(1), *seq*(1)
