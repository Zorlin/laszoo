use laszoo::config::Config;
use laszoo::enrollment::EnrollmentManager;
use laszoo::template::TemplateEngine;
use std::fs;

mod common;
use common::TestEnvironment;

#[tokio::test]
#[ignore = "diff command not yet implemented"]
async fn test_diff_shows_changes() {
    let env = TestEnvironment::new("diff_changes");
    let config = env.create_config();
    
    // Create and enroll a file
    let test_file = env.test_dir.join("config.txt");
    fs::write(&test_file, "line1\nline2\nline3").unwrap();
    
    let mut enrollment_manager = EnrollmentManager::new(config.clone());
    enrollment_manager.enroll_file("testgroup", &test_file, false, false, None, None, Default::default()).await.unwrap();
    
    // Modify the local file
    fs::write(&test_file, "line1\nmodified line2\nline3\nline4").unwrap();
    
    // TODO: When diff is implemented, it should show:
    // - line2 -> modified line2
    // + line4
    
    // The diff command should compare local file with rendered template
    // and show unified diff output
}

#[tokio::test]
#[ignore = "diff command not yet implemented"]
async fn test_diff_with_template_changes() {
    let env = TestEnvironment::new("diff_template");
    let config = env.create_config();
    
    // Create and enroll a file
    let test_file = env.test_dir.join("config.txt");
    fs::write(&test_file, "original content").unwrap();
    
    let mut enrollment_manager = EnrollmentManager::new(config.clone());
    enrollment_manager.enroll_file("testgroup", &test_file, false, false, None, None, Default::default()).await.unwrap();
    
    // Modify the template
    let template_path = config.mfs_mount.join("groups/testgroup").join(format!("{}.lasz", test_file.display()));
    fs::write(&template_path, "template modified content").unwrap();
    
    // TODO: Diff should show what would happen if we applied the template
    // - original content
    // + template modified content
}

#[tokio::test]
#[ignore = "diff command not yet implemented"]
async fn test_diff_with_handlebars() {
    let env = TestEnvironment::new("diff_handlebars");
    let config = env.create_config();
    
    // Create a file
    let test_file = env.test_dir.join("config.txt");
    fs::write(&test_file, "hostname: localhost").unwrap();
    
    let mut enrollment_manager = EnrollmentManager::new(config.clone());
    enrollment_manager.enroll_file("testgroup", &test_file, false, false, None, None, Default::default()).await.unwrap();
    
    // Modify template to use handlebars
    let template_path = config.mfs_mount.join("groups/testgroup").join(format!("{}.lasz", test_file.display()));
    fs::write(&template_path, "hostname: {{ hostname }}").unwrap();
    
    // TODO: Diff should show the rendered difference
    // - hostname: localhost
    // + hostname: <actual-hostname>
}

#[tokio::test]
#[ignore = "diff command not yet implemented"]
async fn test_diff_no_changes() {
    let env = TestEnvironment::new("diff_no_changes");
    let config = env.create_config();
    
    // Create and enroll a file
    let test_file = env.test_dir.join("config.txt");
    fs::write(&test_file, "unchanged content").unwrap();
    
    let mut enrollment_manager = EnrollmentManager::new(config.clone());
    enrollment_manager.enroll_file("testgroup", &test_file, false, false, None, None, Default::default()).await.unwrap();
    
    // TODO: Diff should indicate no changes
    // Output: "No differences found"
}

#[tokio::test]
#[ignore = "diff command not yet implemented"]
async fn test_diff_group_filter() {
    let env = TestEnvironment::new("diff_group_filter");
    let config = env.create_config();
    
    // Create files in different groups
    let file1 = env.test_dir.join("file1.txt");
    let file2 = env.test_dir.join("file2.txt");
    fs::write(&file1, "content1").unwrap();
    fs::write(&file2, "content2").unwrap();
    
    let mut enrollment_manager = EnrollmentManager::new(config.clone());
    enrollment_manager.enroll_file("group1", &file1, false, false, None, None, Default::default()).await.unwrap();
    enrollment_manager.enroll_file("group2", &file2, false, false, None, None, Default::default()).await.unwrap();
    
    // Modify both files
    fs::write(&file1, "modified1").unwrap();
    fs::write(&file2, "modified2").unwrap();
    
    // TODO: `laszoo diff --group group1` should only show changes for file1
    // TODO: `laszoo diff` should show changes for both files
}