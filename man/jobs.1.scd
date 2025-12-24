jobs(1)

# NAME

jobs - list background jobs

# SYNOPSIS

*jobs* [*-l*]

# DESCRIPTION

Display status of jobs that have been started in the current shell session.
Jobs are processes that are running in the background or have been stopped.

# OPTIONS

*-l*
	Long format. Show process IDs in addition to job numbers.

*-h*, *--help*
	Display usage information and exit.

# OUTPUT FORMAT

Each job is displayed with:

- Job number in brackets [N]
- Status (+/- for current/previous job)
- State (Running, Stopped, Done)
- Command name

# EXAMPLES

List all jobs:

	jobs

Long format with PIDs:

	jobs -l

# JOB CONTROL

Jobs can be controlled with:

- *fg* to bring a job to foreground
- *bg* to continue a stopped job in background
- *kill* to terminate a job

# SEE ALSO

*fg*(1), *bg*(1), *kill*(1)
