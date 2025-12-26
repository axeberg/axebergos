//! /proc virtual filesystem
//!
//! Provides a dynamic view into kernel and process state.
//! Files are generated on-demand when read.

use std::collections::HashMap;

/// Content generator for /proc files
pub struct ProcFs {
    /// Cached content for open files (path -> content)
    /// Currently unused but reserved for future caching optimization
    _cache: HashMap<String, Vec<u8>>,
}

impl ProcFs {
    pub fn new() -> Self {
        Self {
            _cache: HashMap::new(),
        }
    }

    /// Check if a path is in /proc
    pub fn is_proc_path(path: &str) -> bool {
        path == "/proc" || path.starts_with("/proc/")
    }

    /// List directory contents for a /proc path
    pub fn list_dir(&self, path: &str, pids: &[u32]) -> Option<Vec<String>> {
        if path == "/proc" {
            // Root of /proc - list PIDs and special files
            let mut entries: Vec<String> = pids.iter().map(|p| p.to_string()).collect();
            entries.extend([
                "self".to_string(),
                "uptime".to_string(),
                "meminfo".to_string(),
                "cpuinfo".to_string(),
                "version".to_string(),
                "loadavg".to_string(),
                "stat".to_string(),
                "mounts".to_string(),
            ]);
            Some(entries)
        } else if let Some(pid_str) = path.strip_prefix("/proc/") {
            // Check if it's a PID directory
            if let Ok(pid) = pid_str.parse::<u32>()
                && pids.contains(&pid)
            {
                return Some(vec![
                    "cmdline".to_string(),
                    "cwd".to_string(),
                    "environ".to_string(),
                    "exe".to_string(),
                    "fd".to_string(),
                    "status".to_string(),
                    "stat".to_string(),
                    "maps".to_string(),
                ]);
            }
            // Check for /proc/[pid]/fd
            if let Some(rest) = pid_str.strip_suffix("/fd")
                && let Ok(_pid) = rest.parse::<u32>()
            {
                // Would list file descriptors here
                return Some(vec!["0".to_string(), "1".to_string(), "2".to_string()]);
            }
            None
        } else {
            None
        }
    }

    /// Check if path exists in /proc
    pub fn exists(&self, path: &str, pids: &[u32]) -> bool {
        if path == "/proc" {
            return true;
        }

        let Some(rest) = path.strip_prefix("/proc/") else {
            return false;
        };

        // Special files at /proc root
        let special_files = [
            "self", "uptime", "meminfo", "cpuinfo", "version", "loadavg", "stat", "mounts",
        ];
        if special_files.contains(&rest) {
            return true;
        }

        // Check for PID directory or file within it
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.is_empty() {
            return false;
        }

        // First part should be a PID or "self"
        if parts[0] == "self" {
            // /proc/self exists if we have any process
            if parts.len() == 1 {
                return true;
            }
            // Check subpath
            let subpath = parts[1..].join("/");
            return Self::is_valid_proc_pid_file(&subpath);
        }

        if let Ok(pid) = parts[0].parse::<u32>() {
            if !pids.contains(&pid) {
                return false;
            }
            if parts.len() == 1 {
                return true; // Just the PID directory
            }
            let subpath = parts[1..].join("/");
            return Self::is_valid_proc_pid_file(&subpath);
        }

        false
    }

    fn is_valid_proc_pid_file(subpath: &str) -> bool {
        matches!(
            subpath,
            "cmdline" | "cwd" | "environ" | "exe" | "fd" | "status" | "stat" | "maps"
        ) || subpath.starts_with("fd/")
    }

    /// Check if path is a directory in /proc
    pub fn is_dir(&self, path: &str, pids: &[u32]) -> bool {
        if path == "/proc" {
            return true;
        }

        let Some(rest) = path.strip_prefix("/proc/") else {
            return false;
        };

        // Check for PID directory
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.is_empty() {
            return false;
        }

        if parts[0] == "self" {
            if parts.len() == 1 {
                return true;
            }
            return parts.len() == 2 && parts[1] == "fd";
        }

        if let Ok(pid) = parts[0].parse::<u32>() {
            if !pids.contains(&pid) {
                return false;
            }
            if parts.len() == 1 {
                return true; // PID directory
            }
            // /proc/[pid]/fd is a directory
            return parts.len() == 2 && parts[1] == "fd";
        }

        false
    }
}

impl Default for ProcFs {
    fn default() -> Self {
        Self::new()
    }
}

/// Information needed to generate /proc content
pub struct ProcContext<'a> {
    pub pid: u32,
    pub ppid: Option<u32>,
    pub name: &'a str,
    pub state: &'a str,
    pub uid: u32,
    pub gid: u32,
    pub cwd: &'a str,
    pub cmdline: &'a str,
    pub environ: &'a [(String, String)],
    pub memory_used: u64,
    pub memory_limit: u64,
}

/// System-wide information for /proc
pub struct SystemContext {
    pub uptime_secs: f64,
    pub total_memory: u64,
    pub used_memory: u64,
    pub free_memory: u64,
    pub num_processes: usize,
}

