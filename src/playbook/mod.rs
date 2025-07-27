use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;
use std::process::Command;
use chrono::{DateTime, Utc};
use tracing::{debug, info, error};
use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookMetadata {
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_run: Option<DateTime<Utc>>,
    pub run_count: u32,
}

pub struct PlaybookManager {
    mount_point: PathBuf,
    playbook_storage_path: PathBuf,
}

impl PlaybookManager {
    pub fn new(config: &Config) -> Result<Self> {
        let mount_point = PathBuf::from(&config.mfs_mount);
        let playbook_storage_path = mount_point.join("playbooks");


        // Ensure playbook directory exists with proper permissions
        Self::ensure_directory_exists(&playbook_storage_path)?;

        Ok(Self {
            mount_point,
            playbook_storage_path,
        })
    }

    /// Ensure a directory exists, using sudo if necessary
    fn ensure_directory_exists(path: &Path) -> Result<()> {
        if path.exists() {
            return Ok(());
        }

        debug!("Attempting to create directory: {:?}", path);
        
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
                let status = Command::new("sudo")
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

    /// Handle privileged directory creation (called when re-executed with sudo)
    pub fn handle_privileged_mkdir(path: &Path) -> Result<()> {
        // Verify we're running as root
        if !nix::unistd::geteuid().is_root() {
            return Err(anyhow!("Expected to be running as root"));
        }
        
        // Create the directory
        fs::create_dir_all(path)?;
        
        // Set permissions to 755
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o755);
            fs::set_permissions(path, perms)?;
        }
        
