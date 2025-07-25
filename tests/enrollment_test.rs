mod common;

use common::*;

#[test]
fn test_enroll_single_file() {
    let env = TestEnvironment::new("enroll_single_file");
    env.setup_git().expect("Failed to setup git");
    
    // Create a test file
    let test_file = env.create_test_file("config/test.conf", "test content");
    
    // Enroll the file using relative path
    let relative_path = test_file.strip_prefix(&env.test_dir).unwrap();
    let output = env.run_laszoo(&["enroll", "testgroup", relative_path.to_str().unwrap()])
        .expect("Failed to run laszoo");
    
    assert!(output.status.success(), "Enrollment failed: {:?}", String::from_utf8_lossy(&output.stderr));
    
    // Check that template was created
    // The template path will include the full absolute path structure
    let abs_test_file = test_file.canonicalize().unwrap();
    let template_path = env.mfs_mount
        .join("groups")
        .join("testgroup")
        .join(abs_test_file.strip_prefix("/").unwrap())
        .with_extension("conf.lasz");
    
    
    assert!(env.file_exists(&template_path), "Template file was not created");
    assert_eq!(env.read_file(&template_path), "test content", "Template content doesn't match");
    
    // Check manifest
    let manifest_path = env.mfs_mount
        .join("groups")
        .join("testgroup")
        .join("manifest.json");
    
    assert!(env.file_exists(&manifest_path), "Group manifest was not created");
}

#[test]
fn test_enroll_directory() {
    let env = TestEnvironment::new("enroll_directory");
    env.setup_git().expect("Failed to setup git");
    
    // Create test directory with files
    env.create_test_file("configs/app.conf", "app config");
    env.create_test_file("configs/db.conf", "db config");
    env.create_test_file("configs/nested/web.conf", "web config");
    
    let dir_path = env.test_dir.join("configs");
    
    // Enroll the directory using relative path
    let relative_path = dir_path.strip_prefix(&env.test_dir).unwrap();
    let output = env.run_laszoo(&["enroll", "testgroup", relative_path.to_str().unwrap()])
        .expect("Failed to run laszoo");
    
    assert!(output.status.success(), "Directory enrollment failed: {:?}", String::from_utf8_lossy(&output.stderr));
    
    // Check that templates were created for all files
    // Templates will be created with absolute path structure
    let abs_dir = dir_path.canonicalize().unwrap();
    
    let app_template = env.mfs_mount
        .join("groups")
        .join("testgroup")
        .join(abs_dir.join("app.conf").strip_prefix("/").unwrap())
        .with_extension("conf.lasz");
    
    let db_template = env.mfs_mount
        .join("groups")
        .join("testgroup")
        .join(abs_dir.join("db.conf").strip_prefix("/").unwrap())
        .with_extension("conf.lasz");
        
    let web_template = env.mfs_mount
        .join("groups")
        .join("testgroup")
        .join(abs_dir.join("nested/web.conf").strip_prefix("/").unwrap())
        .with_extension("conf.lasz");
    
    assert!(env.file_exists(&app_template), "app.conf template not created");
    assert!(env.file_exists(&db_template), "db.conf template not created");
    assert!(env.file_exists(&web_template), "nested/web.conf template not created");
    
    // Check content
    assert_eq!(env.read_file(&app_template), "app config");
    assert_eq!(env.read_file(&db_template), "db config");
    assert_eq!(env.read_file(&web_template), "web config");
}

#[test]
fn test_file_adoption_in_enrolled_directory() {
    let env = TestEnvironment::new("file_adoption");
    env.setup_git().expect("Failed to setup git");
    
    // Create and enroll a directory
    env.create_test_file("mydir/file1.txt", "file 1 content");
    let dir_path = env.test_dir.join("mydir");
    
    let relative_dir = dir_path.strip_prefix(&env.test_dir).unwrap();
    let output = env.run_laszoo(&["enroll", "testgroup", relative_dir.to_str().unwrap()])
        .expect("Failed to run laszoo");
    assert!(output.status.success());
    
    // Now create a new file in the directory
    let new_file = env.create_test_file("mydir/file2.txt", "file 2 content");
    
    // Enroll the specific file (should be adopted into directory)
    let relative_file = new_file.strip_prefix(&env.test_dir).unwrap();
    let output = env.run_laszoo(&["enroll", "testgroup", relative_file.to_str().unwrap()])
        .expect("Failed to run laszoo");
    assert!(output.status.success());
    
    // Check that template was created
    let abs_file = new_file.canonicalize().unwrap();
    let template_path = env.mfs_mount
        .join("groups")
        .join("testgroup")
        .join(abs_file.strip_prefix("/").unwrap())
        .with_extension("txt.lasz");
    assert!(env.file_exists(&template_path));
    
    // Check that machine manifest doesn't have individual entry
    let machine_manifest = env.mfs_mount
        .join("machines")
        .join(&env.hostname)
        .join("manifest.json");
    
    if env.file_exists(&machine_manifest) {
        let content = env.read_file(&machine_manifest);
        assert!(!content.contains("file2.txt"), "File should not have individual manifest entry");
    }
}

