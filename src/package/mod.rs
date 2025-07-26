use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};
use tracing::{info, warn, error, debug};
use serde::{Serialize, Deserialize};

use crate::error::{Result, LaszooError};

/// Package operation types
#[derive(Debug, Clone, PartialEq)]
pub enum PackageOperation {
    /// ^package - Upgrade package
    Upgrade { name: String, post_action: Option<String> },
    /// ++upgrade - Upgrade all packages with start/end actions
    UpgradeAll { start_action: Option<String>, end_action: Option<String> },
    /// +package - Install package
    Install { name: String },
    /// =package - Keep package (don't auto-install/remove)
    Keep { name: String },
    /// !package - Remove package
    Remove { name: String },
    /// !!!package - Purge package
    Purge { name: String },
}

/// Package manager for handling package operations
pub struct PackageManager {
    mfs_mount: PathBuf,
}

impl PackageManager {
    pub fn new(mfs_mount: PathBuf) -> Self {
        Self { mfs_mount }
    }

    /// Get the packages.conf path for a group
    pub fn get_group_packages_path(&self, group: &str) -> PathBuf {
        self.mfs_mount
            .join("groups")
            .join(group)
            .join("etc")
            .join("laszoo")
            .join("packages.conf")
    }

    /// Get the packages.conf path for a machine
    pub fn get_machine_packages_path(&self, hostname: &str) -> PathBuf {
        self.mfs_mount
            .join("machines")
            .join(hostname)
            .join("etc")
            .join("laszoo")
            .join("packages.conf")
    }

    /// Parse a packages.conf file
    pub fn parse_packages_conf(&self, content: &str) -> Result<Vec<PackageOperation>> {
        let mut operations = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            
            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(op) = self.parse_package_line(line)? {
                operations.push(op);
            }
        }

