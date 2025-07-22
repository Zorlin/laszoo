mod cli;
mod config;
mod error;
mod fs;
mod logging;

use clap::Parser;
use tracing::{info, error, debug};
use crate::cli::{Cli, Commands, GroupCommands, SyncStrategy};
use crate::config::Config;
use crate::error::Result;

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
        Commands::Enroll { group, path, force, include_hidden } => {
            info!("Enrolling {:?} into group '{}'", path, group);
            // TODO: Implement enrollment
            println!("Enrollment not yet implemented");
        }
        Commands::Sync { group, strategy } => {
            info!("Synchronizing with strategy: {:?}", strategy);
            // TODO: Implement sync
            println!("Sync not yet implemented");
        }
        Commands::Status { group, detailed } => {
            info!("Showing status");
            // TODO: Implement status
            println!("Status not yet implemented");
        }
        Commands::Rollback { target, commits } => {
            info!("Rolling back {} commits for {}", commits, target);
            // TODO: Implement rollback
            println!("Rollback not yet implemented");
        }
        Commands::Apply { template, output } => {
            info!("Applying template {:?}", template);
            // TODO: Implement template application
            println!("Template application not yet implemented");
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
