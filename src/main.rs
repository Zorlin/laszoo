mod cli;
mod config;
mod error;
mod fs;
mod logging;
mod enrollment;
mod template;
mod monitor;

use clap::Parser;
use tracing::{info, error, debug};
use std::path::{Path, PathBuf};
use crate::cli::{Cli, Commands, GroupCommands};
use crate::config::Config;
use crate::error::{Result, LaszooError};

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        error!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    // Parse command line arguments
    let cli = Cli::parse();
    
    // Load configuration
    let config = Config::load(cli.config.as_deref())?;
    
    // Initialize logging
    logging::init_logging(&config.logging, cli.verbose)?;
    
    debug!("Loaded configuration: {:?}", config);
    info!("Starting Laszoo v{}", env!("CARGO_PKG_VERSION"));
    
    // Handle commands
    match cli.command {
        Commands::Init { mfs_mount } => {
            init_laszoo(&config, &mfs_mount).await?;
        }
        Commands::Enroll { group, paths, force, include_hidden } => {
            enroll_files(&config, &group, paths, force, include_hidden).await?;
        }
        Commands::Sync { group, strategy } => {
            info!("Synchronizing with strategy: {:?}", strategy);
            // TODO: Implement sync
            println!("Sync not yet implemented");
        }
        Commands::Status { group: _, detailed: _ } => {
            show_status(&config).await?;
        }
        Commands::Rollback { target, commits } => {
            info!("Rolling back {} commits for {}", commits, target);
            // TODO: Implement rollback
            println!("Rollback not yet implemented");
        }
        Commands::Apply { template, output: _ } => {
            apply_template(&config, &template).await?;
        }
        Commands::Group { command } => {
            handle_group_command(command).await?;
        }
    }
    
    Ok(())
}

async fn init_laszoo(config: &Config, mfs_mount: &std::path::Path) -> Result<()> {
    info!("Initializing Laszoo with distributed filesystem at {:?}", mfs_mount);
    
    // Check if distributed filesystem is available
    crate::fs::ensure_distributed_fs_available(mfs_mount)?;
    
    // Create Laszoo directory structure
    let laszoo_path = mfs_mount.join(&config.laszoo_dir);
    if !laszoo_path.exists() {
        std::fs::create_dir_all(&laszoo_path)?;
        info!("Created Laszoo directory at {:?}", laszoo_path);
    }
    
    // Get hostname
    let hostname = gethostname::gethostname()
        .to_string_lossy()
        .to_string();
    
    // Create host-specific directory
    let host_path = laszoo_path.join(&hostname);
    if !host_path.exists() {
        std::fs::create_dir_all(&host_path)?;
        info!("Created host directory at {:?}", host_path);
    }
    
    // Save default configuration
    let config_dir = dirs::config_dir()
        .map(|p| p.join("laszoo"))
        .unwrap_or_else(|| std::path::PathBuf::from(".laszoo"));
    
    std::fs::create_dir_all(&config_dir)?;
    let config_file = config_dir.join("config.toml");
    
    let mut init_config = config.clone();
    init_config.mfs_mount = mfs_mount.to_path_buf();
    init_config.save(&config_file)?;
    
    info!("Saved configuration to {:?}", config_file);
    info!("Laszoo initialized successfully!");
    
    Ok(())
}

async fn enroll_files(
    config: &Config, 
    group: &str, 
    paths: Vec<PathBuf>, 
    force: bool,
    include_hidden: bool
) -> Result<()> {
    use crate::enrollment::EnrollmentManager;
    
    // Ensure distributed filesystem is available
    crate::fs::ensure_distributed_fs_available(&config.mfs_mount)?;
    
    // Create enrollment manager
    let manager = EnrollmentManager::new(
        config.mfs_mount.clone(),
        config.laszoo_dir.clone()
    );
    
    let mut enrolled_count = 0;
    let mut error_count = 0;
    
    for path in paths {
        // Expand path if it's a directory
        let files = if path.is_dir() {
            let mut files = Vec::new();
            for entry in walkdir::WalkDir::new(&path)
                .follow_links(false)
                .into_iter()
                .filter_entry(|e| {
                    // Skip hidden files unless requested
                    if !include_hidden && e.file_name()
                        .to_str()
                        .map_or(false, |s| s.starts_with('.')) {
                        return false;
                    }
                    true
                })
            {
                match entry {
                    Ok(e) if e.file_type().is_file() => {
                        files.push(e.path().to_path_buf());
                    }
                    Err(e) => {
                        error!("Error walking directory: {}", e);
                        error_count += 1;
                    }
                    _ => {}
                }
            }
            files
        } else {
            vec![path]
        };
        
        // Enroll each file
        for file_path in files {
            match manager.enroll_file(&file_path, group, force) {
                Ok(_) => {
                    info!("Enrolled: {:?}", file_path);
                    enrolled_count += 1;
                }
                Err(e) => {
                    error!("Failed to enroll {:?}: {}", file_path, e);
                    error_count += 1;
                }
            }
        }
    }
    
    info!("Enrollment complete: {} files enrolled, {} errors", 
          enrolled_count, error_count);
    
    if error_count > 0 {
        Err(LaszooError::Other(
            format!("Enrollment completed with {} errors", error_count)
        ))
    } else {
        Ok(())
    }
}

