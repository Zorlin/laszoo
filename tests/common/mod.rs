use std::path::{Path, PathBuf};
use std::fs;
use std::process::Command;
use uuid::Uuid;

pub struct TestEnvironment {
    pub test_dir: PathBuf,
    pub mfs_mount: PathBuf,
    pub hostname: String,
    pub original_hostname: String,
}

impl TestEnvironment {
    pub fn new(test_name: &str) -> Self {
        // Create a unique test directory
        let test_id = Uuid::new_v4().to_string();
        let test_dir = PathBuf::from("/tmp").join(format!("laszoo-test-{}-{}", test_name, test_id));
        fs::create_dir_all(&test_dir).expect("Failed to create test directory");
        
        // Create a mock MooseFS mount point
        let mfs_mount = test_dir.join("mfs");
        fs::create_dir_all(&mfs_mount).expect("Failed to create MFS mount");
        
        // Get original hostname
        let original_hostname = gethostname::gethostname()
            .to_string_lossy()
            .to_string();
        
        // Generate a test hostname
        let hostname = format!("test-{}", &test_id[..8]);
        
        Self {
            test_dir,
            mfs_mount,
            hostname,
            original_hostname,
        }
    }
    
    pub fn setup_git(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Initialize git repo in MFS mount
        Command::new("git")
            .args(&["init"])
            .current_dir(&self.mfs_mount)
            .output()?;
            
        // Configure git
        Command::new("git")
            .args(&["config", "user.name", "Laszoo Test"])
            .current_dir(&self.mfs_mount)
            .output()?;
            
        Command::new("git")
            .args(&["config", "user.email", "test@laszoo.local"])
            .current_dir(&self.mfs_mount)
            .output()?;
            
        Ok(())
    }
    
    pub fn create_test_file(&self, path: &str, content: &str) -> PathBuf {
        let full_path = self.test_dir.join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent directory");
        }
        fs::write(&full_path, content).expect("Failed to write test file");
        full_path
    }
    
    pub fn read_file(&self, path: &Path) -> String {
        fs::read_to_string(path).expect("Failed to read file")
    }
    
    pub fn file_exists(&self, path: &Path) -> bool {
        path.exists()
    }
    
    pub fn run_laszoo(&self, args: &[&str]) -> Result<std::process::Output, Box<dyn std::error::Error>> {
        // Find the binary - when running tests, cargo sets CARGO_BIN_EXE_<name>
        let binary_path = if let Ok(path) = std::env::var("CARGO_BIN_EXE_laszoo") {
            PathBuf::from(path)
        } else {
            // Get the project root (where Cargo.toml is)
            let mut current_dir = std::env::current_dir()?;
            let mut project_root = None;
            
            // Search upward for Cargo.toml
            loop {
                if current_dir.join("Cargo.toml").exists() {
                    project_root = Some(current_dir);
                    break;
                }
                
                if !current_dir.pop() {
                    break;
                }
            }
            
            let root = project_root.ok_or("Could not find project root")?;
            
            // Check possible paths relative to project root
            let possible_paths = [
                root.join("target/release/laszoo"),
                root.join("target/debug/laszoo"),
            ];
            
            possible_paths.into_iter()
                .find(|p| p.exists())
                .ok_or("Could not find laszoo binary")?
        };
        
        let mut cmd = Command::new(binary_path);
        
        // Set environment variables
        cmd.env("LASZOO_MFS_MOUNT", &self.mfs_mount);
        cmd.env("HOSTNAME", &self.hostname);
        
        // Pass through RUST_LOG if set
        if let Ok(rust_log) = std::env::var("RUST_LOG") {
            cmd.env("RUST_LOG", rust_log);
        }
        
        // Set the working directory to the test directory
        cmd.current_dir(&self.test_dir);
        
        // Add arguments
        cmd.args(args);
        
        Ok(cmd.output()?)
    }
    
    pub fn cleanup(&self) {
        // Remove test directory
        let _ = fs::remove_dir_all(&self.test_dir);
    }
}

impl Drop for TestEnvironment {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Wait for a condition to be true, with timeout
pub fn wait_for<F>(mut condition: F, timeout_secs: u64) -> bool 
where
    F: FnMut() -> bool,
{
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);
    
    while start.elapsed() < timeout {
        if condition() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    
    false
}

/// Compare file contents ignoring whitespace differences
pub fn files_equal_ignore_whitespace(path1: &Path, path2: &Path) -> bool {
    let content1 = fs::read_to_string(path1).unwrap_or_default();
    let content2 = fs::read_to_string(path2).unwrap_or_default();
    
    let normalized1: String = content1.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
        
    let normalized2: String = content2.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
        
    normalized1 == normalized2
}

/// Create a second test environment simulating another machine
pub fn create_second_machine(base_env: &TestEnvironment, machine_name: &str) -> TestEnvironment {
    TestEnvironment {
        test_dir: base_env.test_dir.join(format!("machine-{}", machine_name)),
        mfs_mount: base_env.mfs_mount.clone(), // Share the same MFS mount
        hostname: machine_name.to_string(),
        original_hostname: base_env.original_hostname.clone(),
    }
}