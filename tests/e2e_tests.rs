use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;

mod common;
use common::TestEnvironment;

#[tokio::test]
#[ignore = "requires built binary"]
async fn test_e2e_full_workflow() {
    let env = TestEnvironment::new("e2e_full_workflow");
    let binary_path = env.get_binary_path();
    
    // 1. Initialize Laszoo
    let output = Command::new(&binary_path)
        .args(&["init", "--mfs-mount", env.mfs_mount.to_str().unwrap()])
        .output()
        .expect("Failed to run init");
    
    assert!(output.status.success());
    assert!(env.mfs_mount.join("groups").exists());
    assert!(env.mfs_mount.join("machines").exists());
    
    // 2. Create a test file
    let test_file = env.test_dir.join("app.conf");
    fs::write(&test_file, "port = 8080\nhost = localhost").unwrap();
    
    // 3. Enroll the file
    let output = Command::new(&binary_path)
        .args(&[
            "enroll",
            "webservers",
            test_file.to_str().unwrap(),
            "--action", "converge"
        ])
        .output()
        .expect("Failed to enroll file");
    
    assert!(output.status.success());
    
    // Verify template was created
    let template_path = env.mfs_mount
        .join("groups/webservers")
        .join(format!("{}.lasz", test_file.display()));
    assert!(template_path.exists());
    
    // 4. Join another machine to the group
    let output = Command::new(&binary_path)
        .args(&["group", "webservers", "add"])
        .output()
        .expect("Failed to add to group");
    
    assert!(output.status.success());
    
    // 5. Apply templates
    let output = Command::new(&binary_path)
        .args(&["apply", "webservers"])
        .output()
        .expect("Failed to apply templates");
    
    assert!(output.status.success());
    
    // 6. Check status
    let output = Command::new(&binary_path)
        .args(&["status"])
        .output()
        .expect("Failed to check status");
    
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("webservers"));
}

#[tokio::test]
#[ignore = "requires built binary"]
async fn test_e2e_package_management() {
    let env = TestEnvironment::new("e2e_packages");
    let binary_path = env.get_binary_path();
    
    // Initialize
    Command::new(&binary_path)
        .args(&["init", "--mfs-mount", env.mfs_mount.to_str().unwrap()])
        .output()
        .expect("Failed to init");
    
    // Install packages
    let output = Command::new(&binary_path)
        .args(&[
            "install",
            "webservers",
            "-p", "nginx",
            "-p", "curl"
        ])
        .output()
        .expect("Failed to install packages");
    
    assert!(output.status.success());
    
    // Verify packages.conf was created
    let packages_conf = env.mfs_mount
        .join("groups/webservers/packages.conf");
    assert!(packages_conf.exists());
    
    let content = fs::read_to_string(&packages_conf).unwrap();
    assert!(content.contains("+nginx"));
    assert!(content.contains("+curl"));
}

#[tokio::test]
#[ignore = "requires built binary and watch mode"]
async fn test_e2e_watch_mode() {
    let env = TestEnvironment::new("e2e_watch");
    let binary_path = env.get_binary_path();
    
    // Initialize
    Command::new(&binary_path)
        .args(&["init", "--mfs-mount", env.mfs_mount.to_str().unwrap()])
        .output()
        .expect("Failed to init");
    
    // Create and enroll a file
    let test_file = env.test_dir.join("watch.conf");
    fs::write(&test_file, "initial content").unwrap();
    
    Command::new(&binary_path)
        .args(&[
            "enroll",
            "watchgroup",
            test_file.to_str().unwrap()
        ])
        .output()
        .expect("Failed to enroll");
    
    // Start watch mode in background (would need to spawn and kill)
    // This is complex to test properly without a test harness
    
    // For now, just verify the command exists
    let output = Command::new(&binary_path)
        .args(&["watch", "--help"])
        .output()
        .expect("Failed to run watch help");
    
    assert!(output.status.success());
}

#[tokio::test]
#[ignore = "requires built binary"]
async fn test_e2e_sync_strategies() {
    let env = TestEnvironment::new("e2e_sync");
    let binary_path = env.get_binary_path();
    
    // Initialize
    Command::new(&binary_path)
        .args(&["init", "--mfs-mount", env.mfs_mount.to_str().unwrap()])
        .output()
        .expect("Failed to init");
    
    // Create files with different sync actions
    let converge_file = env.test_dir.join("converge.conf");
    let rollback_file = env.test_dir.join("rollback.conf");
    let freeze_file = env.test_dir.join("freeze.conf");
    let drift_file = env.test_dir.join("drift.conf");
    
    fs::write(&converge_file, "converge content").unwrap();
    fs::write(&rollback_file, "rollback content").unwrap();
    fs::write(&freeze_file, "freeze content").unwrap();
    fs::write(&drift_file, "drift content").unwrap();
    
    // Enroll with different actions
    for (file, action) in &[
        (&converge_file, "converge"),
        (&rollback_file, "rollback"),
        (&freeze_file, "freeze"),
        (&drift_file, "drift"),
    ] {
        let output = Command::new(&binary_path)
            .args(&[
                "enroll",
                "synctest",
                file.to_str().unwrap(),
                "--action", action
            ])
            .output()
            .expect("Failed to enroll");
        
        assert!(output.status.success());
    }
    
    // Modify local files
    fs::write(&converge_file, "modified converge").unwrap();
    fs::write(&rollback_file, "modified rollback").unwrap();
    fs::write(&freeze_file, "modified freeze").unwrap();
    fs::write(&drift_file, "modified drift").unwrap();
    
    // Run sync
    let output = Command::new(&binary_path)
        .args(&["sync", "--group", "synctest"])
        .output()
        .expect("Failed to sync");
    
    assert!(output.status.success());
    
    // Verify behaviors:
    // - Converge: template should be updated
    // - Rollback: local file should be restored
    // - Freeze: no changes
    // - Drift: changes tracked but not applied
}

#[tokio::test]
#[ignore = "requires built binary"]
async fn test_e2e_group_management() {
    let env = TestEnvironment::new("e2e_groups");
    let binary_path = env.get_binary_path();
    
    // Initialize
    Command::new(&binary_path)
        .args(&["init", "--mfs-mount", env.mfs_mount.to_str().unwrap()])
        .output()
        .expect("Failed to init");
    
    // Create a group by adding a machine
    let output = Command::new(&binary_path)
        .args(&["group", "testgroup", "add"])
        .output()
        .expect("Failed to add to group");
    
    assert!(output.status.success());
    
    // List groups
    let output = Command::new(&binary_path)
        .args(&["groups", "list"])
        .output()
        .expect("Failed to list groups");
    
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("testgroup"));
    
    // List machines in group
    let output = Command::new(&binary_path)
        .args(&["group", "testgroup", "list"])
        .output()
        .expect("Failed to list group machines");
    
    assert!(output.status.success());
    
    // Remove from group
    let output = Command::new(&binary_path)
        .args(&["group", "testgroup", "remove"])
        .output()
        .expect("Failed to remove from group");
    
    assert!(output.status.success());
}

mod common {
    use super::*;
    
    impl TestEnvironment {
        pub fn get_binary_path(&self) -> PathBuf {
            // Assume we're running from project root
            let mut path = std::env::current_dir().unwrap();
            path.push("target/release/laszoo");
            
            if !path.exists() {
                path.pop();
                path.pop();
                path.push("debug/laszoo");
            }
            
            if !path.exists() {
                panic!("Laszoo binary not found. Run 'cargo build --release' first.");
            }
            
            path
        }
    }
}