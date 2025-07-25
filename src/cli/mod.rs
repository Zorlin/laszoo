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
    },
    
    /// Unenroll files from Laszoo management
    Unenroll {
        /// Paths to files to unenroll
        #[arg(required = true)]
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
        /// Show status for specific group
        #[arg(short, long)]
        group: Option<String>,
        
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
    
    /// Manage groups
    Group {
        #[command(subcommand)]
        command: GroupCommands,
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

#[derive(Subcommand, Debug)]
pub enum GroupCommands {
    /// Create a new group
    Create {
        /// Name of the group to create
        name: String,
        
        /// Description of the group
        #[arg(short, long)]
        description: Option<String>,
    },
    
    /// List all groups
    List,
    
    /// Delete a group
    Delete {
        /// Name of the group to delete
        name: String,
        
        /// Force deletion even if group has enrolled files
        #[arg(short, long)]
        force: bool,
    },
    
    /// Add host to group
    AddHost {
        /// Group name
        group: String,
        
        /// Host to add (current host if not specified)
        #[arg(long)]
        host: Option<String>,
    },
    
    /// Remove host from group
    RemoveHost {
        /// Group name
        group: String,
        
        /// Host to remove (current host if not specified)
        #[arg(long)]
        host: Option<String>,
    },
}