mod common;

use common::*;
use std::time::Duration;
use std::thread;

#[test]
fn test_watch_mode_local_changes() {
    let env = TestEnvironment::new("watch_local");
    env.setup_git().expect("Failed to setup git");
    
    // Create and enroll a file
    let test_file = env.create_test_file("watch.conf", "initial content");
    let output = env.run_laszoo(&["enroll", "watchgroup", test_file.to_str().unwrap()])
        .expect("Failed to run laszoo");
    assert!(output.status.success());
    
    // Start watch mode in background
    let env_clone = TestEnvironment {
        test_dir: env.test_dir.clone(),
        mfs_mount: env.mfs_mount.clone(),
        hostname: env.hostname.clone(),
        original_hostname: env.original_hostname.clone(),
    };
    
    let watch_thread = thread::spawn(move || {
        let output = env_clone.run_laszoo(&["watch", "-a", "--hard"])
            .expect("Failed to run watch");
        output
    });
    
    // Give watch mode time to start
    thread::sleep(Duration::from_secs(2));
    
    // Modify the file
    std::fs::write(&test_file, "modified content").unwrap();
    
    // Wait for template to be updated
    let template_path = env.mfs_mount
        .join("groups")
        .join("watchgroup")
        .join("watch.conf.lasz");
    
    let updated = wait_for(|| {
        env.file_exists(&template_path) && 
        env.read_file(&template_path) == "modified content"
    }, 5);
    
    assert!(updated, "Template was not updated after file change");
    
    // TODO: Properly stop watch thread (would need process management)
}

#[test]
fn test_converge_sync_action() {
    let env = TestEnvironment::new("converge");
    env.setup_git().expect("Failed to setup git");
    
    // Create a file and enroll with converge action
    let test_file = env.create_test_file("converge.conf", "original");
    
    // First set the sync action for the group
    let _ = env.run_laszoo(&["group", "converge-group", "config", "--action", "converge"])
        .expect("Failed to configure group");
    
    let output = env.run_laszoo(&["enroll", "converge-group", test_file.to_str().unwrap()])
        .expect("Failed to run laszoo");
    assert!(output.status.success());
    
    // Modify the local file
    std::fs::write(&test_file, "locally modified").unwrap();
    
    // Run sync
    let output = env.run_laszoo(&["sync", "converge-group"])
        .expect("Failed to run sync");
    assert!(output.status.success());
    
    // Check that template was updated (converge behavior)
    let template_path = env.mfs_mount
        .join("groups")
        .join("converge-group")
        .join("converge.conf.lasz");
    
    assert_eq!(env.read_file(&template_path), "locally modified", 
        "Template should be updated in converge mode");
}

#[test]
fn test_rollback_sync_action() {
    let env = TestEnvironment::new("rollback");
    env.setup_git().expect("Failed to setup git");
    
    // Create and enroll file
    let test_file = env.create_test_file("rollback.conf", "original content");
    
    // Configure group for rollback
    let _ = env.run_laszoo(&["group", "rollback-group", "config", "--action", "rollback"])
        .expect("Failed to configure group");
    
    let output = env.run_laszoo(&["enroll", "rollback-group", test_file.to_str().unwrap()])
        .expect("Failed to enroll");
    assert!(output.status.success());
    
    // Modify the local file
    std::fs::write(&test_file, "unauthorized change").unwrap();
    
    // Run sync
    let output = env.run_laszoo(&["sync", "rollback-group"])
        .expect("Failed to run sync");
    assert!(output.status.success());
    
    // Check that file was rolled back
    assert_eq!(env.read_file(&test_file), "original content",
        "File should be rolled back to template version");
}

#[test]
fn test_remote_template_application() {
    let env1 = TestEnvironment::new("remote_apply");
    env1.setup_git().expect("Failed to setup git");
    
    // Create second machine sharing same MFS
    let env2 = create_second_machine(&env1, "machine2");
    
    // Machine 1: Create and enroll a file
    let file1 = env1.create_test_file("shared.conf", "machine1 content");
    let output = env1.run_laszoo(&["enroll", "sharedgroup", file1.to_str().unwrap()])
        .expect("Failed to enroll on machine1");
    assert!(output.status.success());
    
    // Machine 2: Join the group
    let output = env2.run_laszoo(&["enroll", "sharedgroup"])
        .expect("Failed to join group on machine2");
    assert!(output.status.success());
    
    // Machine 2 should now have the file
    let file2 = env2.test_dir.join("shared.conf");
    assert!(wait_for(|| env2.file_exists(&file2), 5), "File not created on machine2");
    assert_eq!(env2.read_file(&file2), "machine1 content");
    
    // Machine 1: Modify the file
    std::fs::write(&file1, "updated from machine1").unwrap();
    
    // Update template (simulate watch mode behavior)
    let template_path = env1.mfs_mount
        .join("groups")
        .join("sharedgroup")
        .join("shared.conf.lasz");
    std::fs::write(&template_path, "updated from machine1").unwrap();
    
    // Machine 2: Apply changes
    let output = env2.run_laszoo(&["apply", "sharedgroup"])
        .expect("Failed to apply on machine2");
    assert!(output.status.success());
    
    // Check that machine2 got the update
    assert_eq!(env2.read_file(&file2), "updated from machine1",
        "Machine2 should have received the update");
}

#[test]
fn test_freeze_sync_action() {
    let env = TestEnvironment::new("freeze");
    env.setup_git().expect("Failed to setup git");
    
    // Create and enroll file with freeze action
    let test_file = env.create_test_file("frozen.conf", "frozen content");
    
    let _ = env.run_laszoo(&["group", "freeze-group", "config", "--action", "freeze"])
        .expect("Failed to configure group");
    
    let output = env.run_laszoo(&["enroll", "freeze-group", test_file.to_str().unwrap()])
        .expect("Failed to enroll");
    assert!(output.status.success());
    
    // Try to modify the file
    std::fs::write(&test_file, "attempted change").unwrap();
    
    // Run sync
    let output = env.run_laszoo(&["sync", "freeze-group"])
        .expect("Failed to run sync");
    assert!(output.status.success());
    
    // File should be reverted (frozen)
    assert_eq!(env.read_file(&test_file), "frozen content",
        "File should remain frozen");
}

#[test]
fn test_drift_sync_action() {
    let env = TestEnvironment::new("drift");
    env.setup_git().expect("Failed to setup git");
    
    // Create and enroll file with drift action
    let test_file = env.create_test_file("drift.conf", "initial");
    
    let _ = env.run_laszoo(&["group", "drift-group", "config", "--action", "drift"])
        .expect("Failed to configure group");
    
    let output = env.run_laszoo(&["enroll", "drift-group", test_file.to_str().unwrap()])
        .expect("Failed to enroll");
    assert!(output.status.success());
    
    // Modify the file
    std::fs::write(&test_file, "drifted content").unwrap();
    
    // Run sync (should not change file)
    let output = env.run_laszoo(&["sync", "drift-group"])
        .expect("Failed to run sync");
    assert!(output.status.success());
    
    // File should keep its changes
    assert_eq!(env.read_file(&test_file), "drifted content",
        "File should be allowed to drift");
    
    // Status should show drift
    let output = env.run_laszoo(&["status"])
        .expect("Failed to run status");
    let status_output = String::from_utf8_lossy(&output.stdout);
    assert!(status_output.contains("drift") || status_output.contains("modified"),
        "Status should indicate drift");
}