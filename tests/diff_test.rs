use std::fs;

mod common;
use common::TestEnvironment;

#[tokio::test]
#[ignore = "diff command not yet implemented"]
async fn test_diff_shows_changes() {
    let env = TestEnvironment::new("diff_changes");
    env.setup_git().expect("Failed to setup git");
    
    // Create and enroll a file
    let test_file = env.test_dir.join("config.txt");
    fs::write(&test_file, "line1\nline2\nline3").unwrap();
    
    // Enroll using CLI
    let output = env.run_laszoo(&["enroll", "testgroup", test_file.to_str().unwrap()]).unwrap();
    assert!(output.status.success());
    
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
    env.setup_git().expect("Failed to setup git");
    
    // Create and enroll a file
    let test_file = env.test_dir.join("config.txt");
    fs::write(&test_file, "original content").unwrap();
    
    // Enroll using CLI
    let output = env.run_laszoo(&["enroll", "testgroup", test_file.to_str().unwrap()]).unwrap();
    assert!(output.status.success());
    
    // Modify the template
    let template_path = env.mfs_mount.join("groups/testgroup").join(format!("{}.lasz", test_file.display()));
    fs::write(&template_path, "template modified content").unwrap();
    
    // TODO: Diff should show what would happen if we applied the template
    // - original content
    // + template modified content
}

#[tokio::test]
#[ignore = "diff command not yet implemented"]
async fn test_diff_with_handlebars() {
    let env = TestEnvironment::new("diff_handlebars");
    env.setup_git().expect("Failed to setup git");
    
    // Create a file
    let test_file = env.test_dir.join("config.txt");
    fs::write(&test_file, "hostname: localhost").unwrap();
    
    // Enroll using CLI
    let output = env.run_laszoo(&["enroll", "testgroup", test_file.to_str().unwrap()]).unwrap();
    assert!(output.status.success());
    
    // Modify template to use handlebars
    let template_path = env.mfs_mount.join("groups/testgroup").join(format!("{}.lasz", test_file.display()));
    fs::write(&template_path, "hostname: {{ hostname }}").unwrap();
    
    // TODO: Diff should show the rendered difference
    // - hostname: localhost
    // + hostname: <actual-hostname>
}

#[tokio::test]
#[ignore = "diff command not yet implemented"]
async fn test_diff_no_changes() {
    let env = TestEnvironment::new("diff_no_changes");
    env.setup_git().expect("Failed to setup git");
    
    // Create and enroll a file
    let test_file = env.test_dir.join("config.txt");
    fs::write(&test_file, "unchanged content").unwrap();
    
    // Enroll using CLI
    let output = env.run_laszoo(&["enroll", "testgroup", test_file.to_str().unwrap()]).unwrap();
    assert!(output.status.success());
    
    // TODO: Diff should indicate no changes
    // Output: "No differences found"
}

#[tokio::test]
#[ignore = "diff command not yet implemented"]
async fn test_diff_group_filter() {
    let env = TestEnvironment::new("diff_group_filter");
    env.setup_git().expect("Failed to setup git");
    
    // Create files in different groups
    let file1 = env.test_dir.join("file1.txt");
    let file2 = env.test_dir.join("file2.txt");
    fs::write(&file1, "content1").unwrap();
    fs::write(&file2, "content2").unwrap();
    
    // Enroll using CLI
    let output = env.run_laszoo(&["enroll", "group1", file1.to_str().unwrap()]).unwrap();
    assert!(output.status.success());
    let output = env.run_laszoo(&["enroll", "group2", file2.to_str().unwrap()]).unwrap();
    assert!(output.status.success());
    
    // Modify both files
    fs::write(&file1, "modified1").unwrap();
    fs::write(&file2, "modified2").unwrap();
    
    // TODO: `laszoo diff --group group1` should only show changes for file1
    // TODO: `laszoo diff` should show changes for both files
}