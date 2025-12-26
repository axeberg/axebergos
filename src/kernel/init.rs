//! Init system (PID 1)
//!
//! Provides basic service management and system initialization.
//! Acts as the first process, spawning and managing services.

use std::collections::HashMap;

/// Service state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    /// Service is stopped
    Stopped,
    /// Service is starting
    Starting,
    /// Service is running
    Running,
    /// Service is stopping
    Stopping,
    /// Service has failed
    Failed,
}

/// Service type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceType {
    /// Simple one-shot service
    Oneshot,
    /// Long-running daemon
    Simple,
    /// Forking daemon (not supported in WASM, treated as Simple)
    Forking,
}

/// Service configuration
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    /// Service name
    pub name: String,
    /// Description
    pub description: String,
    /// Command to run
    pub exec_start: String,
    /// Command to run on stop (optional)
    pub exec_stop: Option<String>,
    /// Service type
    pub service_type: ServiceType,
    /// Services that must start before this one
    pub after: Vec<String>,
    /// Services that want this to start with them
    pub wanted_by: Vec<String>,
    /// Restart policy
    pub restart: RestartPolicy,
    /// Environment variables
    pub environment: HashMap<String, String>,
    /// Working directory
    pub working_directory: Option<String>,
}

impl ServiceConfig {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: String::new(),
            exec_start: String::new(),
            exec_stop: None,
            service_type: ServiceType::Simple,
            after: Vec::new(),
            wanted_by: Vec::new(),
            restart: RestartPolicy::No,
            environment: HashMap::new(),
            working_directory: None,
        }
    }
}

/// Restart policy for services
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartPolicy {
    /// Don't restart
    No,
    /// Restart on failure
    OnFailure,
    /// Always restart
    Always,
}

/// A running service
#[derive(Debug, Clone)]
pub struct Service {
    /// Configuration
    pub config: ServiceConfig,
    /// Current state
    pub state: ServiceState,
    /// Process ID (if running)
    pub pid: Option<u32>,
    /// Exit code (if stopped)
    pub exit_code: Option<i32>,
    /// Number of restarts
    pub restart_count: u32,
}

impl Service {
    pub fn new(config: ServiceConfig) -> Self {
        Self {
            config,
            state: ServiceState::Stopped,
            pid: None,
            exit_code: None,
            restart_count: 0,
        }
    }
}

/// System runlevel/target
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Rescue mode (single-user)
    Rescue,
    /// Multi-user mode
    MultiUser,
    /// Graphical mode
    Graphical,
    /// Reboot
    Reboot,
    /// Poweroff
    Poweroff,
}

impl Target {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "rescue" | "rescue.target" => Some(Target::Rescue),
            "multi-user" | "multi-user.target" => Some(Target::MultiUser),
            "graphical" | "graphical.target" => Some(Target::Graphical),
            "reboot" | "reboot.target" => Some(Target::Reboot),
            "poweroff" | "poweroff.target" => Some(Target::Poweroff),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Target::Rescue => "rescue.target",
            Target::MultiUser => "multi-user.target",
            Target::Graphical => "graphical.target",
            Target::Reboot => "reboot.target",
            Target::Poweroff => "poweroff.target",
        }
    }
}

/// The init system
pub struct InitSystem {
    /// Registered services
    services: HashMap<String, Service>,
    /// Current target
    target: Target,
    /// System hostname
    hostname: String,
    /// Boot time (monotonic)
    boot_time: f64,
    /// Whether system is shutting down
    shutting_down: bool,
}

impl InitSystem {
    pub fn new() -> Self {
        let mut init = Self {
            services: HashMap::new(),
            target: Target::MultiUser,
            hostname: "axeberg".to_string(),
            boot_time: 0.0,
            shutting_down: false,
        };

        // Register built-in services
        init.register_builtin_services();
        init
    }

