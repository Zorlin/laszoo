use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::fs;
use tracing::{debug, info};
use crate::config::Config;
use crate::group::GroupManager;
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JetpackGroup {
    pub hosts: Vec<String>,
    pub subgroups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JetpackHostVars {
    #[serde(flatten)]
    pub vars: HashMap<String, JsonValue>,
}

pub struct InventoryGenerator {
    mount_point: PathBuf,
    inventory_path: PathBuf,
    groups_path: PathBuf,
    host_vars_path: PathBuf,
}

impl InventoryGenerator {
    pub fn new(config: &Config) -> Result<Self> {
        let mount_point = PathBuf::from(&config.mfs_mount);
        let inventory_path = mount_point.join("inventory/jetpack");
        let groups_path = inventory_path.join("groups");
        let host_vars_path = inventory_path.join("host_vars");
        let group_vars_path = inventory_path.join("group_vars");

        // Ensure inventory directories exist with proper permissions
        Self::ensure_directory_exists(&groups_path)?;
        Self::ensure_directory_exists(&host_vars_path)?;
        Self::ensure_directory_exists(&group_vars_path)?;

        Ok(Self {
            mount_point,
            inventory_path,
            groups_path,
            host_vars_path,
        })
    }

    /// Ensure a directory exists, using sudo if necessary
    fn ensure_directory_exists(path: &Path) -> Result<()> {
        if path.exists() {
            return Ok(());
        }

        // Try to create directory normally first
        match fs::create_dir_all(path) {
            Ok(_) => {
                debug!("Created directory: {:?}", path);
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                debug!("Permission denied, re-executing with sudo");
                
                // Get the current executable path
                let exe_path = std::env::current_exe()?;
                
                // Re-execute ourselves with sudo
                let status = std::process::Command::new("sudo")
                    .arg(&exe_path)
                    .arg("--privileged-mkdir")
                    .arg(path)
                    .status()?;
                
                if !status.success() {
                    return Err(anyhow!("Failed to create directory with sudo"));
                }
                
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn sync_from_laszoo_groups(&self, config: &Config) -> Result<()> {
        info!("Syncing inventory from Laszoo groups");

        // Read group membership from symlinks in /mnt/laszoo/memberships/
        let memberships_dir = config.mfs_mount.join("memberships");
        let mut groups: HashMap<String, HashSet<String>> = HashMap::new();
        let mut all_hosts: HashSet<String> = HashSet::new();

        // Clear existing group files (but not host_vars)
        if self.groups_path.exists() {
            for entry in fs::read_dir(&self.groups_path)? {
                let entry = entry?;
                if entry.path().extension().and_then(|s| s.to_str()) == Some("yml") {
                    fs::remove_file(entry.path())?;
                }
            }
        }

        if memberships_dir.exists() {
            for entry in fs::read_dir(&memberships_dir)? {
                let entry = entry?;
                let group_path = entry.path();
                
                if !group_path.is_dir() {
                    continue;
                }
                
                let group_name = match group_path.file_name().and_then(|n| n.to_str()) {
                    Some(name) => name.to_string(),
                    None => continue,
                };
                
                let mut hosts = HashSet::new();
                
                // Read symlinks in the group directory
                for member_entry in fs::read_dir(&group_path)? {
                    let member_entry = member_entry?;
                    let member_path = member_entry.path();
                    
                    // Check if it's a symlink
                    if let Ok(metadata) = member_path.symlink_metadata() {
                        if metadata.file_type().is_symlink() {
                            // Get the hostname from the symlink name
                            if let Some(hostname) = member_path.file_name().and_then(|n| n.to_str()) {
                                hosts.insert(hostname.to_string());
                                all_hosts.insert(hostname.to_string());
                            }
                        }
                    }
                }
                
                if !hosts.is_empty() {
                    groups.insert(group_name, hosts);
                }
            }
        }

        // Create a group file for each discovered group
        for (group_name, hosts) in &groups {
            self.create_group_file(group_name, hosts)?;
        }

        // Create an "all" group that includes all hosts
        if !all_hosts.is_empty() {
            self.create_group_file("all", &all_hosts)?;
        }

        // Create/update host_vars files for each unique host
        // This must be done AFTER all groups are created so we can merge all group_vars
        for hostname in &all_hosts {
            self.create_host_vars(hostname)?;
        }

        info!("Inventory sync completed - found {} groups with {} total hosts", groups.len(), all_hosts.len());
        Ok(())
    }

    fn create_group_file(&self, group_name: &str, hosts: &HashSet<String>) -> Result<()> {
        let group_file = self.groups_path.join(group_name);
        
        let jetpack_group = JetpackGroup {
            hosts: hosts.iter().cloned().collect::<Vec<_>>(),
            subgroups: Vec::new(),
        };

        // Convert to YAML format using serde_json first then manually format
        let json_value = serde_json::to_value(&jetpack_group)?;
        let content = format_json_as_yaml(&json_value);
        fs::write(&group_file, content)?;
        
        debug!("Created group file: {:?} with {} hosts", group_file, hosts.len());
        Ok(())
    }

    fn create_host_vars(&self, hostname: &str) -> Result<()> {
        let host_vars_file = self.host_vars_path.join(hostname);
        
        // Check if host vars already exist
        if host_vars_file.exists() {
            debug!("Host vars already exist for {}, skipping", hostname);
            return Ok(());
        }

        // Create basic host vars - DO NOT include merged group vars here for security
        let mut vars = HashMap::new();
        vars.insert("host".to_string(), JsonValue::String(hostname.to_string()));
        vars.insert("laszoo_managed".to_string(), JsonValue::Bool(true));
        
        // Load machine-specific variables from Laszoo
        let machine_vars = self.load_machine_vars(hostname)?;
        for (key, value) in machine_vars {
            vars.insert(key, value);
        }
        
        let host_vars = JetpackHostVars { vars };

        let json_value = serde_json::to_value(&host_vars)?;
        let content = format_json_as_yaml(&json_value);
        fs::write(&host_vars_file, content)?;
        
        debug!("Created host vars file: {:?}", host_vars_file);
        Ok(())
    }

    pub fn update_group_membership(&self, group_name: &str, hosts: &HashSet<String>) -> Result<()> {
        // Update a single group file
        self.create_group_file(group_name, hosts)?;
        
        // Update host_vars for any new hosts
        for hostname in hosts {
            self.create_host_vars(hostname)?;
        }
        
        Ok(())
    }

    pub fn get_inventory_path(&self) -> &Path {
        &self.inventory_path
    }
    
    /// Get merged variables for a host (machine vars + all group vars)
    /// This is done dynamically at runtime for security - not stored on disk
    pub fn get_merged_host_vars(&self, hostname: &str) -> Result<HashMap<String, JsonValue>> {
        let mut merged_vars = HashMap::new();
        
        // Start with group vars (in order of precedence, least specific first)
        let groups = self.get_groups_for_host(hostname)?;
        for group in groups {
            if let Ok(group_vars) = self.load_group_vars(&group) {
                for (key, value) in group_vars {
                    merged_vars.insert(key, value);
                }
            }
        }
        
        // Machine-specific vars override group vars
        let machine_vars = self.load_machine_vars(hostname)?;
        for (key, value) in machine_vars {
            merged_vars.insert(key, value);
        }
        
        // Always add these base vars
        merged_vars.insert("host".to_string(), JsonValue::String(hostname.to_string()));
        merged_vars.insert("laszoo_managed".to_string(), JsonValue::Bool(true));
        
        Ok(merged_vars)
    }
    
    /// Get all groups that a host belongs to
    fn get_groups_for_host(&self, hostname: &str) -> Result<Vec<String>> {
        let mut groups = Vec::new();
        
        // Check each group file to see if it contains this host
        if self.groups_path.exists() {
            for entry in fs::read_dir(&self.groups_path)? {
                let entry = entry?;
                let path = entry.path();
                
                if path.extension().and_then(|s| s.to_str()) == Some("yml") {
                    let content = fs::read_to_string(&path)?;
                    if content.contains(hostname) {
                        // Parse more carefully to ensure it's actually in the hosts list
                        if let Some(group_name) = path.file_stem().and_then(|s| s.to_str()) {
                            // Simple check - could be made more robust with proper YAML parsing
                            if content.contains(&format!("- {}", hostname)) {
                                groups.push(group_name.to_string());
                            }
                        }
                    }
                }
            }
        }
        
        Ok(groups)
    }
    
    /// Load group_vars for a specific group
    fn load_group_vars(&self, group_name: &str) -> Result<HashMap<String, JsonValue>> {
        let group_vars_path = self.inventory_path.join("group_vars").join(group_name);
        let mut vars = HashMap::new();
        
        if group_vars_path.exists() {
            let content = fs::read_to_string(&group_vars_path)?;
            
            // Parse YAML-style key: value pairs
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                
                if let Some((key, value)) = line.split_once(':') {
                    let key = key.trim().to_string();
                    let value = value.trim();
                    
                    // Convert to appropriate JSON value
                    let json_value = if value == "true" {
                        JsonValue::Bool(true)
                    } else if value == "false" {
                        JsonValue::Bool(false)
                    } else if let Ok(num) = value.parse::<i64>() {
                        JsonValue::Number(num.into())
                    } else if let Ok(num) = value.parse::<f64>() {
                        JsonValue::Number(serde_json::Number::from_f64(num).unwrap_or(0.into()))
                    } else {
                        JsonValue::String(value.to_string())
                    };
                    
                    vars.insert(key, json_value);
                }
            }
        }
        
        Ok(vars)
    }
    
    /// Load machine-specific vars from Laszoo machine directory
    fn load_machine_vars(&self, hostname: &str) -> Result<HashMap<String, JsonValue>> {
        let mut vars = HashMap::new();
        
        // Check for vars in /mnt/laszoo/machines/<hostname>/etc/laszoo/vars.yml
        let machine_vars_path = self.mount_point
            .join("machines")
            .join(hostname)
            .join("etc/laszoo/vars.yml");
            
        if machine_vars_path.exists() {
            let content = fs::read_to_string(&machine_vars_path)?;
            
            // Parse YAML-style key: value pairs
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                
                if let Some((key, value)) = line.split_once(':') {
                    let key = key.trim().to_string();
                    let value = value.trim();
                    
                    let json_value = if value == "true" {
                        JsonValue::Bool(true)
                    } else if value == "false" {
                        JsonValue::Bool(false)
                    } else if let Ok(num) = value.parse::<i64>() {
                        JsonValue::Number(num.into())
                    } else if let Ok(num) = value.parse::<f64>() {
                        JsonValue::Number(serde_json::Number::from_f64(num).unwrap_or(0.into()))
                    } else {
                        JsonValue::String(value.to_string())
                    };
                    
                    vars.insert(key, json_value);
                }
            }
            
            debug!("Loaded {} machine-specific vars for {}", vars.len(), hostname);
        }
        
        Ok(vars)
    }

    pub fn create_dynamic_inventory_script(&self) -> Result<()> {
        // Create a dynamic inventory script that reads from the YAML files
        let script_path = self.inventory_path.join("dynamic_inventory.py");
        let script_content = r#"#!/usr/bin/env python3
import json
import os
import yaml
import sys

def main():
    inventory_dir = os.path.dirname(os.path.abspath(__file__))
    groups_dir = os.path.join(inventory_dir, 'groups')
    host_vars_dir = os.path.join(inventory_dir, 'host_vars')
    
    inventory = {
        '_meta': {
            'hostvars': {}
        }
    }
    
    # Load groups
    if os.path.exists(groups_dir):
        for filename in os.listdir(groups_dir):
            if filename not in ['hosts', 'host_vars', 'group_vars'] and not filename.startswith('.'):
                group_name = filename
                with open(os.path.join(groups_dir, filename), 'r') as f:
                    group_data = yaml.safe_load(f)
                    inventory[group_name] = {
                        'hosts': group_data.get('hosts', []),
                        'children': group_data.get('subgroups', [])
                    }
    
    # Load host vars
    if os.path.exists(host_vars_dir):
        for filename in os.listdir(host_vars_dir):
            if not filename.startswith('.'):
                host_name = filename
                with open(os.path.join(host_vars_dir, filename), 'r') as f:
                    host_data = yaml.safe_load(f)
                    inventory['_meta']['hostvars'][host_name] = host_data
    
    print(json.dumps(inventory, indent=2))

if __name__ == '__main__':
    main()
"#;

        fs::write(&script_path, script_content)?;
        
        // Make script executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script_path, perms)?;
        }
        
        info!("Created dynamic inventory script at {:?}", script_path);
        Ok(())
    }
}

// Helper function to format JSON as YAML-like string
fn format_json_as_yaml(value: &JsonValue) -> String {
    match value {
        JsonValue::Object(map) => {
            let mut result = String::new();
            for (key, val) in map {
                match val {
                    JsonValue::Array(arr) => {
                        result.push_str(&format!("{}:\n", key));
                        for item in arr {
                            result.push_str(&format!("  - {}\n", format_yaml_value(item)));
                        }
                    }
                    _ => {
                        result.push_str(&format!("{}: {}\n", key, format_yaml_value(val)));
                    }
                }
            }
            result
        }
        _ => format_yaml_value(value),
    }
}

fn format_yaml_value(value: &JsonValue) -> String {
    match value {
        JsonValue::String(s) => s.clone(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Null => "null".to_string(),
        JsonValue::Array(arr) => {
            format!("[{}]", arr.iter().map(|v| format_yaml_value(v)).collect::<Vec<_>>().join(", "))
        }
        JsonValue::Object(_) => {
            // For nested objects, use JSON representation
            serde_json::to_string(value).unwrap_or_default()
        }
    }
}