use std::path::{Path, PathBuf};
use std::fs;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use tracing::{info, debug, warn};
use crate::error::{LaszooError, Result};
use sha2::{Sha256, Digest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentEntry {
    pub original_path: PathBuf,
    pub checksum: String,
    pub group: String,
    pub enrolled_at: chrono::DateTime<chrono::Utc>,
    pub last_synced: Option<chrono::DateTime<chrono::Utc>>,
    pub template_path: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EnrollmentManifest {
    pub version: String,
    pub entries: HashMap<PathBuf, EnrollmentEntry>,
}

impl EnrollmentManifest {
    pub fn new() -> Self {
        Self {
            version: "1.0".to_string(),
            entries: HashMap::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        
        let content = fs::read_to_string(path)?;
        let manifest = serde_json::from_str(&content)?;
        Ok(manifest)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn is_enrolled(&self, path: &Path) -> Option<&EnrollmentEntry> {
        self.entries.get(path)
    }

    pub fn add_entry(&mut self, entry: EnrollmentEntry) {
        self.entries.insert(entry.original_path.clone(), entry);
    }

    pub fn remove_entry(&mut self, path: &Path) -> Option<EnrollmentEntry> {
        self.entries.remove(path)
    }
}

pub struct EnrollmentManager {
    mfs_mount: PathBuf,
    hostname: String,
}

impl EnrollmentManager {
    pub fn new(mfs_mount: PathBuf, _laszoo_dir: String) -> Self {
        let hostname = gethostname::gethostname()
            .to_string_lossy()
            .to_string();
            
        Self {
            mfs_mount,
            hostname,
        }
    }

    pub fn manifest_path(&self) -> PathBuf {
        crate::fs::get_machine_dir(&self.mfs_mount, "", &self.hostname)
            .join("manifest.json")
    }

    pub fn load_manifest(&self) -> Result<EnrollmentManifest> {
        EnrollmentManifest::load(&self.manifest_path())
    }

    pub fn save_manifest(&self, manifest: &EnrollmentManifest) -> Result<()> {
        manifest.save(&self.manifest_path())
    }

    /// Enroll a file or directory into a group
    pub fn enroll_path(&self, group: &str, path: Option<&Path>, force: bool) -> Result<()> {
        // If no path specified, enroll the machine into the group
        if path.is_none() {
            return self.enroll_machine_to_group(group);
        }

        let path = path.unwrap();
        
        // Ensure path exists
        if !path.exists() {
            return Err(LaszooError::FileNotFound { 
                path: path.to_path_buf() 
            });
        }

        if path.is_file() {
            self.enroll_file(path, group, force)
        } else if path.is_dir() {
            self.enroll_directory(path, group, force)
        } else {
            Err(LaszooError::InvalidPath { 
                path: path.to_path_buf() 
            })
        }
    }

    /// Enroll a file into a group
    pub fn enroll_file(&self, file_path: &Path, group: &str, force: bool) -> Result<()> {
        // Check permissions
        if let Err(_) = fs::metadata(file_path) {
            return Err(LaszooError::PermissionDenied { 
                path: file_path.to_path_buf() 
            });
        }

        // Get absolute path
        let abs_path = file_path.canonicalize()?;
        
        // Load manifest
        let mut manifest = self.load_manifest()?;
        
        // Check if already enrolled
        if let Some(existing) = manifest.is_enrolled(&abs_path) {
            if !force {
                return Err(LaszooError::AlreadyEnrolled {
                    path: abs_path.clone(),
                    group: existing.group.clone(),
                });
            }
            info!("Force enrolling file that was in group '{}'", existing.group);
        }

        // Calculate checksum
        let checksum = self.calculate_checksum(&abs_path)?;
        
        // Read file content
        let content = fs::read_to_string(&abs_path)?;
        
        // Create/update group template
        let group_template_path = crate::fs::get_group_template_path(
            &self.mfs_mount, 
            "", 
            group,
            &abs_path
        )?;
        
        // Ensure parent directory exists
        if let Some(parent) = group_template_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // If this is the first enrollment for this file in this group, create template
        if !group_template_path.exists() {
            fs::write(&group_template_path, &content)?;
            info!("Created group template at {:?}", group_template_path);
            
            // Copy metadata
            self.copy_metadata(&abs_path, &group_template_path)?;
        }
        
        // Create enrollment entry
        let entry = EnrollmentEntry {
            original_path: abs_path.clone(),
            checksum,
            group: group.to_string(),
            enrolled_at: chrono::Utc::now(),
            last_synced: None,
            template_path: Some(group_template_path),
        };
        
        // Add to manifest
        manifest.add_entry(entry);
        self.save_manifest(&manifest)?;
        
        info!("Successfully enrolled {:?} into group '{}'", abs_path, group);
        Ok(())
    }

    /// Enroll a directory recursively
    fn enroll_directory(&self, dir_path: &Path, group: &str, force: bool) -> Result<()> {
        let abs_path = dir_path.canonicalize()?;
        
        for entry in walkdir::WalkDir::new(&abs_path) {
            let entry = entry?;
            if entry.file_type().is_file() {
                if let Err(e) = self.enroll_file(entry.path(), group, force) {
                    warn!("Failed to enroll {:?}: {}", entry.path(), e);
                }
            }
        }
        
        Ok(())
    }

    /// Enroll a machine into a group without specifying files
    fn enroll_machine_to_group(&self, group: &str) -> Result<()> {
        info!("Enrolling machine {} into group {}", self.hostname, group);
        
        // Check if group exists
        let group_dir = crate::fs::get_group_dir(&self.mfs_mount, "", group);
        if !group_dir.exists() {
            return Err(LaszooError::GroupNotFound { 
                name: group.to_string() 
            });
        }
        
        // Apply all templates from the group
        self.apply_group_templates(group)?;
        
        Ok(())
    }

    /// Apply all templates from a group to the local system
    pub fn apply_group_templates(&self, group: &str) -> Result<()> {
        let group_dir = crate::fs::get_group_dir(&self.mfs_mount, "", group);
        
        // Walk the group directory
        for entry in walkdir::WalkDir::new(&group_dir) {
            let entry = entry?;
            if entry.file_type().is_file() && entry.path().extension() == Some(std::ffi::OsStr::new("lasz")) {
                let template_path = entry.path();
                
                // Extract the original file path from the template path
                let relative_path = template_path.strip_prefix(&group_dir)
                    .map_err(|_| LaszooError::Other("Invalid template path structure".to_string()))?;
                
                // Remove only the .lasz extension, keeping any original extension
                let path_str = relative_path.to_string_lossy();
                let original_path = if path_str.ends_with(".lasz") {
                    PathBuf::from("/").join(&path_str[..path_str.len() - 5])
                } else {
                    PathBuf::from("/").join(relative_path)
                };
                
                // Apply the template
                self.apply_template(group, template_path, &original_path)?;
            }
        }
        
        Ok(())
    }

    /// Apply a single template to create/update a local file
    fn apply_template(&self, group: &str, template_path: &Path, target_path: &Path) -> Result<()> {
        info!("Applying template {:?} to {:?}", template_path, target_path);
        
        // Read template content
        let template_content = fs::read_to_string(template_path)?;
        
        // Check for machine-specific .lasz file
        let machine_lasz_path = crate::fs::get_machine_file_path(
            &self.mfs_mount,
            "",
            &self.hostname,
            target_path
        )?.with_extension("lasz");
        
        // Process content based on whether machine-specific template exists
        let final_content = if machine_lasz_path.exists() {
            let machine_content = fs::read_to_string(&machine_lasz_path)?;
            crate::template::process_with_quacks(&template_content, &machine_content)?
        } else {
            // Just process handlebars variables
            crate::template::process_handlebars(&template_content, &self.hostname)?
        };
        
        // Create parent directory if needed
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // Write the processed content
        fs::write(target_path, &final_content)?;
        
        // Copy metadata from template
        self.copy_metadata(template_path, target_path)?;
        
        // Update manifest
        let mut manifest = self.load_manifest()?;
        let checksum = self.calculate_checksum(target_path)?;
        
        let entry = EnrollmentEntry {
            original_path: target_path.to_path_buf(),
            checksum,
            group: group.to_string(),
            enrolled_at: chrono::Utc::now(),
            last_synced: Some(chrono::Utc::now()),
            template_path: Some(template_path.to_path_buf()),
        };
        
        manifest.add_entry(entry);
        self.save_manifest(&manifest)?;
        
        Ok(())
    }

    pub fn unenroll_file(&self, file_path: &Path) -> Result<()> {
        let abs_path = file_path.canonicalize()?;
        let mut manifest = self.load_manifest()?;
        
        if let Some(entry) = manifest.remove_entry(&abs_path) {
            // Note: We don't remove the group template as other machines might be using it
            self.save_manifest(&manifest)?;
            info!("Successfully unenrolled {:?}", abs_path);
            Ok(())
        } else {
            warn!("File {:?} was not enrolled", abs_path);
            Ok(())
        }
    }

    pub fn list_enrolled_files(&self, group: Option<&str>) -> Result<Vec<EnrollmentEntry>> {
        let manifest = self.load_manifest()?;
        let entries: Vec<EnrollmentEntry> = manifest.entries
            .values()
            .filter(|entry| {
                group.map_or(true, |g| entry.group == g)
            })
            .cloned()
            .collect();
        
        Ok(entries)
    }

    pub fn check_file_status(&self, file_path: &Path) -> Result<Option<FileStatus>> {
        let abs_path = file_path.canonicalize()?;
        let manifest = self.load_manifest()?;
        
        if let Some(entry) = manifest.is_enrolled(&abs_path) {
            let current_checksum = self.calculate_checksum(&abs_path)?;
            let status = if current_checksum == entry.checksum {
                FileStatus::Unchanged
            } else {
                FileStatus::Modified
            };
            Ok(Some(status))
        } else {
            Ok(None)
        }
    }

    fn calculate_checksum(&self, path: &Path) -> Result<String> {
        let mut file = fs::File::open(path)?;
        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher)?;
        Ok(format!("{:x}", hasher.finalize()))
    }

    fn copy_metadata(&self, from: &Path, to: &Path) -> Result<()> {
        let metadata = fs::metadata(from)?;
        
        // Copy permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = metadata.permissions();
            fs::set_permissions(to, permissions)?;
        }
        
        // Note: Owner/group copying would require elevated privileges
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let uid = metadata.uid();
            let gid = metadata.gid();
            debug!("Cannot copy ownership (uid: {}, gid: {}) to {:?} - requires elevated privileges", 
                  uid, gid, to);
        }
        
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileStatus {
    Unchanged,
    Modified,
}