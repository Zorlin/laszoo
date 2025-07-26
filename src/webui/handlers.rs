use crate::webui::{WebUIState, FileStatus, GroupInfo, SystemStatus};
use crate::webui::server::AppState;
use crate::enrollment::EnrollmentManager;
use crate::sync::SyncEngine;
use axum::{
    extract::{Path, State},
    Json,
    response::IntoResponse,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }
    
    pub fn error(msg: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg),
        }
    }
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub hostname: String,
    pub mfs_mounted: bool,
    pub service_status: String,
    pub service_mode: String,
}

pub async fn get_status(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let ui_state = state.ui_state.read().await;
    let status = &ui_state.system_status;
    
    Json(StatusResponse {
        hostname: status.hostname.clone(),
        mfs_mounted: status.mfs_mounted,
        service_status: if status.service_running { "running".to_string() } else { "stopped".to_string() },
        service_mode: "watch".to_string(),
    })
}

#[derive(Serialize)]
pub struct GroupsResponse {
    pub groups: Vec<GroupInfo>,
}

pub async fn get_groups(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let ui_state = state.ui_state.read().await;
    Json(GroupsResponse {
        groups: ui_state.groups.clone(),
    })
}

pub async fn get_group_details(
    Path(name): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let ui_state = state.ui_state.read().await;
    
    match ui_state.groups.iter().find(|g| g.name == name) {
        Some(group) => {
            (StatusCode::OK, Json(ApiResponse::success(group))).into_response()
        }
        None => {
            (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::<GroupInfo>::error(format!("Group '{}' not found", name)))
            ).into_response()
        }
    }
}

#[derive(Serialize)]
pub struct FilesResponse {
    pub files: Vec<crate::webui::EnrolledFile>,
}

pub async fn get_enrolled_files(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let ui_state = state.ui_state.read().await;
    Json(FilesResponse {
        files: ui_state.enrolled_files.clone(),
    })
}

#[derive(Deserialize)]
pub struct EnrollRequest {
    pub group: String,
    pub path: String,
    #[serde(rename = "machineSpecific")]
    pub machine_specific: bool,
    pub action: String,
}

pub async fn enroll_file(
    State(state): State<AppState>,
    Json(req): Json<EnrollRequest>,
) -> impl IntoResponse {
    let hostname = gethostname::gethostname().to_string_lossy().to_string();
    let enrollment_manager = EnrollmentManager::new(
        state.config.mfs_mount.clone(),
        hostname,
    );
    
    let path = PathBuf::from(&req.path);
    let action = match req.action.as_str() {
        "converge" => crate::cli::SyncAction::Converge,
        "rollback" => crate::cli::SyncAction::Rollback,
        "freeze" => crate::cli::SyncAction::Freeze,
        "drift" => crate::cli::SyncAction::Drift,
        _ => crate::cli::SyncAction::Converge,
    };
    
    match enrollment_manager.enroll_path(
        &req.group,
        Some(&path),
        false,
        req.machine_specific,
        false,
        None,
        None,
    ) {
        Ok(_) => {
            // Update UI state
            let mut ui_state = state.ui_state.write().await;
            ui_state.enrolled_files.push(crate::webui::EnrolledFile {
                path: req.path,
                group: req.group,
                status: FileStatus::Synced,
                last_modified: chrono::Utc::now(),
            });
            
            (StatusCode::OK, Json(ApiResponse::success("File enrolled successfully"))).into_response()
        }
        Err(e) => {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<&str>::error(format!("Failed to enroll file: {}", e)))
            ).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct UnenrollRequest {
    pub group: String,
    pub path: String,
}

pub async fn unenroll_file(
    State(state): State<AppState>,
    Json(req): Json<UnenrollRequest>,
) -> impl IntoResponse {
    let hostname = gethostname::gethostname().to_string_lossy().to_string();
    let enrollment_manager = EnrollmentManager::new(
        state.config.mfs_mount.clone(),
        hostname,
    );
    
    let path = PathBuf::from(&req.path);
    
    match enrollment_manager.unenroll_file(&path) {
        Ok(_) => {
            // Update UI state
            let mut ui_state = state.ui_state.write().await;
            ui_state.enrolled_files.retain(|f| f.path != req.path || f.group != req.group);
            
            (StatusCode::OK, Json(ApiResponse::success("File unenrolled successfully"))).into_response()
        }
        Err(e) => {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<&str>::error(format!("Failed to unenroll file: {}", e)))
            ).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct SyncRequest {
    pub group: Option<String>,
    pub strategy: String,
}

pub async fn trigger_sync(
    State(state): State<AppState>,
    Json(req): Json<SyncRequest>,
) -> impl IntoResponse {
    let strategy = match req.strategy.as_str() {
        "auto" => crate::cli::SyncStrategy::Auto,
        "rollback" => crate::cli::SyncStrategy::Rollback,
        "forward" => crate::cli::SyncStrategy::Forward,
        "converge" => crate::cli::SyncStrategy::Converge,
        "freeze" => crate::cli::SyncStrategy::Freeze,
        "drift" => crate::cli::SyncStrategy::Drift,
        _ => crate::cli::SyncStrategy::Auto,
    };
    
    // This would trigger an async sync operation
    // For now, just return success
    Json(ApiResponse::success("Sync operation started"))
}

pub async fn get_operations(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let ui_state = state.ui_state.read().await;
    Json(ApiResponse::success(ui_state.active_operations.clone()))
}

#[derive(Serialize)]
pub struct GamepadStatus {
    pub connected: bool,
    pub name: Option<String>,
    pub buttons: Vec<bool>,
    pub axes: Vec<f32>,
}

#[cfg(feature = "gamepad")]
pub async fn gamepad_status(
    State(_state): State<AppState>,
) -> impl IntoResponse {
    // Get gamepad status from the gamepad module
    let status = crate::webui::gamepad::get_gamepad_status();
    Json(ApiResponse::success(status))
}

#[cfg(not(feature = "gamepad"))]
pub async fn gamepad_status(
    State(_state): State<AppState>,
) -> impl IntoResponse {
    // Return empty gamepad status when feature is disabled
    let status = GamepadStatus {
        connected: false,
        name: None,
        buttons: vec![],
        axes: vec![],
    };
    Json(ApiResponse::success(status))
}