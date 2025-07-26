mod common;

use common::*;

#[test]
fn test_package_conf_parsing() {
    use laszoo::package::{PackageManager, PackageOperation};
    use std::path::PathBuf;
    
    let pkg_manager = PackageManager::new(PathBuf::from("/tmp/test"));
    
    let content = r#"
# Test package configuration
^nginx --upgrade=systemctl restart nginx
+nano
=vim
!emacs
!!!old-package
    "#;
    
    let operations = pkg_manager.parse_packages_conf(content).unwrap();
    
    assert_eq!(operations.len(), 5);
    
    // Check upgrade with post-action
    match &operations[0] {
        PackageOperation::Upgrade { name, post_action } => {
            assert_eq!(name, "nginx");
            assert_eq!(post_action.as_deref(), Some("systemctl restart nginx"));
        }
        _ => panic!("Expected Upgrade operation"),
    }
    
    // Check install
    match &operations[1] {
        PackageOperation::Install { name } => {
            assert_eq!(name, "nano");
        }
        _ => panic!("Expected Install operation"),
    }
    
    // Check keep
    match &operations[2] {
        PackageOperation::Keep { name } => {
            assert_eq!(name, "vim");
        }
        _ => panic!("Expected Keep operation"),
    }
    
    // Check remove
    match &operations[3] {
        PackageOperation::Remove { name } => {
            assert_eq!(name, "emacs");
        }
        _ => panic!("Expected Remove operation"),
    }
    
    // Check purge
    match &operations[4] {
        PackageOperation::Purge { name } => {
            assert_eq!(name, "old-package");
        }
        _ => panic!("Expected Purge operation"),
    }
}

#[test]
fn test_package_install_command() {
    let env = TestEnvironment::new("package_install");
    env.setup_git().expect("Failed to setup git");
    
    // Create a test group
    let output = env.run_laszoo(&["group", "webservers", "add"])
        .expect("Failed to add machine to group");
    assert!(output.status.success());
    
    // Install packages
    let output = env.run_laszoo(&["install", "webservers", "-p", "nginx", "-p", "curl"])
        .expect("Failed to install packages");
    
    println!("Install stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("Install stderr: {}", String::from_utf8_lossy(&output.stderr));
    
    assert!(output.status.success());
    
    // Check that packages.conf was created
    let packages_conf = env.mfs_mount
        .join("groups")
        .join("webservers")
        .join("etc")
        .join("laszoo")
        .join("packages.conf");
    
    assert!(packages_conf.exists(), "packages.conf should exist");
    
    let content = std::fs::read_to_string(&packages_conf).unwrap();
    assert!(content.contains("+nginx"));
    assert!(content.contains("+curl"));
}

#[test]
fn test_package_operations_merge() {
    use laszoo::package::{PackageManager, PackageOperation};
    
    let env = TestEnvironment::new("package_merge");
    env.setup_git().expect("Failed to setup git");
    
    let pkg_manager = PackageManager::new(env.mfs_mount.clone());
    let hostname = env.original_hostname.clone();
    
    // Create group packages.conf
    let group_dir = env.mfs_mount
        .join("groups")
        .join("testgroup")
        .join("etc")
        .join("laszoo");
    std::fs::create_dir_all(&group_dir).unwrap();
    
    let group_packages = group_dir.join("packages.conf");
    std::fs::write(&group_packages, "+nginx\n+curl\n!emacs\n").unwrap();
    
    // Create machine packages.conf that overrides
    let machine_dir = env.mfs_mount
        .join("machines")
        .join(&hostname)
        .join("etc")
        .join("laszoo");
    std::fs::create_dir_all(&machine_dir).unwrap();
    
    let machine_packages = machine_dir.join("packages.conf");
    std::fs::write(&machine_packages, "!nginx\n+vim\n").unwrap();
    
    // Load merged operations
    let operations = pkg_manager.load_package_operations("testgroup", Some(&hostname)).unwrap();
    
    // Should have: +curl (from group), !nginx (overridden by machine), +vim (from machine), !emacs (from group)
    assert_eq!(operations.len(), 4);
    
    // Check that nginx was overridden to remove
    let nginx_op = operations.iter().find(|op| {
        match op {
            PackageOperation::Remove { name } => name == "nginx",
            _ => false,
        }
    });
    assert!(nginx_op.is_some(), "nginx should be marked for removal");
    
    // Check that vim was added
    let vim_op = operations.iter().find(|op| {
        match op {
            PackageOperation::Install { name } => name == "vim",
            _ => false,
        }
    });
    assert!(vim_op.is_some(), "vim should be marked for installation");
}