    /// Register built-in services
    fn register_builtin_services(&mut self) {
        // Shell service
        let mut shell = ServiceConfig::new("shell");
        shell.description = "Interactive Shell".to_string();
        shell.exec_start = "/bin/sh".to_string();
        shell.service_type = ServiceType::Simple;
        shell.wanted_by.push("multi-user.target".to_string());
        self.register_service(shell);

        // TTY service
        let mut tty = ServiceConfig::new("tty");
        tty.description = "Virtual Console".to_string();
        tty.exec_start = "/sbin/agetty".to_string();
        tty.service_type = ServiceType::Simple;
        tty.after.push("shell".to_string());
        tty.wanted_by.push("multi-user.target".to_string());
        self.register_service(tty);
    }

    /// Register a service
    pub fn register_service(&mut self, config: ServiceConfig) {
        let name = config.name.clone();
        self.services.insert(name, Service::new(config));
    }

    /// Get a service by name
    pub fn get_service(&self, name: &str) -> Option<&Service> {
        self.services.get(name)
    }

    /// Get all services
    pub fn list_services(&self) -> Vec<&Service> {
        self.services.values().collect()
    }

    /// Start a service
    pub fn start_service(&mut self, name: &str) -> Result<(), String> {
        // First, check if service exists and get dependencies
        let (is_running, deps) = {
            let service = self.services.get(name)
                .ok_or_else(|| format!("Service '{}' not found", name))?;
            (service.state == ServiceState::Running, service.config.after.clone())
        };

        if is_running {
            return Ok(()); // Already running
        }

        // Check dependencies (without holding any borrows)
        for dep in &deps {
            if let Some(dep_svc) = self.services.get(dep)
                && dep_svc.state != ServiceState::Running {
                    return Err(format!("Dependency '{}' not running", dep));
                }
        }

        // Now get the length before borrowing mutably
        let pid = 1000 + self.services.len() as u32;

        // Now get mutable reference and update
        let service = self.services.get_mut(name).unwrap();
        service.state = ServiceState::Running;
        service.pid = Some(pid);

        Ok(())
    }

    /// Stop a service
    pub fn stop_service(&mut self, name: &str) -> Result<(), String> {
        let service = self.services.get_mut(name)
            .ok_or_else(|| format!("Service '{}' not found", name))?;

        if service.state == ServiceState::Stopped {
            return Ok(()); // Already stopped
        }

        service.state = ServiceState::Stopping;
        // In a real implementation, we would signal the process
        service.state = ServiceState::Stopped;
        service.pid = None;

        Ok(())
    }

    /// Restart a service
    pub fn restart_service(&mut self, name: &str) -> Result<(), String> {
        self.stop_service(name)?;
        self.start_service(name)
    }

    /// Enable a service (to start at boot)
    pub fn enable_service(&mut self, name: &str) -> Result<(), String> {
        let service = self.services.get_mut(name)
            .ok_or_else(|| format!("Service '{}' not found", name))?;

        if !service.config.wanted_by.contains(&"multi-user.target".to_string()) {
            service.config.wanted_by.push("multi-user.target".to_string());
        }
        Ok(())
    }

    /// Disable a service
    pub fn disable_service(&mut self, name: &str) -> Result<(), String> {
        let service = self.services.get_mut(name)
            .ok_or_else(|| format!("Service '{}' not found", name))?;

        service.config.wanted_by.retain(|t| t != "multi-user.target");
        Ok(())
    }

    /// Get service status
    pub fn service_status(&self, name: &str) -> Option<ServiceStatus> {
        self.services.get(name).map(|s| ServiceStatus {
            name: s.config.name.clone(),
            description: s.config.description.clone(),
            state: s.state,
            pid: s.pid,
            exit_code: s.exit_code,
        })
    }

    /// Set system target
    pub fn set_target(&mut self, target: Target) {
        self.target = target;

        match target {
            Target::Reboot | Target::Poweroff => {
                self.shutting_down = true;
                // Stop all services
                let names: Vec<String> = self.services.keys().cloned().collect();
                for name in names {
                    let _ = self.stop_service(&name);
                }
            }
            _ => {
                // Start services wanted by this target
                let target_str = target.as_str().to_string();
                let to_start: Vec<String> = self.services
                    .values()
                    .filter(|s| s.config.wanted_by.contains(&target_str))
                    .map(|s| s.config.name.clone())
                    .collect();

                for name in to_start {
                    let _ = self.start_service(&name);
                }
            }
        }
    }

