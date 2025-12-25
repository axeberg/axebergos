stty(1)

# NAME

stty - change and print terminal line settings

# SYNOPSIS

*stty* [-a] ++
*stty* [SETTING]...

# DESCRIPTION

Print or change terminal line settings.

# OPTIONS

*-a*
	Print all current settings in human-readable form.

*--help*
	Display this help and exit.

# SETTINGS

## Special Settings

*sane*
	Reset all settings to sane defaults.

*raw*
	Enable raw mode (no input/output processing).

*cooked*
	Enable cooked (canonical) mode.

## Input Settings

*icrnl* / *-icrnl*
	Translate carriage return to newline.

*inlcr* / *-inlcr*
	Translate newline to carriage return.

*igncr* / *-igncr*
	Ignore carriage return.

*istrip* / *-istrip*
	Strip 8th bit off characters.

*ixon* / *-ixon*
	Enable XON/XOFF flow control.

*ixoff* / *-ixoff*
	Enable XON/XOFF flow control on input.

## Output Settings

*opost* / *-opost*
	Enable output processing.

*onlcr* / *-onlcr*
	Translate newline to carriage return-newline.

*ocrnl* / *-ocrnl*
	Translate carriage return to newline.

## Local Settings

*isig* / *-isig*
	Enable interrupt, quit, and suspend signals.

*icanon* / *-icanon*
	Enable canonical (line-editing) mode.

*echo* / *-echo*
	Echo input characters.

*echoe* / *-echoe*
	Echo erase character as backspace-space-backspace.

*echok* / *-echok*
	Echo newline after kill character.

*echonl* / *-echonl*
	Echo newline even if not echoing other characters.

*noflsh* / *-noflsh*
	Disable flushing after interrupt and quit signals.

*tostop* / *-tostop*
	Stop background jobs that try to write to terminal.

## Control Settings

*cread* / *-cread*
	Enable receiver.

*clocal* / *-clocal*
	Ignore modem control lines.

*parenb* / *-parenb*
	Enable parity generation and detection.

*parodd* / *-parodd*
	Use odd parity (instead of even).

*cs5* / *cs6* / *cs7* / *cs8*
	Set character size to 5, 6, 7, or 8 bits.

# EXAMPLES

Print all settings:

	stty -a

Disable echo:

	stty -echo

Enable raw mode:

	stty raw

Reset to sane defaults:

	stty sane

# SEE ALSO

*tty*(1)
