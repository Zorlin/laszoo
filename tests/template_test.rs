mod common;

use common::*;

#[test]
fn test_handlebars_template_processing() {
    let env = TestEnvironment::new("handlebars");
    env.setup_git().expect("Failed to setup git");
    
    // Create a file with handlebars template
    let content = "server {{ hostname }}\nport 8080";
    let test_file = env.create_test_file("server.conf", content);
    
    // Enroll the file
    let output = env.run_laszoo(&["enroll", "servers", test_file.to_str().unwrap()])
        .expect("Failed to run laszoo");
    assert!(output.status.success());
    
    // Apply the template
    let output = env.run_laszoo(&["apply", "servers"])
        .expect("Failed to run laszoo");
    assert!(output.status.success());
    
    // Check that the file was processed with hostname
    let file_content = env.read_file(&test_file);
    assert!(file_content.contains(&env.hostname), "Hostname not substituted in template");
    assert_eq!(file_content, format!("server {}\nport 8080", env.hostname));
}

#[test]
fn test_quack_tags_processing() {
    let env = TestEnvironment::new("quack_tags");
    env.setup_git().expect("Failed to setup git");
    
    // Create a group template in MFS
    let group_template_path = env.mfs_mount
        .join("groups")
        .join("testgroup")
        .join("quack.conf.lasz");
    std::fs::create_dir_all(group_template_path.parent().unwrap()).unwrap();
    std::fs::write(&group_template_path, "shared line\n{{ quack }}\nend line").unwrap();
    
    // Create machine template with quack content
    let machine_template_path = env.mfs_mount
        .join("machines")
        .join(&env.hostname)
        .join("quack.conf.lasz");
    std::fs::create_dir_all(machine_template_path.parent().unwrap()).unwrap();
    std::fs::write(&machine_template_path, "[[x machine specific content x]]").unwrap();
    
    // Create the local file
    let local_file = env.test_dir.join("quack.conf");
    std::fs::write(&local_file, "placeholder").unwrap();
    
    // Create manifest entry
    let manifest = r#"{
        "version": "1.0",
        "entries": {
            "/quack.conf": {
                "original_path": "/quack.conf",
                "checksum": "test",
                "group": "testgroup",
                "enrolled_at": "2025-01-01T00:00:00Z",
                "last_synced": null,
                "template_path": "/groups/testgroup/quack.conf.lasz",
                "is_hybrid": true
            }
        }
    }"#;
    
    let manifest_path = env.mfs_mount
        .join("machines")
        .join(&env.hostname)
        .join("manifest.json");
    std::fs::create_dir_all(manifest_path.parent().unwrap()).unwrap();
    std::fs::write(&manifest_path, manifest).unwrap();
    
    // Apply template
    let output = env.run_laszoo(&["apply", "testgroup"])
        .expect("Failed to run laszoo");
    
    // Read result
    let result = env.read_file(&local_file);
    assert_eq!(result, "shared line\nmachine specific content\nend line", 
        "Quack tags not processed correctly");
}

#[test]
fn test_multiple_quack_tags() {
    let env = TestEnvironment::new("multi_quack");
    env.setup_git().expect("Failed to setup git");
    
    // Create templates
    let group_template = "first: {{ quack }}\nsecond: {{ quack }}\nthird: {{ quack }}";
    let machine_template = "[[x value1 x]]\n[[x value2 x]]\n[[x value3 x]]";
    
    let group_path = env.mfs_mount
        .join("groups")
        .join("test")
        .join("multi.conf.lasz");
    std::fs::create_dir_all(group_path.parent().unwrap()).unwrap();
    std::fs::write(&group_path, group_template).unwrap();
    
    let machine_path = env.mfs_mount
        .join("machines")
        .join(&env.hostname)
        .join("multi.conf.lasz");
    std::fs::create_dir_all(machine_path.parent().unwrap()).unwrap();
    std::fs::write(&machine_path, machine_template).unwrap();
    
    // Create local file and manifest
    let local_file = env.test_dir.join("multi.conf");
    std::fs::write(&local_file, "placeholder").unwrap();
    
    // Create manifest to enable hybrid mode
    let manifest = format!(r#"{{
        "version": "1.0",
        "entries": {{
            "{}": {{
                "original_path": "{}",
                "checksum": "test",
                "group": "test",
                "enrolled_at": "2025-01-01T00:00:00Z",
                "last_synced": null,
                "template_path": "{}",
                "is_hybrid": true
            }}
        }}
    }}"#, local_file.to_str().unwrap(), local_file.to_str().unwrap(), group_path.to_str().unwrap());
    
    let manifest_path = env.mfs_mount
        .join("machines")
        .join(&env.hostname)
        .join("manifest.json");
    std::fs::write(&manifest_path, manifest).unwrap();
    
    // Apply template
    let output = env.run_laszoo(&["apply", "test"])
        .expect("Failed to apply template");
    assert!(output.status.success());
    
    // Check result
    let result = env.read_file(&local_file);
    assert_eq!(result, "first: value1\nsecond: value2\nthird: value3");
}

