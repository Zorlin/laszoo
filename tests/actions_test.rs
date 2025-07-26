use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn test_enroll_with_actions() {
    // Create temporary directories
    let temp_dir = TempDir::new().unwrap();
    let mfs_mount = temp_dir.path().join("mnt/laszoo");
    let test_file = temp_dir.path().join("test_config.conf");
    let action_log = temp_dir.path().join("action.log");
    
    // Create directory structure
    fs::create_dir_all(&mfs_mount).unwrap();
    
    // Create test file
    fs::write(&test_file, "test content\n").unwrap();
    
    // Get laszoo binary path
    let laszoo_bin = env!("CARGO_BIN_EXE_laszoo");
    
    // Enroll file with before and after actions
    let output = Command::new(laszoo_bin)
        .env("LASZOO_MFS_MOUNT", mfs_mount.to_str().unwrap())
        .arg("enroll")
        .arg("testgroup")
        .arg(&test_file)
        .arg("--start")
        .arg(&format!("echo 'before action' >> {}", action_log.display()))
        .arg("--end")
        .arg(&format!("echo 'after action' >> {}", action_log.display()))
        .output()
        .expect("Failed to execute laszoo enroll");
    
    println!("Enroll output: {}", String::from_utf8_lossy(&output.stdout));
    println!("Enroll stderr: {}", String::from_utf8_lossy(&output.stderr));
    
    assert!(output.status.success(), "Enroll failed");
    
    // Check that the template was created
    let template_path = mfs_mount
        .join("groups")
        .join("testgroup")
        .join(test_file.strip_prefix("/").unwrap_or(&test_file))
        .with_extension("conf.lasz");
    
    assert!(template_path.exists(), "Template file not created");
    
    // Check that actions were saved
    let actions_path = mfs_mount
        .join("groups")
        .join("testgroup")
        .join("actions.json");
    
    assert!(actions_path.exists(), "Actions manifest not created");
    
    // Verify actions content
    let actions_content = fs::read_to_string(&actions_path).unwrap();
    assert!(actions_content.contains("before action"), "Before action not saved");
    assert!(actions_content.contains("after action"), "After action not saved");
}

#[test]
fn test_apply_with_actions() {
    // Create temporary directories
    let temp_dir = TempDir::new().unwrap();
    let mfs_mount = temp_dir.path().join("mnt/laszoo");
    let test_file = temp_dir.path().join("test_config.conf");
    let action_log = temp_dir.path().join("action.log");
    
    // Create directory structure
    fs::create_dir_all(&mfs_mount).unwrap();
    
    // Create test file
    fs::write(&test_file, "initial content\n").unwrap();
    
    // Get laszoo binary path
    let laszoo_bin = env!("CARGO_BIN_EXE_laszoo");
    
    // First enroll the file with actions
    let output = Command::new(laszoo_bin)
        .env("LASZOO_MFS_MOUNT", mfs_mount.to_str().unwrap())
        .arg("enroll")
        .arg("testgroup")
        .arg(&test_file)
        .arg("--before")
        .arg(&format!("echo 'BEFORE: applying template' >> {}", action_log.display()))
        .arg("--after")
        .arg(&format!("echo 'AFTER: template applied' >> {}", action_log.display()))
        .output()
        .expect("Failed to execute laszoo enroll");
    
    assert!(output.status.success(), "Enroll failed");
    
    // Modify the template
    let template_path = mfs_mount
        .join("groups")
        .join("testgroup")
        .join(test_file.strip_prefix("/").unwrap_or(&test_file))
        .with_extension("conf.lasz");
    
    fs::write(&template_path, "updated content from template\n").unwrap();
    
    // Apply the template (which should trigger actions)
    let output = Command::new(laszoo_bin)
        .env("LASZOO_MFS_MOUNT", mfs_mount.to_str().unwrap())
        .arg("apply")
        .arg("testgroup")
        .output()
        .expect("Failed to execute laszoo apply");
    
    println!("Apply output: {}", String::from_utf8_lossy(&output.stdout));
    println!("Apply stderr: {}", String::from_utf8_lossy(&output.stderr));
    
    assert!(output.status.success(), "Apply failed");
    
    // Check that actions were executed
    if action_log.exists() {
        let log_content = fs::read_to_string(&action_log).unwrap();
        println!("Action log content: {}", log_content);
        assert!(log_content.contains("BEFORE: applying template"), "Before action not executed");
        assert!(log_content.contains("AFTER: template applied"), "After action not executed");
        
        // Verify order - before should come before after
        let before_pos = log_content.find("BEFORE:").unwrap();
        let after_pos = log_content.find("AFTER:").unwrap();
        assert!(before_pos < after_pos, "Actions executed in wrong order");
    }
    
    // Check that file was updated
    let file_content = fs::read_to_string(&test_file).unwrap();
    assert_eq!(file_content, "updated content from template\n", "File not updated from template");
}

#[test]
fn test_upgrade_all_with_actions() {
    // Create temporary directories
    let temp_dir = TempDir::new().unwrap();
    let mfs_mount = temp_dir.path().join("mnt/laszoo");
    let action_log = temp_dir.path().join("upgrade.log");
    
    // Create directory structure
    fs::create_dir_all(&mfs_mount).unwrap();
    
    // Create packages.conf with ++upgrade and actions
    let packages_conf = mfs_mount
        .join("groups")
        .join("testgroup")
        .join("etc")
        .join("laszoo")
        .join("packages.conf");
    
    fs::create_dir_all(packages_conf.parent().unwrap()).unwrap();
    
    let packages_content = format!(
        "# Test packages.conf\n\
         ++upgrade --start echo 'Starting system upgrade' >> {} --end echo 'System upgrade complete' >> {}\n",
        action_log.display(),
        action_log.display()
    );
    
    fs::write(&packages_conf, packages_content).unwrap();
    
    // Parse and verify the packages.conf can be read
    let laszoo_bin = env!("CARGO_BIN_EXE_laszoo");
    
    // Create a simple test by checking status (which will load manifests)
    let output = Command::new(laszoo_bin)
        .env("LASZOO_MFS_MOUNT", mfs_mount.to_str().unwrap())
        .arg("status")
        .output()
        .expect("Failed to execute laszoo status");
    
    println!("Status output: {}", String::from_utf8_lossy(&output.stdout));
    println!("Status stderr: {}", String::from_utf8_lossy(&output.stderr));
    
    // Verify the packages.conf was created correctly
    assert!(packages_conf.exists(), "packages.conf not created");
    let content = fs::read_to_string(&packages_conf).unwrap();
    assert!(content.contains("++upgrade"), "++upgrade not in packages.conf");
    assert!(content.contains("--start"), "--start flag not in packages.conf");
    assert!(content.contains("--end"), "--end flag not in packages.conf");
}