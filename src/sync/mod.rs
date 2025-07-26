use std::path::{Path, PathBuf};
use std::collections::HashMap;
use tracing::{info, warn, error, debug};
use crate::error::{LaszooError, Result};
use crate::enrollment::{EnrollmentManager, EnrollmentEntry};
use crate::template::TemplateEngine;
use crate::cli::SyncStrategy;
use sha2::{Sha256, Digest};

pub struct SyncEngine {
    mfs_mount: PathBuf,
    hostname: String,
    template_engine: TemplateEngine,
}

#[derive(Debug)]
pub struct SyncOperation {
    pub file_path: PathBuf,
    pub group: String,
    pub template_path: PathBuf,
    pub operation_type: SyncOperationType,
}

#[derive(Debug, Clone)]
pub enum SyncOperationType {
    /// Restore local file from template (template wins)
    Rollback { 
        template_content: String,
    },
    /// Update template with local changes (local wins)
    Forward {
        local_content: String,
    },
    /// Merge local changes into template preserving variables
    Converge {
        local_content: String,
        template_content: String,
    },
    /// Local changes detected but strategy is freeze (no action)
    Freeze,
    /// Local changes detected but strategy is drift (report only)
    Drift,
}

impl SyncEngine {
    pub fn new(mfs_mount: PathBuf, _laszoo_dir: String) -> Result<Self> {
        let hostname = gethostname::gethostname()
            .to_string_lossy()
            .to_string();
            
        let template_engine = TemplateEngine::new()?;
        
        Ok(Self {
            mfs_mount,
            hostname,
            template_engine,
        })
    }
    
    /// Analyze files in a group and determine sync operations needed
    pub async fn analyze_group(&self, group: &str, strategy: &SyncStrategy) -> Result<Vec<SyncOperation>> {
        let mut operations = Vec::new();
        
        // Get enrolled files for this group on current host
        let manager = EnrollmentManager::new(
            self.mfs_mount.clone(),
            "".to_string()
        );
        
        // First check group manifest
        let group_manifest = manager.load_group_manifest(group)?;
        
        // Then check machine manifest
        let machine_manifest = manager.load_manifest()?;
        
        // Combine entries from both manifests
        let mut all_entries = Vec::new();
        
        // Add group entries
        for entry in group_manifest.entries.values() {
            if entry.group == group {
                all_entries.push(entry.clone());
            }
        }
        
        // Add machine-specific entries
        for entry in machine_manifest.entries.values() {
            if entry.group == group {
                all_entries.push(entry.clone());
            }
        }
        
        // For each enrolled file, check if it differs from template
        for entry in all_entries {
            if let Some(operation) = self.analyze_file(&entry, group, strategy).await? {
                operations.push(operation);
            }
        }
        
        Ok(operations)
    }
    
    /// Analyze a single file and determine if sync is needed
    async fn analyze_file(&self, entry: &EnrollmentEntry, group: &str, strategy: &SyncStrategy) -> Result<Option<SyncOperation>> {
        let file_path = &entry.original_path;
        
        // Skip directories
        if entry.checksum == "directory" {
            return Ok(None);
        }
        
        // Get template path
        let template_path = if let Some(path) = &entry.template_path {
            path.clone()
        } else {
            // Construct template path
            let manager = EnrollmentManager::new(self.mfs_mount.clone(), "".to_string());
            manager.get_group_template_path(group, file_path)?
        };
        
        // Check if template exists
        if !template_path.exists() {
            warn!("Template missing for enrolled file: {:?}", file_path);
            return Ok(None);
        }
        
        // Check if local file exists
        if !file_path.exists() {
            // File is missing locally but has a template - needs rollback
            let template_content = std::fs::read_to_string(&template_path)?;
            return Ok(Some(SyncOperation {
                file_path: file_path.clone(),
                group: group.to_string(),
                template_path: template_path.clone(),
                operation_type: SyncOperationType::Rollback { template_content },
            }));
        }
        
        // Calculate current file checksum
        let current_checksum = self.calculate_checksum(file_path)?;
        
        // Check if file has changed from enrolled checksum
        if current_checksum == entry.checksum {
            // File hasn't changed
            return Ok(None);
        }
        
        // File has changed - determine operation based on strategy
        let local_content = std::fs::read_to_string(file_path)?;
        let template_content = std::fs::read_to_string(&template_path)?;
        
        let operation_type = match strategy {
            SyncStrategy::Converge => {
                SyncOperationType::Converge {
                    local_content,
                    template_content,
                }
            }
            SyncStrategy::Rollback => {
                SyncOperationType::Rollback { 
                    template_content,
                }
            }
            SyncStrategy::Forward => {
                SyncOperationType::Forward {
                    local_content,
                }
            }
            SyncStrategy::Freeze => {
                SyncOperationType::Freeze
            }
            SyncStrategy::Drift => {
                SyncOperationType::Drift
            }
            SyncStrategy::Auto => {
                // Default to converge for auto
                SyncOperationType::Converge {
                    local_content,
                    template_content,
                }
            }
        };
        
        Ok(Some(SyncOperation {
            file_path: file_path.clone(),
            group: group.to_string(),
            template_path,
            operation_type,
        }))
    }
    
