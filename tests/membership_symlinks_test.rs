use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_membership_symlinks() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let temp_path = temp_dir.path();
    
    // Create the MooseFS mount structure
    let machines_dir = temp_path.join("machines");
    let groups_dir = temp_path.join("groups");
    let memberships_dir = temp_path.join("memberships");
    
    fs::create_dir_all(&machines_dir).unwrap();
    fs::create_dir_all(&groups_dir).unwrap();
    fs::create_dir_all(&memberships_dir).unwrap();
    
    // Build the binary path
    let binary_path = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("release")
        .join("laszoo");
    
    // Create a test file to enroll
    let test_file = temp_path.join("test.conf");
    fs::write(&test_file, "test content").unwrap();
    
    // Test 1: Enroll a file into a group (which also adds machine to group)
    let output = Command::new(&binary_path)
        .args(&["enroll", "testgroup", test_file.to_str().unwrap()])
        .env("LASZOO_MFS_MOUNT", temp_path.to_str().unwrap())
        .output()
        .expect("Failed to execute laszoo");
    
    if !output.status.success() {
        panic!("Enrollment failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    
    // Get the hostname
    let hostname = gethostname::gethostname()
        .to_string_lossy()
        .to_string();
    
    // Check that membership symlink was created
    let symlink_path = memberships_dir.join("testgroup").join(&hostname);
    assert!(symlink_path.exists(), "Membership symlink was not created");
    
    // Verify it's actually a symlink
    assert!(symlink_path.symlink_metadata().unwrap().file_type().is_symlink(), 
            "Path is not a symlink");
    
    // Verify the symlink points to the correct location
    let target = fs::read_link(&symlink_path).unwrap();
    let expected_target = Path::new("../../machines").join(&hostname);
    assert_eq!(target, expected_target, "Symlink points to wrong location");
    
    // Create another test file to enroll
    let test_file2 = temp_path.join("test2.conf");
    fs::write(&test_file2, "test content 2").unwrap();
    
    // Test 2: Enroll machine into another group
    let output = Command::new(&binary_path)
        .args(&["enroll", "anothergroup", test_file2.to_str().unwrap()])
        .env("LASZOO_MFS_MOUNT", temp_path.to_str().unwrap())
        .output()
        .expect("Failed to execute laszoo");
    
    if !output.status.success() {
        panic!("Second enrollment failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    
    // Check second membership symlink
    let symlink_path2 = memberships_dir.join("anothergroup").join(&hostname);
    assert!(symlink_path2.exists(), "Second membership symlink was not created");
    
    // Both symlinks should exist
    assert!(symlink_path.exists(), "First symlink was removed");
    assert!(symlink_path2.exists(), "Second symlink was not created");
    
    // Test 3: Remove machine from first group
    let output = Command::new(&binary_path)
        .args(&["group", "testgroup", "remove", &hostname])
        .env("LASZOO_MFS_MOUNT", temp_path.to_str().unwrap())
        .output()
        .expect("Failed to execute laszoo");
    
    if !output.status.success() {
        panic!("Group removal failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    
    // First symlink should be removed, second should remain
    assert!(!symlink_path.exists(), "First symlink was not removed");
    assert!(symlink_path2.exists(), "Second symlink was incorrectly removed");
    
    // Verify groups.conf was updated correctly
    let groups_file = machines_dir
        .join(&hostname)
        .join("etc")
        .join("laszoo")
        .join("groups.conf");
    
    let groups_content = fs::read_to_string(&groups_file).unwrap();
    assert!(!groups_content.contains("testgroup"), "testgroup still in groups.conf");
    assert!(groups_content.contains("anothergroup"), "anothergroup missing from groups.conf");
}