#[test]
fn test_partial_quack_tags() {
    let env = TestEnvironment::new("partial_quack");
    env.setup_git().expect("Failed to setup git");
    
    // Group template has 3 quack placeholders
    let group_template = "A: {{ quack }}\nB: {{ quack }}\nC: {{ quack }}";
    // Machine template only provides 2
    let machine_template = "[[x first x]]\n[[x second x]]";
    
    // Set up the templates and apply
    let group_path = env.mfs_mount.join("groups").join("partial").join("test.conf.lasz");
    std::fs::create_dir_all(group_path.parent().unwrap()).unwrap();
    std::fs::write(&group_path, group_template).unwrap();
    
    let machine_path = env.mfs_mount.join("machines").join(&env.hostname).join("test.conf.lasz");
    std::fs::create_dir_all(machine_path.parent().unwrap()).unwrap();
    std::fs::write(&machine_path, machine_template).unwrap();
    
    let local_file = env.test_dir.join("test.conf");
    std::fs::write(&local_file, "temp").unwrap();
    
    // Create manifest
    let manifest = format!(r#"{{
        "version": "1.0",
        "entries": {{
            "{}": {{
                "original_path": "{}",
                "checksum": "test",
                "group": "partial",
                "enrolled_at": "2025-01-01T00:00:00Z",
                "last_synced": null,
                "template_path": "{}",
                "is_hybrid": true
            }}
        }}
    }}"#, local_file.to_str().unwrap(), local_file.to_str().unwrap(), group_path.to_str().unwrap());
    
    let manifest_path = env.mfs_mount.join("machines").join(&env.hostname).join("manifest.json");
    std::fs::write(&manifest_path, manifest).unwrap();
    
    // Apply and check
    let output = env.run_laszoo(&["apply", "partial"]).expect("Failed to apply");
    assert!(output.status.success());
    
    let result = env.read_file(&local_file);
    // Third quack should be empty
    assert_eq!(result, "A: first\nB: second\nC: ");
}

#[test]
fn test_multiline_quack_tags() {
    let env = TestEnvironment::new("multiline_quack");
    env.setup_git().expect("Failed to setup git");
    
    let group_template = "config:\n{{ quack }}\nend";
    let machine_template = "[[x line1\nline2\nline3 x]]";
    
    // Set up the templates
    let group_path = env.mfs_mount.join("groups").join("multi").join("config.lasz");
    std::fs::create_dir_all(group_path.parent().unwrap()).unwrap();
    std::fs::write(&group_path, group_template).unwrap();
    
    let machine_path = env.mfs_mount.join("machines").join(&env.hostname).join("config.lasz");
    std::fs::create_dir_all(machine_path.parent().unwrap()).unwrap();
    std::fs::write(&machine_path, machine_template).unwrap();
    
    let local_file = env.test_dir.join("config");
    std::fs::write(&local_file, "temp").unwrap();
    
    // Create manifest
    let manifest = format!(r#"{{
        "version": "1.0",
        "entries": {{
            "{}": {{
                "original_path": "{}",
                "checksum": "test",
                "group": "multi",
                "enrolled_at": "2025-01-01T00:00:00Z",
                "last_synced": null,
                "template_path": "{}",
                "is_hybrid": true
            }}
        }}
    }}"#, local_file.to_str().unwrap(), local_file.to_str().unwrap(), group_path.to_str().unwrap());
    
    let manifest_path = env.mfs_mount.join("machines").join(&env.hostname).join("manifest.json");
    std::fs::write(&manifest_path, manifest).unwrap();
    
    // Apply and check
    let output = env.run_laszoo(&["apply", "multi"]).expect("Failed to apply");
    assert!(output.status.success());
    
    let result = env.read_file(&local_file);
    assert_eq!(result, "config:\nline1\nline2\nline3\nend");
}