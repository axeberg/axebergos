//! /dev virtual filesystem
//!
//! Provides device file abstractions. Most devices are simulated
//! for the WASM environment.

use std::collections::HashSet;

/// Device filesystem manager
pub struct DevFs {
    /// Available devices
    devices: HashSet<&'static str>,
}

impl DevFs {
    pub fn new() -> Self {
        let mut devices = HashSet::new();
        // Standard devices
        devices.insert("console");
        devices.insert("null");
        devices.insert("zero");
        devices.insert("random");
        devices.insert("urandom");
        devices.insert("stdin");
        devices.insert("stdout");
        devices.insert("stderr");
        devices.insert("tty");
        devices.insert("ptmx");
        devices.insert("fd");  // Directory - symlinks to /proc/self/fd

        Self { devices }
    }

    /// Check if a path is in /dev
    pub fn is_dev_path(path: &str) -> bool {
        path == "/dev" || path.starts_with("/dev/")
    }

    /// List directory contents
    pub fn list_dir(&self, path: &str) -> Option<Vec<String>> {
        if path == "/dev" {
            let entries: Vec<String> = self.devices.iter().map(|s| s.to_string()).collect();
            Some(entries)
        } else if path == "/dev/fd" {
            // Would list open file descriptors
            Some(vec!["0".to_string(), "1".to_string(), "2".to_string()])
        } else {
            None
        }
    }

    /// Check if a path exists in /dev
    pub fn exists(&self, path: &str) -> bool {
        if path == "/dev" {
            return true;
        }
        if let Some(name) = path.strip_prefix("/dev/") {
            // Handle nested paths like /dev/fd/0
            if name.starts_with("fd/") {
                return true; // Simplified - assume fd paths exist
            }
            self.devices.contains(name)
        } else {
            false
        }
    }

    /// Check if a path is a directory
    pub fn is_dir(&self, path: &str) -> bool {
        path == "/dev" || path == "/dev/fd"
    }

    /// Get device info
    pub fn device_info(&self, name: &str) -> Option<DeviceInfo> {
        match name {
            "console" => Some(DeviceInfo {
                dev_type: DeviceType::Char,
                major: 5,
                minor: 1,
                mode: 0o620,
            }),
            "null" => Some(DeviceInfo {
                dev_type: DeviceType::Char,
                major: 1,
                minor: 3,
                mode: 0o666,
            }),
            "zero" => Some(DeviceInfo {
                dev_type: DeviceType::Char,
                major: 1,
                minor: 5,
                mode: 0o666,
            }),
            "random" => Some(DeviceInfo {
                dev_type: DeviceType::Char,
                major: 1,
                minor: 8,
                mode: 0o666,
            }),
            "urandom" => Some(DeviceInfo {
                dev_type: DeviceType::Char,
                major: 1,
                minor: 9,
                mode: 0o666,
            }),
            "tty" => Some(DeviceInfo {
                dev_type: DeviceType::Char,
                major: 5,
                minor: 0,
                mode: 0o666,
            }),
            "ptmx" => Some(DeviceInfo {
                dev_type: DeviceType::Char,
                major: 5,
                minor: 2,
                mode: 0o666,
            }),
            "stdin" | "stdout" | "stderr" => Some(DeviceInfo {
                dev_type: DeviceType::Symlink,
                major: 0,
                minor: 0,
                mode: 0o777,
            }),
            _ => None,
        }
    }
}

impl Default for DevFs {
    fn default() -> Self {
        Self::new()
    }
}

/// Device type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeviceType {
    Char,
    Block,
    Symlink,
}

/// Device information
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub dev_type: DeviceType,
    pub major: u32,
    pub minor: u32,
    pub mode: u16,
}
