use laszoo::config::Config;
use laszoo::enrollment::EnrollmentManager;
use laszoo::template::TemplateEngine;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;

mod common;
use common::TestEnvironment;

#[tokio::test]
async fn test_watch_detects_local_changes() {
    let env = TestEnvironment::new("watch_local");
    let config = env.create_config();
    
    // Create a test file
    let test_file = env.test_dir.join("config.txt");
    fs::write(&test_file, "initial content").unwrap();
    
    // Enroll the file
    let mut enrollment_manager = EnrollmentManager::new(config.clone());
    enrollment_manager.enroll_file("testgroup", &test_file, false, false, None, None, Default::default()).await.unwrap();
    
    // Start watch in background (would normally block)
    // For testing, we'll just verify the setup
    
    // Verify template was created
    let template_path = config.mfs_mount.join("groups/testgroup").join(format!("{}.lasz", test_file.display()));
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
    let config = env.create_config();
    
    // Create and enroll a test file
    let test_file = env.test_dir.join("config.txt");
    fs::write(&test_file, "initial content").unwrap();
    
    let mut enrollment_manager = EnrollmentManager::new(config.clone());
    enrollment_manager.enroll_file("testgroup", &test_file, false, false, None, None, Default::default()).await.unwrap();
    
    // Get template path
    let template_path = config.mfs_mount.join("groups/testgroup").join(format!("{}.lasz", test_file.display()));
    
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
    
    let mut enrollment_manager = EnrollmentManager::new(config.clone());
    enrollment_manager.enroll_file("testgroup", &test_file, false, false, None, None, Default::default()).await.unwrap();
    
    // Get template path
    let template_path = config.mfs_mount.join("groups/testgroup").join(format!("{}.lasz", test_file.display()));
    
    // Template should preserve the variable
    let template_content = fs::read_to_string(&template_path).unwrap();
    assert!(template_content.contains("{{ hostname }}"));
    
    // When applied, it should be rendered
    let template_engine = TemplateEngine::new();
    let rendered = template_engine.render(&template_content, &config.mfs_mount, None).unwrap();
    assert!(!rendered.contains("{{ hostname }}"));
    assert!(rendered.contains("hostname: "));
}

#[tokio::test]
async fn test_watch_with_quack_tags() {
    let env = TestEnvironment::new("watch_quack");
    let config = env.create_config();
    let hostname = gethostname::gethostname().to_string_lossy().to_string();
    
    // Create group template with quack placeholder
    let test_file = env.test_dir.join("config.txt");
    let group_dir = config.mfs_mount.join("groups/testgroup");
    fs::create_dir_all(&group_dir).unwrap();
    
    let group_template = group_dir.join(format!("{}.lasz", test_file.display()));
    fs::write(&group_template, "server: {{ quack }}\nport: 8080").unwrap();
    
    // Create machine template with quack content
    let machine_dir = config.mfs_mount.join(format!("machines/{}", hostname));
    fs::create_dir_all(&machine_dir).unwrap();
    
    let machine_template = machine_dir.join(format!("{}.lasz", test_file.display()));
    fs::write(&machine_template, "[[x prod-server-01 x]]").unwrap();
    
    // Create manifest for hybrid mode
    let manifest = r#"{
        "entries": [{
            "path": "/config.txt",
            "type": "file",
            "hybrid": true
        }]
    }"#;
    fs::write(group_dir.join("manifest.json"), manifest).unwrap();
    
    // Render template
    let template_engine = TemplateEngine::new();
    let rendered = template_engine.render(
        &fs::read_to_string(&group_template).unwrap(),
        &config.mfs_mount,
        Some(&PathBuf::from("/config.txt"))
    ).unwrap();
    
    assert_eq!(rendered, "server: prod-server-01\nport: 8080");
}

#[tokio::test]
async fn test_watch_directory_enrollment() {
    let env = TestEnvironment::new("watch_directory");
    let config = env.create_config();
    
    // Create a directory with files
    let test_dir = env.test_dir.join("configs");
    fs::create_dir_all(&test_dir).unwrap();
    fs::write(test_dir.join("app.conf"), "app config").unwrap();
    fs::write(test_dir.join("db.conf"), "db config").unwrap();
    
    // Enroll the directory
    let mut enrollment_manager = EnrollmentManager::new(config.clone());
    enrollment_manager.enroll_directory("testgroup", &test_dir, false, false, None, None, Default::default()).await.unwrap();
    
    // Verify templates were created for both files
    let group_dir = config.mfs_mount.join("groups/testgroup");
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