async fn apply_template(config: &Config, template_path: &Path) -> Result<()> {
    use crate::template::TemplateEngine;
    use std::collections::HashMap;
    
    // Read template file
    let template_content = std::fs::read_to_string(template_path)
        .map_err(|_| LaszooError::FileNotFound { 
            path: template_path.to_path_buf() 
        })?;
    
    // Create template engine
    let engine = TemplateEngine::new()?;
    
    // Get hostname for variables
    let hostname = gethostname::gethostname()
        .to_string_lossy()
        .to_string();
    
    // Build default variables
    let mut variables = HashMap::new();
    variables.insert("hostname".to_string(), serde_json::json!(hostname));
    variables.insert("laszoo_dir".to_string(), serde_json::json!(config.laszoo_dir));
    
    // Add environment variables
    for (key, value) in std::env::vars() {
        if key.starts_with("LASZOO_") {
            let var_name = key.strip_prefix("LASZOO_").unwrap().to_lowercase();
            variables.insert(var_name, serde_json::json!(value));
        }
    }
    
    // Process template (preserve quack tags by default)
    let result = engine.process_template(&template_content, &variables, true)?;
    
    // Output result
    println!("{}", result);
    
    info!("Successfully processed template {:?}", template_path);
    Ok(())
}

async fn show_status(config: &Config) -> Result<()> {
    use crate::enrollment::EnrollmentManager;
    
    // Ensure distributed filesystem is available
    crate::fs::ensure_distributed_fs_available(&config.mfs_mount)?;
    
    // Create enrollment manager
    let manager = EnrollmentManager::new(
        config.mfs_mount.clone(),
        config.laszoo_dir.clone()
    );
    
    // Load manifest and show enrolled files
    let manifest = manager.load_manifest()?;
    
    println!("=== Laszoo Status ===");
    println!("MooseFS Mount: {:?}", config.mfs_mount);
    println!("Hostname: {}", gethostname::gethostname().to_string_lossy());
    println!();
    
    if manifest.entries.is_empty() {
        println!("No files enrolled.");
    } else {
        println!("Enrolled Files:");
        
        // Group by group name
        let mut groups: std::collections::HashMap<String, Vec<_>> = std::collections::HashMap::new();
        for (path, entry) in &manifest.entries {
            groups.entry(entry.group.clone())
                .or_insert_with(Vec::new)
                .push((path, entry));
        }
        
        for (group, files) in groups {
            println!("\n  Group: {}", group);
            for (path, entry) in files {
                let status = manager.check_file_status(path)?
                    .map(|s| match s {
                        crate::enrollment::FileStatus::Unchanged => "✓",
                        crate::enrollment::FileStatus::Modified => "●",
                    })
                    .unwrap_or("✗");
                    
                println!("    {} {:?}", status, path);
                if let Some(last_synced) = &entry.last_synced {
                    println!("       Last synced: {}", last_synced.format("%Y-%m-%d %H:%M:%S"));
                }
            }
        }
    }
    
    Ok(())
}

async fn handle_group_command(command: GroupCommands) -> Result<()> {
    match command {
        GroupCommands::Create { name, description } => {
            info!("Creating group '{}' with description: {:?}", name, description);
            // TODO: Implement group creation
            println!("Group creation not yet implemented");
        }
        GroupCommands::List => {
            info!("Listing groups");
            // TODO: Implement group listing
            println!("Group listing not yet implemented");
        }
        GroupCommands::Delete { name, force } => {
            info!("Deleting group '{}' (force: {})", name, force);
            // TODO: Implement group deletion
            println!("Group deletion not yet implemented");
        }
        GroupCommands::AddHost { group, host } => {
            let hostname = host.unwrap_or_else(|| {
                gethostname::gethostname().to_string_lossy().to_string()
            });
            info!("Adding host '{}' to group '{}'", hostname, group);
            // TODO: Implement host addition
            println!("Host addition not yet implemented");
        }
        GroupCommands::RemoveHost { group, host } => {
            let hostname = host.unwrap_or_else(|| {
                gethostname::gethostname().to_string_lossy().to_string()
            });
            info!("Removing host '{}' from group '{}'", hostname, group);
            // TODO: Implement host removal
            println!("Host removal not yet implemented");
        }
    }
    
    Ok(())
}
