use laszoo::config::Config;
use laszoo::enrollment::EnrollmentManager;
use laszoo::git::GitManager;
use std::fs;
use std::process::Command;

mod common;
use common::TestEnvironment;

#[tokio::test]
#[ignore = "auto-commit on enrollment not yet implemented"]
async fn test_auto_commit_on_enrollment() {
    let env = TestEnvironment::new("auto_commit_enroll");
    let config = env.create_config();
    
    // Initialize git repo
    Command::new("git")
        .args(&["init"])
        .current_dir(&config.mfs_mount)
        .output()
        .expect("Failed to init git");
    
    // Create and enroll a file
    let test_file = env.test_dir.join("config.txt");
    fs::write(&test_file, "test content").unwrap();
    
    let mut enrollment_manager = EnrollmentManager::new(config.clone());
    enrollment_manager.enroll_file("testgroup", &test_file, false, false, None, None, Default::default()).await.unwrap();
    
    // TODO: Check that a commit was made
    let output = Command::new("git")
        .args(&["log", "--oneline"])
        .current_dir(&config.mfs_mount)
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
    let config = env.create_config();
    
    // Initialize git repo
    Command::new("git")
        .args(&["init"])
        .current_dir(&config.mfs_mount)
        .output()
        .expect("Failed to init git");
    
    // TODO: Mock or check if Ollama is available
    // If available, commit message should be AI-generated
    // If not, should fall back to generic message
}

#[tokio::test]
#[ignore = "auto-commit on enrollment not yet implemented"]
async fn test_auto_commit_batch_enrollment() {
    let env = TestEnvironment::new("auto_commit_batch");
    let config = env.create_config();
    
    // Initialize git repo
    Command::new("git")
        .args(&["init"])
        .current_dir(&config.mfs_mount)
        .output()
        .expect("Failed to init git");
    
    // Create multiple files
    let file1 = env.test_dir.join("file1.txt");
    let file2 = env.test_dir.join("file2.txt");
    let file3 = env.test_dir.join("file3.txt");
    fs::write(&file1, "content1").unwrap();
    fs::write(&file2, "content2").unwrap();
    fs::write(&file3, "content3").unwrap();
    
    // Enroll all files at once
    let mut enrollment_manager = EnrollmentManager::new(config.clone());
    enrollment_manager.enroll_file("testgroup", &file1, false, false, None, None, Default::default()).await.unwrap();
    enrollment_manager.enroll_file("testgroup", &file2, false, false, None, None, Default::default()).await.unwrap();
    enrollment_manager.enroll_file("testgroup", &file3, false, false, None, None, Default::default()).await.unwrap();
    
    // TODO: Should create a single commit for all enrollments
    // or intelligently batch them
}

#[tokio::test]
#[ignore = "auto-commit on enrollment not yet implemented"]
async fn test_auto_commit_directory_enrollment() {
    let env = TestEnvironment::new("auto_commit_dir");
    let config = env.create_config();
    
    // Initialize git repo
    Command::new("git")
        .args(&["init"])
        .current_dir(&config.mfs_mount)
        .output()
        .expect("Failed to init git");
    
    // Create a directory with files
    let test_dir = env.test_dir.join("configs");
    fs::create_dir_all(&test_dir).unwrap();
    fs::write(test_dir.join("app.conf"), "app config").unwrap();
    fs::write(test_dir.join("db.conf"), "db config").unwrap();
    
    // Enroll the directory
    let mut enrollment_manager = EnrollmentManager::new(config.clone());
    enrollment_manager.enroll_directory("testgroup", &test_dir, false, false, None, None, Default::default()).await.unwrap();
    
    // TODO: Should create a commit for the directory enrollment
    let output = Command::new("git")
        .args(&["log", "--oneline"])
        .current_dir(&config.mfs_mount)
        .output()
        .expect("Failed to get git log");
    
    let log = String::from_utf8_lossy(&output.stdout);
    assert!(log.contains("configs") || log.contains("directory"));
}

#[tokio::test]
#[ignore = "auto-commit on enrollment not yet implemented"]
async fn test_no_commit_on_failed_enrollment() {
    let env = TestEnvironment::new("auto_commit_fail");
    let config = env.create_config();
    
    // Initialize git repo
    Command::new("git")
        .args(&["init"])
        .current_dir(&config.mfs_mount)
        .output()
        .expect("Failed to init git");
    
    // Try to enroll a non-existent file
    let test_file = env.test_dir.join("nonexistent.txt");
    
    let mut enrollment_manager = EnrollmentManager::new(config.clone());
    let result = enrollment_manager.enroll_file("testgroup", &test_file, false, false, None, None, Default::default()).await;
    
    assert!(result.is_err());
    
    // TODO: No commit should be made for failed enrollment
    let output = Command::new("git")
        .args(&["log", "--oneline"])
        .current_dir(&config.mfs_mount)
        .output()
        .expect("Failed to get git log");
    
    let log = String::from_utf8_lossy(&output.stdout);
    assert!(log.is_empty() || !log.contains("nonexistent"));
}