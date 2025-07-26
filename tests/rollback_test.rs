use laszoo::config::Config;
use laszoo::enrollment::EnrollmentManager;
use std::fs;
use std::process::Command;

mod common;
use common::TestEnvironment;

#[tokio::test]
#[ignore = "rollback command not yet implemented"]
async fn test_rollback_single_commit() {
    let env = TestEnvironment::new("rollback_single");
    let config = env.create_config();
    
    // Initialize git repo
    Command::new("git")
        .args(&["init"])
        .current_dir(&config.mfs_mount)
        .output()
        .expect("Failed to init git");
    
    // Create and enroll a file
    let test_file = env.test_dir.join("config.txt");
    fs::write(&test_file, "version 1").unwrap();
    
    let mut enrollment_manager = EnrollmentManager::new(config.clone());
    enrollment_manager.enroll_file("testgroup", &test_file, false, false, None, None, Default::default()).await.unwrap();
    
    // Make a commit
    Command::new("git")
        .args(&["add", "."])
        .current_dir(&config.mfs_mount)
        .output()
        .expect("Failed to stage files");
    
    Command::new("git")
        .args(&["commit", "-m", "Initial version"])
        .current_dir(&config.mfs_mount)
        .output()
        .expect("Failed to commit");
    
    // Modify the template
    let template_path = config.mfs_mount.join("groups/testgroup").join(format!("{}.lasz", test_file.display()));
    fs::write(&template_path, "version 2").unwrap();
    
    // Commit the change
    Command::new("git")
        .args(&["add", "."])
        .current_dir(&config.mfs_mount)
        .output()
        .expect("Failed to stage files");
    
    Command::new("git")
        .args(&["commit", "-m", "Updated version"])
        .current_dir(&config.mfs_mount)
        .output()
        .expect("Failed to commit");
    
    // TODO: Run rollback command
    // `laszoo rollback testgroup`
    
    // Template should be back to version 1
    let content = fs::read_to_string(&template_path).unwrap();
    assert_eq!(content, "version 1");
}

#[tokio::test]
#[ignore = "rollback command not yet implemented"]
async fn test_rollback_multiple_commits() {
    let env = TestEnvironment::new("rollback_multiple");
    let config = env.create_config();
    
    // Initialize git repo and make multiple commits
    // TODO: Test `laszoo rollback testgroup --commits 3`
    // Should rollback 3 commits
}

#[tokio::test]
#[ignore = "rollback command not yet implemented"]
async fn test_rollback_specific_file() {
    let env = TestEnvironment::new("rollback_file");
    let config = env.create_config();
    
    // TODO: Test rolling back a specific file
    // `laszoo rollback /path/to/file.txt`
    // Should only rollback changes to that file
}

#[tokio::test]
#[ignore = "rollback command not yet implemented"]
async fn test_rollback_with_uncommitted_changes() {
    let env = TestEnvironment::new("rollback_uncommitted");
    let config = env.create_config();
    
    // TODO: Test behavior when there are uncommitted changes
    // Should either:
    // 1. Refuse to rollback and prompt to commit/stash
    // 2. Stash changes, rollback, then reapply
}

#[tokio::test]
#[ignore = "rollback command not yet implemented"]
async fn test_rollback_affects_local_files() {
    let env = TestEnvironment::new("rollback_apply");
    let config = env.create_config();
    
    // TODO: After rollback, should also update local files
    // to match the rolled-back templates
    // Similar to running `laszoo apply` after rollback
}