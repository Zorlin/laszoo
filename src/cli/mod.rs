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
        
        /// Disable automatic enrollment of new files in watched directories
        #[arg(long)]
        no_autoenroll: bool,
        
        /// Command to run before applying changes
        #[arg(long, value_name = "COMMAND", alias = "start")]
        before: Option<String>,
        
        /// Command to run after applying changes
        #[arg(long, value_name = "COMMAND", alias = "end")]
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
    
    /// Install packages on all systems in a group
    Install {
        /// Group name followed by package names to install
        /// Example: laszoo install webservers nginx php mysql
        #[arg(required = true, num_args = 2..)]
        args: Vec<String>,
        
        /// Command to run after installing/updating each package
        #[arg(long)]
        after: Option<String>,
    },
    
    /// Uninstall packages from all systems in a group
    Uninstall {
        /// Group name followed by package names to uninstall
        /// Example: laszoo uninstall webservers nginx php
        #[arg(required = true, num_args = 2..)]
        args: Vec<String>,
        
        /// Command to run before uninstalling packages
        #[arg(long)]
        before: Option<String>,
        
        /// Command to run after uninstalling packages
        #[arg(long)]
        after: Option<String>,
        
        /// Purge packages (remove configuration files)
        #[arg(long)]
        purge: bool,
    },
    
    /// Apply package updates to all systems in a group
    Patch {
        /// Group name to patch
        group: String,
        
        /// Command to run before patching
        #[arg(long)]
        before: Option<String>,
        
        /// Command to run after patching
        #[arg(long)]
        after: Option<String>,
        
        /// Apply patches in a rolling fashion (one machine at a time)
        #[arg(long)]
        rolling: bool,
        
        /// Use full-upgrade instead of upgrade (for Proxmox/Debian)
        #[arg(long)]
        full_upgrade: bool,
        
        /// Use dist-upgrade instead of upgrade (for Proxmox/Debian)
        #[arg(long)]
        dist_upgrade: bool,
    },
    
    /// Manage Laszoo as a system service
    Service {
        #[command(subcommand)]
        command: ServiceCommands,
    },
    
    /// Launch the web UI
    WebUI {
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,
        
        /// Bind address
        #[arg(short, long, default_value = "0.0.0.0")]
        bind: String,
    },
    
    /// Show differences between local files and templates
    Diff {
        /// Group name to check differences for
        #[arg(short, long)]
        group: Option<String>,
        
        /// Specific file to check (all files if not specified)
        #[arg(short, long)]
        file: Option<PathBuf>,
        
        /// Show unified diff output
        #[arg(short, long)]
        unified: bool,
        
        /// Context lines for unified diff
        #[arg(short, long, default_value = "3")]
        context: usize,
    },
    
    /// Manage Jetpack playbooks
    Playbook {
        #[command(subcommand)]
        command: PlaybookCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum PlaybookCommands {
    /// Add a playbook to the distributed storage
    Add {
        /// Path to the playbook directory or file
        path: PathBuf,
        
        /// Optional name for the playbook (defaults to directory name)
        #[arg(short, long)]
        name: Option<String>,
        
        /// Custom path in the mount point to store the playbook
        #[arg(short, long)]
        path_override: Option<PathBuf>,
    },
    
    /// List available playbooks
    List,
    
    /// Run a playbook
    Run {
        /// Name or path of the playbook to run
        name: String,
        
        /// Custom inventory path (defaults to /mnt/laszoo/inventory/jetpack)
        #[arg(short, long)]
        inventory: Option<PathBuf>,
        
        /// Additional arguments to pass to the playbook
        #[arg(last = true)]
        args: Vec<String>,
    },
    
    /// Remove a playbook from distributed storage
    Remove {
        /// Name of the playbook to remove
        name: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ServiceCommands {
    /// Install Laszoo as a systemd service
    Install {
        /// Enable hard mode (propagate deletions)
        #[arg(long)]
        hard: bool,
        
        /// User to run service as
        #[arg(long, default_value = "root")]
        user: String,
        
        /// Additional arguments to pass to laszoo watch
        #[arg(long)]
        extra_args: Option<String>,
    },
    
    /// Uninstall the Laszoo systemd service
    Uninstall,
    
    /// Show status of the Laszoo service
    Status,
    
    /// Start the Laszoo service
    Start,
    
    /// Stop the Laszoo service
    Stop,
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