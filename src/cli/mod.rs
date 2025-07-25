use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "laszoo")]
#[command(about = "Distributed configuration management with MooseFS", long_about = None)]
#[command(version)]
pub struct Cli {
    /// Path to configuration file
    #[arg(short, long, value_name = "FILE", env = "LASZOO_CONFIG")]
    pub config: Option<PathBuf>,

    /// Enable verbose output
    #[arg(short, long)]
    pub verbose: bool,

    /// Perform a dry run without making changes
    #[arg(long)]
    pub dry_run: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Enroll files or directories into Laszoo management
    Enroll {
        /// Group name to enroll files into
        group: String,
        
        /// Paths to files or directories to enroll
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        
        /// Force re-enrollment if already enrolled
        #[arg(short, long)]
        force: bool,
        
        /// Include hidden files when enrolling directories
        #[arg(long)]
        include_hidden: bool,
        
        /// Create machine-specific enrollment
        #[arg(short, long)]
        machine: bool,
        
        /// Create hybrid enrollment (machine template provides values for group template)
        #[arg(long, conflicts_with = "machine")]
        hybrid: bool,
        
        /// Command to run before applying changes
        #[arg(long, value_name = "COMMAND")]
        before: Option<String>,
        
        /// Command to run after applying changes
        #[arg(long, value_name = "COMMAND")]
        after: Option<String>,
        
        /// Sync action: converge (default), rollback, freeze, or drift
        #[arg(long, default_value = "converge")]
        action: SyncAction,
    },
    
    /// Unenroll files from Laszoo management
    Unenroll {
        /// Group name to unenroll files from (if provided without paths, unenrolls all files from group)
        #[arg(required_unless_present = "paths")]
        group: Option<String>,
        
        /// Paths to files to unenroll
        paths: Vec<PathBuf>,
    },
    
    /// Synchronize configuration files
    Sync {
        /// Specific group to sync (all groups if not specified)
        #[arg(short, long)]
        group: Option<String>,
        
        /// Sync strategy to use
        #[arg(short, long, value_enum, default_value = "auto")]
        strategy: SyncStrategy,
    },
    
    /// Show status of enrolled files and synchronization
    Status {
        /// Show detailed status information
        #[arg(short, long)]
        detailed: bool,
    },
    
    /// Rollback changes to configuration files
    Rollback {
        /// File or group to rollback
        target: String,
        
        /// Number of commits to rollback
        #[arg(short, long, default_value = "1")]
        commits: u32,
    },
    
    /// Apply templates from a group to the local system
    Apply {
        /// Group name to apply templates from
        group: String,
        
        /// Apply only specific files (all if not specified)
        #[arg(short, long)]
        files: Vec<PathBuf>,
    },
    
    /// Manage group membership
    Group {
        /// Group name
        name: String,
        
        #[command(subcommand)]
        command: GroupCommands,
    },
    
    /// List all groups
    Groups {
        #[command(subcommand)]
        command: GroupsCommands,
    },
    
    /// Initialize Laszoo in current directory
    Init {
        /// Shared filesystem mount point
        #[arg(long, default_value = "/mnt/laszoo")]
        mfs_mount: PathBuf,
    },
    
    /// Commit changes with AI-generated message
    Commit {
        /// Additional context for commit message generation
        #[arg(short, long)]
        message: Option<String>,
        
        /// Stage all changes before committing
        #[arg(short, long)]
        all: bool,
    },
    
    /// Watch for file changes using filesystem events
    Watch {
        /// Specific group to watch (all groups if not specified)
        #[arg(short, long)]
        group: Option<String>,
        
        /// Debounce interval in seconds (deprecated, kept for compatibility)
        #[arg(short, long, default_value = "1", hide = true)]
        interval: u64,
        
        /// Apply changes automatically without prompting
        #[arg(short, long)]
        auto: bool,
        
        /// Propagate deletions (delete local files if templates are deleted, delete templates if local files are deleted)
        #[arg(long)]
        hard: bool,
    },
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum SyncStrategy {
    /// Automatically choose strategy based on majority
    Auto,
    /// Rollback minority changes to majority configuration
    Rollback,
    /// Forward local changes to other hosts
    Forward,
}

#[derive(clap::ValueEnum, Clone, Debug, Default)]
pub enum SyncAction {
    /// Capture changes from local system and apply to template (default)
    #[default]
    Converge,
    /// Rollback local changes to match template
    Rollback,
    /// Freeze local file, preventing further template updates
    Freeze,
    /// Allow drift but track differences for auditing
    Drift,
}

#[derive(Subcommand, Debug)]
pub enum GroupCommands {
    /// Add a machine to this group
    Add {
        /// Machine name to add (current machine if not specified)
        machine: Option<String>,
    },
    
    /// Remove a machine from this group
    Remove {
        /// Machine name to remove (current machine if not specified)
        machine: Option<String>,
        
        /// Keep the group even if it's empty
        #[arg(long)]
        keep: bool,
    },
    
    /// List machines in this group
    List,
    
    /// Rename this group
    Rename {
        /// New name for the group
        new_name: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum GroupsCommands {
    /// List all groups
    List,
}