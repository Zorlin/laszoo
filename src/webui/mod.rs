pub mod server;
pub mod handlers;
#[cfg(feature = "gamepad")]
pub mod gamepad;
pub mod websocket;

use askama::Template;
use crate::error::Result;
use crate::config::Config;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct WebUI {
    config: Arc<Config>,
    state: Arc<RwLock<WebUIState>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WebUIState {
    pub enrolled_files: Vec<EnrolledFile>,
    pub groups: Vec<GroupInfo>,
    pub system_status: SystemStatus,
    pub active_operations: Vec<ActiveOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrolledFile {
    pub path: String,
    pub group: String,
    pub status: FileStatus,
    pub last_modified: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileStatus {
    Synced,
    Modified,
    Drifted,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInfo {
    pub name: String,
    pub machines: Vec<String>,
    pub file_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemStatus {
    pub hostname: String,
    pub mfs_mounted: bool,
    pub service_running: bool,
    pub last_sync: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveOperation {
    pub id: String,
    pub operation_type: String,
    pub progress: f32,
    pub message: String,
}

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate;

impl WebUI {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(WebUIState::default())),
        }
    }
    
    pub async fn start(&self, port: u16) -> Result<()> {
        let server = server::WebServer::new(
            self.config.clone(),
            self.state.clone(),
        );
        
        server.run(port).await
    }
    
    pub fn state(&self) -> Arc<RwLock<WebUIState>> {
        self.state.clone()
    }
}