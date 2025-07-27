use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use chrono::{DateTime, Utc};
use tracing::{debug, info, warn, error};
use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookEntry {
    pub path: PathBuf,
    pub description: Option<String>,
    pub added_by: String,
    pub added_at: DateTime<Utc>,
    pub last_run: Option<DateTime<Utc>>,
    pub run_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookManifest {
    pub playbooks: HashMap<String, PlaybookEntry>,
}

impl Default for PlaybookManifest {
    fn default() -> Self {
        Self {
            playbooks: HashMap::new(),
        }
    }
}

pub struct PlaybookManager {
    mount_point: PathBuf,
    playbook_storage_path: PathBuf,
    manifest_path: PathBuf,
}

impl PlaybookManager {
    pub fn new(config: &Config) -> Result<Self> {
        let mount_point = PathBuf::from(&config.mfs_mount);
        let playbook_storage_path = mount_point.join("playbooks");
        let manifest_path = playbook_storage_path.join("manifest.json");

        // Ensure playbook directory exists with proper permissions
        Self::ensure_directory_exists(&playbook_storage_path)?;

        Ok(Self {
            mount_point,
            playbook_storage_path,
            manifest_path,
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
                info!("Permission denied, escalating privileges to create directory: {:?}", path);
                
                // Escalate to root
                let uid_before = unsafe { libc::geteuid() };
                sudo::escalate_if_needed().map_err(|e| anyhow!("Failed to escalate privileges: {}", e))?;
                
                // Now we're root, create the directory
                let result = fs::create_dir_all(path);
                
                // Set proper permissions if successful
                if result.is_ok() {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let mut perms = fs::metadata(path)?.permissions();
                        perms.set_mode(0o755);
                        let _ = fs::set_permissions(path, perms);
                    }
                }
                
                // Drop privileges back to original user
                if uid_before != 0 {
                    unsafe {
                        libc::seteuid(uid_before);
                    }
                }
                
                result?;
                debug!("Created directory with elevated privileges: {:?}", path);
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn load_manifest(&self) -> Result<PlaybookManifest> {
        if !self.manifest_path.exists() {
            debug!("Playbook manifest not found, creating new one");
            return Ok(PlaybookManifest::default());
        }

        let content = fs::read_to_string(&self.manifest_path)?;
        let manifest: PlaybookManifest = serde_json::from_str(&content)?;
        Ok(manifest)
    }

    pub fn save_manifest(&self, manifest: &PlaybookManifest) -> Result<()> {
        let content = serde_json::to_string_pretty(manifest)?;
        fs::write(&self.manifest_path, content)?;
        debug!("Saved playbook manifest");
        Ok(())
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

        // Load current manifest
        let mut manifest = self.load_manifest()?;

        // Check if playbook already exists
        if manifest.playbooks.contains_key(&playbook_name) {
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
        if source_path.is_dir() {
            copy_dir_all(source_path, &dest_path)?;
        } else {
            // Single file playbook
            Self::ensure_directory_exists(&dest_path)?;
            let file_name = source_path.file_name().unwrap_or("playbook.yml".as_ref());
            fs::copy(source_path, dest_path.join(file_name))?;
        }

        // Get current user and hostname
        let user = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
        let hostname = gethostname::gethostname().to_string_lossy().to_string();
        let added_by = format!("{}@{}", user, hostname);

        // Add to manifest
        let entry = PlaybookEntry {
            path: dest_path,
            description: None,
            added_by,
            added_at: Utc::now(),
            last_run: None,
            run_count: 0,
        };

        manifest.playbooks.insert(playbook_name.clone(), entry);
        self.save_manifest(&manifest)?;

        info!("Added playbook: {}", playbook_name);
        Ok(playbook_name)
    }

    pub fn list_playbooks(&self) -> Result<Vec<(String, PlaybookEntry)>> {
        let manifest = self.load_manifest()?;
        let mut playbooks: Vec<_> = manifest.playbooks.into_iter().collect();
        playbooks.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(playbooks)
    }

    pub fn remove_playbook(&self, name: &str) -> Result<()> {
        let mut manifest = self.load_manifest()?;

        let entry = manifest.playbooks.remove(name)
            .ok_or_else(|| anyhow!("Playbook '{}' not found", name))?;

        // Remove the playbook directory
        if entry.path.exists() {
            fs::remove_dir_all(&entry.path)?;
        }

        self.save_manifest(&manifest)?;
        info!("Removed playbook: {}", name);
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
        let manifest = self.load_manifest()?;
        
        // First check exact match in manifest
        if let Some(entry) = manifest.playbooks.get(name) {
            return Ok(entry.path.clone());
        }

        // Check for playbook.yml in named directory
        let playbook_dir = self.playbook_storage_path.join(name);
        if playbook_dir.exists() {
            let playbook_file = playbook_dir.join("playbook.yml");
            if playbook_file.exists() {
                return Ok(playbook_file);
            }
        }

        // Check for direct .yml file
        let yml_file = self.playbook_storage_path.join(format!("{}.yml", name));
        if yml_file.exists() {
            return Ok(yml_file);
        }

        // Check in current directory as fallback
        let current_dir_path = std::env::current_dir()?.join(name);
        if current_dir_path.exists() {
            return Ok(current_dir_path);
        }

        Err(anyhow!("Playbook '{}' not found", name))
    }

    pub fn update_last_run(&self, name: &str) -> Result<()> {
        let mut manifest = self.load_manifest()?;
        
        if let Some(entry) = manifest.playbooks.get_mut(name) {
            entry.last_run = Some(Utc::now());
            entry.run_count += 1;
            self.save_manifest(&manifest)?;
        }
        
        Ok(())
    }

    pub fn run_playbook(&self, name: &str, inventory_path: Option<PathBuf>, extra_args: Vec<String>) -> Result<()> {
        let playbook_path = self.resolve_playbook_path(name)?;
        
        // Default inventory path if not specified
        let inventory = inventory_path.unwrap_or_else(|| {
            self.mount_point.join("inventory/jetpack")
        });

        info!("Running playbook: {} from {:?}", name, playbook_path);
        info!("Using inventory: {:?}", inventory);

        // Use Jetpack API to run the playbook
        let result = jetpack::run_playbook(&playbook_path.to_string_lossy())
            .inventory(&inventory.to_string_lossy())
            .local() // Run locally by default
            .threads(4)
            .run()?;

        if result.success {
            info!("Playbook completed successfully. Hosts processed: {}", result.hosts_processed);
            
            // Update last run time if this was a managed playbook
            if let Ok(manifest) = self.load_manifest() {
                if manifest.playbooks.contains_key(name) {
                    let _ = self.update_last_run(name);
                }
            }
        } else {
            return Err(anyhow!("Playbook execution failed"));
        }

        Ok(())
    }
}

// Helper function to copy directories recursively
fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}