    /// Execute sync operations
    pub async fn execute_sync(
        &self,
        operations: Vec<SyncOperation>,
        dry_run: bool,
    ) -> Result<()> {
        if dry_run {
            info!("DRY RUN: Would perform {} sync operations", operations.len());
            for op in &operations {
                match &op.operation_type {
                    SyncOperationType::Rollback { .. } => {
                        println!("  [ROLLBACK] {:?} - restore from template", op.file_path);
                    }
                    SyncOperationType::Forward { .. } => {
                        println!("  [FORWARD] {:?} - update template with local changes", op.file_path);
                    }
                    SyncOperationType::Converge { .. } => {
                        println!("  [CONVERGE] {:?} - merge local changes into template", op.file_path);
                    }
                    SyncOperationType::Freeze => {
                        println!("  [FREEZE] {:?} - no action (frozen)", op.file_path);
                    }
                    SyncOperationType::Drift => {
                        println!("  [DRIFT] {:?} - detected drift (no action)", op.file_path);
                    }
                }
            }
            return Ok(());
        }
        
        // Execute operations
        for operation in operations {
            match self.execute_operation(operation).await {
                Ok(_) => {}
                Err(e) => {
                    error!("Failed to execute sync operation: {}", e);
                    return Err(e);
                }
            }
        }
        
        Ok(())
    }
    
    
    /// Execute a single sync operation
    async fn execute_operation(&self, operation: SyncOperation) -> Result<()> {
        match operation.operation_type {
            SyncOperationType::Rollback { template_content } => {
                info!("Rolling back {:?} to template version", operation.file_path);
                
                // Process template to handle variables
                let processed_content = crate::template::process_handlebars(&template_content, &self.hostname)?;
                
                // Write processed content to local file
                std::fs::write(&operation.file_path, &processed_content)?;
                
                // Update local manifest with new checksum
                let manager = EnrollmentManager::new(
                    self.mfs_mount.clone(),
                    "".to_string()
                );
                
                // Update checksum in manifest
                let mut manifest = manager.load_manifest()?;
                if let Some(entry) = manifest.entries.get_mut(&operation.file_path) {
                    entry.checksum = self.calculate_checksum(&operation.file_path)?;
                    entry.last_synced = Some(chrono::Utc::now());
                    manager.save_manifest(&manifest)?;
                }
                
                info!("Successfully rolled back {:?}", operation.file_path);
            }
            SyncOperationType::Forward { local_content } => {
                info!("Forwarding {:?} changes to template", operation.file_path);
                
                // Write local content to template
                std::fs::write(&operation.template_path, &local_content)?;
                
                // Update checksum in manifest
                let manager = EnrollmentManager::new(
                    self.mfs_mount.clone(),
                    "".to_string()
                );
                
                let mut manifest = manager.load_manifest()?;
                if let Some(entry) = manifest.entries.get_mut(&operation.file_path) {
                    entry.checksum = self.calculate_checksum(&operation.file_path)?;
                    entry.last_synced = Some(chrono::Utc::now());
                    manager.save_manifest(&manifest)?;
                }
                
                info!("Successfully updated template for {:?}", operation.file_path);
            }
            SyncOperationType::Converge { local_content, template_content } => {
                info!("Converging {:?} - merging local changes into template", operation.file_path);
                
                // Use template engine to merge changes
                let merged_content = self.template_engine.merge_file_changes_to_template(
                    &template_content,
                    &local_content
                )?;
                
                // Write merged content to template
                std::fs::write(&operation.template_path, &merged_content)?;
                
                // Update checksum in manifest
                let manager = EnrollmentManager::new(
                    self.mfs_mount.clone(),
                    "".to_string()
                );
                
                let mut manifest = manager.load_manifest()?;
                if let Some(entry) = manifest.entries.get_mut(&operation.file_path) {
                    entry.checksum = self.calculate_checksum(&operation.file_path)?;
                    entry.last_synced = Some(chrono::Utc::now());
                    manager.save_manifest(&manifest)?;
                }
                
                info!("Successfully converged {:?}", operation.file_path);
            }
            SyncOperationType::Freeze => {
                info!("File {:?} is frozen - no action taken", operation.file_path);
            }
            SyncOperationType::Drift => {
                warn!("Drift detected in {:?} - no action taken", operation.file_path);
            }
        }
        
        Ok(())
    }
    
    fn calculate_checksum(&self, path: &Path) -> Result<String> {
        let mut file = std::fs::File::open(path)?;
        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher)?;
        Ok(format!("{:x}", hasher.finalize()))
    }
}