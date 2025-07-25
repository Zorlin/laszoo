mod common;

use common::*;

#[test]
fn test_machine_join_group() {
    let env = TestEnvironment::new("machine_join");
    env.setup_git().expect("Failed to setup git");
    
    // Machine should automatically join group when enrolling
    let test_file = env.create_test_file("join.conf", "content");
    let relative_path = test_file.strip_prefix(&env.test_dir).unwrap();
    let output = env.run_laszoo(&["enroll", "joingroup", relative_path.to_str().unwrap()])
        .expect("Failed to enroll");
    assert!(output.status.success());
    
    // Check that machine is in group
    // Note: When enrolling files, the actual system hostname is used, not the test env hostname
    let groups_file = env.mfs_mount
        .join("machines")
        .join(&env.original_hostname)
        .join("etc")
        .join("laszoo")
        .join("groups.conf");
    
    assert!(env.file_exists(&groups_file), "groups.conf not created");
    let groups_content = env.read_file(&groups_file);
    assert!(groups_content.contains("joingroup"), "Machine not added to group");
}

#[test]
#[ignore = "Group list command not yet implemented"]
fn test_list_group_members() {
    let env1 = TestEnvironment::new("list_members");
    env1.setup_git().expect("Failed to setup git");
    
    // Create multiple machines
    let env2 = create_second_machine(&env1, "member2");
    let env3 = create_second_machine(&env1, "member3");
    
    // Save hostnames before moving environments
    let hostname1 = env1.hostname.clone();
    let hostname2 = env2.hostname.clone();
    let hostname3 = env3.hostname.clone();
    
    // Each machine joins the group
    for (env, name) in &[(&env1, "file1"), (&env2, "file2"), (&env3, "file3")] {
        let file = env.create_test_file(&format!("{}.conf", name), "content");
        let relative_path = file.strip_prefix(&env.test_dir).unwrap();
        let output = env.run_laszoo(&["enroll", "membergroup", relative_path.to_str().unwrap()])
            .expect("Failed to enroll");
        assert!(output.status.success());
    }
    
    // List group members
    let output = env1.run_laszoo(&["group", "membergroup", "list"])
        .expect("Failed to list group");
    
    let output_str = String::from_utf8_lossy(&output.stdout);
    assert!(output_str.contains(&hostname1));
    assert!(output_str.contains(&hostname2));
    assert!(output_str.contains(&hostname3));
}

#[test]
#[ignore = "Group rename command not yet implemented"]
fn test_group_rename() {
    let env = TestEnvironment::new("group_rename");
    env.setup_git().expect("Failed to setup git");
    
    // Create a group by enrolling a file
    let test_file = env.create_test_file("rename.conf", "content");
    let relative_path = test_file.strip_prefix(&env.test_dir).unwrap();
    let output = env.run_laszoo(&["enroll", "oldname", relative_path.to_str().unwrap()])
        .expect("Failed to enroll");
    assert!(output.status.success());
    
    // Rename the group
    let output = env.run_laszoo(&["group", "oldname", "rename", "newname"])
        .expect("Failed to rename group");
    assert!(output.status.success());
    
    // Check that old group directory doesn't exist
    let old_group_dir = env.mfs_mount.join("groups").join("oldname");
    assert!(!env.file_exists(&old_group_dir), "Old group directory still exists");
    
    // Check that new group directory exists
    let new_group_dir = env.mfs_mount.join("groups").join("newname");
    assert!(env.file_exists(&new_group_dir), "New group directory doesn't exist");
    
    // Check that machine's groups.conf was updated
    let groups_file = env.mfs_mount
        .join("machines")
        .join(&env.original_hostname)
        .join("etc")
        .join("laszoo")
        .join("groups.conf");
    
    let groups_content = env.read_file(&groups_file);
    assert!(!groups_content.contains("oldname"), "Old group name still in groups.conf");
    assert!(groups_content.contains("newname"), "New group name not in groups.conf");
}

