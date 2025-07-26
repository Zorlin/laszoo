use crate::error::{LaszooError, Result};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

pub struct ServiceManager {
    binary_path: String,
}

impl ServiceManager {
    pub fn new() -> Result<Self> {
        // Get the path to the current executable
        let binary_path = std::env::current_exe()
            .map_err(|e| LaszooError::Other(format!("Failed to get current executable path: {}", e)))?
            .to_string_lossy()
            .to_string();
        
        Ok(Self { binary_path })
    }
    
    pub fn install(&self, hard: bool, user: &str, extra_args: Option<&str>) -> Result<()> {
        // Check if running as root
        if !self.is_root() {
            return Err(LaszooError::Other(
                "Service installation requires root privileges. Please run with sudo.".to_string()
            ));
        }
        
        // Create /etc/default/laszoo
        self.create_defaults_file(hard, user)?;
        
        // Create systemd service file
        self.create_service_file(user, extra_args)?;
        
        // Reload systemd and enable service
        self.reload_systemd()?;
        self.enable_service()?;
        self.start_service()?;
        
        println!("✓ Laszoo service installed and started successfully");
        println!("  - Service runs as user: {}", user);
        if hard {
            println!("  - Hard mode enabled (propagates deletions)");
        }
        println!("\nUse 'systemctl status laszoo' to check service status");
        
        Ok(())
    }
    
    pub fn uninstall(&self) -> Result<()> {
        if !self.is_root() {
            return Err(LaszooError::Other(
                "Service uninstallation requires root privileges. Please run with sudo.".to_string()
            ));
        }
        
        // Stop and disable service
        let _ = self.stop_service();
        let _ = self.disable_service();
        
        // Remove service file
        let service_path = "/etc/systemd/system/laszoo.service";
        if Path::new(service_path).exists() {
            fs::remove_file(service_path)?;
        }
        
        // Remove defaults file
        let defaults_path = "/etc/default/laszoo";
        if Path::new(defaults_path).exists() {
            fs::remove_file(defaults_path)?;
        }
        
        // Reload systemd
        self.reload_systemd()?;
        
        println!("✓ Laszoo service uninstalled successfully");
        
        Ok(())
    }
    
    pub fn status(&self) -> Result<()> {
        let output = Command::new("systemctl")
            .args(&["status", "laszoo", "--no-pager"])
            .output()
            .map_err(|e| LaszooError::Other(format!("Failed to check service status: {}", e)))?;
        
        // Print output regardless of exit status
        print!("{}", String::from_utf8_lossy(&output.stdout));
        if !output.stderr.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
        }
        
        Ok(())
    }
    
    fn is_root(&self) -> bool {
        unsafe { libc::geteuid() == 0 }
    }
    
    fn create_defaults_file(&self, hard: bool, user: &str) -> Result<()> {
        let content = format!(
            r#"# Laszoo service configuration
# This file is sourced by the systemd service

# User to run the service as
LASZOO_USER="{}"

# Enable hard mode (propagate deletions)
LASZOO_HARD="{}"

# Additional arguments for laszoo watch
# LASZOO_EXTRA_ARGS="--group mygroup"
LASZOO_EXTRA_ARGS=""

# Mount point for MooseFS/CephFS
LASZOO_MOUNT="/mnt/laszoo"
"#,
            user,
            if hard { "true" } else { "false" }
        );
        
        let path = "/etc/default/laszoo";
        let mut file = fs::File::create(path)
            .map_err(|e| LaszooError::Other(format!("Failed to create {}: {}", path, e)))?;
        
        file.write_all(content.as_bytes())
            .map_err(|e| LaszooError::Other(format!("Failed to write {}: {}", path, e)))?;
        
        Ok(())
    }
    
    fn create_service_file(&self, user: &str, extra_args: Option<&str>) -> Result<()> {
        let service_content = format!(
            r#"[Unit]
Description=Laszoo Configuration Management
Documentation=https://github.com/laszoo/laszoo
After=network.target
# Wait for MooseFS/CephFS mount
RequiresMountsFor=/mnt/laszoo

[Service]
Type=simple
User={user}
Group={user}
# Source defaults file
EnvironmentFile=-/etc/default/laszoo
# Build command with conditional arguments
ExecStartPre=/bin/bash -c 'if ! mountpoint -q ${{LASZOO_MOUNT:-/mnt/laszoo}}; then echo "Warning: ${{LASZOO_MOUNT:-/mnt/laszoo}} is not mounted"; fi'
ExecStart=/bin/bash -c '{binary} watch -a ${{LASZOO_HARD:+--hard}} ${{LASZOO_EXTRA_ARGS}} {extra}'
Restart=always
RestartSec=30
# Restart if MooseFS/CephFS becomes unavailable
RestartPreventExitStatus=
# Kill only the main process
KillMode=process
# Give it time to finish current operations
TimeoutStopSec=60
# Log to journal
StandardOutput=journal
StandardError=journal
# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectHome=false
ProtectSystem=false
# Need filesystem access
ReadWritePaths=/

[Install]
WantedBy=multi-user.target
"#,
            user = user,
            binary = self.binary_path,
            extra = extra_args.unwrap_or("")
        );
        
        let path = "/etc/systemd/system/laszoo.service";
        let mut file = fs::File::create(path)
            .map_err(|e| LaszooError::Other(format!("Failed to create {}: {}", path, e)))?;
        
        file.write_all(service_content.as_bytes())
            .map_err(|e| LaszooError::Other(format!("Failed to write {}: {}", path, e)))?;
        
        Ok(())
    }
    
    fn reload_systemd(&self) -> Result<()> {
        let output = Command::new("systemctl")
            .arg("daemon-reload")
            .output()
            .map_err(|e| LaszooError::Other(format!("Failed to reload systemd: {}", e)))?;
        
        if !output.status.success() {
            return Err(LaszooError::Other(
                format!("Failed to reload systemd: {}", String::from_utf8_lossy(&output.stderr))
            ));
        }
        
        Ok(())
    }
    
    fn enable_service(&self) -> Result<()> {
        let output = Command::new("systemctl")
            .args(&["enable", "laszoo.service"])
            .output()
            .map_err(|e| LaszooError::Other(format!("Failed to enable service: {}", e)))?;
        
        if !output.status.success() {
            return Err(LaszooError::Other(
                format!("Failed to enable service: {}", String::from_utf8_lossy(&output.stderr))
            ));
        }
        
        Ok(())
    }
    
    fn disable_service(&self) -> Result<()> {
        let output = Command::new("systemctl")
            .args(&["disable", "laszoo.service"])
            .output()
            .map_err(|e| LaszooError::Other(format!("Failed to disable service: {}", e)))?;
        
        // Ignore errors for disable - service might not exist
        Ok(())
    }
    
    fn start_service(&self) -> Result<()> {
        let output = Command::new("systemctl")
            .args(&["start", "laszoo.service"])
            .output()
            .map_err(|e| LaszooError::Other(format!("Failed to start service: {}", e)))?;
        
        if !output.status.success() {
            return Err(LaszooError::Other(
                format!("Failed to start service: {}", String::from_utf8_lossy(&output.stderr))
            ));
        }
        
        Ok(())
    }
    
    fn stop_service(&self) -> Result<()> {
        let output = Command::new("systemctl")
            .args(&["stop", "laszoo.service"])
            .output()
            .map_err(|e| LaszooError::Other(format!("Failed to stop service: {}", e)))?;
        
        // Ignore errors for stop - service might not be running
        Ok(())
    }
}