/// Generate content for a /proc file
pub fn generate_proc_content(
    path: &str,
    current_pid: u32,
    proc_ctx: Option<&ProcContext>,
    sys_ctx: &SystemContext,
) -> Option<Vec<u8>> {
    let rest = path.strip_prefix("/proc/")?;

    // System-wide files
    match rest {
        "uptime" => {
            let content = format!(
                "{:.2} {:.2}\n",
                sys_ctx.uptime_secs,
                sys_ctx.uptime_secs * 0.9
            );
            return Some(content.into_bytes());
        }
        "meminfo" => {
            let content = format!(
                "MemTotal:       {} kB\n\
                 MemFree:        {} kB\n\
                 MemAvailable:   {} kB\n\
                 Buffers:        0 kB\n\
                 Cached:         0 kB\n",
                sys_ctx.total_memory / 1024,
                sys_ctx.free_memory / 1024,
                sys_ctx.free_memory / 1024,
            );
            return Some(content.into_bytes());
        }
        "cpuinfo" => {
            let content = "processor\t: 0\n\
                           vendor_id\t: AxebergOS\n\
                           model name\t: Virtual CPU @ WASM\n\
                           cpu MHz\t\t: 1000.000\n\
                           cache size\t: 256 KB\n\
                           flags\t\t: wasm virtual\n\n";
            return Some(content.to_string().into_bytes());
        }
        "version" => {
            let content = "AxebergOS version 0.1.0 (rustc) #1 WASM\n";
            return Some(content.to_string().into_bytes());
        }
        "loadavg" => {
            let load = sys_ctx.num_processes as f64 * 0.1;
            let content = format!(
                "{:.2} {:.2} {:.2} {}/{} 1\n",
                load,
                load * 0.9,
                load * 0.8,
                sys_ctx.num_processes,
                sys_ctx.num_processes
            );
            return Some(content.into_bytes());
        }
        "stat" => {
            let content = format!(
                "cpu  0 0 0 0 0 0 0 0 0 0\n\
                 processes {}\n\
                 procs_running 1\n\
                 procs_blocked 0\n",
                sys_ctx.num_processes
            );
            return Some(content.into_bytes());
        }
        "mounts" => {
            let content = "/ / memfs rw 0 0\n\
                           /proc /proc proc rw 0 0\n";
            return Some(content.to_string().into_bytes());
        }
        "self" => {
            // This is actually a symlink, but we return None for directory
            return None;
        }
        _ => {}
    }

    // Parse path for PID-specific files
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.is_empty() {
        return None;
    }

    // Handle /proc/self/... by substituting current PID
    let (pid, subparts) = if parts[0] == "self" {
        (current_pid, &parts[1..])
    } else if let Ok(pid) = parts[0].parse::<u32>() {
        (pid, &parts[1..])
    } else {
        return None;
    };

    // Need process context for PID-specific files
    let ctx = proc_ctx?;
    if ctx.pid != pid {
        return None; // Wrong process context provided
    }

    if subparts.is_empty() {
        return None; // Directory listing, not file content
    }

    match subparts[0] {
        "cmdline" => {
            // Command line with null separators
            let content = ctx.cmdline.replace(' ', "\0") + "\0";
            Some(content.into_bytes())
        }
        "cwd" => {
            // This is typically a symlink, return the path
            Some(ctx.cwd.as_bytes().to_vec())
        }
        "exe" => {
            // Return executable path
            Some(format!("/bin/{}", ctx.name).into_bytes())
        }
        "environ" => {
            // Environment variables with null separators
            let content: String = ctx
                .environ
                .iter()
                .map(|(k, v)| format!("{}={}\0", k, v))
                .collect();
            Some(content.into_bytes())
        }
        "status" => {
            let content = format!(
                "Name:\t{}\n\
                 State:\t{}\n\
                 Pid:\t{}\n\
                 PPid:\t{}\n\
                 Uid:\t{}\t{}\t{}\t{}\n\
                 Gid:\t{}\t{}\t{}\t{}\n\
                 VmSize:\t{} kB\n\
                 VmRSS:\t{} kB\n",
                ctx.name,
                ctx.state,
                ctx.pid,
                ctx.ppid
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "0".to_string()),
                ctx.uid,
                ctx.uid,
                ctx.uid,
                ctx.uid,
                ctx.gid,
                ctx.gid,
                ctx.gid,
                ctx.gid,
                ctx.memory_limit / 1024,
                ctx.memory_used / 1024,
            );
            Some(content.into_bytes())
        }
        "stat" => {
            // Simplified /proc/[pid]/stat format
            let content = format!(
                "{} ({}) {} {} {} 0 0 0 0 0 0 0 0 0 0 0 1 0 0 {} 0\n",
                ctx.pid,
                ctx.name,
                ctx.state.chars().next().unwrap_or('S'),
                ctx.ppid.unwrap_or(0),
                ctx.pid, // pgrp
                ctx.memory_used,
            );
            Some(content.into_bytes())
        }
        "maps" => {
            // Memory maps (simplified)
            let content = format!(
                "00000000-{:08x} r-xp 00000000 00:00 0 [code]\n\
                 {:08x}-{:08x} rw-p 00000000 00:00 0 [heap]\n",
                ctx.memory_limit,
                ctx.memory_limit,
                ctx.memory_limit + ctx.memory_used,
            );
            Some(content.into_bytes())
        }
        "fd" => {
            if subparts.len() == 1 {
                return None; // Directory
            }
            // /proc/[pid]/fd/N - return path to fd (simplified)
            let fd_num = subparts[1];
            match fd_num {
                "0" => Some("/dev/stdin".as_bytes().to_vec()),
                "1" => Some("/dev/stdout".as_bytes().to_vec()),
                "2" => Some("/dev/stderr".as_bytes().to_vec()),
                _ => Some(format!("pipe:[{}]", fd_num).into_bytes()),
            }
        }
        _ => None,
    }
}
