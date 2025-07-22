use thiserror::Error;
use std::path::PathBuf;

#[derive(Error, Debug)]
pub enum LaszooError {
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("File not found: {path}")]
    FileNotFound { path: PathBuf },
    
    #[error("Permission denied: {path}")]
    PermissionDenied { path: PathBuf },
    
    #[error("Distributed filesystem not available at {path}")]
    DistributedFSNotAvailable { path: PathBuf },
    
    #[error("File already enrolled in group {group}: {path}")]
    AlreadyEnrolled { path: PathBuf, group: String },
    
    #[error("Group not found: {name}")]
    GroupNotFound { name: String },
    
    #[error("Template error: {0}")]
    Template(String),
    
    #[error("Synchronization conflict: {0}")]
    SyncConflict(String),
    
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),
    
    #[error("Notify error: {0}")]
    Notify(#[from] notify::Error),
    
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    
    #[error("Other error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, LaszooError>;