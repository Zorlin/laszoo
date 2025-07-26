use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn test_sync_detect_changes() {
    // Create temporary directories
    let temp_dir = TempDir::new().unwrap();
    let mfs_mount = temp_dir.path().join("mnt/laszoo");
    let test_file = temp_dir.path().join("test_config.conf");
    
    // Create directory structure
    fs::create_dir_all(&mfs_mount).unwrap();
    
    // Create test file
    fs::write(&test_file, "original content\n").unwrap();
    
    // Get laszoo binary path
    let laszoo_bin = env!("CARGO_BIN_EXE_laszoo");
    
    // Enroll file
    let output = Command::new(laszoo_bin)
        .env("LASZOO_MFS_MOUNT", mfs_mount.to_str().unwrap())
        .arg("enroll")
        .arg("testgroup")
        .arg(&test_file)
        .output()
        .expect("Failed to execute laszoo enroll");
    
    println!("Enroll output: {}", String::from_utf8_lossy(&output.stdout));
    println!("Enroll stderr: {}", String::from_utf8_lossy(&output.stderr));
    
    assert!(output.status.success(), "Enroll failed");
    
    // Modify the local file
    fs::write(&test_file, "modified content\n").unwrap();
    
    // Run sync in dry-run mode to see what would happen
    let output = Command::new(laszoo_bin)
        .env("LASZOO_MFS_MOUNT", mfs_mount.to_str().unwrap())
        .arg("sync")
        .arg("--group")
        .arg("testgroup")
        .arg("--dry-run")
        .output()
        .expect("Failed to execute laszoo sync");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Sync output: {}", stdout);
    println!("Sync stderr: {}", String::from_utf8_lossy(&output.stderr));
    
    assert!(output.status.success(), "Sync failed");
    
    // Check that change was detected
    assert!(stdout.contains("Found 1 files needing synchronization"), 
        "Sync did not detect the file change");
}

#[test]
fn test_sync_rollback() {
    // Create temporary directories
    let temp_dir = TempDir::new().unwrap();
    let mfs_mount = temp_dir.path().join("mnt/laszoo");
    let test_file = temp_dir.path().join("test_config.conf");
    
    // Create directory structure
    fs::create_dir_all(&mfs_mount).unwrap();
    
    // Create test file
    fs::write(&test_file, "original content\n").unwrap();
    
    // Get laszoo binary path
    let laszoo_bin = env!("CARGO_BIN_EXE_laszoo");
    
    // Enroll file
    let output = Command::new(laszoo_bin)
        .env("LASZOO_MFS_MOUNT", mfs_mount.to_str().unwrap())
        .arg("enroll")
        .arg("testgroup")
        .arg(&test_file)
        .output()
        .expect("Failed to execute laszoo enroll");
    
    assert!(output.status.success(), "Enroll failed");
    
    // Modify the local file
    fs::write(&test_file, "modified content\n").unwrap();
    
    // Run sync with rollback strategy
    let output = Command::new(laszoo_bin)
        .env("LASZOO_MFS_MOUNT", mfs_mount.to_str().unwrap())
        .arg("sync")
        .arg("--group")
        .arg("testgroup")
        .arg("--strategy")
        .arg("rollback")
        .output()
        .expect("Failed to execute laszoo sync");
    
    println!("Sync output: {}", String::from_utf8_lossy(&output.stdout));
    println!("Sync stderr: {}", String::from_utf8_lossy(&output.stderr));
    
    assert!(output.status.success(), "Sync failed");
    
    // Check that file was rolled back to original content
    let file_content = fs::read_to_string(&test_file).unwrap();
    assert_eq!(file_content, "original content\n", "File was not rolled back");
}

#[test]
fn test_sync_forward() {
    // Create temporary directories
    let temp_dir = TempDir::new().unwrap();
    let mfs_mount = temp_dir.path().join("mnt/laszoo");
    let test_file = temp_dir.path().join("test_config.conf");
    
    // Create directory structure
    fs::create_dir_all(&mfs_mount).unwrap();
    
    // Create test file
    fs::write(&test_file, "original content\n").unwrap();
    
    // Get laszoo binary path
    let laszoo_bin = env!("CARGO_BIN_EXE_laszoo");
    
    // Enroll file
    let output = Command::new(laszoo_bin)
        .env("LASZOO_MFS_MOUNT", mfs_mount.to_str().unwrap())
        .arg("enroll")
        .arg("testgroup")
        .arg(&test_file)
        .output()
        .expect("Failed to execute laszoo enroll");
    
    assert!(output.status.success(), "Enroll failed");
    
    // Modify the local file
    fs::write(&test_file, "modified content\n").unwrap();
    
    // Run sync with forward strategy
    let output = Command::new(laszoo_bin)
        .env("LASZOO_MFS_MOUNT", mfs_mount.to_str().unwrap())
        .arg("sync")
        .arg("--group")
        .arg("testgroup")
        .arg("--strategy")
        .arg("forward")
        .output()
        .expect("Failed to execute laszoo sync");
    
    println!("Sync output: {}", String::from_utf8_lossy(&output.stdout));
    println!("Sync stderr: {}", String::from_utf8_lossy(&output.stderr));
    
    assert!(output.status.success(), "Sync failed");
    
    // Check that template was updated
    let template_path = mfs_mount
        .join("groups")
        .join("testgroup")
        .join(test_file.strip_prefix("/").unwrap_or(&test_file))
        .with_extension("conf.lasz");
    
    let template_content = fs::read_to_string(&template_path).unwrap();
    assert_eq!(template_content, "modified content\n", "Template was not updated");
}