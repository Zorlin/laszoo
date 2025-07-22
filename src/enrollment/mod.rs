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
    laszoo_dir: String,
    hostname: String,
}

impl EnrollmentManager {
    pub fn new(mfs_mount: PathBuf, laszoo_dir: String) -> Self {
        let hostname = gethostname::gethostname()
            .to_string_lossy()
            .to_string();
            
        Self {
            mfs_mount,
            laszoo_dir,
            hostname,
        }
    }

    pub fn manifest_path(&self) -> PathBuf {
        crate::fs::get_host_dir(&self.mfs_mount, &self.laszoo_dir, &self.hostname)
            .join("manifest.json")
    }

    pub fn load_manifest(&self) -> Result<EnrollmentManifest> {
        EnrollmentManifest::load(&self.manifest_path())
    }

    pub fn save_manifest(&self, manifest: &EnrollmentManifest) -> Result<()> {
        manifest.save(&self.manifest_path())
    }

    pub fn enroll_file(&self, file_path: &Path, group: &str, force: bool) -> Result<()> {
        // Ensure file exists
        if !file_path.exists() {
            return Err(LaszooError::FileNotFound { 
                path: file_path.to_path_buf() 
            });
        }

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
        
        // Create symlink in MooseFS
        let template_path = crate::fs::get_template_path(
            &self.mfs_mount, 
            &self.laszoo_dir, 
            &self.hostname,
            &abs_path
        )?;
        
        // Ensure parent directory exists
        if let Some(parent) = template_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // Copy file to MooseFS
        fs::copy(&abs_path, &template_path)?;
        info!("Copied file to {:?}", template_path);
        
        // Create enrollment entry
        let entry = EnrollmentEntry {
            original_path: abs_path.clone(),
            checksum,
            group: group.to_string(),
            enrolled_at: chrono::Utc::now(),
            last_synced: None,
            template_path: Some(template_path),
        };
        
        // Add to manifest
        manifest.add_entry(entry);
        self.save_manifest(&manifest)?;
        
        info!("Successfully enrolled {:?} into group '{}'", abs_path, group);
        Ok(())
    }

    pub fn unenroll_file(&self, file_path: &Path) -> Result<()> {
        let abs_path = file_path.canonicalize()?;
        let mut manifest = self.load_manifest()?;
        
        if let Some(entry) = manifest.remove_entry(&abs_path) {
            // Remove template file if it exists
            if let Some(template_path) = &entry.template_path {
                if template_path.exists() {
                    fs::remove_file(template_path)?;
                    debug!("Removed template file: {:?}", template_path);
                }
            }
            
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
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileStatus {
    Unchanged,
    Modified,
}