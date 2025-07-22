use std::path::{Path, PathBuf};
use std::collections::HashMap;
use tracing::{info, warn, error, debug};
use crate::error::{LaszooError, Result};
use crate::enrollment::{EnrollmentManager, EnrollmentEntry};
use crate::template::TemplateEngine;
use crate::cli::SyncStrategy;

pub struct SyncEngine {
    mfs_mount: PathBuf,
    laszoo_dir: String,
    hostname: String,
    template_engine: TemplateEngine,
}

#[derive(Debug)]
pub struct SyncOperation {
    pub file_path: PathBuf,
    pub group: String,
    pub operation_type: SyncOperationType,
    pub source_hosts: Vec<String>,
    pub target_hosts: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum SyncOperationType {
    /// Copy unchanged file from majority to minority hosts
    Rollback { 
        majority_content: String,
        majority_hosts: Vec<String>,
    },
    /// Propagate local changes to all other hosts
    Forward {
        local_content: String,
    },
    /// Create merged template when hosts diverge
    CreateTemplate {
        template_content: String,
        divergent_sections: HashMap<String, Vec<String>>,
    },
}

impl SyncEngine {
    pub fn new(mfs_mount: PathBuf, laszoo_dir: String) -> Result<Self> {
        let hostname = gethostname::gethostname()
            .to_string_lossy()
            .to_string();
            
        let template_engine = TemplateEngine::new()?;
        
        Ok(Self {
            mfs_mount,
            laszoo_dir,
            hostname,
            template_engine,
        })
    }
    
    /// Analyze files in a group and determine sync operations needed
    pub async fn analyze_group(&self, group: &str) -> Result<Vec<SyncOperation>> {
        let mut operations = Vec::new();
        
        // Get all hosts in the distributed filesystem
        let all_hosts = self.discover_hosts()?;
        info!("Discovered {} hosts in cluster", all_hosts.len());
        
        // Get enrolled files for this group on current host
        let manager = EnrollmentManager::new(
            self.mfs_mount.clone(),
            self.laszoo_dir.clone()
        );
        let local_entries = manager.list_enrolled_files(Some(group))?;
        
        // For each enrolled file, check status across all hosts
        for entry in local_entries {
            let file_status = self.analyze_file_across_hosts(
                &entry,
                &all_hosts,
                group
            ).await?;
            
            if let Some(operation) = file_status {
                operations.push(operation);
            }
        }
        
        Ok(operations)
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
                    SyncOperationType::Rollback { majority_hosts, .. } => {
                        println!("  [ROLLBACK] {:?} - restore from majority ({} hosts)", 
                            op.file_path, majority_hosts.len());
                    }
                    SyncOperationType::Forward { .. } => {
                        println!("  [FORWARD] {:?} - propagate to {} hosts", 
                            op.file_path, op.target_hosts.len());
                    }
                    SyncOperationType::CreateTemplate { divergent_sections, .. } => {
                        println!("  [TEMPLATE] {:?} - create template with {} divergent sections", 
                            op.file_path, divergent_sections.len());
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
    
    /// Determine sync strategy automatically based on majority
    pub fn determine_sync_strategy(
        &self,
        strategy: &SyncStrategy,
        file_versions: &HashMap<String, Vec<String>>, // checksum -> hosts
    ) -> SyncOperationType {
        match strategy {
            SyncStrategy::Auto => {
                // Find majority version
                let (majority_checksum, majority_hosts) = file_versions.iter()
                    .max_by_key(|(_, hosts)| hosts.len())
                    .unwrap();
                    
                let total_hosts = file_versions.values()
                    .map(|h| h.len())
                    .sum::<usize>();
                    
                let majority_percentage = (majority_hosts.len() * 100) / total_hosts;
                
                if majority_percentage >= 60 {
                    // Clear majority, rollback minority
                    info!("Auto strategy: Rollback ({}% majority)", majority_percentage);
                    self.determine_sync_strategy(&SyncStrategy::Rollback, file_versions)
                } else {
                    // No clear majority, create template
                    info!("Auto strategy: Create template (no clear majority)");
                    self.create_template_operation(file_versions)
                }
            }
            SyncStrategy::Rollback => {
                // Find majority and rollback others
                let (majority_checksum, majority_hosts) = file_versions.iter()
                    .max_by_key(|(_, hosts)| hosts.len())
                    .unwrap();
                    
                // TODO: Read actual content from a majority host
                let majority_content = String::new(); // Placeholder
                
                SyncOperationType::Rollback {
                    majority_content,
                    majority_hosts: majority_hosts.clone(),
                }
            }
            SyncStrategy::Forward => {
                // Forward local changes to all other hosts
                let local_content = String::new(); // TODO: Read local file
                
                SyncOperationType::Forward {
                    local_content,
                }
            }
        }
    }
    
    /// Discover all hosts in the cluster
    fn discover_hosts(&self) -> Result<Vec<String>> {
        let laszoo_base = crate::fs::get_laszoo_base(&self.mfs_mount, &self.laszoo_dir);
        
        let mut hosts = Vec::new();
        if laszoo_base.exists() {
            for entry in std::fs::read_dir(laszoo_base)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    if let Some(hostname) = entry.file_name().to_str() {
                        hosts.push(hostname.to_string());
                    }
                }
            }
        }
        
        Ok(hosts)
    }
    
