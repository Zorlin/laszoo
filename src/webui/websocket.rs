use crate::webui::server::AppState;
use axum::extract::ws::{Message, WebSocket};
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use std::time::Duration;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    // Client -> Server
    Subscribe { channel: String },
    Unsubscribe { channel: String },
    Command { action: String, data: serde_json::Value },
    
    // Server -> Client
    Update { channel: String, data: serde_json::Value },
    Notification { level: String, message: String },
    Error { message: String },
    Pong,
}

pub async fn handle_websocket(socket: WebSocket, state: AppState) {
    let (sender, receiver) = socket.split();
    let mut sender = sender;
    let mut receiver = receiver;
    let (tx, mut rx) = mpsc::channel::<WsMessage>(100);
    
    // Spawn task to send messages to client
    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let json = match serde_json::to_string(&msg) {
                Ok(j) => j,
                Err(_) => continue,
            };
            
            if sender.send(Message::Text(json)).await.is_err() {
                break;
            }
        }
    });
    
    // Spawn task to send periodic updates
    let tx_updates = tx.clone();
    let state_clone = state.clone();
    let mut update_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        
        loop {
            interval.tick().await;
            
            // Send system status update
            let ui_state = state_clone.ui_state.read().await;
            let update = WsMessage::Update {
                channel: "status".to_string(),
                data: serde_json::to_value(&ui_state.system_status).unwrap_or_default(),
            };
            
            if tx_updates.send(update).await.is_err() {
                break;
            }
        }
    });
    
    // Handle incoming messages
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    let ws_msg: Result<WsMessage, _> = serde_json::from_str(&text);
                    
                    match ws_msg {
                        Ok(WsMessage::Subscribe { channel }) => {
                            // Handle subscription
                            let _ = tx.send(WsMessage::Notification {
                                level: "info".to_string(),
                                message: format!("Subscribed to {}", channel),
                            }).await;
                        }
                        Ok(WsMessage::Command { action, data }) => {
                            // Handle commands
                            handle_command(&state, &action, data, &tx).await;
                        }
                        Ok(_) => {}
                        Err(_) => {
                            let _ = tx.send(WsMessage::Error {
                                message: "Invalid message format".to_string(),
                            }).await;
                        }
                    }
                }
                Message::Ping(_) => {
                    let _ = tx.send(WsMessage::Pong).await;
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });
    
    // Wait for any task to complete
    tokio::select! {
        _ = (&mut send_task) => {
            recv_task.abort();
            update_task.abort();
        }
        _ = (&mut recv_task) => {
            send_task.abort();
            update_task.abort();
        }
        _ = (&mut update_task) => {
            send_task.abort();
            recv_task.abort();
        }
    }
}

async fn handle_command(
    state: &AppState,
    action: &str,
    data: serde_json::Value,
    tx: &mpsc::Sender<WsMessage>,
) {
    match action {
        "refresh_status" => {
            let ui_state = state.ui_state.read().await;
            let _ = tx.send(WsMessage::Update {
                channel: "status".to_string(),
                data: serde_json::to_value(&ui_state.system_status).unwrap_or_default(),
            }).await;
        }
        "refresh_files" => {
            let ui_state = state.ui_state.read().await;
            let _ = tx.send(WsMessage::Update {
                channel: "files".to_string(),
                data: serde_json::to_value(&ui_state.enrolled_files).unwrap_or_default(),
            }).await;
        }
        "refresh_groups" => {
            let ui_state = state.ui_state.read().await;
            let _ = tx.send(WsMessage::Update {
                channel: "groups".to_string(),
                data: serde_json::to_value(&ui_state.groups).unwrap_or_default(),
            }).await;
        }
        _ => {
            let _ = tx.send(WsMessage::Error {
                message: format!("Unknown command: {}", action),
            }).await;
        }
    }
}