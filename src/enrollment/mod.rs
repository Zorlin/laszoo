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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_hybrid: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enrolled_directory: Option<PathBuf>,
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
    
    pub fn group_manifest_path(&self, group: &str) -> PathBuf {
        crate::fs::get_group_dir(&self.mfs_mount, "", group)
            .join("manifest.json")
    }

    pub fn load_manifest(&self) -> Result<EnrollmentManifest> {
        EnrollmentManifest::load(&self.manifest_path())
    }
    
    pub fn load_group_manifest(&self, group: &str) -> Result<EnrollmentManifest> {
        EnrollmentManifest::load(&self.group_manifest_path(group))
    }

    pub fn save_manifest(&self, manifest: &EnrollmentManifest) -> Result<()> {
        manifest.save(&self.manifest_path())
    }
    
    pub fn save_group_manifest(&self, group: &str, manifest: &EnrollmentManifest) -> Result<()> {
        manifest.save(&self.group_manifest_path(group))
    }

    /// Enroll a file or directory into a group
    pub fn enroll_path(&self, group: &str, path: Option<&Path>, force: bool, machine_specific: bool, hybrid: bool) -> Result<()> {
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
            self.enroll_file(path, group, force, machine_specific, hybrid)
        } else if path.is_dir() {
            self.enroll_directory(path, group, force, machine_specific, hybrid)
        } else {
            Err(LaszooError::InvalidPath { 
                path: path.to_path_buf() 
            })
        }
    }

    /// Enroll a file into a group
    pub fn enroll_file(&self, file_path: &Path, group: &str, force: bool, machine_specific: bool, hybrid: bool) -> Result<()> {
        // First ensure this machine is in the group
        self.add_machine_to_group(group)?;
        
        // Get absolute path
        let abs_path = file_path.canonicalize()?;
        
        // Check if this file is within any already-enrolled directories
        let group_manifest = self.load_group_manifest(group)?;
        for (enrolled_path, entry) in &group_manifest.entries {
            if entry.checksum == "directory" {
                // Check if our file is within this directory
                if abs_path.starts_with(enrolled_path) {
                    // This file is within an enrolled directory, just create the template
                    info!("File {:?} is within enrolled directory {:?}, adopting into directory", abs_path, enrolled_path);
                    
                    // Read file content
                    let content = fs::read_to_string(&abs_path)?;
                    
                    // Create group template
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
                    
                    // Create template
                    fs::write(&group_template_path, &content)?;
                    info!("Created group template at {:?}", group_template_path);
                    
                    // Copy metadata
                    self.copy_metadata(&abs_path, &group_template_path)?;
                    
                    info!("Successfully adopted {:?} into enrolled directory '{:?}'", abs_path, enrolled_path);
                    return Ok(());
                }
            }
        }
        
        // Not within any enrolled directory, proceed with normal enrollment
        self.enroll_file_with_dir(file_path, group, force, machine_specific, hybrid, None)
    }
    
    /// Enroll a file into a group with optional directory tracking
    fn enroll_file_with_dir(&self, file_path: &Path, group: &str, force: bool, machine_specific: bool, hybrid: bool, enrolled_directory: Option<&Path>) -> Result<()> {
        // Check permissions
        if let Err(_) = fs::metadata(file_path) {
            return Err(LaszooError::PermissionDenied { 
                path: file_path.to_path_buf() 
            });
        }

        // Get absolute path
        let abs_path = file_path.canonicalize()?;
        
        // Calculate checksum
        let checksum = self.calculate_checksum(&abs_path)?;
        
        // Read file content
        let content = fs::read_to_string(&abs_path)?;
        
        if machine_specific || hybrid {
            // Create machine-specific template
            let mut machine_template_path = crate::fs::get_machine_file_path(
                &self.mfs_mount,
                "",
                &self.hostname,
                &abs_path
            )?;
            // Append .lasz to the full filename (preserving original extension)
            let filename = machine_template_path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            machine_template_path.set_file_name(format!("{}.lasz", filename));
            
            // Ensure parent directory exists
            if let Some(parent) = machine_template_path.parent() {
                fs::create_dir_all(parent)?;
            }
            
            // Write machine-specific template
            fs::write(&machine_template_path, &content)?;
            info!("Created machine-specific template at {:?}", machine_template_path);
            
            // Copy metadata
            self.copy_metadata(&abs_path, &machine_template_path)?;
            
            // Load machine manifest and add entry
            let mut machine_manifest = self.load_manifest()?;
            
            // For machine-specific enrollment, we always allow it to override
            // Just warn if it was already enrolled
            if let Some(existing) = machine_manifest.is_enrolled(&abs_path) {
                info!("Overriding previous enrollment in group '{}'", existing.group);
            }
            
            let machine_entry = EnrollmentEntry {
                original_path: abs_path.clone(),
                checksum: checksum.clone(),
                group: group.to_string(),
                enrolled_at: chrono::Utc::now(),
                last_synced: None,
                template_path: Some(machine_template_path),
                is_hybrid: if hybrid { Some(true) } else { None },
                enrolled_directory: enrolled_directory.map(|p| p.to_path_buf()),
            };
            
            machine_manifest.add_entry(machine_entry);
            self.save_manifest(&machine_manifest)?;
            
            info!("Successfully enrolled {:?} as machine-specific for group '{}'", abs_path, group);
        } else {
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
            
            // Load group manifest and add entry
            let mut group_manifest = self.load_group_manifest(group)?;
            
            // Check if already enrolled in group manifest
            if let Some(existing) = group_manifest.is_enrolled(&abs_path) {
                if !force {
                    return Err(LaszooError::AlreadyEnrolled {
                        path: abs_path.clone(),
                        group: existing.group.clone(),
                    });
                }
                info!("Force enrolling file in group manifest");
            }
            
            let group_entry = EnrollmentEntry {
                original_path: abs_path.clone(),
                checksum,
                group: group.to_string(),
                enrolled_at: chrono::Utc::now(),
                last_synced: None,
                template_path: Some(group_template_path),
                is_hybrid: None,
                enrolled_directory: enrolled_directory.map(|p| p.to_path_buf()),
            };
            
            group_manifest.add_entry(group_entry);
            self.save_group_manifest(group, &group_manifest)?;
            
            info!("Successfully enrolled {:?} into group '{}'", abs_path, group);
        }
        
        Ok(())
    }

    /// Enroll a directory recursively
    fn enroll_directory(&self, dir_path: &Path, group: &str, force: bool, machine_specific: bool, hybrid: bool) -> Result<()> {
        // First ensure this machine is in the group
        self.add_machine_to_group(group)?;
        
        let abs_path = dir_path.canonicalize()?;
        
        // First enroll the directory itself as a marker
        if machine_specific {
            // Create machine-specific directory entry
            let mut machine_manifest = self.load_manifest()?;
            
            // Check if already enrolled
            if let Some(existing) = machine_manifest.is_enrolled(&abs_path) {
                if !force {
                    return Err(LaszooError::AlreadyEnrolled {
                        path: abs_path.clone(),
                        group: existing.group.clone(),
                    });
                }
                info!("Force enrolling directory in machine manifest");
            }
            
            let machine_entry = EnrollmentEntry {
                original_path: abs_path.clone(),
                checksum: "directory".to_string(),  // Special marker for directories
                group: group.to_string(),
                enrolled_at: chrono::Utc::now(),
                last_synced: None,
                template_path: None,  // Directories don't have templates
                is_hybrid: if hybrid { Some(true) } else { None },
                enrolled_directory: Some(abs_path.clone()),  // Mark this as an enrolled directory
            };
            
            machine_manifest.add_entry(machine_entry);
            self.save_manifest(&machine_manifest)?;
            
            info!("Successfully enrolled directory {:?} as machine-specific for group '{}'", abs_path, group);
        } else {
            // Create group directory entry
            let mut group_manifest = self.load_group_manifest(group)?;
            
            // Check if already enrolled in group manifest
            if let Some(existing) = group_manifest.is_enrolled(&abs_path) {
                if !force {
                    return Err(LaszooError::AlreadyEnrolled {
                        path: abs_path.clone(),
                        group: existing.group.clone(),
                    });
                }
                info!("Force enrolling directory in group manifest");
            }
            
            let group_entry = EnrollmentEntry {
                original_path: abs_path.clone(),
                checksum: "directory".to_string(),  // Special marker for directories
                group: group.to_string(),
                enrolled_at: chrono::Utc::now(),
                last_synced: None,
                template_path: None,  // Directories don't have templates
                is_hybrid: None,
                enrolled_directory: Some(abs_path.clone()),  // Mark this as an enrolled directory
            };
            
            group_manifest.add_entry(group_entry);
            self.save_group_manifest(group, &group_manifest)?;
            
            info!("Successfully enrolled directory {:?} into group '{}'", abs_path, group);
        }
        
        // Now copy all existing files in the directory to templates
        for entry in walkdir::WalkDir::new(&abs_path) {
            let entry = entry?;
            if entry.file_type().is_file() {
                // Create template for this file
                let file_path = entry.path();
                let content = fs::read_to_string(file_path)?;
                
                let group_template_path = crate::fs::get_group_template_path(
                    &self.mfs_mount, 
                    "", 
                    group,
                    file_path
                )?;
                
                // Ensure parent directory exists
                if let Some(parent) = group_template_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                
                // Create template if it doesn't exist
                if !group_template_path.exists() {
                    fs::write(&group_template_path, &content)?;
                    self.copy_metadata(file_path, &group_template_path)?;
                    debug!("Created template for directory file: {:?}", group_template_path);
                }
            }
        }
        
        Ok(())
    }

    /// Enroll a machine into a group without specifying files
    fn enroll_machine_to_group(&self, group: &str) -> Result<()> {
        info!("Enrolling machine {} into group {}", self.hostname, group);
        
        // Add machine to group
        self.add_machine_to_group(group)?;
        
        // Apply all templates from the group
        self.apply_group_templates(group)?;
        
        Ok(())
    }
    
    /// Add this machine to a group (creates group if needed)
    pub fn add_machine_to_group(&self, group: &str) -> Result<()> {
        // Create group directory if it doesn't exist
        let group_dir = crate::fs::get_group_dir(&self.mfs_mount, "", group);
        if !group_dir.exists() {
            fs::create_dir_all(&group_dir)?;
            info!("Created new group '{}'", group);
        }
        
        // Update machine's groups.conf
        let groups_file = self.mfs_mount
            .join("machines")
            .join(&self.hostname)
            .join("etc")
            .join("laszoo")
            .join("groups.conf");
        
        // Create directory if needed
        if let Some(parent) = groups_file.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // Read existing groups
        let mut groups: Vec<String> = if groups_file.exists() {
            fs::read_to_string(&groups_file)?
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            Vec::new()
        };
        
        // Add group if not already present
        if !groups.contains(&group.to_string()) {
            groups.push(group.to_string());
            groups.sort();
            
            // Write back
            fs::write(&groups_file, groups.join("\n") + "\n")?;
            info!("Added machine '{}' to group '{}'", self.hostname, group);
        }
        
        Ok(())
    }

    /// Get the group template path for a file
    pub fn get_group_template_path(&self, group: &str, file_path: &Path) -> Result<PathBuf> {
        let group_dir = crate::fs::get_group_dir(&self.mfs_mount, "", group);
        let relative_path = if file_path.is_absolute() {
            file_path.strip_prefix("/").unwrap_or(file_path)
        } else {
            file_path
        };
        
        let mut template_path = group_dir.join(relative_path);
        let current_name = template_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        template_path.set_file_name(format!("{}.lasz", current_name));
        
        Ok(template_path)
    }
    
    /// Apply a single template file to its target location
    pub fn apply_single_template(&self, template_path: &Path, target_path: &Path) -> Result<()> {
        // Read template content
        let template_content = std::fs::read_to_string(template_path)?;
        
        // Process the template
        let final_content = crate::template::process_handlebars(&template_content, &self.hostname)?;
        
        // Create parent directory if needed
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        // Write the processed content
        std::fs::write(target_path, &final_content)?;
        
        // Copy metadata from template
        self.copy_metadata(template_path, target_path)?;
        
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
        
        // Fix the machine-specific path - we need the relative path from root
        let relative_path = if target_path.is_absolute() {
            target_path.strip_prefix("/").unwrap_or(target_path)
        } else {
            target_path
        };
        
        // Build the machine-specific template path - preserve original extension
        let mut machine_lasz_path = crate::fs::get_machine_dir(&self.mfs_mount, "", &self.hostname)
            .join(relative_path);
        let current_name = machine_lasz_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        machine_lasz_path.set_file_name(format!("{}.lasz", current_name));
        
        // Check if this is a hybrid enrollment
        let machine_manifest = self.load_manifest()?;
        let is_hybrid = machine_manifest.is_enrolled(target_path)
            .and_then(|e| e.is_hybrid)
            .unwrap_or(false);
        
        // Process content based on whether machine-specific template exists
        let final_content = if machine_lasz_path.exists() {
            info!("Using machine-specific template from {:?}", machine_lasz_path);
            let machine_content = fs::read_to_string(&machine_lasz_path)?;
            
            if is_hybrid {
                info!("Processing in hybrid mode");
                // In hybrid mode, use group template with machine template providing quack values
                crate::template::process_with_quacks(&template_content, &machine_content)?
            } else {
                // Machine-specific template takes full precedence - just process it for quack tags
                crate::template::process_handlebars(&machine_content, &self.hostname)?
            }
        } else {
            // Just process handlebars variables and quack tags from group template
            crate::template::process_handlebars(&template_content, &self.hostname)?
        };
        
        // Create parent directory if needed
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // Write the processed content
        debug!("Writing content to {:?}, length: {}", target_path, final_content.len());
        debug!("Content: {:?}", final_content);
        fs::write(target_path, &final_content)?;
        
        // Copy metadata from template
        self.copy_metadata(template_path, target_path)?;
        
        // Update manifest
        let mut manifest = self.load_manifest()?;
        let checksum = self.calculate_checksum(target_path)?;
        
        // Check group manifest to see if this file has enrolled_directory info
        let enrolled_directory = if let Ok(group_manifest) = self.load_group_manifest(group) {
            group_manifest.is_enrolled(target_path)
                .and_then(|e| e.enrolled_directory.as_ref())
                .map(|p| p.to_path_buf())
        } else {
            None
        };
        
        let entry = EnrollmentEntry {
            original_path: target_path.to_path_buf(),
            checksum,
            group: group.to_string(),
            enrolled_at: chrono::Utc::now(),
            last_synced: Some(chrono::Utc::now()),
            template_path: Some(template_path.to_path_buf()),
            is_hybrid: if is_hybrid { Some(true) } else { None },
            enrolled_directory,
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
        // First check if file exists
        if !file_path.exists() {
            // File is missing - but we need to check if it's enrolled
            // Use the provided path as-is since we can't canonicalize a missing file
            
            // Check machine manifest
            let manifest = self.load_manifest()?;
            if manifest.is_enrolled(file_path).is_some() {
                return Ok(None); // File is enrolled but missing
            }
            
            // Check group manifests
            let groups_file = self.mfs_mount
                .join("machines")
                .join(&self.hostname)
                .join("etc")
                .join("laszoo")
                .join("groups.conf");
            
            if groups_file.exists() {
                let groups: Vec<String> = fs::read_to_string(&groups_file)?
                    .lines()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                
                for group in groups {
                    if let Ok(group_manifest) = self.load_group_manifest(&group) {
                        if group_manifest.is_enrolled(file_path).is_some() {
                            return Ok(None); // File is enrolled but missing
                        }
                    }
                }
            }
            
            return Ok(None); // Not enrolled and doesn't exist
        }
        
        // File exists, check its status
        let abs_path = file_path.canonicalize()?;
        
        // First check machine manifest
        let manifest = self.load_manifest()?;
        if let Some(entry) = manifest.is_enrolled(&abs_path) {
            let current_checksum = self.calculate_checksum(&abs_path)?;
            let status = if current_checksum == entry.checksum {
                FileStatus::Unchanged
            } else {
                FileStatus::Modified
            };
            return Ok(Some(status));
        }
        
        // If not in machine manifest, check all group manifests
        // Read machine's groups.conf to see which groups to check
        let groups_file = self.mfs_mount
            .join("machines")
            .join(&self.hostname)
            .join("etc")
            .join("laszoo")
            .join("groups.conf");
        
        if groups_file.exists() {
            let groups: Vec<String> = fs::read_to_string(&groups_file)?
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            
            for group in groups {
                if let Ok(group_manifest) = self.load_group_manifest(&group) {
                    if let Some(entry) = group_manifest.is_enrolled(&abs_path) {
                        let current_checksum = self.calculate_checksum(&abs_path)?;
                        let status = if current_checksum == entry.checksum {
                            FileStatus::Unchanged
                        } else {
                            FileStatus::Modified
                        };
                        return Ok(Some(status));
                    }
                }
            }
        }
        
        Ok(None)
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