        Ok(operations)
    }

    /// Parse a single package line
    fn parse_package_line(&self, line: &str) -> Result<Option<PackageOperation>> {
        // Handle upgrade all: ++upgrade or ++upgrade --start cmd --end cmd
        if line.starts_with("++upgrade") {
            let mut start_action = None;
            let mut end_action = None;
            
            // Parse --start and --end flags
            if line.contains("--start") || line.contains("--end") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                let mut i = 0;
                while i < parts.len() {
                    if parts[i] == "--start" && i + 1 < parts.len() {
                        // Collect all parts until next flag or end
                        let mut cmd_parts = vec![];
                        i += 1;
                        while i < parts.len() && !parts[i].starts_with("--") {
                            cmd_parts.push(parts[i]);
                            i += 1;
                        }
                        start_action = Some(cmd_parts.join(" "));
                    } else if parts[i] == "--end" && i + 1 < parts.len() {
                        // Collect all parts until next flag or end
                        let mut cmd_parts = vec![];
                        i += 1;
                        while i < parts.len() && !parts[i].starts_with("--") {
                            cmd_parts.push(parts[i]);
                            i += 1;
                        }
                        end_action = Some(cmd_parts.join(" "));
                    } else {
                        i += 1;
                    }
                }
            }
            
            return Ok(Some(PackageOperation::UpgradeAll { start_action, end_action }));
        }
        
        // Handle upgrade with post-action: ^nginx --upgrade=systemctl restart nginx
        if line.starts_with('^') {
            let parts: Vec<&str> = line[1..].splitn(2, "--upgrade=").collect();
            let name = parts[0].trim().to_string();
            let post_action = parts.get(1).map(|s| s.trim().to_string());
            
            return Ok(Some(PackageOperation::Upgrade { name, post_action }));
        }

        // Handle install: +package
        if line.starts_with('+') && !line.starts_with("++") {
            let name = line[1..].trim().to_string();
            return Ok(Some(PackageOperation::Install { name }));
        }

        // Handle keep: =package
        if line.starts_with('=') {
            let name = line[1..].trim().to_string();
            return Ok(Some(PackageOperation::Keep { name }));
        }

        // Handle purge: !!!package
        if line.starts_with("!!!") {
            let name = line[3..].trim().to_string();
            return Ok(Some(PackageOperation::Purge { name }));
        }

        // Handle remove: !package
        if line.starts_with('!') {
            let name = line[1..].trim().to_string();
            return Ok(Some(PackageOperation::Remove { name }));
        }

        warn!("Ignoring invalid package line: {}", line);
        Ok(None)
    }

    /// Load package operations for a group and optionally a specific machine
    pub fn load_package_operations(&self, group: &str, hostname: Option<&str>) -> Result<Vec<PackageOperation>> {
        let mut operations = Vec::new();
        let mut operation_map: HashMap<String, PackageOperation> = HashMap::new();

        // First, load group packages
        let group_path = self.get_group_packages_path(group);
        if group_path.exists() {
            debug!("Loading group packages from: {}", group_path.display());
            let content = std::fs::read_to_string(&group_path)?;
            let group_ops = self.parse_packages_conf(&content)?;
            
            // Add to map
            for op in group_ops {
                match &op {
                    PackageOperation::UpgradeAll { .. } => {
                        // UpgradeAll is a special operation that doesn't have a package name
                        operations.push(op);
                    }
                    _ => {
                        let name = match &op {
                            PackageOperation::Upgrade { name, .. } => name,
                            PackageOperation::Install { name } => name,
                            PackageOperation::Keep { name } => name,
                            PackageOperation::Remove { name } => name,
                            PackageOperation::Purge { name } => name,
                            PackageOperation::UpgradeAll { .. } => unreachable!(),
                        };
                        operation_map.insert(name.clone(), op);
                    }
                }
            }
        }

        // Then, override with machine-specific packages if provided
        if let Some(host) = hostname {
            let machine_path = self.get_machine_packages_path(host);
            if machine_path.exists() {
                debug!("Loading machine packages from: {}", machine_path.display());
                let content = std::fs::read_to_string(&machine_path)?;
                let machine_ops = self.parse_packages_conf(&content)?;
                
                // Override group operations
                for op in machine_ops {
                    match &op {
                        PackageOperation::UpgradeAll { .. } => {
                            // UpgradeAll is a special operation that doesn't have a package name
                            operations.push(op);
                        }
                        _ => {
                            let name = match &op {
                                PackageOperation::Upgrade { name, .. } => name,
                                PackageOperation::Install { name } => name,
                                PackageOperation::Keep { name } => name,
                                PackageOperation::Remove { name } => name,
                                PackageOperation::Purge { name } => name,
                                PackageOperation::UpgradeAll { .. } => unreachable!(),
                            };
                            operation_map.insert(name.clone(), op);
                        }
                    }
                }
            }
        }

        // Convert map back to vec
        operations.extend(operation_map.into_values());
        Ok(operations)
    }

    /// Add packages to a group's packages.conf
    pub fn add_packages_to_group(&self, group: &str, packages: &[String], upgrade: bool) -> Result<()> {
        let packages_path = self.get_group_packages_path(group);
        
        // Ensure directory exists
        if let Some(parent) = packages_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Load existing packages
        let mut existing_ops = if packages_path.exists() {
            let content = std::fs::read_to_string(&packages_path)?;
            self.parse_packages_conf(&content)?
        } else {
            Vec::new()
        };

        // Create a set of existing package names for deduplication
        let mut existing_names: HashSet<String> = existing_ops.iter().filter_map(|op| {
            match op {
                PackageOperation::Upgrade { name, .. } => Some(name.clone()),
                PackageOperation::Install { name } => Some(name.clone()),
                PackageOperation::Keep { name } => Some(name.clone()),
                PackageOperation::Remove { name } => Some(name.clone()),
                PackageOperation::Purge { name } => Some(name.clone()),
                PackageOperation::UpgradeAll { .. } => None, // UpgradeAll doesn't have a package name
            }
        }).collect();

        // Add new packages
        for package in packages {
            if !existing_names.contains(package) {
                let op = if upgrade {
                    PackageOperation::Upgrade { name: package.clone(), post_action: None }
                } else {
                    PackageOperation::Install { name: package.clone() }
                };
                existing_ops.push(op);
                existing_names.insert(package.clone());
            }
        }

        // Write back to file
        self.write_packages_conf(&packages_path, &existing_ops)?;
        
        info!("Added {} packages to group '{}'", packages.len(), group);
        Ok(())
    }

    /// Write package operations to a packages.conf file
    fn write_packages_conf(&self, path: &Path, operations: &[PackageOperation]) -> Result<()> {
        let mut content = String::new();
        
        // Add header
        content.push_str("# Laszoo Package Configuration\n");
        content.push_str("# Syntax:\n");
        content.push_str("# ^package - Upgrade package\n");
        content.push_str("# ^package --upgrade=command - Upgrade with post-action\n");
        content.push_str("# ++upgrade - Upgrade all packages\n");
        content.push_str("# ++upgrade --start cmd --end cmd - Upgrade all with start/end actions\n");
        content.push_str("# +package - Install package\n");
        content.push_str("# =package - Keep package (don't auto-install/remove)\n");
        content.push_str("# !package - Remove package\n");
        content.push_str("# !!!package - Purge package\n\n");

        // Write operations
        for op in operations {
            match op {
                PackageOperation::Upgrade { name, post_action } => {
                    if let Some(action) = post_action {
                        content.push_str(&format!("^{} --upgrade={}\n", name, action));
                    } else {
                        content.push_str(&format!("^{}\n", name));
                    }
                }
                PackageOperation::UpgradeAll { start_action, end_action } => {
                    let mut line = String::from("++upgrade");
                    if let Some(start) = start_action {
                        line.push_str(&format!(" --start {}", start));
                    }
                    if let Some(end) = end_action {
                        line.push_str(&format!(" --end {}", end));
                    }
                    content.push_str(&format!("{}\n", line));
                }
                PackageOperation::Install { name } => {
                    content.push_str(&format!("+{}\n", name));
                }
                PackageOperation::Keep { name } => {
                    content.push_str(&format!("={}\n", name));
                }
                PackageOperation::Remove { name } => {
                    content.push_str(&format!("!{}\n", name));
                }
                PackageOperation::Purge { name } => {
                    content.push_str(&format!("!!!{}\n", name));
                }
            }
        }

        std::fs::write(path, content)?;
        Ok(())
    }

    /// Detect the package manager on the current system
    pub fn detect_package_manager() -> Result<PackageManagerType> {
        // Check for various package managers
        if std::path::Path::new("/usr/bin/apt-get").exists() {
            Ok(PackageManagerType::Apt)
        } else if std::path::Path::new("/usr/bin/yum").exists() {
            Ok(PackageManagerType::Yum)
        } else if std::path::Path::new("/usr/bin/dnf").exists() {
            Ok(PackageManagerType::Dnf)
        } else if std::path::Path::new("/usr/bin/pacman").exists() {
            Ok(PackageManagerType::Pacman)
        } else if std::path::Path::new("/usr/bin/zypper").exists() {
            Ok(PackageManagerType::Zypper)
        } else if std::path::Path::new("/usr/bin/apk").exists() {
            Ok(PackageManagerType::Apk)
        } else {
            Err(LaszooError::Other("No supported package manager found".to_string()))
        }
    }

    /// Apply package operations on the local system
    pub async fn apply_operations(&self, operations: &[PackageOperation]) -> Result<()> {
        let pkg_mgr = Self::detect_package_manager()?;
        
        for op in operations {
            match op {
                PackageOperation::Install { name } => {
                    info!("Installing package: {}", name);
                    self.install_package(&pkg_mgr, name).await?;
                }
                PackageOperation::Upgrade { name, post_action } => {
                    info!("Upgrading package: {}", name);
                    self.upgrade_package(&pkg_mgr, name).await?;
                    
                    if let Some(action) = post_action {
                        info!("Running post-upgrade action: {}", action);
                        self.run_command(action).await?;
                    }
                }
                PackageOperation::UpgradeAll { start_action, end_action } => {
                    if let Some(action) = start_action {
                        info!("Running pre-upgrade action: {}", action);
                        self.run_command(action).await?;
                    }
                    
                    info!("Upgrading all packages");
                    self.system_upgrade(&pkg_mgr).await?;
                    
                    if let Some(action) = end_action {
                        info!("Running post-upgrade action: {}", action);
                        self.run_command(action).await?;
                    }
                }
                PackageOperation::Remove { name } => {
                    info!("Removing package: {}", name);
                    self.remove_package(&pkg_mgr, name).await?;
                }
                PackageOperation::Purge { name } => {
                    info!("Purging package: {}", name);
                    self.purge_package(&pkg_mgr, name).await?;
                }
                PackageOperation::Keep { name } => {
                    debug!("Keeping package: {} (no action needed)", name);
                }
            }
        }

        Ok(())
    }

    /// Install a package using the appropriate package manager
    async fn install_package(&self, pkg_mgr: &PackageManagerType, package: &str) -> Result<()> {
        let cmd = match pkg_mgr {
            PackageManagerType::Apt => format!("apt-get install -y {}", package),
            PackageManagerType::Yum => format!("yum install -y {}", package),
            PackageManagerType::Dnf => format!("dnf install -y {}", package),
            PackageManagerType::Pacman => format!("pacman -S --noconfirm {}", package),
            PackageManagerType::Zypper => format!("zypper install -y {}", package),
            PackageManagerType::Apk => format!("apk add {}", package),
        };

        self.run_command(&cmd).await
    }

    /// Upgrade a package
    async fn upgrade_package(&self, pkg_mgr: &PackageManagerType, package: &str) -> Result<()> {
        let cmd = match pkg_mgr {
            PackageManagerType::Apt => format!("apt-get install --only-upgrade -y {}", package),
            PackageManagerType::Yum => format!("yum update -y {}", package),
            PackageManagerType::Dnf => format!("dnf upgrade -y {}", package),
            PackageManagerType::Pacman => format!("pacman -S --noconfirm {}", package),
            PackageManagerType::Zypper => format!("zypper update -y {}", package),
            PackageManagerType::Apk => format!("apk upgrade {}", package),
        };

        self.run_command(&cmd).await
    }

    /// Remove a package
    async fn remove_package(&self, pkg_mgr: &PackageManagerType, package: &str) -> Result<()> {
        let cmd = match pkg_mgr {
            PackageManagerType::Apt => format!("apt-get remove -y {}", package),
            PackageManagerType::Yum => format!("yum remove -y {}", package),
            PackageManagerType::Dnf => format!("dnf remove -y {}", package),
            PackageManagerType::Pacman => format!("pacman -R --noconfirm {}", package),
            PackageManagerType::Zypper => format!("zypper remove -y {}", package),
            PackageManagerType::Apk => format!("apk del {}", package),
        };

        self.run_command(&cmd).await
    }

    /// Purge a package
    async fn purge_package(&self, pkg_mgr: &PackageManagerType, package: &str) -> Result<()> {
        let cmd = match pkg_mgr {
            PackageManagerType::Apt => format!("apt-get purge -y {}", package),
            PackageManagerType::Yum => format!("yum remove -y {}", package), // No purge in yum
            PackageManagerType::Dnf => format!("dnf remove -y {}", package), // No purge in dnf
            PackageManagerType::Pacman => format!("pacman -Rn --noconfirm {}", package),
            PackageManagerType::Zypper => format!("zypper remove -y --clean-deps {}", package),
            PackageManagerType::Apk => format!("apk del --purge {}", package),
        };

        self.run_command(&cmd).await
    }

    /// Run a system upgrade
    pub async fn system_upgrade(&self, pkg_mgr: &PackageManagerType) -> Result<()> {
        let cmd = match pkg_mgr {
            PackageManagerType::Apt => "apt-get upgrade -y",
            PackageManagerType::Yum => "yum upgrade -y",
            PackageManagerType::Dnf => "dnf upgrade -y",
            PackageManagerType::Pacman => "pacman -Syu --noconfirm",
            PackageManagerType::Zypper => "zypper update -y",
            PackageManagerType::Apk => "apk upgrade",
        };

        self.run_command(cmd).await
    }

    /// Run a shell command
    async fn run_command(&self, cmd: &str) -> Result<()> {
        use tokio::process::Command;
        
        debug!("Running command: {}", cmd);
        
        let output = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .await?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(LaszooError::Other(format!("Command failed: {}", stderr)))
        }
    }
}

/// Supported package manager types
#[derive(Debug, Clone, Copy)]
pub enum PackageManagerType {
    Apt,
    Yum,
    Dnf,
    Pacman,
    Zypper,
    Apk,
}