    /// Get current target
    pub fn get_target(&self) -> Target {
        self.target
    }

    /// Check if system is shutting down
    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down
    }

    /// Get hostname
    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    /// Set hostname
    pub fn set_hostname(&mut self, hostname: &str) {
        self.hostname = hostname.to_string();
    }

    /// Get boot time
    pub fn boot_time(&self) -> f64 {
        self.boot_time
    }

    /// Set boot time
    pub fn set_boot_time(&mut self, time: f64) {
        self.boot_time = time;
    }

    /// Reap zombie processes (called periodically)
    pub fn reap_zombies(&mut self, zombies: &[(u32, i32)]) {
        for (pid, exit_code) in zombies {
            // Find service with this PID
            for service in self.services.values_mut() {
                if service.pid == Some(*pid) {
                    service.state = ServiceState::Stopped;
                    service.exit_code = Some(*exit_code);
                    service.pid = None;

                    // Handle restart policy
                    match service.config.restart {
                        RestartPolicy::Always => {
                            service.restart_count += 1;
                            service.state = ServiceState::Starting;
                        }
                        RestartPolicy::OnFailure if *exit_code != 0 => {
                            service.restart_count += 1;
                            service.state = ServiceState::Starting;
                        }
                        _ => {}
                    }
                    break;
                }
            }
        }
    }
}

impl Default for InitSystem {
    fn default() -> Self {
        Self::new()
    }
}

/// Service status information
#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub name: String,
    pub description: String,
    pub state: ServiceState,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_system_new() {
        let init = InitSystem::new();
        assert_eq!(init.target, Target::MultiUser);
        assert!(!init.shutting_down);
        assert!(!init.services.is_empty()); // Has builtin services
    }

    #[test]
    fn test_service_registration() {
        let mut init = InitSystem::new();

        let mut config = ServiceConfig::new("test-service");
        config.description = "Test Service".to_string();
        config.exec_start = "/bin/test".to_string();

        init.register_service(config);

        let service = init.get_service("test-service").unwrap();
        assert_eq!(service.config.name, "test-service");
        assert_eq!(service.state, ServiceState::Stopped);
    }

    #[test]
    fn test_service_start_stop() {
        let mut init = InitSystem::new();

        let config = ServiceConfig::new("test");
        init.register_service(config);

        init.start_service("test").unwrap();
        assert_eq!(init.get_service("test").unwrap().state, ServiceState::Running);

        init.stop_service("test").unwrap();
        assert_eq!(init.get_service("test").unwrap().state, ServiceState::Stopped);
    }

    #[test]
    fn test_target_parsing() {
        assert_eq!(Target::parse("rescue"), Some(Target::Rescue));
        assert_eq!(Target::parse("multi-user.target"), Some(Target::MultiUser));
        assert_eq!(Target::parse("graphical"), Some(Target::Graphical));
        assert_eq!(Target::parse("invalid"), None);
    }

    #[test]
    fn test_shutdown() {
        let mut init = InitSystem::new();
        init.set_target(Target::Poweroff);
        assert!(init.is_shutting_down());
    }

    #[test]
    fn test_hostname() {
        let mut init = InitSystem::new();
        assert_eq!(init.hostname(), "axeberg");

        init.set_hostname("test-host");
        assert_eq!(init.hostname(), "test-host");
    }

    #[test]
    fn test_enable_disable() {
        let mut init = InitSystem::new();

        let mut config = ServiceConfig::new("test");
        config.wanted_by.clear(); // Start with no targets
        init.register_service(config);

        init.enable_service("test").unwrap();
        let service = init.get_service("test").unwrap();
        assert!(service.config.wanted_by.contains(&"multi-user.target".to_string()));

        init.disable_service("test").unwrap();
        let service = init.get_service("test").unwrap();
        assert!(!service.config.wanted_by.contains(&"multi-user.target".to_string()));
    }
}
