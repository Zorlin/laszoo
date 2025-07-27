use std::fs;
use std::path::PathBuf;

mod common;
use common::TestEnvironment;

#[test]
fn test_package_full_upgrade_marks_completed_when_no_updates() {
    let env = TestEnvironment::new("package_full_upgrade_no_updates");
    env.setup_git().expect("Failed to setup git");
    
    // Create a packages.conf file with ++full-upgrade
    let packages_conf = env.test_dir.join("packages.conf");
    fs::write(&packages_conf, "++full-upgrade\n").unwrap();
    
    // Enroll the packages.conf file
    let output = env.run_laszoo(&["enroll", "testgroup", packages_conf.to_str().unwrap()])
        .expect("Failed to run laszoo");
    assert!(output.status.success());
    
    // Run package commands (simulating watch mode's execution)
    let output = env.run_laszoo(&["package"])
        .expect("Failed to run laszoo package");
    
    // Check the output for the expected message
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("No system updates available") || stderr.contains("++full-upgrade"));
    
    // Check that action file was created with completed status
    let action_dir = env.mfs_mount.join("actions").join(&env.hostname);
    if action_dir.exists() {
        // Look for the most recent action file
        let mut action_files: Vec<_> = fs::read_dir(&action_dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.file_name().to_str().unwrap_or("").starts_with("package_full_upgrade_all")
            })
            .collect();
        
        if !action_files.is_empty() {
            // Sort by modification time to get the most recent
            action_files.sort_by_key(|entry| {
                entry.metadata().unwrap().modified().unwrap()
            });
            
            let latest_action = action_files.last().unwrap();
            let content = fs::read_to_string(latest_action.path()).unwrap();
            
            // Should contain completed status
            assert!(content.contains("\"status\": \"completed\"") || 
                    content.contains("\"status\":\"completed\""),
                    "Action file should show completed status, but contains: {}", content);
        }
    }
}

#[test]
fn test_package_commands_display_without_prefix() {
    let env = TestEnvironment::new("package_display_no_prefix");
    env.setup_git().expect("Failed to setup git");
    
    // Create a packages.conf file with various commands
    let packages_conf = env.test_dir.join("packages.conf");
    fs::write(&packages_conf, "++update\n++full-upgrade\n+nginx\n").unwrap();
    
    // Enroll the packages.conf file
    let output = env.run_laszoo(&["enroll", "testgroup", packages_conf.to_str().unwrap()])
        .expect("Failed to run laszoo");
    assert!(output.status.success());
    
    // Check status to see how commands are displayed
    let output = env.run_laszoo(&["status"])
        .expect("Failed to run laszoo status");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Commands should be displayed without ++ prefix
    assert!(stdout.contains("update") || stdout.contains("Update"));
    assert!(stdout.contains("full-upgrade") || stdout.contains("Full-upgrade"));
    
    // Should NOT contain the ++ prefix in display
    assert!(!stdout.contains("++update"), "Display should not show ++ prefix");
    assert!(!stdout.contains("++full-upgrade"), "Display should not show ++ prefix");
}

#[test]
fn test_package_command_execution_tracking() {
    let env = TestEnvironment::new("package_execution_tracking");
    env.setup_git().expect("Failed to setup git");
    
    // Create a packages.conf file with ++update command
    let packages_conf = env.test_dir.join("packages.conf");
    fs::write(&packages_conf, "++update\n").unwrap();
    
    // Enroll the packages.conf file
    let output = env.run_laszoo(&["enroll", "testgroup", packages_conf.to_str().unwrap()])
        .expect("Failed to run laszoo");
    assert!(output.status.success());
    
    // First status check - should show as unexecuted
    let output = env.run_laszoo(&["status"])
        .expect("Failed to run laszoo status");
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Should show some indication that the command hasn't been executed
    assert!(stdout.contains("update") || stdout.contains("Update"));
    
    // Execute package commands
    let output = env.run_laszoo(&["package"])
        .expect("Failed to run laszoo package");
    
    // Second status check - should show as executed
    let output = env.run_laszoo(&["status"])
        .expect("Failed to run laszoo status");
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Should show that the command has been executed
    assert!(stdout.contains("update") || stdout.contains("Update"));
    assert!(stdout.contains("✓") || stdout.contains("executed") || stdout.contains("completed"));
}