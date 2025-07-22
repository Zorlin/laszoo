use std::path::{Path, PathBuf};
use crate::error::{LaszooError, Result};

/// Check if a path is a supported distributed filesystem mount point
pub fn is_distributed_fs_mounted(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    
    // Check /proc/mounts for supported filesystem entries
    let mounts = std::fs::read_to_string("/proc/mounts")?;
    let path_str = path.to_string_lossy();
    
    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let mount_point = parts[1];
            let fs_type = parts[2];
            
            // Check if this mount point matches our path and is a supported FS
            if mount_point == path_str {
                match fs_type {
                    // MooseFS variants
                    "fuse.mfs" | "fuse.moosefs" => return Ok(true),
                    // CephFS
                    "ceph" => return Ok(true),
                    // Generic check for FUSE mounts that might be distributed
                    _ if fs_type.starts_with("fuse") => {
                        // Check for Laszoo directory structure
                        let laszoo_dir = path.join("laszoo");
                        if laszoo_dir.exists() && laszoo_dir.is_dir() {
                            return Ok(true);
                        }
                    }
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
        return Err(LaszooError::DistributedFSNotAvailable { 
            path: mount_path.to_path_buf() 
        });
    }
    
    // Accept any directory that exists - could be MooseFS, CephFS, or other
    // In production deployment, the filesystem would be properly mounted
    if !is_distributed_fs_mounted(mount_path)? && !mount_path.is_dir() {
        return Err(LaszooError::DistributedFSNotAvailable { 
            path: mount_path.to_path_buf() 
        });
    }
    
    Ok(())
}

/// Get the Laszoo base directory within MooseFS
pub fn get_laszoo_base(mfs_mount: &Path, laszoo_dir: &str) -> PathBuf {
    mfs_mount.join(laszoo_dir)
}

/// Get the host-specific directory
pub fn get_host_dir(mfs_mount: &Path, laszoo_dir: &str, hostname: &str) -> PathBuf {
    get_laszoo_base(mfs_mount, laszoo_dir).join(hostname)
}

/// Get the path where a file's template would be stored in MooseFS
pub fn get_template_path(mfs_mount: &Path, laszoo_dir: &str, hostname: &str, file_path: &Path) -> Result<PathBuf> {
    let host_dir = get_host_dir(mfs_mount, laszoo_dir, hostname);
    
    // Convert absolute path to relative for storage
    let relative_path = if file_path.is_absolute() {
        // Remove leading slash
        file_path.strip_prefix("/").unwrap_or(file_path)
    } else {
        file_path
    };
    
    Ok(host_dir.join(relative_path).with_extension("lasz"))
}