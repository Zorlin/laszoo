use crate::config::Config;
use crate::error::{LaszooError, Result};
use crate::webui::{IndexTemplate, WebUIState, websocket};
use askama_axum::IntoResponse;
use axum::{
    extract::{State, WebSocketUpgrade},
    response::Html,
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::{
    cors::CorsLayer,
    services::ServeDir,
};

pub struct WebServer {
    config: Arc<Config>,
    state: Arc<RwLock<WebUIState>>,
}

impl WebServer {
    pub fn new(config: Arc<Config>, state: Arc<RwLock<WebUIState>>) -> Self {
        Self { config, state }
    }
    
    pub async fn run(self, port: u16) -> Result<()> {
        let app = self.create_router();
        
        let addr = format!("0.0.0.0:{}", port);
        println!("Web UI starting on http://{}", addr);
        
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .map_err(|e| LaszooError::Other(format!("Failed to bind to {}: {}", addr, e)))?;
        
        axum::serve(listener, app)
            .await
            .map_err(|e| LaszooError::Other(format!("Server error: {}", e)))?;
        
        Ok(())
    }
    
    fn create_router(self) -> Router {
        let state = AppState {
            config: self.config,
            ui_state: self.state,
        };
        
        Router::new()
            // API routes
            .route("/api/status", get(crate::webui::handlers::get_status))
            .route("/api/groups", get(crate::webui::handlers::get_groups))
            .route("/api/groups/:name", get(crate::webui::handlers::get_group_details))
            .route("/api/files", get(crate::webui::handlers::get_enrolled_files))
            .route("/api/files/enroll", post(crate::webui::handlers::enroll_file))
            .route("/api/files/unenroll", post(crate::webui::handlers::unenroll_file))
            .route("/api/operations", get(crate::webui::handlers::get_operations))
            
            // WebSocket for real-time updates
            .route("/ws", get(ws_handler))
            
            // Gamepad API
            .route("/api/gamepad/status", get(crate::webui::handlers::gamepad_status))
            
            // Serve static files
            .nest_service("/static", ServeDir::new("static"))
            
            // Main page
            .route("/", get(index_handler))
            
            .layer(CorsLayer::permissive())
            .with_state(state)
    }
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub ui_state: Arc<RwLock<WebUIState>>,
}

async fn index_handler() -> impl IntoResponse {
    IndexTemplate {}
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| websocket::handle_websocket(socket, state))
}