    /// Analyze a file across all hosts
    async fn analyze_file_across_hosts(
        &self,
        entry: &EnrollmentEntry,
        all_hosts: &[String],
        group: &str,
    ) -> Result<Option<SyncOperation>> {
        let mut file_versions: HashMap<String, Vec<String>> = HashMap::new();
        let file_path = &entry.original_path;
        
        // Check file on each host
        for host in all_hosts {
            let host_manifest_path = self.mfs_mount
                .join(&self.laszoo_dir)
                .join(host)
                .join("manifest.json");
                
            if host_manifest_path.exists() {
                // Load host's manifest
                let manifest_content = std::fs::read_to_string(&host_manifest_path)?;
                let manifest: crate::enrollment::EnrollmentManifest = 
                    serde_json::from_str(&manifest_content)?;
                
                // Check if this host has the file enrolled
                if let Some(host_entry) = manifest.is_enrolled(file_path) {
                    if host_entry.group == group {
                        file_versions.entry(host_entry.checksum.clone())
                            .or_insert_with(Vec::new)
                            .push(host.clone());
                    }
                }
            }
        }
        
        // If file exists on multiple hosts with different versions, sync is needed
        if file_versions.len() > 1 {
            let operation_type = self.determine_sync_strategy(
                &SyncStrategy::Auto,
                &file_versions
            );
            
            let all_involved_hosts: Vec<String> = file_versions.values()
                .flatten()
                .cloned()
                .collect();
            
            Ok(Some(SyncOperation {
                file_path: file_path.clone(),
                group: group.to_string(),
                operation_type,
                source_hosts: vec![self.hostname.clone()],
                target_hosts: all_involved_hosts,
            }))
        } else {
            // No sync needed
            Ok(None)
        }
    }
    
    /// Execute a single sync operation
    async fn execute_operation(&self, operation: SyncOperation) -> Result<()> {
        match operation.operation_type {
            SyncOperationType::Rollback { majority_content, majority_hosts } => {
                info!("Rolling back {:?} to majority version from {:?}", 
                    operation.file_path, majority_hosts);
                
                // Write majority content to local file
                std::fs::write(&operation.file_path, majority_content)?;
                
                // Update local manifest with new checksum
                let manager = EnrollmentManager::new(
                    self.mfs_mount.clone(),
                    self.laszoo_dir.clone()
                );
                
                // Re-enroll to update checksum
                manager.enroll_file(&operation.file_path, &operation.group, true)?;
            }
            SyncOperationType::Forward { local_content } => {
                info!("Forwarding {:?} to all hosts", operation.file_path);
                
                // Copy local file to all other hosts' template paths
                for host in &operation.target_hosts {
                    if host != &self.hostname {
                        let target_template = crate::fs::get_template_path(
                            &self.mfs_mount,
                            &self.laszoo_dir,
                            host,
                            &operation.file_path
                        )?;
                        
                        if let Some(parent) = target_template.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        
                        std::fs::write(&target_template, &local_content)?;
                        debug!("Forwarded to {}: {:?}", host, target_template);
                    }
                }
            }
            SyncOperationType::CreateTemplate { template_content, divergent_sections } => {
                info!("Creating template for {:?} with {} divergent sections", 
                    operation.file_path, divergent_sections.len());
                
                // Save template to local template path
                let template_path = crate::fs::get_template_path(
                    &self.mfs_mount,
                    &self.laszoo_dir,
                    &self.hostname,
                    &operation.file_path
                )?;
                
                std::fs::write(&template_path, template_content)?;
                
                // Log divergent sections
                for (content, hosts) in divergent_sections {
                    warn!("Divergent section on hosts {:?}: {}", hosts, content);
                }
            }
        }
        
        Ok(())
    }
    
    /// Create a template operation when hosts diverge
    fn create_template_operation(
        &self,
        file_versions: &HashMap<String, Vec<String>>,
    ) -> SyncOperationType {
        // TODO: Actually read file contents from each host and merge
        let template_content = String::new(); // Placeholder
        let divergent_sections = HashMap::new(); // Placeholder
        
        SyncOperationType::CreateTemplate {
            template_content,
            divergent_sections,
        }
    }
}