        Ok(())
    }

    /// List all playbooks by scanning the directory
    pub fn list_playbooks(&self) -> Result<Vec<(String, PathBuf)>> {
        let mut playbooks = Vec::new();
        
        if !self.playbook_storage_path.exists() {
            return Ok(playbooks);
        }
        
        for entry in fs::read_dir(&self.playbook_storage_path)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_dir() {
                // Check for metadata file
                let metadata_path = path.join(".laszoo-metadata.json");
                if metadata_path.exists() {
                    // Try to read the metadata to get the proper name
                    if let Ok(content) = fs::read_to_string(&metadata_path) {
                        if let Ok(metadata) = serde_json::from_str::<PlaybookMetadata>(&content) {
                            playbooks.push((metadata.name, path));
                            continue;
                        }
                    }
                }
                
                // Fallback to directory name if no metadata
                if let Some(name) = path.file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string()) {
                    playbooks.push((name, path));
                }
            }
        }
        
        playbooks.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(playbooks)
    }

    pub fn add_playbook(&self, source_path: &Path, name: Option<String>, custom_path: Option<PathBuf>) -> Result<String> {
        if !source_path.exists() {
            return Err(anyhow!("Source path does not exist: {:?}", source_path));
        }

        // Determine the playbook name
        let playbook_name = if let Some(name) = name {
            name
        } else {
            source_path
                .file_name()
                .ok_or_else(|| anyhow!("Could not determine playbook name from path"))?
                .to_string_lossy()
                .to_string()
        };

        // Check if playbook already exists
        let existing = self.list_playbooks()?;
        if existing.iter().any(|(name, _)| name == &playbook_name) {
            return Err(anyhow!("Playbook '{}' already exists", playbook_name));
        }

        // Determine destination path
        let dest_path = if let Some(custom) = custom_path {
            self.mount_point.join(custom)
        } else {
            self.playbook_storage_path.join(&playbook_name)
        };

        // Ensure parent directory exists
        if let Some(parent) = dest_path.parent() {
            Self::ensure_directory_exists(parent)?;
        }

        // Copy playbook to destination
        info!("Copying playbook from {:?} to {:?}", source_path, dest_path);
        
        if source_path.is_dir() {
            match copy_dir_all(source_path, &dest_path) {
                Ok(_) => info!("Successfully copied playbook directory"),
                Err(e) => {
                    error!("Failed to copy playbook directory: {}", e);
                    return Err(e);
                }
            }
        } else {
            // Single file playbook
            Self::ensure_directory_exists(&dest_path)?;
            let file_name = source_path.file_name().unwrap_or("playbook.yml".as_ref());
            let dest_file = dest_path.join(file_name);
            
            match fs::copy(source_path, &dest_file) {
                Ok(_) => info!("Successfully copied playbook file"),
                Err(e) => {
                    error!("Failed to copy playbook file: {}", e);
                    return Err(e.into());
                }
            }
        }

        // Create metadata file in the playbook directory
        let metadata = PlaybookMetadata {
            name: playbook_name.clone(),
            description: None,
            created_at: Utc::now(),
            last_run: None,
            run_count: 0,
        };
        
        let metadata_path = dest_path.join(".laszoo-metadata.json");
        let metadata_content = serde_json::to_string_pretty(&metadata)?;
        fs::write(metadata_path, metadata_content)?;

        info!("Added playbook: {}", playbook_name);
        Ok(playbook_name)
    }

    pub fn remove_playbook(&self, name: &str) -> Result<()> {
        let playbooks = self.list_playbooks()?;
        
        let entry = playbooks.iter()
            .find(|(n, _)| n == name)
            .ok_or_else(|| anyhow!("Playbook '{}' not found", name))?;

        // Remove the playbook directory
        if entry.1.exists() {
            Self::remove_directory(&entry.1)?;
        }

        info!("Removed playbook: {}", name);
        Ok(())
    }

    /// Remove a directory, using sudo if necessary
    fn remove_directory(path: &Path) -> Result<()> {
        // Try to remove normally first
        match fs::remove_dir_all(path) {
            Ok(_) => {
                debug!("Removed directory: {:?}", path);
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                debug!("Permission denied, re-executing with sudo");
                
                // Get the current executable path
                let exe_path = std::env::current_exe()?;
                
                // Re-execute ourselves with sudo
                let status = Command::new("sudo")
                    .arg(&exe_path)
                    .arg("--privileged-rmdir")
                    .arg(path)
                    .status()?;
                
                if !status.success() {
                    return Err(anyhow!("Failed to remove directory with sudo"));
                }
                
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Handle privileged directory removal (called when re-executed with sudo)
    pub fn handle_privileged_rmdir(path: &Path) -> Result<()> {
        // Verify we're running as root
        if !nix::unistd::geteuid().is_root() {
            return Err(anyhow!("Expected to be running as root"));
        }
        
        // Remove the directory
        fs::remove_dir_all(path)?;
        Ok(())
    }

    pub fn resolve_playbook_path(&self, name: &str) -> Result<PathBuf> {
        // If it's an absolute path, return as is
        if name.starts_with('/') {
            return Ok(PathBuf::from(name));
        }

        // If it's a relative path, resolve from current directory
        if name.starts_with("./") || name.starts_with("../") {
            return Ok(std::env::current_dir()?.join(name));
        }

        // Check in playbook storage
        let playbooks = self.list_playbooks()?;
        
        // First check exact match
        if let Some((_, path)) = playbooks.iter().find(|(n, _)| n == name) {
            let playbook_file = path.join("playbook.yml");
            if playbook_file.exists() {
                return Ok(playbook_file);
            }
            let playbook_file = path.join("playbook.yaml");
            if playbook_file.exists() {
                return Ok(playbook_file);
            }
        }

        // Check for direct .yml file
        let yml_path = self.playbook_storage_path.join(format!("{}.yml", name));
        if yml_path.exists() {
            return Ok(yml_path);
        }

        Err(anyhow!("Playbook '{}' not found", name))
    }

    pub fn update_last_run(&self, name: &str) -> Result<()> {
        let playbooks = self.list_playbooks()?;
        
        if let Some((_, path)) = playbooks.iter().find(|(n, _)| n == name) {
            let metadata_path = path.join(".laszoo-metadata.json");
            
            // Load existing metadata or create new
            let mut metadata = if metadata_path.exists() {
                let content = fs::read_to_string(&metadata_path)?;
                serde_json::from_str(&content)?
            } else {
                PlaybookMetadata {
                    name: name.to_string(),
                    description: None,
                    created_at: Utc::now(),
                    last_run: None,
                    run_count: 0,
                }
            };
            
            // Update metadata
            metadata.last_run = Some(Utc::now());
            metadata.run_count += 1;
            
            // Save metadata
            let content = serde_json::to_string_pretty(&metadata)?;
            fs::write(metadata_path, content)?;
        }
        
        Ok(())
    }

    pub fn run_playbook(&self, name: &str, inventory_path: Option<PathBuf>, _extra_args: Vec<String>) -> Result<()> {
        let playbook_path = self.resolve_playbook_path(name)?;
        
        // Default inventory path if not specified
        let inventory = inventory_path.unwrap_or_else(|| {
            self.mount_point.join("inventory/jetpack")
        });
        
        info!("Running playbook: {} from {:?}", name, playbook_path);
        info!("Using inventory: {:?}", inventory);
        
        // Update last run time
        if let Err(e) = self.update_last_run(name) {
            debug!("Failed to update last run time: {}", e);
        }
        
        // TODO: Actually run the playbook using Jetpack
        // For now, just print what we would do
        info!("Would execute: jetpack --playbook {:?} --inventory {:?}", playbook_path, inventory);
        
        Ok(())
    }
}

/// Copy directory recursively
fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    // Create destination directory with escalation if needed
    match fs::create_dir_all(dst) {
        Ok(_) => {},
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            debug!("Permission denied creating {:?}, re-executing with sudo", dst);
            
            // Get the current executable path
            let exe_path = std::env::current_exe()?;
            
            // Re-execute ourselves with sudo
            let status = Command::new("sudo")
                .arg(&exe_path)
                .arg("--privileged-mkdir")
                .arg(dst)
                .status()?;
                
            if !status.success() {
                return Err(anyhow!("Failed to create directory with sudo"));
            }
        }
        Err(e) => return Err(e.into()),
    }
    
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        
        if ty.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            match fs::copy(&src_path, &dst_path) {
                Ok(_) => {},
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    // For files, we need a different approach - copy with sudo
                    debug!("Permission denied copying file, need sudo");
                    return Err(anyhow!("Permission denied copying {}. Please run this command with sudo.", src_path.display()));
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
    
    Ok(())
}