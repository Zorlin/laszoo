use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time;
use notify::{Watcher, RecursiveMode, Event, EventKind, Config as NotifyConfig};
use tracing::{info, debug, warn, error};
use crate::error::{LaszooError, Result};
use crate::enrollment::{EnrollmentManager, FileStatus};
use crate::template::TemplateEngine;
use sha2::{Sha256, Digest};

pub struct FileMonitor {
    enrollment_manager: Arc<EnrollmentManager>,
    template_engine: Arc<TemplateEngine>,
    changes: Arc<Mutex<Vec<FileChange>>>,
}

#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: PathBuf,
    pub change_type: ChangeType,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub old_checksum: Option<String>,
    pub new_checksum: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChangeType {
    Modified,
    Created,
    Deleted,
    Renamed { from: PathBuf, to: PathBuf },
}

impl FileMonitor {
    pub fn new(
        enrollment_manager: Arc<EnrollmentManager>,
        template_engine: Arc<TemplateEngine>,
    ) -> Self {
        Self {
            enrollment_manager,
            template_engine,
            changes: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    /// Start monitoring enrolled files for changes
    pub async fn start_monitoring(&self, poll_interval: u64) -> Result<()> {
        info!("Starting file monitoring with {}s poll interval", poll_interval);
        
        let manager = Arc::clone(&self.enrollment_manager);
        let changes = Arc::clone(&self.changes);
        
        // Spawn monitoring task
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(poll_interval));
            
            loop {
                interval.tick().await;
                
                // Check all enrolled files
                match manager.list_enrolled_files(None) {
                    Ok(entries) => {
                        for entry in entries {
                            if let Err(e) = check_file_changes(
                                &manager,
                                &entry.original_path,
                                &entry.checksum,
                                &changes
                            ).await {
                                error!("Error checking file {:?}: {}", entry.original_path, e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error listing enrolled files: {}", e);
                    }
                }
            }
        });
        
        Ok(())
    }
    
    /// Start watching specific paths using inotify/FSEvents
    pub async fn watch_paths(&self, paths: Vec<PathBuf>, debounce_ms: u64) -> Result<()> {
        use notify::event::{ModifyKind, CreateKind, RemoveKind, RenameMode};
        
        let (tx, rx) = std::sync::mpsc::channel();
        let changes = Arc::clone(&self.changes);
        let manager = Arc::clone(&self.enrollment_manager);
        
        // Create watcher with debouncing
        let config = NotifyConfig::default()
            .with_poll_interval(Duration::from_millis(debounce_ms));
            
        let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            if let Ok(event) = result {
                let _ = tx.send(event);
            }
        })?;
        
        // Watch all paths
        for path in paths {
            watcher.watch(&path, RecursiveMode::Recursive)?;
            info!("Watching path: {:?}", path);
        }
        
        // Spawn event processing task
        tokio::spawn(async move {
            // Keep watcher alive
            let _watcher = watcher;
            
            while let Ok(event) = rx.recv() {
                debug!("File event: {:?}", event);
                
                let change_type = match event.kind {
                    EventKind::Modify(ModifyKind::Data(_)) |
                    EventKind::Modify(ModifyKind::Any) => {
                        Some(ChangeType::Modified)
                    }
                    EventKind::Create(CreateKind::File) => {
                        Some(ChangeType::Created)
                    }
                    EventKind::Remove(RemoveKind::File) => {
                        Some(ChangeType::Deleted)
                    }
                    EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                        if event.paths.len() == 2 {
                            Some(ChangeType::Renamed {
                                from: event.paths[0].clone(),
                                to: event.paths[1].clone(),
                            })
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                
                if let Some(change_type) = change_type {
                    for path in event.paths {
                        // Check if this file is enrolled
                        let manifest = match manager.load_manifest() {
                            Ok(m) => m,
                            Err(e) => {
                                error!("Failed to load manifest: {}", e);
                                continue;
                            }
                        };
                        
                        if let Some(entry) = manifest.is_enrolled(&path) {
                            let new_checksum = if path.exists() {
                                calculate_checksum(&path).ok()
                            } else {
                                None
                            };
                            
                            let change = FileChange {
                                path: path.clone(),
                                change_type: change_type.clone(),
                                timestamp: chrono::Utc::now(),
                                old_checksum: Some(entry.checksum.clone()),
                                new_checksum,
                            };
                            
                            let mut changes_lock = changes.lock().await;
                            changes_lock.push(change.clone());
                            
                            info!("Detected change: {:?}", change);
                        }
                    }
                }
            }
        });
        
        Ok(())
    }
    
    /// Get pending changes
    pub async fn get_changes(&self) -> Vec<FileChange> {
        let changes = self.changes.lock().await;
        changes.clone()
    }
    
    /// Clear tracked changes
    pub async fn clear_changes(&self) {
        let mut changes = self.changes.lock().await;
        changes.clear();
    }
}

async fn check_file_changes(
    manager: &EnrollmentManager,
    path: &Path,
    old_checksum: &str,
    changes: &Arc<Mutex<Vec<FileChange>>>,
) -> Result<()> {
    match manager.check_file_status(path)? {
        Some(FileStatus::Modified) => {
            let new_checksum = calculate_checksum(path)?;
            
            let change = FileChange {
                path: path.to_path_buf(),
                change_type: ChangeType::Modified,
                timestamp: chrono::Utc::now(),
                old_checksum: Some(old_checksum.to_string()),
                new_checksum: Some(new_checksum),
            };
            
            let mut changes_lock = changes.lock().await;
            changes_lock.push(change.clone());
            
            info!("File modified: {:?}", path);
        }
        Some(FileStatus::Unchanged) => {
            // No change
        }
        None => {
            // File no longer exists
            let change = FileChange {
                path: path.to_path_buf(),
                change_type: ChangeType::Deleted,
                timestamp: chrono::Utc::now(),
                old_checksum: Some(old_checksum.to_string()),
                new_checksum: None,
            };
            
            let mut changes_lock = changes.lock().await;
            changes_lock.push(change);
            
            warn!("Enrolled file deleted: {:?}", path);
        }
    }
    
    Ok(())
}

fn calculate_checksum(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}