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
        Commands::Watch { group, interval, auto, hard } => {
            watch_for_changes(&config, group.as_deref(), interval, auto, hard).await?;
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
        
        // Load enrollments from both machine and group manifests
        let mut enrollments: HashMap<PathBuf, crate::enrollment::EnrollmentEntry> = HashMap::new();
        
        // Load from group manifest
        if let Ok(group_manifest) = enrollment_manager.load_group_manifest(group_name) {
            for (path, entry) in group_manifest.entries {
                enrollments.insert(path, entry);
            }
        }
        
        // Load from machine manifest (machine-specific enrollments)
        let machine_manifest = enrollment_manager.load_manifest()?;
        for (path, entry) in &machine_manifest.entries {
            if &entry.group == group_name {
                enrollments.insert(path.clone(), entry.clone());
            }
        }
        
        if enrollments.is_empty() {
            println!("    (nothing enrolled)");
            continue;
        }
        
        // Sort enrollments by path
        let mut entries: Vec<_> = enrollments.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        
        for (path, entry) in entries {
            // Check if this is a directory or file enrollment
            if entry.checksum == "directory" {
                // Handle directory enrollment
                let dir_path = path;
                
                // Count file statuses in directory
                let mut file_count = 0;
                let mut unchanged_count = 0;
                let mut modified_count = 0;
                let mut missing_count = 0;
                let mut new_count = 0;
                
                if dir_path.exists() && dir_path.is_dir() {
                    if let Ok(entries) = std::fs::read_dir(dir_path) {
                        for entry in entries.flatten() {
                            if let Ok(metadata) = entry.metadata() {
                                if metadata.is_file() {
                                    file_count += 1;
                                    let file_path = entry.path();
                                    
                                    // Check if template exists for this file
                                    let template_path = enrollment_manager.get_group_template_path(group_name, &file_path)?;
                                    
                                    if template_path.exists() {
                                        // Template exists, check if file matches
                                        if let Ok(template_content) = std::fs::read_to_string(&template_path) {
                                            if let Ok(file_content) = std::fs::read_to_string(&file_path) {
                                                // Process template to compare
                                                if let Ok(processed) = crate::template::process_handlebars(&template_content, &hostname) {
                                                    if processed == file_content {
                                                        unchanged_count += 1;
                                                    } else {
                                                        modified_count += 1;
                                                    }
                                                } else {
                                                    modified_count += 1; // Can't process, assume modified
                                                }
                                            } else {
                                                missing_count += 1; // Can't read file
                                            }
                                        } else {
                                            missing_count += 1; // Can't read template
                                        }
                                    } else {
                                        new_count += 1; // No template yet
                                    }
                                }
                            }
                        }
                    }
                    
                    // Determine overall directory status
                    let status = if modified_count > 0 {
                        "●"
                    } else if missing_count > 0 {
                        "✗"
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
                    
                    if new_count > 0 {
                        let percent = (new_count * 100) / file_count;
                        status_parts.push(format!("? {}% new ({}/{})", percent, new_count, file_count));
                    }
                    
                    println!("    {} {} ({})", status, dir_path.display(), status_parts.join(", "));
                } else {
                    // Directory doesn't exist
                    println!("    ✗ {} (directory missing)", dir_path.display());
                }
                
                if detailed {
                    // Show individual files when in detailed mode
                    if dir_path.exists() && dir_path.is_dir() {
                        if let Ok(entries) = std::fs::read_dir(dir_path) {
                            let mut files: Vec<_> = entries.flatten()
                                .filter_map(|e| {
                                    let metadata = e.metadata().ok()?;
                                    if metadata.is_file() {
                                        Some(e.path())
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            files.sort();
                            
                            for file_path in files {
                                // Check if template exists for this file
                                let template_path = enrollment_manager.get_group_template_path(group_name, &file_path)?;
                                
                                let file_status = if template_path.exists() {
                                    // Template exists, check if file matches
                                    if let Ok(template_content) = std::fs::read_to_string(&template_path) {
                                        if let Ok(file_content) = std::fs::read_to_string(&file_path) {
                                            // Process template to compare
                                            if let Ok(processed) = crate::template::process_handlebars(&template_content, &hostname) {
                                                if processed == file_content {
                                                    "✓"
                                                } else {
                                                    "●"
                                                }
                                            } else {
                                                "?"
                                            }
                                        } else {
                                            "?"
                                        }
                                    } else {
                                        "?"
                                    }
                                } else {
                                    "?"  // No template yet
                                };
                                
                                let relative_path = file_path.strip_prefix(dir_path)
                                    .unwrap_or(&file_path);
                                println!("      {} {}", file_status, relative_path.display());
                            }
                        }
                    }
                    
                    if let Some(last_synced) = &entry.last_synced {
                        println!("      Last synced: {}", last_synced.format("%Y-%m-%d %H:%M:%S"));
                    }
                    println!("      Enrolled: {}", entry.enrolled_at.format("%Y-%m-%d %H:%M:%S"));
                    if entry.is_hybrid == Some(true) {
                        println!("      Mode: hybrid");
                    }
                }
            } else {
                // Handle individual file enrollment
                let file_path = path;
                let status = if file_path.exists() {
                    // Check if file matches template
                    if let Some(template_path) = &entry.template_path {
                        if template_path.exists() {
                            if let Ok(template_content) = std::fs::read_to_string(template_path) {
                                if let Ok(file_content) = std::fs::read_to_string(file_path) {
                                    if let Ok(processed) = crate::template::process_handlebars(&template_content, &hostname) {
                                        if processed == file_content {
                                            "✓"
                                        } else {
                                            "●"
                                        }
                                    } else {
                                        "?"
                                    }
                                } else {
                                    "?"
                                }
                            } else {
                                "?"
                            }
                        } else {
                            "✗" // Template missing
                        }
                    } else {
                        "?" // No template path
                    }
                } else {
                    "✗" // File missing
                };
                
                println!("    {} {}", status, file_path.display());
                
                if detailed {
                    if let Some(last_synced) = &entry.last_synced {
                        println!("      Last synced: {}", last_synced.format("%Y-%m-%d %H:%M:%S"));
                    }
                    if let Some(template_path) = &entry.template_path {
                        println!("      Template: {}", template_path.display());
                    }
                    println!("      Enrolled: {}", entry.enrolled_at.format("%Y-%m-%d %H:%M:%S"));
                    if entry.is_hybrid == Some(true) {
                        println!("      Mode: hybrid");
                    }
                }
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
async fn watch_for_changes(config: &Config, group: Option<&str>, _interval: u64, auto: bool, hard: bool) -> Result<()> {
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
    
    // Collect all enrolled paths to watch from manifests
    let mut watch_paths = HashSet::new();
    let mut path_to_group_map = std::collections::HashMap::new();
    let mut enrolled_directories = HashSet::new();
    let mut enrolled_files = HashSet::new();
    
    for group_name in &groups_to_watch {
        // Load both group and machine manifests
        if let Ok(group_manifest) = enrollment_manager.load_group_manifest(group_name) {
            for (path, entry) in &group_manifest.entries {
                watch_paths.insert(path.clone());
                path_to_group_map.insert(path.clone(), group_name.clone());
                
                if entry.checksum == "directory" {
                    enrolled_directories.insert(path.clone());
                } else {
                    enrolled_files.insert(path.clone());
                }
            }
        }
        
        let machine_manifest = enrollment_manager.load_manifest()?;
        for (path, entry) in &machine_manifest.entries {
            if &entry.group == group_name {
                watch_paths.insert(path.clone());
                path_to_group_map.insert(path.clone(), group_name.clone());
                
                if entry.checksum == "directory" {
                    enrolled_directories.insert(path.clone());
                } else {
                    enrolled_files.insert(path.clone());
                }
            }
        }
    }
    
    if watch_paths.is_empty() {
        println!("No enrolled paths found in the specified group(s).");
        return Ok(());
    }
    
    println!("Watching {} paths ({} directories, {} files) across {} group(s):", 
        watch_paths.len(), enrolled_directories.len(), enrolled_files.len(), groups_to_watch.len());
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
    
    // Watch enrolled paths
    let mut watched_count = 0;
    
    // Watch directories recursively
    for dir in &enrolled_directories {
        if dir.exists() {
            if let Err(e) = watcher.watch(dir, RecursiveMode::Recursive) {
                warn!("Failed to watch directory {:?}: {}", dir, e);
            } else {
                watched_count += 1;
                debug!("Watching directory {:?} recursively", dir);
            }
        } else {
            warn!("Enrolled directory does not exist: {:?}", dir);
        }
    }
    
    // Watch individual files (and their parent directories non-recursively)
    let mut watched_file_dirs = HashSet::new();
    for file in &enrolled_files {
        if let Some(parent) = file.parent() {
            if !watched_file_dirs.contains(parent) && parent.exists() {
                if let Err(e) = watcher.watch(parent, RecursiveMode::NonRecursive) {
                    warn!("Failed to watch parent directory {:?}: {}", parent, e);
                } else {
                    watched_file_dirs.insert(parent.to_path_buf());
                    debug!("Watching parent directory {:?} for file {:?}", parent, file);
                }
            }
        }
        if file.exists() {
            watched_count += 1;
        } else {
            warn!("Enrolled file does not exist: {:?}", file);
        }
    }
    
    // Also watch the MooseFS mount for template changes to commit
    let mfs_groups_dir = config.mfs_mount.join("groups");
    if mfs_groups_dir.exists() {
        if let Err(e) = watcher.watch(&mfs_groups_dir, RecursiveMode::Recursive) {
            warn!("Failed to watch MooseFS groups directory: {}", e);
        } else {
            debug!("Watching MooseFS groups directory for template changes");
        }
    }
    
    if watched_count == 0 {
        println!("Warning: No directories could be watched.");
    } else {
        println!("Successfully watching {} directories.", watched_count);
    }
    
    // Initial scan for missing files if --hard is enabled
    if hard {
        println!("\nScanning enrolled directories for missing files...");
        let mut missing_files = Vec::new();
        
        // For enrolled directories, scan templates and check if files exist
        for dir in &enrolled_directories {
            if let Some(group) = path_to_group_map.get(dir) {
                let group_dir = crate::fs::get_group_dir(&config.mfs_mount, "", group);
                
                // Scan templates in this group directory
                for entry in walkdir::WalkDir::new(&group_dir).into_iter().filter_map(|e| e.ok()) {
                        if entry.file_type().is_file() && 
                           entry.path().extension() == Some(std::ffi::OsStr::new("lasz")) {
                            
                            // Extract the original file path from template path
                            if let Ok(relative_path) = entry.path().strip_prefix(&group_dir) {
                                let path_str = relative_path.to_string_lossy();
                                if path_str.ends_with(".lasz") {
                                    // The relative path is like "tmp/test/file.txt.lasz", we need "/tmp/test/file.txt"
                                    let original_path = PathBuf::from("/").join(&path_str[..path_str.len() - 5]);
                                    
                                    debug!("Checking template: {:?} -> original path: {:?}", entry.path(), original_path);
                                    
                                    // Check if this file is within our enrolled directory
                                    if original_path.starts_with(dir) && !original_path.exists() {
                                        missing_files.push((original_path, group.clone(), entry.path().to_path_buf()));
                                    }
                                }
                            }
                        }
                    }
            }
        }
        
        // For enrolled files, check if they exist
        for file in &enrolled_files {
            if !file.exists() {
                if let Some(group) = path_to_group_map.get(file) {
                    let template_path = enrollment_manager.get_group_template_path(group, file)?;
                    if template_path.exists() {
                        missing_files.push((file.clone(), group.clone(), template_path));
                    }
                }
            }
        }
        
        if !missing_files.is_empty() {
            println!("Found {} missing file(s):", missing_files.len());
            for (path, group, template_path) in &missing_files {
                println!("  ✗ {} (group: {})", path.display(), group);
                
                // Load group configuration to get sync action
                let (_before_trigger, _after_trigger, sync_action) = 
                    load_group_config(&config.mfs_mount, group)?;
                
                // For converge with --hard, delete the template
                if matches!(sync_action, SyncAction::Converge) {
                    std::fs::remove_file(template_path)?;
                    println!("    → Deleted template for missing file");
                }
            }
            
            // Commit template deletions if any were made
            if config.auto_commit && !missing_files.is_empty() {
                println!("\nAuto-committing template deletions...");
                if let Err(e) = commit_changes(config, Some("Removed templates for missing files"), true).await {
                    error!("Failed to auto-commit: {}", e);
                }
            }
        }
    }
    
    // Process events
    let mut debounce_buffer = HashSet::new();
    let mut template_changes = HashSet::new();
    let debounce_duration = Duration::from_millis(500);
    let mut last_event_time = std::time::Instant::now();
    let mut last_template_time = std::time::Instant::now();
    
    loop {
        // Check for events with timeout
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => {
                match event.kind {
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) => {
                        for path in event.paths {
                            // Check if it's a template change in MooseFS
                            if path.starts_with(&mfs_groups_dir) && 
                               (path.extension() == Some(std::ffi::OsStr::new("lasz")) ||
                                path.extension() == Some(std::ffi::OsStr::new("json"))) {
                                template_changes.insert(path);
                                last_template_time = std::time::Instant::now();
                            }
                            // Check if it's a file we should track
                            else if path.is_file() {
                                let mut should_track = false;
                                
                                // Check if it's an enrolled file
                                if enrolled_files.contains(&path) {
                                    should_track = true;
                                } else {
                                    // Check if it's within an enrolled directory
                                    for dir in &enrolled_directories {
                                        if path.starts_with(dir) {
                                            should_track = true;
                                            break;
                                        }
                                    }
                                }
                                
                                if should_track {
                                    debounce_buffer.insert(path);
                                    last_event_time = std::time::Instant::now();
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
                    let mut files_by_group: HashMap<String, Vec<PathBuf>> = HashMap::new();
                    
                    for path in &debounce_buffer {
                        // Find which group this file belongs to
                        let mut found_group = None;
                        
                        // Check if this is an enrolled file
                        if enrolled_files.contains(path) {
                            if let Some(group) = path_to_group_map.get(path) {
                                found_group = Some(group.clone());
                            }
                        } else {
                            // Check if it's within an enrolled directory
                            for dir in &enrolled_directories {
                                if path.starts_with(dir) {
                                    if let Some(group) = path_to_group_map.get(dir) {
                                        found_group = Some(group.clone());
                                        break;
                                    }
                                }
                            }
                        }
                        
                        if let Some(group) = found_group {
                            affected_groups.insert(group.clone());
                            files_by_group.entry(group.clone())
                                .or_insert_with(Vec::new)
                                .push(path.clone());
                            
                            let status_char = if path.exists() {
                                // Check if template exists
                                let template_path = enrollment_manager.get_group_template_path(&group, path)?;
                                if template_path.exists() {
                                    "●"  // Modified
                                } else {
                                    "?"  // New file
                                }
                            } else {
                                "✗"  // Deleted
                            };
                            println!("  {} {} (group: {})", status_char, path.display(), group);
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
                        // Process changes for each affected group
                        for group_name in affected_groups {
                            // Load group configuration to get sync action
                            let (_before_trigger, _after_trigger, sync_action) = 
                                load_group_config(&config.mfs_mount, &group_name)?;
                            
                            println!("\nProcessing group '{}' with sync action: {:?}", group_name, sync_action);
                            
                            // Process each changed file in this group according to sync action
                            if let Some(files) = files_by_group.get(&group_name) {
                                for path in files {
                                    match handle_file_change(
                                        &enrollment_manager,
                                        path,
                                        &group_name,
                                        &sync_action,
                                        hard,
                                    ).await {
                                        Ok(template_changed) => {
                                            if template_changed {
                                                println!("✓ Updated template for {}", path.display());
                                            }
                                        }
                                        Err(e) => {
                                            error!("Failed to handle change for {}: {}", path.display(), e);
                                            println!("✗ Failed to handle change for {}: {}", path.display(), e);
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        println!("Changes not applied.");
                    }
                    
                    // Clear the buffer
                    debounce_buffer.clear();
                    println!(); // Add blank line for readability
                }
                
                // Check if we should commit template changes
                if !template_changes.is_empty() && 
                   last_template_time.elapsed() > debounce_duration {
                    
                    debug!("Template changes detected: {} files", template_changes.len());
                    
                    // Auto-commit template changes
                    if config.auto_commit {
                        println!("\nAuto-committing template changes...");
                        if let Err(e) = commit_changes(config, Some("Template changes from watch mode"), true).await {
                            error!("Failed to auto-commit template changes: {}", e);
                        }
                    }
                    
                    // Clear the template changes buffer
                    template_changes.clear();
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

/// Handle a file change according to the sync action
async fn handle_file_change(
    enrollment_manager: &crate::enrollment::EnrollmentManager,
    file_path: &Path,
    group: &str,
    sync_action: &SyncAction,
    hard: bool,
) -> Result<bool> {
    use crate::template::TemplateEngine;
    
    let template_path = enrollment_manager.get_group_template_path(group, file_path)?;
    let template_exists = template_path.exists();
    let file_exists = file_path.exists();
    
    match (file_exists, template_exists, sync_action) {
        // File deleted locally
        (false, true, SyncAction::Converge) => {
            if hard {
                // Delete template if --hard is specified
                std::fs::remove_file(&template_path)?;
                info!("Deleted template for removed file: {:?}", file_path);
                Ok(true)
            } else {
                // Just show as missing without --hard
                println!("  File deleted locally: {} (template preserved)", file_path.display());
                Ok(false)
            }
        },
        
        // File deleted locally with rollback - restore from template
        (false, true, SyncAction::Rollback) => {
            // Apply template to restore file
            enrollment_manager.apply_single_template(&template_path, file_path)?;
            println!("  Restored deleted file from template: {}", file_path.display());
            Ok(false)
        },
        
        // File modified locally with converge - update template
        (true, true, SyncAction::Converge) => {
            // Read current file content
            let file_content = std::fs::read_to_string(file_path)?;
            
            // Load template to preserve variables
            let template_content = std::fs::read_to_string(&template_path)?;
            
            // Use template engine to merge changes while preserving variables
            let template_engine = TemplateEngine::new()?;
            let updated_template = template_engine.merge_file_changes_to_template(
                &template_content,
                &file_content,
            )?;
            
            // Write updated template
            std::fs::write(&template_path, &updated_template)?;
            info!("Updated template with local changes: {:?}", template_path);
            Ok(true)
        },
        
        // File modified locally with rollback - restore from template
        (true, true, SyncAction::Rollback) => {
            // Apply template to revert changes
            enrollment_manager.apply_single_template(&template_path, file_path)?;
            println!("  Rolled back local changes from template: {}", file_path.display());
            Ok(false)
        },
        
        // File modified with freeze - do nothing
        (true, true, SyncAction::Freeze) => {
            println!("  Frozen file, changes ignored: {}", file_path.display());
            Ok(false)
        },
        
        // File modified with drift - track but don't sync
        (true, true, SyncAction::Drift) => {
            println!("  Drift allowed, changes tracked: {}", file_path.display());
            // TODO: Record drift in audit log
            Ok(false)
        },
        
        // Template deleted but file exists
        (true, false, _) => {
            if hard {
                // Delete local file if --hard is specified
                std::fs::remove_file(file_path)?;
                println!("  Deleted local file (template was removed): {}", file_path.display());
            } else {
                println!("  Template missing for: {} (local file preserved)", file_path.display());
            }
            Ok(false)
        },
        
        // Both deleted - nothing to do
        (false, false, _) => Ok(false),
        
        // New file created locally
        (true, false, SyncAction::Converge) => {
            // This is handled separately for new files in watched directories
            Ok(false)
        },
        
        _ => Ok(false),
    }
}

/// Load group configuration including triggers and sync action
fn load_group_config(mfs_mount: &Path, group: &str) -> Result<(Option<String>, Option<String>, SyncAction)> {
    use serde::{Serialize, Deserialize};
    
    #[derive(Serialize, Deserialize, Default)]
    struct GroupConfig {
        #[serde(skip_serializing_if = "Option::is_none")]
        before_trigger: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        after_trigger: Option<String>,
        sync_action: String,
    }
    
    let config_path = mfs_mount
        .join("groups")
        .join(group)
        .join("config.json");
    
    if !config_path.exists() {
        // Default to converge if no config exists
        return Ok((None, None, SyncAction::Converge));
    }
    
    let content = std::fs::read_to_string(&config_path)?;
    let config: GroupConfig = serde_json::from_str(&content)?;
    
    let sync_action = match config.sync_action.as_str() {
        "rollback" => SyncAction::Rollback,
        "freeze" => SyncAction::Freeze,
        "drift" => SyncAction::Drift,
        _ => SyncAction::Converge,
    };
    
    Ok((config.before_trigger, config.after_trigger, sync_action))
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