#[test]
#[ignore = "Group config command not yet implemented"]
fn test_group_config_sync_action() {
    let env = TestEnvironment::new("group_config");
    env.setup_git().expect("Failed to setup git");
    
    // Configure group with specific sync action
    let output = env.run_laszoo(&["group", "configgroup", "config", "--action", "rollback"])
        .expect("Failed to configure group");
    assert!(output.status.success());
    
    // Check that config was saved
    let config_file = env.mfs_mount
        .join("groups")
        .join("configgroup")
        .join("config.toml");
    
    assert!(env.file_exists(&config_file), "Group config file not created");
    let config_content = env.read_file(&config_file);
    assert!(config_content.contains("rollback"), "Sync action not saved in config");
}

#[test]
#[ignore = "Group config command not yet implemented"]
fn test_group_with_before_after_triggers() {
    let env = TestEnvironment::new("group_triggers");
    env.setup_git().expect("Failed to setup git");
    
    // Configure group with triggers
    let output = env.run_laszoo(&[
        "group", "triggergroup", "config",
        "--before", "echo 'before' > /tmp/before.txt",
        "--after", "echo 'after' > /tmp/after.txt"
    ]).expect("Failed to configure group");
    assert!(output.status.success());
    
    // Check config
    let config_file = env.mfs_mount
        .join("groups")
        .join("triggergroup")
        .join("config.toml");
    
    let config_content = env.read_file(&config_file);
    assert!(config_content.contains("before"), "Before trigger not saved");
    assert!(config_content.contains("after"), "After trigger not saved");
}

#[test]
#[ignore = "Group remove command not yet implemented"]
fn test_machine_leave_group() {
    let env = TestEnvironment::new("machine_leave");
    env.setup_git().expect("Failed to setup git");
    
    // Join a group
    let test_file = env.create_test_file("leave.conf", "content");
    let relative_path = test_file.strip_prefix(&env.test_dir).unwrap();
    let output = env.run_laszoo(&["enroll", "leavegroup", relative_path.to_str().unwrap()])
        .expect("Failed to enroll");
    assert!(output.status.success());
    
    // Verify machine is in group
    let groups_file = env.mfs_mount
        .join("machines")
        .join(&env.original_hostname)
        .join("etc")
        .join("laszoo")
        .join("groups.conf");
    assert!(env.read_file(&groups_file).contains("leavegroup"));
    
    // Leave the group
    let output = env.run_laszoo(&["group", "leavegroup", "remove", &env.original_hostname])
        .expect("Failed to remove from group");
    assert!(output.status.success());
    
    // Verify machine is no longer in group
    let groups_content = env.read_file(&groups_file);
    assert!(!groups_content.contains("leavegroup"), "Machine still in group after removal");
}

#[test]
#[ignore = "Group remove command not yet implemented"]
fn test_empty_group_deletion() {
    let env1 = TestEnvironment::new("empty_group");
    env1.setup_git().expect("Failed to setup git");
    
    // Create a second machine
    let env2 = create_second_machine(&env1, "temp-member");
    
    // Both machines join a group
    for env in &[&env1, &env2] {
        let file = env.create_test_file("temp.conf", "content");
        let relative_path = file.strip_prefix(&env.test_dir).unwrap();
        let output = env.run_laszoo(&["enroll", "tempgroup", relative_path.to_str().unwrap()])
            .expect("Failed to enroll");
        assert!(output.status.success());
    }
    
    // Remove first machine
    let output = env1.run_laszoo(&["group", "tempgroup", "remove", &env1.original_hostname])
        .expect("Failed to remove machine");
    assert!(output.status.success());
    
    // Group should still exist
    let group_dir = env1.mfs_mount.join("groups").join("tempgroup");
    assert!(env1.file_exists(&group_dir), "Group removed too early");
    
    // Remove second machine (last member)
    let output = env1.run_laszoo(&["group", "tempgroup", "remove", "temp-member"])
        .expect("Failed to remove last machine");
    assert!(output.status.success());
    
    // Group should be deleted now
    assert!(!env1.file_exists(&group_dir), "Empty group not deleted");
}