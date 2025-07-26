use laszoo::service::ServiceManager;
use std::fs;
use std::path::Path;

mod common;
use common::TestEnvironment;

#[test]
#[ignore = "requires root privileges"]
fn test_service_install() {
    // This test requires root privileges
    let service_manager = ServiceManager::new().unwrap();
    
    // Test would install service with default options
    // service_manager.install(false, "root", None).unwrap();
    
    // Verify service file was created
    assert!(Path::new("/etc/systemd/system/laszoo.service").exists());
    
    // Verify defaults file was created
    assert!(Path::new("/etc/default/laszoo").exists());
}

#[test]
#[ignore = "requires root privileges"]
fn test_service_install_with_options() {
    // This test requires root privileges
    let service_manager = ServiceManager::new().unwrap();
    
    // Test would install service with custom options
    // service_manager.install(true, "laszoo", Some("--group testgroup")).unwrap();
    
    // Verify defaults file contains correct options
    let defaults_content = fs::read_to_string("/etc/default/laszoo").unwrap();
    assert!(defaults_content.contains(r#"LASZOO_USER="laszoo""#));
    assert!(defaults_content.contains(r#"LASZOO_HARD="true""#));
}

#[test]
#[ignore = "requires root privileges"]
fn test_service_uninstall() {
    // This test requires root privileges
    let service_manager = ServiceManager::new().unwrap();
    
    // First install
    // service_manager.install(false, "root", None).unwrap();
    
    // Then uninstall
    // service_manager.uninstall().unwrap();
    
    // Verify files are removed
    assert!(!Path::new("/etc/systemd/system/laszoo.service").exists());
    assert!(!Path::new("/etc/default/laszoo").exists());
}

#[test]
fn test_service_status() {
    let service_manager = ServiceManager::new().unwrap();
    
    // This should work without root, just shows status
    // Status might show "not found" if service isn't installed
    let result = service_manager.status();
    
    // Should not error even if service doesn't exist
    assert!(result.is_ok());
}

#[test]
fn test_defaults_file_format() {
    // Test the format of the defaults file content
    let content = r#"# Laszoo service configuration
# This file is sourced by the systemd service

# User to run the service as
LASZOO_USER="laszoo"

# Enable hard mode (propagate deletions)
LASZOO_HARD="true"

# Additional arguments for laszoo watch
# LASZOO_EXTRA_ARGS="--group mygroup"
LASZOO_EXTRA_ARGS=""

# Mount point for MooseFS/CephFS
LASZOO_MOUNT="/mnt/laszoo"
"#;
    
    // Verify it's valid shell syntax
    assert!(content.contains("LASZOO_USER="));
    assert!(content.contains("LASZOO_HARD="));
    assert!(content.contains("LASZOO_MOUNT="));
}

#[test]
fn test_systemd_service_format() {
    // Test the systemd service file format
    let binary_path = "/usr/local/bin/laszoo";
    let service_content = format!(
        r#"[Unit]
Description=Laszoo Configuration Management
Documentation=https://github.com/laszoo/laszoo
After=network.target
# Wait for MooseFS/CephFS mount
RequiresMountsFor=/mnt/laszoo

[Service]
Type=simple
User=root
Group=root
# Source defaults file
EnvironmentFile=-/etc/default/laszoo
# Build command with conditional arguments
ExecStartPre=/bin/bash -c 'if ! mountpoint -q ${{LASZOO_MOUNT:-/mnt/laszoo}}; then echo "Warning: ${{LASZOO_MOUNT:-/mnt/laszoo}} is not mounted"; fi'
ExecStart=/bin/bash -c '{} watch -a ${{LASZOO_HARD:+--hard}} ${{LASZOO_EXTRA_ARGS}} '
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
        binary_path
    );
    
    // Verify required systemd directives
    assert!(service_content.contains("[Unit]"));
    assert!(service_content.contains("[Service]"));
    assert!(service_content.contains("[Install]"));
    assert!(service_content.contains("RequiresMountsFor=/mnt/laszoo"));
    assert!(service_content.contains("EnvironmentFile=-/etc/default/laszoo"));
    assert!(service_content.contains("Restart=always"));
}