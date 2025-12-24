proc(5)

# NAME

proc - process information pseudo-filesystem

# DESCRIPTION

The proc filesystem is a pseudo-filesystem which provides an interface
to kernel data structures. It is commonly mounted at /proc.

Most of the files in the proc filesystem are read-only. The content
is generated dynamically when the file is read.

# TOP-LEVEL FILES

*/proc/uptime*
	System uptime in seconds. Shows idle time as the second value.

*/proc/meminfo*
	System memory statistics:
	- MemTotal: total memory available
	- MemFree: unused memory
	- MemAvailable: available for allocation

*/proc/cpuinfo*
	CPU information including model name and number of processors.

*/proc/version*
	Kernel version string.

*/proc/loadavg*
	System load averages and process counts.

*/proc/stat*
	Overall system statistics including CPU time and boot time.

*/proc/self*
	A symbolic link to the current process's /proc/[pid] directory.

# PER-PROCESS DIRECTORIES

Each running process has a directory /proc/[pid] containing:

*/proc/[pid]/status*
	Human-readable process status including:
	- Name, State, Pid, PPid
	- Uid, Gid
	- VmSize, VmRSS (memory usage)

*/proc/[pid]/cmdline*
	Command line used to start the process.

*/proc/[pid]/environ*
	Environment variables, NUL-separated.

*/proc/[pid]/cwd*
	Current working directory (simulated link).

*/proc/[pid]/stat*
	Process status in a single-line format for parsing.

*/proc/[pid]/statm*
	Memory usage statistics.

*/proc/[pid]/maps*
	Memory mappings (placeholder).

*/proc/[pid]/fd*
	Directory containing open file descriptors.

# EXAMPLES

View system uptime:

	cat /proc/uptime

View current process status:

	cat /proc/self/status

List all process PIDs:

	ls /proc

View memory information:

	cat /proc/meminfo

# SEE ALSO

*ps*(1), *top*(1), *cat*(1)