#[test]
fn test_machine_specific_enrollment() {
    let env = TestEnvironment::new("machine_specific");
    env.setup_git().expect("Failed to setup git");
    
    // Create a test file
    let test_file = env.create_test_file("special.conf", "machine specific content");
    
    // Enroll as machine-specific
    let relative_path = test_file.strip_prefix(&env.test_dir).unwrap();
    let output = env.run_laszoo(&["enroll", "testgroup", relative_path.to_str().unwrap(), "--machine"])
        .expect("Failed to run laszoo");
    
    assert!(output.status.success(), "Machine enrollment failed: {:?}", String::from_utf8_lossy(&output.stderr));
    
    // Check that machine template was created
    // Note: When using --machine flag, laszoo uses the actual system hostname from gethostname, not the env variable
    let abs_file = test_file.canonicalize().unwrap();
    let machine_template = env.mfs_mount
        .join("machines")
        .join(&env.original_hostname)  // Use the actual system hostname
        .join(abs_file.strip_prefix("/").unwrap())
        .with_extension("conf.lasz");
    
    assert!(env.file_exists(&machine_template), "Machine template not created");
    assert_eq!(env.read_file(&machine_template), "machine specific content");
    
    // Check that no group template was created  
    let group_template = env.mfs_mount
        .join("groups")
        .join("testgroup")
        .join(abs_file.strip_prefix("/").unwrap())
        .with_extension("conf.lasz");
    
    assert!(!env.file_exists(&group_template), "Group template should not exist for machine-specific enrollment");
}

#[test] 
fn test_hybrid_enrollment() {
    let env = TestEnvironment::new("hybrid");
    env.setup_git().expect("Failed to setup git");
    
    // Create a test file
    let test_file = env.create_test_file("hybrid.conf", "local content");
    
    // Enroll as hybrid
    let relative_path = test_file.strip_prefix(&env.test_dir).unwrap();
    let output = env.run_laszoo(&["enroll", "testgroup", relative_path.to_str().unwrap(), "--hybrid"])
        .expect("Failed to run laszoo");
    
    assert!(output.status.success());
    
    // For hybrid enrollment, machine template is created using actual hostname
    let abs_file = test_file.canonicalize().unwrap();
    let machine_template = env.mfs_mount
        .join("machines")
        .join(&env.original_hostname)  // Use actual system hostname
        .join(abs_file.strip_prefix("/").unwrap())
        .with_extension("conf.lasz");
    
    let group_template = env.mfs_mount
        .join("groups")
        .join("testgroup")
        .join(abs_file.strip_prefix("/").unwrap())
        .with_extension("conf.lasz");
    
    assert!(env.file_exists(&machine_template), "Machine template not created for hybrid");
    assert!(!env.file_exists(&group_template), "Group template should not exist on first enrollment");
}

#[test]
fn test_force_enrollment() {
    let env = TestEnvironment::new("force_enroll");
    env.setup_git().expect("Failed to setup git");
    
    // Create and enroll a file
    let test_file = env.create_test_file("force.conf", "original content");
    let relative_path = test_file.strip_prefix(&env.test_dir).unwrap();
    
    let output = env.run_laszoo(&["enroll", "group1", relative_path.to_str().unwrap()])
        .expect("Failed to run laszoo");
    assert!(output.status.success());
    
    // Currently, enrolling in another group is allowed (file can be in multiple groups)
    let output = env.run_laszoo(&["enroll", "group2", relative_path.to_str().unwrap()])
        .expect("Failed to run laszoo");
    assert!(output.status.success(), "Enrolling in second group should succeed");
    
    // Check that file is in both groups
    let abs_file = test_file.canonicalize().unwrap();
    let template_path1 = env.mfs_mount
        .join("groups")
        .join("group1")
        .join(abs_file.strip_prefix("/").unwrap())
        .with_extension("conf.lasz");
    let template_path2 = env.mfs_mount
        .join("groups")
        .join("group2")
        .join(abs_file.strip_prefix("/").unwrap())
        .with_extension("conf.lasz");
        
    assert!(env.file_exists(&template_path1), "Template should exist in group1");
    assert!(env.file_exists(&template_path2), "Template should exist in group2");
    
    // Test that force flag works when re-enrolling in the same group
    let output = env.run_laszoo(&["enroll", "group1", relative_path.to_str().unwrap()])
        .expect("Failed to run laszoo");
    assert!(!output.status.success(), "Re-enrolling in same group should fail without --force");
    
    let output = env.run_laszoo(&["enroll", "group1", relative_path.to_str().unwrap(), "--force"])
        .expect("Failed to run laszoo");
    assert!(output.status.success(), "Force re-enrollment in same group should succeed");
}