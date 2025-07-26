use std::path::{Path, PathBuf};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use tracing::{info, debug};
use crate::error::Result;

/// Action configuration for files/directories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionConfig {
    /// Command to run before applying changes
    pub before: Option<String>,
    /// Command to run after applying changes
    pub after: Option<String>,
}

/// Actions manifest for storing file-specific actions
#[derive(Debug, Serialize, Deserialize)]
pub struct ActionsManifest {
    pub version: String,
    pub actions: HashMap<PathBuf, ActionConfig>,
}

impl ActionsManifest {
    pub fn new() -> Self {
        Self {
            version: "1.0".to_string(),
            actions: HashMap::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        
        let content = std::fs::read_to_string(path)?;
        let manifest = serde_json::from_str(&content)?;
        Ok(manifest)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn set_actions(&mut self, file_path: &Path, before: Option<String>, after: Option<String>) {
        if before.is_none() && after.is_none() {
            // Remove entry if both are None
            self.actions.remove(file_path);
        } else {
            self.actions.insert(
                file_path.to_path_buf(),
                ActionConfig { before, after }
            );
        }
    }

    pub fn get_actions(&self, file_path: &Path) -> Option<&ActionConfig> {
        self.actions.get(file_path)
    }
}

/// Action manager for handling triggers
pub struct ActionManager {
    mfs_mount: PathBuf,
    hostname: String,
}

impl ActionManager {
    pub fn new(mfs_mount: PathBuf) -> Self {
        let hostname = gethostname::gethostname()
            .to_string_lossy()
            .to_string();
            
        Self {
            mfs_mount,
            hostname,
        }
    }

    /// Get the actions manifest path for a group
    pub fn get_group_actions_path(&self, group: &str) -> PathBuf {
        self.mfs_mount
            .join("groups")
            .join(group)
            .join("actions.json")
    }

    /// Get the actions manifest path for the machine
    pub fn get_machine_actions_path(&self) -> PathBuf {
        self.mfs_mount
            .join("machines")
            .join(&self.hostname)
            .join("actions.json")
    }

    /// Load actions for a file from both group and machine manifests
    pub fn load_actions_for_file(&self, group: &str, file_path: &Path) -> Result<Option<ActionConfig>> {
        // First check machine-specific actions
        let machine_manifest = ActionsManifest::load(&self.get_machine_actions_path())?;
        if let Some(actions) = machine_manifest.get_actions(file_path) {
            return Ok(Some(actions.clone()));
        }

        // Then check group actions
        let group_manifest = ActionsManifest::load(&self.get_group_actions_path(group))?;
        if let Some(actions) = group_manifest.get_actions(file_path) {
            return Ok(Some(actions.clone()));
        }

        Ok(None)
    }

    /// Set actions for a file in a group
    pub fn set_group_actions(&self, group: &str, file_path: &Path, before: Option<String>, after: Option<String>) -> Result<()> {
        let manifest_path = self.get_group_actions_path(group);
        let mut manifest = ActionsManifest::load(&manifest_path)?;
        
        manifest.set_actions(file_path, before.clone(), after.clone());
        manifest.save(&manifest_path)?;
        
        if before.is_some() || after.is_some() {
            info!("Set actions for {} in group {}", file_path.display(), group);
            if let Some(b) = &before {
                debug!("  Before: {}", b);
            }
            if let Some(a) = &after {
                debug!("  After: {}", a);
            }
        } else {
            info!("Removed actions for {} in group {}", file_path.display(), group);
        }
        
        Ok(())
    }

    /// Set actions for a file on this machine
    pub fn set_machine_actions(&self, file_path: &Path, before: Option<String>, after: Option<String>) -> Result<()> {
        let manifest_path = self.get_machine_actions_path();
        let mut manifest = ActionsManifest::load(&manifest_path)?;
        
        manifest.set_actions(file_path, before.clone(), after.clone());
        manifest.save(&manifest_path)?;
        
        if before.is_some() || after.is_some() {
            info!("Set machine-specific actions for {}", file_path.display());
            if let Some(b) = &before {
                debug!("  Before: {}", b);
            }
            if let Some(a) = &after {
                debug!("  After: {}", a);
            }
        } else {
            info!("Removed machine-specific actions for {}", file_path.display());
        }
        
        Ok(())
    }

    /// Execute an action command
    pub fn execute_action(&self, command: &str) -> Result<()> {
        use std::process::Command;
        
        info!("Executing action: {}", command);
        
        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.is_empty() {
                debug!("Action output: {}", stdout);
            }
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(crate::error::LaszooError::Other(
                format!("Action command failed: {}", stderr)
            ))
        }
    }

    /// Execute before and after actions for a file
    pub fn execute_file_actions(&self, group: &str, file_path: &Path, phase: ActionPhase) -> Result<()> {
        if let Some(actions) = self.load_actions_for_file(group, file_path)? {
            match phase {
                ActionPhase::Before => {
                    if let Some(cmd) = &actions.before {
                        self.execute_action(cmd)?;
                    }
                }
                ActionPhase::After => {
                    if let Some(cmd) = &actions.after {
                        self.execute_action(cmd)?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// Phase of action execution
#[derive(Debug, Clone, Copy)]
pub enum ActionPhase {
    Before,
    After,
}