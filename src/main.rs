mod cli;
mod config;
mod error;
mod fs;
mod logging;
mod enrollment;
mod template;
mod monitor;
mod sync;
mod git;
mod group;

use clap::Parser;
use tracing::{info, error, debug, warn};
use std::path::{Path, PathBuf};
use std::collections::HashMap;

use crate::{
    cli::{Cli, Commands, GroupCommands, GroupsCommands, SyncAction},
    config::Config,
    error::{Result, LaszooError},
};

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments
    let cli = Cli::parse();
    
    // Load configuration
    let config = Config::load(cli.config.as_deref())?;
    
    // Initialize logging
    crate::logging::init_logging(&config.logging, cli.verbose)?;
    
    // Log startup info
    info!("Starting Laszoo v{}", env!("CARGO_PKG_VERSION"));
    
    match cli.command {
        Commands::Init { mfs_mount } => {
            init_laszoo(&config, &mfs_mount).await?;
        }
        Commands::Commit { message, all } => {
            commit_changes(&config, message.as_deref(), all).await?;
        }
        Commands::Enroll { group, paths, force, include_hidden, machine, hybrid, before, after, action } => {
            enroll_files(&config, &group, paths, force, include_hidden, machine, hybrid, before, after, action).await?;
        }
        Commands::Unenroll { group, paths } => {
            unenroll_files(&config, group, paths).await?;
        }
        Commands::Sync { group, strategy } => {
            sync_files(&config, group.as_deref(), &strategy, cli.dry_run).await?;
        }
        Commands::Status { detailed } => {
            show_status(&config, detailed).await?;
        }
        Commands::Rollback { target, commits } => {
            info!("Rolling back {} commits for {}", commits, target);
            // TODO: Implement rollback
            println!("Rollback not yet implemented");
        }
        Commands::Apply { group, files } => {
            apply_group_templates(&config, &group, files).await?;
        }
        Commands::Group { name, command } => {
            handle_group_command(&name, command).await?;
        }
        Commands::Groups { command } => {
            handle_groups_command(command).await?;
        }
        Commands::Watch { group, interval, auto } => {
            watch_for_changes(&config, group.as_deref(), interval, auto).await?;
        }
    }
    
    Ok(())
}
async fn init_laszoo(config: &Config, mfs_mount: &std::path::Path) -> Result<()> {
    info!("Initializing Laszoo with distributed filesystem at {:?}", mfs_mount);
    
    // Check if distributed filesystem is available
    crate::fs::ensure_distributed_fs_available(mfs_mount)?;
    
    // Mount point is the Laszoo directory itself - no subdirectory needed
    
    // Create machines directory
    let machines_dir = mfs_mount.join("machines");
    if !machines_dir.exists() {
        std::fs::create_dir_all(&machines_dir)?;
        info!("Created machines directory at {:?}", machines_dir);
    }
    
    // Create groups directory
    let groups_dir = mfs_mount.join("groups");
    if !groups_dir.exists() {
        std::fs::create_dir_all(&groups_dir)?;
        info!("Created groups directory at {:?}", groups_dir);
    }
    
    // Get hostname
    let hostname = gethostname::gethostname()
        .to_string_lossy()
        .to_string();
    
    // Create host-specific directory
    let host_path = machines_dir.join(&hostname);
    if !host_path.exists() {
        std::fs::create_dir_all(&host_path)?;
        info!("Created host directory at {:?}", host_path);
    }
    
    // Initialize git repository
    let git = crate::git::GitManager::new(mfs_mount.to_path_buf());
    git.init_repo()?;
    info!("Initialized git repository at {:?}", mfs_mount);
    
    // Create initial .gitignore
    let gitignore = mfs_mount.join(".gitignore");
    if !gitignore.exists() {
        std::fs::write(&gitignore, "# Laszoo Git Ignore\n*.swp\n*.tmp\n.DS_Store\n")?;
    }
    
    // TODO: Implement git commit with Ollama integration
    // git.stage_all()?;
    // git.commit(&config.ollama_endpoint, &config.ollama_model, "Initial Laszoo setup").await?;
    // info!("Created initial git commit");
    
    // Save configuration
    let config_path = mfs_mount.join("laszoo.toml");
    if !config_path.exists() {
        config.save(&config_path)?;
        info!("Saved configuration to {:?}", config_path);
    }
    
    println!("Laszoo initialized successfully at {:?}", mfs_mount);
    println!("Hostname: {}", hostname);
    
    Ok(())
}

