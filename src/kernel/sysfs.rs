//! /sys virtual filesystem
//!
//! Provides a view into kernel object attributes. In a WASM environment,
//! this is largely simulated but provides useful system information.

use std::collections::HashMap;

/// Sysfs manager
pub struct SysFs {
    /// Static content cache
    _cache: HashMap<String, Vec<u8>>,
}

impl SysFs {
    pub fn new() -> Self {
        Self {
            _cache: HashMap::new(),
        }
    }

    /// Check if a path is in /sys
    pub fn is_sys_path(path: &str) -> bool {
        path == "/sys" || path.starts_with("/sys/")
    }

    /// List directory contents
    pub fn list_dir(&self, path: &str) -> Option<Vec<String>> {
        match path {
            "/sys" => Some(vec![
                "block".to_string(),
                "bus".to_string(),
                "class".to_string(),
                "devices".to_string(),
                "firmware".to_string(),
                "fs".to_string(),
                "kernel".to_string(),
                "module".to_string(),
                "power".to_string(),
            ]),
            "/sys/kernel" => Some(vec![
                "hostname".to_string(),
                "ostype".to_string(),
                "osrelease".to_string(),
                "version".to_string(),
            ]),
            "/sys/class" => Some(vec!["tty".to_string(), "mem".to_string()]),
            "/sys/class/tty" => Some(vec!["console".to_string(), "tty0".to_string()]),
            "/sys/class/mem" => Some(vec![
                "null".to_string(),
                "zero".to_string(),
                "random".to_string(),
                "urandom".to_string(),
            ]),
            "/sys/devices" => Some(vec!["system".to_string(), "virtual".to_string()]),
            "/sys/devices/system" => Some(vec!["cpu".to_string(), "memory".to_string()]),
            "/sys/devices/system/cpu" => Some(vec![
                "cpu0".to_string(),
                "online".to_string(),
                "present".to_string(),
            ]),
            "/sys/devices/system/cpu/cpu0" => Some(vec!["cpufreq".to_string()]),
            "/sys/fs" => Some(vec!["cgroup".to_string()]),
            "/sys/power" => Some(vec!["state".to_string()]),
            "/sys/block" | "/sys/bus" | "/sys/firmware" | "/sys/module" => {
                Some(Vec::new()) // Empty directories
            }
            _ => None,
        }
    }

    /// Check if a path exists
    pub fn exists(&self, path: &str) -> bool {
        if path == "/sys" {
            return true;
        }
        // Check if it's a directory
        if self.list_dir(path).is_some() {
            return true;
        }
        // Check if it's a known file
        self.generate_content(path).is_some()
    }

    /// Check if a path is a directory
    pub fn is_dir(&self, path: &str) -> bool {
        self.list_dir(path).is_some()
    }

    /// Generate content for sysfs files
    pub fn generate_content(&self, path: &str) -> Option<Vec<u8>> {
        let content = match path {
            "/sys/kernel/hostname" => "axeberg",
            "/sys/kernel/ostype" => "AxebergOS",
            "/sys/kernel/osrelease" => "0.1.0",
            "/sys/kernel/version" => "#1 WASM",
            "/sys/devices/system/cpu/online" => "0",
            "/sys/devices/system/cpu/present" => "0",
            "/sys/power/state" => "mem disk standby freeze\n",
            _ => return None,
        };
        Some(format!("{}\n", content).into_bytes())
    }
}

impl Default for SysFs {
    fn default() -> Self {
        Self::new()
    }
}
