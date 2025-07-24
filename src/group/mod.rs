use std::path::PathBuf;
use std::collections::{HashMap, HashSet};
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use tracing::{info, debug, warn};
use crate::error::{LaszooError, Result};
use crate::fs::get_laszoo_base;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub name: String,
    pub description: Option<String>,
    pub hosts: HashSet<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GroupManifest {
    pub version: String,
    pub groups: HashMap<String, Group>,
}

pub struct GroupManager {
    mfs_mount: PathBuf,
    laszoo_dir: String,
}

impl GroupManager {
    pub fn new(mfs_mount: PathBuf, laszoo_dir: String) -> Self {
        Self { mfs_mount, laszoo_dir }
    }
    
    /// Load the group manifest
    pub fn load_manifest(&self) -> Result<GroupManifest> {
        let manifest_path = self.manifest_path();
        
        if !manifest_path.exists() {
            debug!("Group manifest not found, creating new one");
            return Ok(GroupManifest {
                version: "1.0".to_string(),
                groups: HashMap::new(),
            });
        }
        
        let content = std::fs::read_to_string(&manifest_path)?;
        let manifest: GroupManifest = serde_json::from_str(&content)
            .map_err(|e| LaszooError::Other(format!("Failed to parse group manifest: {}", e)))?;
            
        Ok(manifest)
    }
    
    /// Save the group manifest
    pub fn save_manifest(&self, manifest: &GroupManifest) -> Result<()> {
        let manifest_path = self.manifest_path();
        
        // Ensure parent directory exists
        if let Some(parent) = manifest_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let content = serde_json::to_string_pretty(manifest)
            .map_err(|e| LaszooError::Other(format!("Failed to serialize group manifest: {}", e)))?;
            
        std::fs::write(&manifest_path, content)?;
        info!("Saved group manifest to {:?}", manifest_path);
        
        Ok(())
    }
    
    /// Create a new group
    pub fn create_group(&self, name: &str, description: Option<String>) -> Result<()> {
        let mut manifest = self.load_manifest()?;
        
        if manifest.groups.contains_key(name) {
            return Err(LaszooError::Other(format!("Group '{}' already exists", name)));
        }
        
        let group = Group {
            name: name.to_string(),
            description,
            hosts: HashSet::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        
        manifest.groups.insert(name.to_string(), group);
        self.save_manifest(&manifest)?;
        
        info!("Created group '{}'", name);
        Ok(())
    }
    
    /// Delete a group
    pub fn delete_group(&self, name: &str, force: bool) -> Result<()> {
        let mut manifest = self.load_manifest()?;
        
        match manifest.groups.get(name) {
            Some(group) => {
                if !group.hosts.is_empty() && !force {
                    return Err(LaszooError::Other(
                        format!("Group '{}' has {} hosts. Use --force to delete anyway", 
                            name, group.hosts.len())
                    ));
                }
                
                // Check if any enrolled files reference this group
                if !force {
                    let enrollment_count = self.count_enrolled_files_in_group(name)?;
                    if enrollment_count > 0 {
                        return Err(LaszooError::Other(
                            format!("Group '{}' has {} enrolled files. Use --force to delete anyway", 
                                name, enrollment_count)
                        ));
                    }
                }
                
                manifest.groups.remove(name);
                self.save_manifest(&manifest)?;
                
                info!("Deleted group '{}'", name);
                Ok(())
            }
            None => Err(LaszooError::Other(format!("Group '{}' not found", name))),
        }
    }
    
    /// List all groups
    pub fn list_groups(&self) -> Result<Vec<Group>> {
        let manifest = self.load_manifest()?;
        let mut groups: Vec<Group> = manifest.groups.values().cloned().collect();
        groups.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(groups)
    }
    
    /// Add a host to a group
    pub fn add_host_to_group(&self, group_name: &str, hostname: &str) -> Result<()> {
        let mut manifest = self.load_manifest()?;
        
        match manifest.groups.get_mut(group_name) {
            Some(group) => {
                if group.hosts.insert(hostname.to_string()) {
                    group.updated_at = Utc::now();
                    self.save_manifest(&manifest)?;
                    info!("Added host '{}' to group '{}'", hostname, group_name);
                } else {
                    warn!("Host '{}' is already in group '{}'", hostname, group_name);
                }
                Ok(())
            }
            None => Err(LaszooError::Other(format!("Group '{}' not found", group_name))),
        }
    }
    
    /// Remove a host from a group
    pub fn remove_host_from_group(&self, group_name: &str, hostname: &str) -> Result<()> {
        let mut manifest = self.load_manifest()?;
        
        match manifest.groups.get_mut(group_name) {
            Some(group) => {
                if group.hosts.remove(hostname) {
                    group.updated_at = Utc::now();
                    self.save_manifest(&manifest)?;
                    info!("Removed host '{}' from group '{}'", hostname, group_name);
                } else {
                    warn!("Host '{}' is not in group '{}'", hostname, group_name);
                }
                Ok(())
            }
            None => Err(LaszooError::Other(format!("Group '{}' not found", group_name))),
        }
    }
    
    /// Check if a host is in a group
    pub fn is_host_in_group(&self, group_name: &str, hostname: &str) -> Result<bool> {
        let manifest = self.load_manifest()?;
        
        match manifest.groups.get(group_name) {
            Some(group) => Ok(group.hosts.contains(hostname)),
            None => Ok(false),
        }
    }
    
    /// Get groups for a host
    pub fn get_groups_for_host(&self, hostname: &str) -> Result<Vec<String>> {
        let manifest = self.load_manifest()?;
        
        let groups: Vec<String> = manifest.groups
            .iter()
            .filter(|(_, group)| group.hosts.contains(hostname))
            .map(|(name, _)| name.clone())
            .collect();
            
        Ok(groups)
    }
    
    /// Count enrolled files in a group (helper for deletion check)
    fn count_enrolled_files_in_group(&self, group_name: &str) -> Result<usize> {
        let base_path = get_laszoo_base(&self.mfs_mount, &self.laszoo_dir);
        let mut count = 0;
        
        // Check all host directories
        if base_path.exists() {
            for entry in std::fs::read_dir(&base_path)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    let manifest_path = entry.path().join("manifest.json");
                    if manifest_path.exists() {
                        let content = std::fs::read_to_string(&manifest_path)?;
                        if let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&content) {
                            if let Some(entries) = manifest.get("entries").and_then(|e| e.as_object()) {
                                for (_, entry) in entries {
                                    if let Some(group) = entry.get("group").and_then(|g| g.as_str()) {
                                        if group == group_name {
                                            count += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        Ok(count)
    }
    
    /// Get the path to the group manifest
    fn manifest_path(&self) -> PathBuf {
        get_laszoo_base(&self.mfs_mount, &self.laszoo_dir).join("groups.json")
    }
}