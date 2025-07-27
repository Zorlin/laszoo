use std::fs;
use std::process::Command;

mod common;
use common::TestEnvironment;

#[tokio::test]
#[ignore = "auto-commit on enrollment not yet implemented"]
async fn test_auto_commit_on_enrollment() {
    let env = TestEnvironment::new("auto_commit_enroll");
    env.setup_git().expect("Failed to setup git");
    
    // Create and enroll a file
    let test_file = env.test_dir.join("config.txt");
    fs::write(&test_file, "test content").unwrap();
    
    // Enroll using CLI
    let output = env.run_laszoo(&["enroll", "testgroup", test_file.to_str().unwrap()]).unwrap();
    assert!(output.status.success());
    
    // TODO: Check that a commit was made
    let output = Command::new("git")
        .args(&["log", "--oneline"])
        .current_dir(&env.mfs_mount)
        .output()
        .expect("Failed to get git log");
    
    let log = String::from_utf8_lossy(&output.stdout);
    // Should contain a commit for the enrollment
    assert!(log.contains("Enrolled") || log.contains("config.txt"));
}

#[tokio::test]
#[ignore = "auto-commit on enrollment not yet implemented"]
async fn test_auto_commit_with_ollama() {
    let env = TestEnvironment::new("auto_commit_ollama");
    env.setup_git().expect("Failed to setup git");
    
    // TODO: Mock or check if Ollama is available
    // If available, commit message should be AI-generated
    // If not, should fall back to generic message
}

#[tokio::test]
#[ignore = "auto-commit on enrollment not yet implemented"]
async fn test_auto_commit_batch_enrollment() {
    let env = TestEnvironment::new("auto_commit_batch");
    env.setup_git().expect("Failed to setup git");
    
    // Create multiple files
    let file1 = env.test_dir.join("file1.txt");
    let file2 = env.test_dir.join("file2.txt");
    let file3 = env.test_dir.join("file3.txt");
    fs::write(&file1, "content1").unwrap();
    fs::write(&file2, "content2").unwrap();
    fs::write(&file3, "content3").unwrap();
    
    // Enroll all files at once using CLI
    let output = env.run_laszoo(&["enroll", "testgroup", file1.to_str().unwrap()]).unwrap();
    assert!(output.status.success());
    let output = env.run_laszoo(&["enroll", "testgroup", file2.to_str().unwrap()]).unwrap();
    assert!(output.status.success());
    let output = env.run_laszoo(&["enroll", "testgroup", file3.to_str().unwrap()]).unwrap();
    assert!(output.status.success());
    
    // TODO: Should create a single commit for all enrollments
    // or intelligently batch them
}

#[tokio::test]
#[ignore = "auto-commit on enrollment not yet implemented"]
async fn test_auto_commit_directory_enrollment() {
    let env = TestEnvironment::new("auto_commit_dir");
    env.setup_git().expect("Failed to setup git");
    
    // Create a directory with files
    let test_dir = env.test_dir.join("configs");
    fs::create_dir_all(&test_dir).unwrap();
    fs::write(test_dir.join("app.conf"), "app config").unwrap();
    fs::write(test_dir.join("db.conf"), "db config").unwrap();
    
    // Enroll the directory using CLI
    let output = env.run_laszoo(&["enroll", "testgroup", test_dir.to_str().unwrap()]).unwrap();
    assert!(output.status.success());
    
    // TODO: Should create a commit for the directory enrollment
    let output = Command::new("git")
        .args(&["log", "--oneline"])
        .current_dir(&env.mfs_mount)
        .output()
        .expect("Failed to get git log");
    
    let log = String::from_utf8_lossy(&output.stdout);
    assert!(log.contains("configs") || log.contains("directory"));
}

#[tokio::test]
#[ignore = "auto-commit on enrollment not yet implemented"]
async fn test_no_commit_on_failed_enrollment() {
    let env = TestEnvironment::new("auto_commit_fail");
    env.setup_git().expect("Failed to setup git");
    
    // Try to enroll a non-existent file
    let test_file = env.test_dir.join("nonexistent.txt");
    
    // Try to enroll using CLI - should fail
    let output = env.run_laszoo(&["enroll", "testgroup", test_file.to_str().unwrap()]).unwrap();
    assert!(!output.status.success());
    
    // TODO: No commit should be made for failed enrollment
    let output = Command::new("git")
        .args(&["log", "--oneline"])
        .current_dir(&env.mfs_mount)
        .output()
        .expect("Failed to get git log");
    
    let log = String::from_utf8_lossy(&output.stdout);
    assert!(log.is_empty() || !log.contains("nonexistent"));
}