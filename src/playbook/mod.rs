use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json;
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
    pub main_file: String,  // The main YAML file (e.g., "site.yml", "playbook.yml")
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

    /// Detect the main playbook file in a directory
    fn detect_main_playbook_file(dir_path: &Path) -> Result<String> {
        // Priority order for common playbook file names
        let candidates = ["site.yml", "site.yaml", "playbook.yml", "playbook.yaml", "main.yml", "main.yaml"];
        
        for candidate in &candidates {
            if dir_path.join(candidate).exists() {
                return Ok(candidate.to_string());
            }
        }
        
        // If no common names found, look for any .yml/.yaml file
        for entry in fs::read_dir(dir_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "yml" || ext == "yaml" {
                        if let Some(name) = path.file_name() {
                            return Ok(name.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
        
        Err(anyhow!("No YAML playbook files found in directory"))
    }

    pub fn add_playbook(&self, source_path: &Path, name: Option<String>, custom_path: Option<PathBuf>) -> Result<String> {
        if !source_path.exists() {
            return Err(anyhow!("Source path does not exist: {:?}", source_path));
        }

        // Handle case where user specifies a YAML file directly
        let (actual_source_path, main_file_name) = if source_path.is_file() {
            // Check if it's a YAML file
            if let Some(ext) = source_path.extension() {
                if ext == "yml" || ext == "yaml" {
                    // Use the parent directory as source and record the file name
                    let parent = source_path.parent()
                        .ok_or_else(|| anyhow!("YAML file has no parent directory"))?;
                    let file_name = source_path.file_name()
                        .ok_or_else(|| anyhow!("Could not get file name"))?
                        .to_string_lossy()
                        .to_string();
                    (parent, Some(file_name))
                } else {
                    return Err(anyhow!("File must be a .yml or .yaml file"));
                }
            } else {
                return Err(anyhow!("File must have .yml or .yaml extension"));
            }
        } else {
            // Directory - try to auto-detect main file
            let main_file = Self::detect_main_playbook_file(source_path)?;
            (source_path, Some(main_file))
        };

        // Determine the playbook name
        let playbook_name = if let Some(name) = name {
            name
        } else {
            actual_source_path
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
        info!("Copying playbook from {:?} to {:?}", actual_source_path, dest_path);
        
        match copy_dir_all(actual_source_path, &dest_path) {
            Ok(_) => info!("Successfully copied playbook directory"),
            Err(e) => {
                error!("Failed to copy playbook directory: {}", e);
                return Err(e);
            }
        }

        // Create metadata file in the playbook directory
        let metadata = PlaybookMetadata {
            name: playbook_name.clone(),
            description: None,
            main_file: main_file_name.unwrap_or_else(|| "playbook.yml".to_string()),
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
        
        // First check exact match and use metadata to get main file
        if let Some((_, path)) = playbooks.iter().find(|(n, _)| n == name) {
            let metadata_path = path.join(".laszoo-metadata.json");
            if metadata_path.exists() {
                if let Ok(content) = fs::read_to_string(&metadata_path) {
                    if let Ok(metadata) = serde_json::from_str::<PlaybookMetadata>(&content) {
                        let main_file_path = path.join(&metadata.main_file);
                        if main_file_path.exists() {
                            return Ok(main_file_path);
                        }
                    }
                }
            }
            
            // Fallback to old behavior if metadata is missing
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

    pub fn update_last_run(&self, name: &str, success: bool, output: Option<String>) -> Result<()> {
        // Get current hostname
        let hostname = gethostname::gethostname()
            .to_string_lossy()
            .to_string();
        
        // Create the run log directory structure
        let run_log_dir = self.mount_point
            .join("machines")
            .join(&hostname)
            .join("etc/laszoo/playbooks")
            .join(name);
        
        // Ensure directory exists
        Self::ensure_directory_exists(&run_log_dir)?;
        
        // Create timestamp for log file
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S_%f").to_string();
        let log_file = run_log_dir.join(format!("{}.log", timestamp));
        
        // Create log entry
        let mut log_entry = serde_json::json!({
            "hostname": hostname,
            "playbook": name,
            "timestamp": Utc::now().to_rfc3339(),
            "timestamp_filename": timestamp,
            "success": success,
            "user": std::env::var("USER").unwrap_or_else(|_| "unknown".to_string()),
        });
        
        // Add output if provided
        if let Some(output_text) = output {
            log_entry["output"] = serde_json::Value::String(output_text);
        }
        
        // Write log file
        fs::write(&log_file, serde_json::to_string_pretty(&log_entry)?)?;
        info!("Recorded playbook run at {:?}", log_file);
        
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
        
        // Run the playbook using Jetpack
        let result = self.execute_jetpack_playbook(&playbook_path, &inventory, extra_args);
        
        // Update last run time with success status and output
        let (success, output) = match &result {
            Ok(output) => (true, Some(output.clone())),
            Err(e) => (false, Some(e.to_string())),
        };
        
        if let Err(e) = self.update_last_run(name, success, output) {
            debug!("Failed to update last run time: {}", e);
        }
        
        result.map(|_| ())
    }
    
    fn execute_jetpack_playbook(&self, playbook_path: &Path, inventory_path: &Path, _extra_args: Vec<String>) -> Result<String> {
        use jetpack::run_playbook;
        use gag::BufferRedirect;
        use std::io::Read;
        
        // Convert path to string
        let playbook_str = playbook_path.to_string_lossy();
        
        // Get current hostname
        let hostname = gethostname::gethostname()
            .to_string_lossy()
            .to_string();
        
        info!("Running playbook in local mode on host: {}", hostname);
        
        // Build the runner - use local mode but with proper inventory
        let mut builder = run_playbook(&playbook_str).local();
        
        // Always use inventory for proper group/host variable resolution
        // Jetpack will now automatically merge all group_vars into host_vars
        if inventory_path.exists() && inventory_path.read_dir()?.next().is_some() {
            info!("Using inventory data from {:?} for variable resolution", inventory_path);
            let inventory_str = inventory_path.to_string_lossy();
            builder = builder.inventory(&inventory_str);
            
            // Limit to current host instead of localhost
            builder = builder.limit_hosts(vec![hostname.clone()]);
        } else {
            // If no inventory, still limit to current hostname
            builder = builder.limit_hosts(vec![hostname.clone()]);
        }
        
        // Check if verbose mode is requested
        let verbose = std::env::var("LASZOO_VERBOSE").is_ok() || _extra_args.contains(&"--verbose".to_string()) || _extra_args.contains(&"-v".to_string());
        
        if verbose {
            // In verbose mode, don't capture - let output flow through
            builder = builder.verbose();
            let result = builder.run();
            
            // Return empty string for output in verbose mode since it's already printed
            match result {
                Ok(result) => {
                    if result.success {
                        info!("Playbook completed successfully on {} hosts", result.hosts_processed);
                        Ok(String::new())
                    } else {
                        Err(anyhow!("Playbook failed on one or more hosts"))
                    }
                }
                Err(e) => Err(anyhow!("Jetpack execution failed: {}", e))
            }
        } else {
            // Capture stdout while running the playbook
            let mut buffer = BufferRedirect::stdout()?;
            let result = builder.run();
            
            // Get the captured output
            let mut output = String::new();
            buffer.read_to_string(&mut output)?;
            drop(buffer); // This restores stdout
            
            // Print the output to console as well
            print!("{}", output);
            
            // Check result
            match result {
                Ok(result) => {
                    if result.success {
                        info!("Playbook completed successfully on {} hosts", result.hosts_processed);
                        Ok(output)
                    } else {
                        Err(anyhow!("Playbook failed on one or more hosts"))
                    }
                }
                Err(e) => Err(anyhow!("Jetpack execution failed: {}", e))
            }
        }
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