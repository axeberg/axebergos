cal(1)

# NAME

cal - display a calendar

# SYNOPSIS

*cal* [_MONTH_] [_YEAR_]

# DESCRIPTION

Display a calendar for the current or specified month. The current
day is marked with an asterisk (*).

# OPTIONS

*-h*, *--help*
	Display usage information and exit.

# ARGUMENTS

_MONTH_
	Month to display (1-12). Defaults to current month.

_YEAR_
	Year to display. Defaults to current year.

# OUTPUT

The calendar displays:
- Month and year header
- Day-of-week abbreviations (Su Mo Tu We Th Fr Sa)
- Days arranged in a grid
- Current day marked with asterisk

# EXAMPLES

Display current month:

	cal

Display January 2025:

	cal 1 2025

Display current month of 2024:

	cal 2024

# SEE ALSO

*date*(1)
