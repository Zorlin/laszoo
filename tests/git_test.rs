mod common;

use common::*;
use std::process::Command;

#[test]
#[ignore = "Auto-commit on enrollment not yet implemented"]
fn test_auto_commit_on_enrollment() {
    let env = TestEnvironment::new("git_auto_commit");
    env.setup_git().expect("Failed to setup git");
    
    // Create and enroll a file
    let test_file = env.create_test_file("autocommit.conf", "test content");
    let relative_path = test_file.strip_prefix(&env.test_dir).unwrap();
    let output = env.run_laszoo(&["enroll", "gitgroup", relative_path.to_str().unwrap()])
        .expect("Failed to run laszoo");
    assert!(output.status.success());
    
    // Check git log
    let git_output = Command::new("git")
        .args(&["log", "--oneline"])
        .current_dir(&env.mfs_mount)
        .output()
        .expect("Failed to run git log");
    
    let log = String::from_utf8_lossy(&git_output.stdout);
    assert!(!log.is_empty(), "No commits found");
    assert!(log.contains("enroll") || log.contains("Enroll"), 
        "Commit message should mention enrollment");
}

#[test]
#[ignore = "Auto-commit on enrollment not yet implemented"]
fn test_commit_with_multiple_changes() {
    let env = TestEnvironment::new("git_multi_commit");
    env.setup_git().expect("Failed to setup git");
    
    // Create and enroll multiple files
    let file1 = env.create_test_file("file1.conf", "content 1");
    let file2 = env.create_test_file("file2.conf", "content 2");
    let file3 = env.create_test_file("dir/file3.conf", "content 3");
    
    // Enroll all files
    for file in &[&file1, &file2, &file3] {
        let relative_path = file.strip_prefix(&env.test_dir).unwrap();
        let output = env.run_laszoo(&["enroll", "multigroup", relative_path.to_str().unwrap()])
            .expect("Failed to enroll file");
        assert!(output.status.success());
    }
    
    // Check that all changes are in git
    let status_output = Command::new("git")
        .args(&["status", "--porcelain"])
        .current_dir(&env.mfs_mount)
        .output()
        .expect("Failed to run git status");
    
    let status = String::from_utf8_lossy(&status_output.stdout);
    assert!(status.is_empty() || !status.contains("??"), 
        "All files should be committed, but found untracked files");
}

#[test]
#[ignore = "Auto-commit on enrollment not yet implemented"]
fn test_generic_commit_message_fallback() {
    let env = TestEnvironment::new("git_generic_message");
    env.setup_git().expect("Failed to setup git");
    
    // Ensure Ollama is not available by setting invalid endpoint
    std::env::set_var("LASZOO_OLLAMA_ENDPOINT", "http://invalid-endpoint:99999");
    
    // Create and enroll a file
    let test_file = env.create_test_file("generic.conf", "test content");
    let relative_path = test_file.strip_prefix(&env.test_dir).unwrap();
    let output = env.run_laszoo(&["enroll", "genericgroup", relative_path.to_str().unwrap()])
        .expect("Failed to run laszoo");
    assert!(output.status.success());
    
    // Check git log for generic message
    let git_output = Command::new("git")
        .args(&["log", "-1", "--pretty=%B"])
        .current_dir(&env.mfs_mount)
        .output()
        .expect("Failed to run git log");
    
    let commit_msg = String::from_utf8_lossy(&git_output.stdout);
    
    // Should have a generic but descriptive message
    assert!(commit_msg.contains("Added") || commit_msg.contains("Modified") || 
            commit_msg.contains("Updated") || commit_msg.contains("Created"),
        "Commit message should be descriptive even without Ollama");
}

#[test]
#[ignore = "Auto-commit on remote changes not yet implemented"]
fn test_no_commit_on_remote_changes() {
    let env1 = TestEnvironment::new("git_no_remote_commit");
    env1.setup_git().expect("Failed to setup git");
    
    // Create second machine
    let env2 = create_second_machine(&env1, "machine2");
    
    // Machine 1: Create and enroll a file
    let file1 = env1.create_test_file("remote.conf", "machine1 content");
    let relative_path = file1.strip_prefix(&env1.test_dir).unwrap();
    let output = env1.run_laszoo(&["enroll", "remotegroup", relative_path.to_str().unwrap()])
        .expect("Failed to enroll on machine1");
    assert!(output.status.success());
    
    // Get initial commit count
    let initial_commits = Command::new("git")
        .args(&["rev-list", "--count", "HEAD"])
        .current_dir(&env1.mfs_mount)
        .output()
        .expect("Failed to count commits");
    let initial_count: i32 = String::from_utf8_lossy(&initial_commits.stdout)
        .trim()
        .parse()
        .unwrap_or(0);
    
    // Machine 2: First enroll into the group, then the template will be applied
    let _file2 = env2.create_test_file("remote.conf", "to be replaced");
    let output = env2.run_laszoo(&["enroll", "remotegroup"])
        .expect("Failed to enroll on machine2");
    assert!(output.status.success());
    
    // Check that no new commits were made
    let final_commits = Command::new("git")
        .args(&["rev-list", "--count", "HEAD"])
        .current_dir(&env1.mfs_mount)
        .output()
        .expect("Failed to count commits");
    let final_count: i32 = String::from_utf8_lossy(&final_commits.stdout)
        .trim()
        .parse()
        .unwrap_or(0);
    
    assert_eq!(initial_count, final_count, 
        "No commits should be made when applying remote changes");
}

#[test]
fn test_gitignore_functionality() {
    let env = TestEnvironment::new("gitignore");
    env.setup_git().expect("Failed to setup git");
    
    // Create a .gitignore file
    let gitignore_content = "*.secret\npasswords/\n*.key";
    std::fs::write(env.mfs_mount.join(".gitignore"), gitignore_content).unwrap();
    
    // Create files that should be ignored
    env.create_test_file("config.secret", "secret data");
    env.create_test_file("passwords/admin.txt", "admin123");
    env.create_test_file("private.key", "key content");
    
    // Create a file that should not be ignored
    let normal_file = env.create_test_file("normal.conf", "normal content");
    
    // Enroll the normal file
    let relative_path = normal_file.strip_prefix(&env.test_dir).unwrap();
    let output = env.run_laszoo(&["enroll", "gitgroup", relative_path.to_str().unwrap()])
        .expect("Failed to enroll");
    assert!(output.status.success());
    
    // Check git status
    let status_output = Command::new("git")
        .args(&["status", "--porcelain"])
        .current_dir(&env.mfs_mount)
        .output()
        .expect("Failed to run git status");
    
    let status = String::from_utf8_lossy(&status_output.stdout);
    
    // Secret files should not appear in git status
    assert!(!status.contains("config.secret"), "Secret files should be ignored");
    assert!(!status.contains("passwords"), "Password directory should be ignored");
    assert!(!status.contains("private.key"), "Key files should be ignored");
}