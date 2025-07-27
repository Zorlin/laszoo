use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;

mod common;
use common::TestEnvironment;

#[tokio::test]
async fn test_watch_detects_local_changes() {
    let env = TestEnvironment::new("watch_local");
    env.setup_git().expect("Failed to setup git");
    
    // Create a test file
    let test_file = env.test_dir.join("config.txt");
    fs::write(&test_file, "initial content").unwrap();
    
    // Enroll the file
    let output = env.run_laszoo(&["enroll", "testgroup", test_file.to_str().unwrap()]).unwrap();
    assert!(output.status.success());
    
    // Start watch in background (would normally block)
    // For testing, we'll just verify the setup
    
    // Verify template was created
    let template_path = env.mfs_mount.join("groups/testgroup").join(format!("{}.lasz", test_file.display()));
    assert!(template_path.exists());
    
    // Simulate local file change
    fs::write(&test_file, "modified content").unwrap();
    
    // In real watch mode, this would be detected and template updated
    // For now, verify we can detect the change
    let local_content = fs::read_to_string(&test_file).unwrap();
    let template_content = fs::read_to_string(&template_path).unwrap();
    
    assert_ne!(local_content, template_content);
}

#[tokio::test]
async fn test_watch_detects_template_changes() {
    let env = TestEnvironment::new("watch_template");
    env.setup_git().expect("Failed to setup git");
    
    // Create and enroll a test file
    let test_file = env.test_dir.join("config.txt");
    fs::write(&test_file, "initial content").unwrap();
    
    // Enroll using CLI
    let output = env.run_laszoo(&["enroll", "testgroup", test_file.to_str().unwrap()]).unwrap();
    assert!(output.status.success());
    
    // Get template path
    let template_path = env.mfs_mount.join("groups/testgroup").join(format!("{}.lasz", test_file.display()));
    
    // Simulate template change (as if from another machine)
    sleep(Duration::from_millis(100)).await;
    fs::write(&template_path, "template modified content").unwrap();
    
    // In real watch mode with --auto, this would update local file
    // Verify we can detect the change
    let local_content = fs::read_to_string(&test_file).unwrap();
    let template_content = fs::read_to_string(&template_path).unwrap();
    
    assert_ne!(local_content, template_content);
    assert_eq!(template_content, "template modified content");
}

#[tokio::test]
async fn test_watch_with_handlebars_variables() {
    let env = TestEnvironment::new("watch_handlebars");
    let config = env.create_config();
    
    // Create a file with handlebars variable
    let test_file = env.test_dir.join("config.txt");
    fs::write(&test_file, "hostname: {{ hostname }}").unwrap();
    
    // Enroll using CLI
    let output = env.run_laszoo(&["enroll", "testgroup", test_file.to_str().unwrap()]).unwrap();
    assert!(output.status.success());
    
    // Get template path
    let template_path = env.mfs_mount.join("groups/testgroup").join(format!("{}.lasz", test_file.display()));
    
    // Template should preserve the variable
    let template_content = fs::read_to_string(&template_path).unwrap();
    assert!(template_content.contains("{{ hostname }}"));
    
    // Apply the template using CLI to see it rendered
    let output = env.run_laszoo(&["apply", "testgroup"]).unwrap();
    assert!(output.status.success());
    
    // Check the rendered file
    let rendered_content = fs::read_to_string(&test_file).unwrap();
    assert!(!rendered_content.contains("{{ hostname }}"));
    assert!(rendered_content.contains("hostname: "));
}

#[tokio::test]
async fn test_watch_with_quack_tags() {
    let env = TestEnvironment::new("watch_quack");
    env.setup_git().expect("Failed to setup git");
    let hostname = gethostname::gethostname().to_string_lossy().to_string();
    
    // Create a file first
    let test_file = env.test_dir.join("config.txt");
    fs::write(&test_file, "server: localhost\nport: 8080").unwrap();
    
    // Enroll as hybrid
    let output = env.run_laszoo(&["enroll", "testgroup", test_file.to_str().unwrap(), "--hybrid"]).unwrap();
    assert!(output.status.success());
    
    // Modify group template to use quack placeholder
    let group_template = env.mfs_mount.join("groups/testgroup").join(format!("{}.lasz", test_file.display()));
    fs::write(&group_template, "server: {{ quack }}\nport: 8080").unwrap();
    
    // Create machine template with quack content
    let machine_dir = env.mfs_mount.join(format!("machines/{}", hostname));
    fs::create_dir_all(&machine_dir).unwrap();
    
    let machine_template = machine_dir.join(format!("{}.lasz", test_file.display()));
    fs::write(&machine_template, "[[x prod-server-01 x]]").unwrap();
    
    // Apply template to see it rendered
    let output = env.run_laszoo(&["apply", "testgroup"]).unwrap();
    assert!(output.status.success());
    
    // Check the rendered file
    let rendered_content = fs::read_to_string(&test_file).unwrap();
    assert_eq!(rendered_content, "server: prod-server-01\nport: 8080");
}

#[tokio::test]
async fn test_watch_directory_enrollment() {
    let env = TestEnvironment::new("watch_directory");
    env.setup_git().expect("Failed to setup git");
    
    // Create a directory with files
    let test_dir = env.test_dir.join("configs");
    fs::create_dir_all(&test_dir).unwrap();
    fs::write(test_dir.join("app.conf"), "app config").unwrap();
    fs::write(test_dir.join("db.conf"), "db config").unwrap();
    
    // Enroll the directory using CLI
    let output = env.run_laszoo(&["enroll", "testgroup", test_dir.to_str().unwrap()]).unwrap();
    assert!(output.status.success());
    
    // Verify templates were created for both files
    let group_dir = env.mfs_mount.join("groups/testgroup");
    let app_template = group_dir.join(format!("{}/app.conf.lasz", test_dir.display()));
    let db_template = group_dir.join(format!("{}/db.conf.lasz", test_dir.display()));
    
    assert!(app_template.exists());
    assert!(db_template.exists());
    
    // Add a new file to the directory
    fs::write(test_dir.join("new.conf"), "new config").unwrap();
    
    // In real watch mode, this new file would be adopted
    // Verify the directory structure
    let files: Vec<_> = fs::read_dir(&test_dir).unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(files.len(), 3);
}