async fn enroll_files(
    config: &Config, 
    group: &str, 
    paths: Vec<PathBuf>, 
    force: bool,
    _include_hidden: bool,
    machine: bool,
    hybrid: bool,
    before: Option<String>,
    after: Option<String>,
    action: crate::cli::SyncAction
) -> Result<()> {
    use crate::enrollment::EnrollmentManager;
    
    // Ensure distributed filesystem is available
    crate::fs::ensure_distributed_fs_available(&config.mfs_mount)?;
    
    // Create enrollment manager
    let manager = EnrollmentManager::new(
        config.mfs_mount.clone(),
        "".to_string()
    );
    
    // If no paths provided, enroll the machine into the group
    if paths.is_empty() {
        manager.enroll_path(group, None, force, machine, hybrid)?;
        info!("Successfully enrolled machine into group '{}'", group);
        
        // Store triggers and action for this group if provided
        if before.is_some() || after.is_some() || !matches!(action, crate::cli::SyncAction::Converge) {
            store_group_config(&config.mfs_mount, group, before.as_deref(), after.as_deref(), &action)?;
        }
        
        return Ok(());
    }
    
    let mut enrolled_count = 0;
    let mut error_count = 0;
    
    for path in paths {
        match manager.enroll_path(group, Some(&path), force, machine, hybrid) {
            Ok(_) => {
                info!("Enrolled: {:?}", path);
                enrolled_count += 1;
            }
            Err(e) => {
                error!("Failed to enroll {:?}: {}", path, e);
                error_count += 1;
            }
        }
    }
    
    // Store triggers and action for this group if provided
    if enrolled_count > 0 && (before.is_some() || after.is_some() || !matches!(action, crate::cli::SyncAction::Converge)) {
        store_group_config(&config.mfs_mount, group, before.as_deref(), after.as_deref(), &action)?;
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

async fn apply_group_templates(config: &Config, group: &str, files: Vec<PathBuf>) -> Result<()> {
    use crate::enrollment::EnrollmentManager;
    
    // Ensure distributed filesystem is available
    crate::fs::ensure_distributed_fs_available(&config.mfs_mount)?;
    
    // Create enrollment manager
    let manager = EnrollmentManager::new(
        config.mfs_mount.clone(),
        "".to_string()
    );
    
    info!("Applying all templates from group '{}'", group);
    
    if files.is_empty() {
        // Apply all templates from the group
        manager.apply_group_templates(group)?;
    } else {
        // Apply specific files
        for _file in files {
            // TODO: Implement selective file application
            warn!("Selective file application not yet implemented");
        }
    }
    
    println!("Successfully applied all templates from group '{}'", group);
    Ok(())
}

async fn unenroll_files(config: &Config, group: Option<String>, paths: Vec<PathBuf>) -> Result<()> {
    use crate::enrollment::EnrollmentManager;
    
    // Ensure distributed filesystem is available
    crate::fs::ensure_distributed_fs_available(&config.mfs_mount)?;
    
    // Create enrollment manager
    let manager = EnrollmentManager::new(
        config.mfs_mount.clone(),
        "".to_string()
    );
    
    // If only group provided without paths, show enrolled files in that group
    if let Some(group_name) = group {
        if paths.is_empty() {
            info!("Listing files enrolled in group '{}'", group_name);
            let entries = manager.list_enrolled_files(Some(&group_name))?;
            
            if entries.is_empty() {
                println!("No files enrolled in group '{}'", group_name);
            } else {
                println!("Files enrolled in group '{}':", group_name);
                for entry in entries {
                    println!("  - {}", entry.original_path.display());
                }
            }
            return Ok(());
        }
    }
    
    // Unenroll specified files
    let mut unenrolled_count = 0;
    let mut error_count = 0;
    
    for path in paths {
        match manager.unenroll_file(&path) {
            Ok(()) => {
                info!("Unenrolled: {:?}", path);
                unenrolled_count += 1;
            }
            Err(e) => {
                error!("Failed to unenroll {:?}: {}", path, e);
                error_count += 1;
            }
        }
    }
    
    info!("Unenrollment complete: {} files unenrolled, {} errors", 
          unenrolled_count, error_count);
    
    if error_count > 0 {
        Err(LaszooError::Other(
            format!("Unenrollment completed with {} errors", error_count)
        ))
    } else {
        Ok(())
    }
}

async fn show_status(config: &Config, detailed: bool) -> Result<()> {
    use crate::enrollment::EnrollmentManager;
    
    // Ensure distributed filesystem is available
    crate::fs::ensure_distributed_fs_available(&config.mfs_mount)?;
    
    let hostname = gethostname::gethostname()
        .to_string_lossy()
        .to_string();
    
    println!("=== Laszoo Status ===");
    println!("Mount Point: {:?}", config.mfs_mount);
    println!("Hostname: {}", hostname);
    
    // Read machine's groups.conf
    let groups_file = config.mfs_mount
        .join("machines")
        .join(&hostname)
        .join("etc")
        .join("laszoo")
        .join("groups.conf");
    
    let machine_groups: Vec<String> = if groups_file.exists() {
        std::fs::read_to_string(&groups_file)?
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        Vec::new()
    };
    
    if machine_groups.is_empty() {
        println!("\nThis machine is not in any groups and has no enrolled files.");
        return Ok(());
    }
    
    println!("\nGroups this machine belongs to:");
    for group in &machine_groups {
        println!("  • {}", group);
    }
    
    // Create enrollment manager
    let enrollment_manager = EnrollmentManager::new(
        config.mfs_mount.clone(),
        "".to_string()
    );
    
    println!("\nEnrolled Files by Group:");
    
    for group_name in &machine_groups {
        println!("\n  [{}]", group_name);
        
        // Load files from both machine and group manifests
        let mut files: HashMap<PathBuf, crate::enrollment::EnrollmentEntry> = HashMap::new();
        
        // Load from group manifest
        if let Ok(group_manifest) = enrollment_manager.load_group_manifest(group_name) {
            for (path, entry) in group_manifest.entries {
                files.insert(path, entry);
            }
        }
        
        // Load from machine manifest (machine-specific files)
        let machine_manifest = enrollment_manager.load_manifest()?;
        for (path, entry) in &machine_manifest.entries {
            if &entry.group == group_name {
                files.insert(path.clone(), entry.clone());
            }
        }
        
        if files.is_empty() {
            println!("    (no files enrolled)");
            continue;
        }
        
        // Group files by enrolled directory
        let mut by_directory: HashMap<Option<PathBuf>, Vec<(&PathBuf, &crate::enrollment::EnrollmentEntry)>> = HashMap::new();
        let mut individual_files = Vec::new();
        
        for (path, entry) in &files {
            if let Some(dir) = &entry.enrolled_directory {
                by_directory.entry(Some(dir.clone()))
                    .or_insert_with(Vec::new)
                    .push((path, entry));
            } else {
                individual_files.push((path, entry));
            }
        }
        
        // Show grouped directories first
        for (dir_opt, files) in by_directory {
            if let Some(dir_path) = dir_opt {
                // Count file statuses
                let file_count = files.len();
                let mut unchanged_count = 0;
                let mut modified_count = 0;
                let mut missing_count = 0;
                
                for (path, _) in &files {
                    match enrollment_manager.check_file_status(path)? {
                        Some(crate::enrollment::FileStatus::Unchanged) => unchanged_count += 1,
                        Some(crate::enrollment::FileStatus::Modified) => modified_count += 1,
                        None => missing_count += 1,
                    }
                }
                
                // Determine overall directory status
                let status = if !dir_path.exists() {
                    "✗"
                } else if missing_count == file_count {
                    "✗"
                } else if modified_count > 0 {
                    "●"
                } else {
                    "✓"
                };
                
                // Build status string
                let mut status_parts = vec![format!("1 directory, {} files", file_count)];
                
                if unchanged_count > 0 {
                    let percent = (unchanged_count * 100) / file_count;
                    status_parts.push(format!("✓ {}% ok ({}/{})", percent, unchanged_count, file_count));
                }
                
                if modified_count > 0 {
                    let percent = (modified_count * 100) / file_count;
                    status_parts.push(format!("● {}% modified ({}/{})", percent, modified_count, file_count));
                }
                
                if missing_count > 0 {
                    let percent = (missing_count * 100) / file_count;
                    status_parts.push(format!("✗ {}% missing ({}/{})", percent, missing_count, file_count));
                }
                
                println!("    {} {} ({})", status, dir_path.display(), status_parts.join(", "));
                
                if detailed {
                    for (path, entry) in files {
                        let file_status = enrollment_manager.check_file_status(path)?
                            .map(|s| match s {
                                crate::enrollment::FileStatus::Unchanged => "✓",
                                crate::enrollment::FileStatus::Modified => "●",
                            })
                            .unwrap_or("✗");
                        println!("      {} {}", file_status, path.display());
                        
                        if let Some(last_synced) = &entry.last_synced {
                            println!("        Last synced: {}", last_synced.format("%Y-%m-%d %H:%M:%S"));
                        }
                    }
                }
            }
        }
        
        // Show individual files
        individual_files.sort_by(|a, b| a.0.cmp(&b.0));
        for (path, entry) in individual_files {
            let status = enrollment_manager.check_file_status(path)?
                .map(|s| match s {
                    crate::enrollment::FileStatus::Unchanged => "✓",
                    crate::enrollment::FileStatus::Modified => "●",
                })
                .unwrap_or("✗");
                
            println!("    {} {}", status, path.display());
            
            if detailed {
                if let Some(last_synced) = &entry.last_synced {
                    println!("      Last synced: {}", last_synced.format("%Y-%m-%d %H:%M:%S"));
                }
                if let Some(template_path) = &entry.template_path {
                    println!("      Template: {}", template_path.display());
                }
                println!("      Enrolled: {}", entry.enrolled_at.format("%Y-%m-%d %H:%M:%S"));
            }
        }
    }
    
    println!("\nLegend: ✓ = unchanged, ● = modified locally, ✗ = missing");
    
    Ok(())
}

async fn sync_files(
    config: &Config,
    group: Option<&str>,
    _strategy: &crate::cli::SyncStrategy,
    dry_run: bool,
) -> Result<()> {
    use crate::sync::SyncEngine;
    
    // Ensure distributed filesystem is available
    crate::fs::ensure_distributed_fs_available(&config.mfs_mount)?;
    
    // Create sync engine
    let engine = SyncEngine::new(
        config.mfs_mount.clone(),
        "".to_string()
    )?;
    
    if let Some(group_name) = group {
        // Sync specific group
        info!("Analyzing group '{}' for synchronization", group_name);
        let operations = engine.analyze_group(group_name).await?;
        
        if operations.is_empty() {
            info!("No synchronization needed for group '{}'", group_name);
        } else {
            info!("Found {} files needing synchronization", operations.len());
            engine.execute_sync(operations, dry_run).await?;
        }
    } else {
        // Sync all groups
        info!("Analyzing all groups for synchronization");
        
        // Get all unique groups from manifest
        let manager = crate::enrollment::EnrollmentManager::new(
            config.mfs_mount.clone(),
            "".to_string()
        );
        let manifest = manager.load_manifest()?;
        let groups: std::collections::HashSet<_> = manifest.entries
            .values()
            .map(|e| e.group.clone())
            .collect();
        
        let mut total_operations = 0;
        for group_name in groups {
            info!("Analyzing group '{}'", group_name);
            let operations = engine.analyze_group(&group_name).await?;
            total_operations += operations.len();
            
            if !operations.is_empty() {
                engine.execute_sync(operations, dry_run).await?;
            }
        }
        
        if total_operations == 0 {
            info!("No synchronization needed across all groups");
        } else {
            info!("Synchronized {} files across all groups", total_operations);
        }
    }
    
    Ok(())
}

async fn commit_changes(
    config: &Config,
    user_message: Option<&str>,
    stage_all: bool,
) -> Result<()> {
    use crate::git::GitManager;
    
    // Use the mount point as the git repo
    let git = GitManager::new(config.mfs_mount.clone());
    
    // Check if there are changes
    if !git.has_changes()? {
        info!("No changes to commit");
        return Ok(());
    }
    
    // Show status
    let statuses = git.get_status()?;
    println!("Git status:");
    for (path, status) in &statuses {
        let status_char = match status {
            s if s.contains(git2::Status::INDEX_NEW) => "A",
            s if s.contains(git2::Status::INDEX_MODIFIED) => "M",
            s if s.contains(git2::Status::INDEX_DELETED) => "D",
            s if s.contains(git2::Status::WT_NEW) => "?",
            s if s.contains(git2::Status::WT_MODIFIED) => "M",
            s if s.contains(git2::Status::WT_DELETED) => "D",
            _ => " ",
        };
        println!("  {} {:?}", status_char, path);
    }
    
    // Stage files if requested
    if stage_all {
        info!("Staging all changes");
        git.stage_all()?;
    } else {
        // Check if anything is staged
        let has_staged = statuses.iter().any(|(_, s)| {
            s.contains(git2::Status::INDEX_NEW) ||
            s.contains(git2::Status::INDEX_MODIFIED) ||
            s.contains(git2::Status::INDEX_DELETED)
        });
        
        if !has_staged {
            warn!("No files staged for commit. Use --all to stage all changes.");
            return Ok(());
        }
    }
    
    // Create commit with AI-generated message
    info!("Generating commit message with {}", config.ollama_model);
    let commit_id = git.commit_with_ai(
        &config.ollama_endpoint,
        &config.ollama_model,
        user_message,
    ).await?;
    
    info!("Successfully created commit: {}", commit_id);
    Ok(())
}

async fn handle_group_command(group_name: &str, command: GroupCommands) -> Result<()> {
    // Load config to get MFS mount
    let config = Config::load(None)?;
    
    // Ensure distributed filesystem is available
    crate::fs::ensure_distributed_fs_available(&config.mfs_mount)?;
    
    match command {
        GroupCommands::Add { machine } => {
            let machine_name = machine.unwrap_or_else(|| {
                gethostname::gethostname().to_string_lossy().to_string()
            });
            
            info!("Adding machine '{}' to group '{}'", machine_name, group_name);
            
            // Create group directory if it doesn't exist
            let group_dir = config.mfs_mount.join("groups").join(group_name);
            if !group_dir.exists() {
                std::fs::create_dir_all(&group_dir)?;
                info!("Created new group '{}'", group_name);
            }
            
            // Update machine's groups.conf
            update_machine_groups(&config.mfs_mount, &machine_name, group_name, true)?;
            
            println!("Successfully added machine '{}' to group '{}'", machine_name, group_name);
        }
        GroupCommands::Remove { machine, keep } => {
            let machine_name = machine.unwrap_or_else(|| {
                gethostname::gethostname().to_string_lossy().to_string()
            });
            
            info!("Removing machine '{}' from group '{}'", machine_name, group_name);
            
            // First check if the machine is actually in the group
            let groups_file = config.mfs_mount
                .join("machines")
                .join(&machine_name)
                .join("etc")
                .join("laszoo")
                .join("groups.conf");
            
            let mut in_group = false;
            if groups_file.exists() {
                let groups: Vec<String> = std::fs::read_to_string(&groups_file)?
                    .lines()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                
                in_group = groups.contains(&group_name.to_string());
            }
            
            if !in_group {
                println!("Machine '{}' is not in group '{}'", machine_name, group_name);
                return Ok(());
            }
            
            // Update machine's groups.conf
            update_machine_groups(&config.mfs_mount, &machine_name, group_name, false)?;
            
            // Check if this was the last member of the group
            if !keep {
                let mut has_members = false;
                let machines_dir = config.mfs_mount.join("machines");
                
                if let Ok(entries) = std::fs::read_dir(&machines_dir) {
                    for entry in entries.flatten() {
                        if let Some(other_machine) = entry.file_name().to_str() {
                            if other_machine != machine_name {
                                let other_groups_file = machines_dir
                                    .join(other_machine)
                                    .join("etc")
                                    .join("laszoo")
                                    .join("groups.conf");
                                
                                if other_groups_file.exists() {
                                    let content = std::fs::read_to_string(&other_groups_file)?;
                                    if content.lines().any(|l| l.trim() == group_name) {
                                        has_members = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                
                if !has_members {
                    // Remove the group directory
                    let group_dir = config.mfs_mount.join("groups").join(group_name);
                    if group_dir.exists() {
                        std::fs::remove_dir_all(&group_dir)?;
                        info!("Removed empty group '{}'", group_name);
                    }
                }
            }
            
            println!("Successfully removed machine '{}' from group '{}'", machine_name, group_name);
        }
        GroupCommands::List {} => {
            info!("Listing machines in group '{}'", group_name);
            
            let machines = list_machines_in_group(&config.mfs_mount, group_name)?;
            
            if machines.is_empty() {
                println!("No machines in group '{}'", group_name);
            } else {
                println!("Machines in group '{}':", group_name);
                for machine in machines {
                    println!("  • {}", machine);
                }
            }
        }
        GroupCommands::Rename { new_name } => {
            info!("Renaming group '{}' to '{}'", group_name, new_name);
            
            // Check if new group already exists
            let new_group_dir = config.mfs_mount.join("groups").join(&new_name);
            if new_group_dir.exists() {
                return Err(LaszooError::Other(format!("Group '{}' already exists", new_name)));
            }
            
            // Rename group directory
            let old_group_dir = config.mfs_mount.join("groups").join(group_name);
            if old_group_dir.exists() {
                std::fs::rename(&old_group_dir, &new_group_dir)?;
            }
            
            // Update all machines' groups.conf files
            let machines_dir = config.mfs_mount.join("machines");
            if let Ok(entries) = std::fs::read_dir(&machines_dir) {
                for entry in entries.flatten() {
                    if let Some(machine_name) = entry.file_name().to_str() {
                        let groups_file = machines_dir
                            .join(machine_name)
                            .join("etc")
                            .join("laszoo")
                            .join("groups.conf");
                        
                        if groups_file.exists() {
                            let content = std::fs::read_to_string(&groups_file)?;
                            let groups: Vec<String> = content
                                .lines()
                                .map(|l| if l.trim() == group_name { new_name.to_string() } else { l.to_string() })
                                .collect();
                            
                            std::fs::write(&groups_file, groups.join("\n") + "\n")?;
                        }
                    }
                }
            }
            
            println!("Successfully renamed group '{}' to '{}'", group_name, new_name);
        }
    }
    
    Ok(())
}

async fn handle_groups_command(command: GroupsCommands) -> Result<()> {
    // Load config to get MFS mount
    let config = Config::load(None)?;
    
    // Ensure distributed filesystem is available
    crate::fs::ensure_distributed_fs_available(&config.mfs_mount)?;
    
    match command {
        GroupsCommands::List {} => {
            info!("Listing all groups");
            
            let groups_dir = config.mfs_mount.join("groups");
            
            if !groups_dir.exists() {
                println!("No groups exist yet.");
                return Ok(());
            }
            
            let mut groups = Vec::new();
            
            if let Ok(entries) = std::fs::read_dir(&groups_dir) {
                for entry in entries.flatten() {
                    if let Ok(metadata) = entry.metadata() {
                        if metadata.is_dir() {
                            if let Some(group_name) = entry.file_name().to_str() {
                                // Count machines in this group
                                let machines = list_machines_in_group(&config.mfs_mount, group_name)?;
                                groups.push((group_name.to_string(), machines.len()));
                            }
                        }
                    }
                }
            }
            
            if groups.is_empty() {
                println!("No groups exist yet.");
            } else {
                groups.sort_by(|a, b| a.0.cmp(&b.0));
                
                println!("Groups:");
                for (group_name, machine_count) in groups {
                    println!("  • {} ({} machine{})", 
                        group_name, 
                        machine_count,
                        if machine_count == 1 { "" } else { "s" }
                    );
                }
            }
        }
    }
    
    Ok(())
}

// Helper function to update machine's groups.conf
fn update_machine_groups(mfs_mount: &Path, machine_name: &str, group_name: &str, add: bool) -> Result<()> {
    let groups_file = mfs_mount
        .join("machines")
        .join(machine_name)
        .join("etc")
        .join("laszoo")
        .join("groups.conf");
    
    // Create directory if needed
    if let Some(parent) = groups_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    // Read existing groups
    let mut groups: Vec<String> = if groups_file.exists() {
        std::fs::read_to_string(&groups_file)?
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        Vec::new()
    };
    
    if add {
        if !groups.contains(&group_name.to_string()) {
            groups.push(group_name.to_string());
            groups.sort();
        }
    } else {
        groups.retain(|g| g != group_name);
    }
    
    // Write back
    std::fs::write(&groups_file, groups.join("\n") + "\n")?;
    
    // Update symlinks in group directories
    update_group_symlinks(mfs_mount, machine_name, &groups)?;
    
    Ok(())
}

// Helper function to update symlinks in group directories
fn update_group_symlinks(mfs_mount: &Path, machine_name: &str, groups: &[String]) -> Result<()> {
    let groups_dir = mfs_mount.join("groups");
    let machine_path = Path::new("../../../machines").join(machine_name);
    
    // Remove old symlinks
    if let Ok(entries) = std::fs::read_dir(&groups_dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_dir() {
                    let symlink_path = entry.path().join("machines").join(machine_name);
                    if symlink_path.exists() || symlink_path.is_symlink() {
                        let _ = std::fs::remove_file(&symlink_path);
                    }
                }
            }
        }
    }
    
    // Create new symlinks
    for group in groups {
        let group_machines_dir = groups_dir.join(group).join("machines");
        
        // Create machines directory if needed
        if !group_machines_dir.exists() {
            std::fs::create_dir_all(&group_machines_dir)?;
        }
        
        let symlink_path = group_machines_dir.join(machine_name);
        
        // Create symlink
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let _ = symlink(&machine_path, &symlink_path);
        }
    }
    
    Ok(())
}

// Helper function to list machines in a group
fn list_machines_in_group(mfs_mount: &Path, group_name: &str) -> Result<Vec<String>> {
    let machines_dir = mfs_mount.join("machines");
    let mut machines = Vec::new();
    
    if let Ok(entries) = std::fs::read_dir(&machines_dir) {
        for entry in entries.flatten() {
            if let Some(machine_name) = entry.file_name().to_str() {
                let groups_file = machines_dir
                    .join(machine_name)
                    .join("etc")
                    .join("laszoo")
                    .join("groups.conf");
                
                if groups_file.exists() {
                    let content = std::fs::read_to_string(&groups_file)?;
                    if content.lines().any(|l| l.trim() == group_name) {
                        machines.push(machine_name.to_string());
                    }
                }
            }
        }
    }
    
    machines.sort();
    Ok(machines)
}
async fn watch_for_changes(config: &Config, group: Option<&str>, _interval: u64, auto: bool) -> Result<()> {
    use notify::{Watcher, RecursiveMode, Event, EventKind};
    use std::sync::mpsc::channel;
    use std::time::Duration;
    use std::collections::HashSet;
    
    info!("Starting watch mode for group: {:?}, auto: {}", group, auto);
    
    let hostname = gethostname::gethostname().to_string_lossy().to_string();
    let enrollment_manager = crate::enrollment::EnrollmentManager::new(
        config.mfs_mount.clone(),
        hostname.clone(),
    );
    
    println!("Starting watch mode...");
    if auto {
        println!("Auto-apply mode enabled - changes will be applied automatically");
    } else {
        println!("Manual mode - you will be prompted before applying changes");
    }
    println!("Press Ctrl+C to stop watching\n");
    
    // Get machine's groups
    let groups_file = config.mfs_mount
        .join("machines")
        .join(&hostname)
        .join("etc")
        .join("laszoo")
        .join("groups.conf");
    
    let machine_groups: Vec<String> = if groups_file.exists() {
        std::fs::read_to_string(&groups_file)?
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        Vec::new()
    };
    
    // Filter groups based on command line argument
    let groups_to_watch = if let Some(group_name) = group {
        if machine_groups.contains(&group_name.to_string()) {
            vec![group_name.to_string()]
        } else {
            error!("Machine is not in group '{}'", group_name);
            return Err(LaszooError::Other(format!("Machine is not in group '{}'", group_name)));
        }
    } else {
        machine_groups
    };
    
    if groups_to_watch.is_empty() {
        println!("This machine is not in any groups. Nothing to watch.");
        return Ok(());
    }
    
    // Collect all files and directories to watch from manifests
    let mut watch_paths = HashSet::new();
    let mut watch_dirs = HashSet::new();
    let mut file_to_group_map = std::collections::HashMap::new();
    let mut dir_to_group_map = std::collections::HashMap::new();
    
    for group_name in &groups_to_watch {
        // Load both group and machine manifests
        if let Ok(group_manifest) = enrollment_manager.load_group_manifest(group_name) {
            for (path, entry) in &group_manifest.entries {
                watch_paths.insert(path.clone());
                file_to_group_map.insert(path.clone(), group_name.clone());
                
                // Also track enrolled directories
                if let Some(enrolled_dir) = &entry.enrolled_directory {
                    watch_dirs.insert(enrolled_dir.clone());
                    dir_to_group_map.insert(enrolled_dir.clone(), group_name.clone());
                }
            }
        }
        
        let machine_manifest = enrollment_manager.load_manifest()?;
        for (path, entry) in &machine_manifest.entries {
            if &entry.group == group_name {
                watch_paths.insert(path.clone());
                file_to_group_map.insert(path.clone(), group_name.clone());
                
                // Also track enrolled directories
                if let Some(enrolled_dir) = &entry.enrolled_directory {
                    watch_dirs.insert(enrolled_dir.clone());
                    dir_to_group_map.insert(enrolled_dir.clone(), group_name.clone());
                }
            }
        }
    }
    
    if watch_paths.is_empty() {
        println!("No enrolled files found in the specified group(s).");
        return Ok(());
    }
    
    println!("Watching {} files across {} group(s):", watch_paths.len(), groups_to_watch.len());
    for group in &groups_to_watch {
        println!("  • {}", group);
    }
    println!();
    
    // Create a channel for file events
    let (tx, rx) = channel();
    
    // Create a debounced watcher
    let mut watcher = notify::recommended_watcher(move |event: std::result::Result<Event, notify::Error>| {
        if let Ok(event) = event {
            let _ = tx.send(event);
        }
    })?;
    
    // Watch files and directories
    let mut watched_count = 0;
    
    // Watch individual files
    for path in &watch_paths {
        if path.exists() {
            if let Err(e) = watcher.watch(path, RecursiveMode::NonRecursive) {
                warn!("Failed to watch {:?}: {}", path, e);
            } else {
                watched_count += 1;
            }
        } else {
            // Watch parent directory for file creation
            if let Some(parent) = path.parent() {
                if parent.exists() {
                    if let Err(e) = watcher.watch(parent, RecursiveMode::NonRecursive) {
                        warn!("Failed to watch parent directory {:?}: {}", parent, e);
                    }
                }
            }
        }
    }
    
    // Watch enrolled directories for new files
    for dir in &watch_dirs {
        if dir.exists() {
            if let Err(e) = watcher.watch(dir, RecursiveMode::NonRecursive) {
                warn!("Failed to watch directory {:?}: {}", dir, e);
            } else {
                debug!("Watching directory {:?} for new files", dir);
            }
        }
    }
    
    if watched_count == 0 && watch_dirs.is_empty() {
        println!("Warning: No files or directories could be watched.");
    } else {
        println!("Successfully watching {} files and {} directories.", watched_count, watch_dirs.len());
    }
    
    // Process events
    let mut debounce_buffer = HashSet::new();
    let debounce_duration = Duration::from_millis(500);
    let mut last_event_time = std::time::Instant::now();
    
    loop {
        // Check for events with timeout
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => {
                match event.kind {
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) => {
                        for path in event.paths {
                            // Check if it's a watched file
                            if watch_paths.contains(&path) {
                                debounce_buffer.insert(path);
                                last_event_time = std::time::Instant::now();
                            } else if path.is_file() {
                                // Check if it's a new file in a watched directory
                                if let Some(parent) = path.parent() {
                                    if watch_dirs.contains(parent) {
                                        debounce_buffer.insert(path);
                                        last_event_time = std::time::Instant::now();
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Check if we should process debounced events
                if !debounce_buffer.is_empty() && 
                   last_event_time.elapsed() > debounce_duration {
                    
                    // Process the buffered changes
                    println!("\n[{}] Changes detected in {} file(s):", 
                        chrono::Local::now().format("%H:%M:%S"),
                        debounce_buffer.len()
                    );
                    
                    let mut affected_groups = HashSet::new();
                    for path in &debounce_buffer {
                        // Check if it's an enrolled file
                        if let Some(group) = file_to_group_map.get(path) {
                            affected_groups.insert(group.clone());
                            let status_char = if path.exists() {
                                // Check if modified
                                match enrollment_manager.check_file_status(path)? {
                                    Some(crate::enrollment::FileStatus::Modified) => "●",
                                    Some(crate::enrollment::FileStatus::Unchanged) => "✓",
                                    None => "?",
                                }
                            } else {
                                "✗"
                            };
                            println!("  {} {} (group: {})", status_char, path.display(), group);
                        } else {
                            // Check if it's a new file in an enrolled directory
                            if let Some(parent) = path.parent() {
                                if let Some(group) = dir_to_group_map.get(parent) {
                                    println!("  ? {} (new file in group: {})", path.display(), group);
                                    affected_groups.insert(group.clone());
                                }
                            }
                        }
                    }
                    
                    // Apply changes if auto mode is enabled or user confirms
                    let should_apply = if auto {
                        true
                    } else {
                        print!("\nApply changes? [y/N] ");
                        use std::io::{self, Write};
                        io::stdout().flush()?;
                        
                        let mut input = String::new();
                        io::stdin().read_line(&mut input)?;
                        input.trim().to_lowercase() == "y"
                    };
                    
                    if should_apply {
                        for group_name in affected_groups {
                            println!("\nApplying templates from group '{}'...", group_name);
                            match enrollment_manager.apply_group_templates(&group_name) {
                                Ok(()) => {
                                    println!("✓ Successfully applied templates from group '{}'", group_name);
                                }
                                Err(e) => {
                                    error!("Failed to apply templates for group '{}': {}", group_name, e);
                                    println!("✗ Failed to apply templates for group '{}': {}", group_name, e);
                                }
                            }
                        }
                        
                        // Auto-commit if enabled
                        if config.auto_commit {
                            println!("\nAuto-committing changes...");
                            if let Err(e) = commit_changes(config, Some("Auto-commit from watch mode"), false).await {
                                error!("Failed to auto-commit: {}", e);
                            }
                        }
                    } else {
                        println!("Changes not applied.");
                    }
                    
                    // Clear the buffer
                    debounce_buffer.clear();
                    println!(); // Add blank line for readability
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                error!("Watcher channel disconnected");
                break;
            }
        }
    }
    
    Ok(())
}

/// Store group configuration including triggers and sync action
fn store_group_config(
    mfs_mount: &Path,
    group: &str,
    before: Option<&str>,
    after: Option<&str>,
    action: &SyncAction,
) -> Result<()> {
    use serde::{Serialize, Deserialize};
    
    #[derive(Serialize, Deserialize, Default)]
    struct GroupConfig {
        #[serde(skip_serializing_if = "Option::is_none")]
        before_trigger: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        after_trigger: Option<String>,
        sync_action: String,
    }
    
    let config = GroupConfig {
        before_trigger: before.map(|s| s.to_string()),
        after_trigger: after.map(|s| s.to_string()),
        sync_action: match action {
            SyncAction::Converge => "converge".to_string(),
            SyncAction::Rollback => "rollback".to_string(),
            SyncAction::Freeze => "freeze".to_string(),
            SyncAction::Drift => "drift".to_string(),
        },
    };
    
    let config_path = mfs_mount
        .join("groups")
        .join(group)
        .join("config.json");
    
    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    let json = serde_json::to_string_pretty(&config)?;
    std::fs::write(&config_path, json)?;
    
    info!("Stored group configuration for '{}'", group);
    if let Some(cmd) = before {
        info!("  Before trigger: {}", cmd);
    }
    if let Some(cmd) = after {
        info!("  After trigger: {}", cmd);
    }
    info!("  Sync action: {:?}", action);
    
    Ok(())
}