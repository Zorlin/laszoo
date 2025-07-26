use laszoo::config::Config;
use laszoo::enrollment::{EnrollmentManager, EnrollmentManifest};
use laszoo::template::TemplateEngine;
use laszoo::sync::{SyncEngine, SyncOperationType};
use laszoo::package::{PackageManager, PackageOperation};
use std::fs;
use std::path::PathBuf;

mod common;
use common::TestEnvironment;

#[test]
fn test_config_creation() {
    let config = Config {
        mfs_mount: PathBuf::from("/mnt/laszoo"),
        ollama_endpoint: "http://localhost:11434".to_string(),
        ollama_model: "llama2".to_string(),
        log_level: "info".to_string(),
        log_format: "pretty".to_string(),
        auto_commit: true,
    };
    
    assert_eq!(config.mfs_mount, PathBuf::from("/mnt/laszoo"));
    assert_eq!(config.ollama_endpoint, "http://localhost:11434");
    assert!(config.auto_commit);
}

#[test]
fn test_enrollment_manifest_serialization() {
    let mut manifest = EnrollmentManifest::new();
    manifest.add_entry("/etc/test.conf".into(), laszoo::enrollment::FileType::File, false, false, None, None, Default::default());
    
    let json = serde_json::to_string(&manifest).unwrap();
    let deserialized: EnrollmentManifest = serde_json::from_str(&json).unwrap();
    
    assert_eq!(manifest.entries.len(), deserialized.entries.len());
}

#[test]
fn test_template_engine_handlebars() {
    let engine = TemplateEngine::new();
    let template = "Hello, {{ name }}!";
    let rendered = engine.render(template, &PathBuf::from("/mnt/laszoo"), None).unwrap();
    
    // Should render with hostname
    assert!(rendered.contains("Hello, "));
    assert!(!rendered.contains("{{ name }}"));
}

#[test]
fn test_template_engine_quack_tags() {
    let content = "Server: [[x prod-01 x]]\nPort: 8080";
    let engine = TemplateEngine::new();
    let extracted = engine.extract_quack_tags(content);
    
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted[0], "prod-01");
}

#[test]
fn test_sync_operation_types() {
    let op = SyncOperationType::Rollback {
        template_content: "template content".to_string(),
    };
    
    match op {
        SyncOperationType::Rollback { template_content } => {
            assert_eq!(template_content, "template content");
        }
        _ => panic!("Wrong operation type"),
    }
}

#[test]
fn test_package_operation_parsing() {
    let op = PackageOperation::from_line("+nginx").unwrap();
    match op {
        PackageOperation::Install { name } => {
            assert_eq!(name, "nginx");
        }
        _ => panic!("Wrong operation type"),
    }
    
    let op = PackageOperation::from_line("^docker").unwrap();
    match op {
        PackageOperation::Upgrade { name, post_action } => {
            assert_eq!(name, "docker");
            assert!(post_action.is_none());
        }
        _ => panic!("Wrong operation type"),
    }
    
    let op = PackageOperation::from_line("!unwanted-package").unwrap();
    match op {
        PackageOperation::Remove { name } => {
            assert_eq!(name, "unwanted-package");
        }
        _ => panic!("Wrong operation type"),
    }
}

#[test]
fn test_file_checksum_calculation() {
    let env = TestEnvironment::new("checksum_test");
    let test_file = env.test_dir.join("test.txt");
    fs::write(&test_file, "test content").unwrap();
    
    let checksum1 = laszoo::fs::calculate_file_checksum(&test_file).unwrap();
    let checksum2 = laszoo::fs::calculate_file_checksum(&test_file).unwrap();
    
    // Same content should have same checksum
    assert_eq!(checksum1, checksum2);
    
    // Change content
    fs::write(&test_file, "different content").unwrap();
    let checksum3 = laszoo::fs::calculate_file_checksum(&test_file).unwrap();
    
    // Different content should have different checksum
    assert_ne!(checksum1, checksum3);
}

#[test]
fn test_group_membership() {
    let env = TestEnvironment::new("group_membership");
    let config = env.create_config();
    
    // Create membership symlink
    let memberships_dir = config.mfs_mount.join("memberships/testgroup");
    fs::create_dir_all(&memberships_dir).unwrap();
    
    let hostname = gethostname::gethostname().to_string_lossy().to_string();
    let symlink_path = memberships_dir.join(&hostname);
    let target = PathBuf::from("../../machines").join(&hostname);
    
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &symlink_path).unwrap();
    
    // Verify symlink exists
    assert!(symlink_path.exists());
    assert!(symlink_path.read_link().is_ok());
}

#[test]
fn test_action_config() {
    use laszoo::action::ActionConfig;
    
    let config = ActionConfig {
        before: Some("echo 'starting'".to_string()),
        after: Some("echo 'done'".to_string()),
    };
    
    assert_eq!(config.before.unwrap(), "echo 'starting'");
    assert_eq!(config.after.unwrap(), "echo 'done'");
}

#[test]
fn test_git_manager_init() {
    let env = TestEnvironment::new("git_init");
    let git_manager = laszoo::git::GitManager::new(env.test_dir.clone());
    
    // Initialize repo
    git_manager.init_repo().unwrap();
    
    // Verify .git directory exists
    assert!(env.test_dir.join(".git").exists());
}

#[test]
fn test_path_normalization() {
    // Test absolute path
    let abs_path = PathBuf::from("/etc/test.conf");
    assert!(abs_path.is_absolute());
    
    // Test relative path conversion
    let rel_path = PathBuf::from("test.conf");
    let abs = std::fs::canonicalize(&rel_path).unwrap_or(rel_path);
    assert!(abs.is_absolute() || abs.to_str().unwrap().contains("test.conf"));
}