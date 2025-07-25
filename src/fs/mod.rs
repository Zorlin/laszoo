use std::path::{Path, PathBuf};
use crate::error::{LaszooError, Result};

/// Check if a path is within a supported distributed filesystem
pub fn is_distributed_fs_mounted(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    
    // Check /proc/mounts for supported filesystem entries
    let mounts = std::fs::read_to_string("/proc/mounts")?;
    let path_str = path.to_string_lossy();
    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    
    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let mount_point = parts[1];
            let fs_type = parts[2];
            
            // Check if our path is within this mount point
            if path_str.starts_with(mount_point) || canonical_path.starts_with(mount_point) {
                match fs_type {
                    // MooseFS variants
                    "fuse.mfs" | "fuse.moosefs" | "fuse.mfsmount" => return Ok(true),
                    // CephFS
                    "ceph" => return Ok(true),
                    // Accept any FUSE mount that could be distributed
                    _ if fs_type.starts_with("fuse") => return Ok(true),
                    _ => {}
                }
            }
        }
    }
    
    Ok(false)
}

/// Check if a path is any FUSE mount
fn is_fuse_mount(path: &Path) -> Result<bool> {
    let mounts = std::fs::read_to_string("/proc/mounts")?;
    let path_str = path.to_string_lossy();
    
    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let mount_point = parts[1];
            let fs_type = parts[2];
            
            if mount_point == path_str && fs_type.starts_with("fuse") {
                return Ok(true);
            }
        }
    }
    
    Ok(false)
}

/// Ensure the distributed filesystem mount is available
pub fn ensure_distributed_fs_available(mount_path: &Path) -> Result<()> {
    if !mount_path.exists() {
        // Try to create the directory if it doesn't exist
        if let Some(parent) = mount_path.parent() {
            if parent.exists() && is_distributed_fs_mounted(parent)? {
                std::fs::create_dir_all(mount_path)?;
            } else {
                return Err(LaszooError::DistributedFSNotAvailable { 
                    path: mount_path.to_path_buf() 
                });
            }
        } else {
            return Err(LaszooError::DistributedFSNotAvailable { 
                path: mount_path.to_path_buf() 
            });
        }
    }
    
    // Check if the path is within a distributed filesystem
    if !is_distributed_fs_mounted(mount_path)? {
        // For development/testing, accept any directory
        if !mount_path.is_dir() {
            return Err(LaszooError::DistributedFSNotAvailable { 
                path: mount_path.to_path_buf() 
            });
        }
    }
    
    Ok(())
}

/// Get the Laszoo base directory within MooseFS
pub fn get_laszoo_base(mfs_mount: &Path, _laszoo_dir: &str) -> PathBuf {
    mfs_mount.to_path_buf()
}

/// Get the machines directory
pub fn get_machines_dir(mfs_mount: &Path, _laszoo_dir: &str) -> PathBuf {
    mfs_mount.join("machines")
}

/// Get the groups directory  
pub fn get_groups_dir(mfs_mount: &Path, _laszoo_dir: &str) -> PathBuf {
    mfs_mount.join("groups")
}

/// Get a specific machine directory
pub fn get_machine_dir(mfs_mount: &Path, _laszoo_dir: &str, hostname: &str) -> PathBuf {
    mfs_mount.join("machines").join(hostname)
}

/// Get a specific machine file path
pub fn get_machine_file_path(mfs_mount: &Path, _laszoo_dir: &str, hostname: &str, file_path: &Path) -> Result<PathBuf> {
    let machine_dir = mfs_mount.join("machines").join(hostname);
    
    // Strip leading / from file path
    let relative_path = file_path.strip_prefix("/").unwrap_or(file_path);
    
    Ok(machine_dir.join(relative_path))
}

/// Get the host-specific directory
pub fn get_host_dir(mfs_mount: &Path, _laszoo_dir: &str, hostname: &str) -> PathBuf {
    mfs_mount.join("machines").join(hostname)
}

/// Get the group-specific directory
pub fn get_group_dir(mfs_mount: &Path, _laszoo_dir: &str, group_name: &str) -> PathBuf {
    mfs_mount.join("groups").join(group_name)
}

/// Get the path where a file's template would be stored in MooseFS for a specific host
pub fn get_host_file_path(mfs_mount: &Path, _laszoo_dir: &str, hostname: &str, file_path: &Path) -> Result<PathBuf> {
    let host_dir = mfs_mount.join("machines").join(hostname);
    
    // Convert absolute path to relative for storage
    let relative_path = if file_path.is_absolute() {
        // Remove leading slash
        file_path.strip_prefix("/").unwrap_or(file_path)
    } else {
        file_path
    };
    
    Ok(host_dir.join(relative_path))
}

/// Get the path where a group template would be stored
pub fn get_group_template_path(mfs_mount: &Path, _laszoo_dir: &str, group_name: &str, file_path: &Path) -> Result<PathBuf> {
    let group_dir = mfs_mount.join("groups").join(group_name);
    
    // Convert absolute path to relative for storage
    let relative_path = if file_path.is_absolute() {
        // Remove leading slash
        file_path.strip_prefix("/").unwrap_or(file_path)
    } else {
        file_path
    };
    
    Ok(group_dir.join(relative_path).with_extension("lasz"))
}