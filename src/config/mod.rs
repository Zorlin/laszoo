use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use crate::error::{LaszooError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// MooseFS mount point
    #[serde(default = "default_mfs_mount")]
    pub mfs_mount: PathBuf,
    
    /// Laszoo data directory in MooseFS
    #[serde(default = "default_laszoo_dir")]
    pub laszoo_dir: String,
    
    /// Default sync strategy
    #[serde(default = "default_sync_strategy")]
    pub default_sync_strategy: String,
    
    /// Enable automatic Git commits
    #[serde(default = "default_auto_commit")]
    pub auto_commit: bool,
    
    /// Ollama API endpoint
    #[serde(default = "default_ollama_endpoint")]
    pub ollama_endpoint: String,
    
    /// Ollama model to use for commit messages
    #[serde(default = "default_ollama_model")]
    pub ollama_model: String,
    
    /// File monitoring settings
    #[serde(default)]
    pub monitoring: MonitoringConfig,
    
    /// Logging configuration
    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Enable file system monitoring
    #[serde(default = "default_true")]
    pub enabled: bool,
    
    /// Debounce delay in milliseconds
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
    
    /// Polling interval for remote changes in seconds
    #[serde(default = "default_poll_interval")]
    pub poll_interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub level: String,
    
    /// Log format (pretty, json, compact)
    #[serde(default = "default_log_format")]
    pub format: String,
    
    /// Log file path (if any)
    pub file: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mfs_mount: default_mfs_mount(),
            laszoo_dir: default_laszoo_dir(),
            default_sync_strategy: default_sync_strategy(),
            auto_commit: default_auto_commit(),
            ollama_endpoint: default_ollama_endpoint(),
            ollama_model: default_ollama_model(),
            monitoring: MonitoringConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            debounce_ms: default_debounce_ms(),
            poll_interval: default_poll_interval(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
            file: None,
        }
    }
}

impl Config {
    /// Load configuration from multiple sources with priority:
    /// 1. Command line arguments (highest)
    /// 2. Environment variables
    /// 3. Config file
    /// 4. Defaults (lowest)
    pub fn load(config_path: Option<&Path>) -> Result<Self> {
        let mut config = Self::default();
        
        // Try to load from config file
        if let Some(path) = config_path {
            config = Self::from_file(path)?;
        } else {
            // Try default config locations
            let locations = vec![
                PathBuf::from("/etc/laszoo/config.toml"),
                dirs::config_dir()
                    .map(|p| p.join("laszoo/config.toml"))
                    .unwrap_or_default(),
                dirs::home_dir()
                    .map(|p| p.join(".laszoo/config.toml"))
                    .unwrap_or_default(),
            ];
            
            for location in locations {
                if location.exists() {
                    config = Self::from_file(&location)?;
                    break;
                }
            }
        }
        
        // Apply environment variable overrides
        config.apply_env_overrides();
        
        Ok(config)
    }
    
    fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
    
    fn apply_env_overrides(&mut self) {
        if let Ok(mount) = std::env::var("LASZOO_MFS_MOUNT") {
            self.mfs_mount = PathBuf::from(mount);
        }
        
        if let Ok(dir) = std::env::var("LASZOO_DIR") {
            self.laszoo_dir = dir;
        }
        
        if let Ok(strategy) = std::env::var("LASZOO_SYNC_STRATEGY") {
            self.default_sync_strategy = strategy;
        }
        
        if let Ok(auto) = std::env::var("LASZOO_AUTO_COMMIT") {
            self.auto_commit = auto.parse().unwrap_or(self.auto_commit);
        }
        
        if let Ok(endpoint) = std::env::var("LASZOO_OLLAMA_ENDPOINT") {
            self.ollama_endpoint = endpoint;
        }
        
        if let Ok(model) = std::env::var("LASZOO_OLLAMA_MODEL") {
            self.ollama_model = model;
        }
        
        if let Ok(level) = std::env::var("LASZOO_LOG_LEVEL") {
            self.logging.level = level;
        }
    }
    
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| LaszooError::Other(format!("Failed to serialize config: {}", e)))?;
        
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        std::fs::write(path, content)?;
        Ok(())
    }
    
    /// Get the full path to the Laszoo directory in MooseFS
    pub fn laszoo_path(&self) -> PathBuf {
        self.mfs_mount.join(&self.laszoo_dir)
    }
    
    /// Get the path for a specific host's data
    pub fn host_path(&self, hostname: &str) -> PathBuf {
        self.laszoo_path().join(hostname)
    }
}

// Default value functions
fn default_mfs_mount() -> PathBuf {
    PathBuf::from("/mnt/mfs")
}

fn default_laszoo_dir() -> String {
    "laszoo".to_string()
}

fn default_sync_strategy() -> String {
    "auto".to_string()
}

fn default_auto_commit() -> bool {
    true
}

fn default_ollama_endpoint() -> String {
    "http://localhost:11434".to_string()
}

fn default_ollama_model() -> String {
    "llama3.2".to_string()
}

fn default_true() -> bool {
    true
}

fn default_debounce_ms() -> u64 {
    500
}

fn default_poll_interval() -> u64 {
    30
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "pretty".to_string()
}