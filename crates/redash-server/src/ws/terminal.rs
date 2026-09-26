use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use redash_core::probe::agent::AgentDetector;
use redash_core::session::pty::PtyChannel;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct TerminalQuery {
    pub cols: Option<u32>,
    pub rows: Option<u32>,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum ClientTerminalMessage {
    #[serde(rename = "input")]
    Input { data: String },
    #[serde(rename = "resize")]
    Resize { cols: u32, rows: u32 },
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Serialize)]
#[serde(tag = "type")]
pub enum ServerTerminalMessage {
    #[serde(rename = "output")]
    Output { data: String },
    #[serde(rename = "agent")]
    Agent {
        name: String,
        state: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cost_usd: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tokens: Option<u64>,
    },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "pong")]
    Pong,
}

pub async fn terminal_ws_handler(
    ws: WebSocketUpgrade,
    Path(host_id): Path<String>,
    Query(query): Query<TerminalQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_terminal_socket(socket, host_id, query, state))
}

async fn handle_terminal_socket(
    mut socket: WebSocket,
    host_id: String,
    query: TerminalQuery,
    state: AppState,
) {
    let host = match state.get_host(&host_id).await {
        Some(h) => h,
        None => {
            let err_msg = ServerTerminalMessage::Error {
                message: format!("Host not found: {}", host_id),
            };
            if let Ok(json) = serde_json::to_string(&err_msg) {
                let _ = socket.send(Message::Text(json.into())).await;
            }
            return;
        }
    };

    let session = match state.session_mgr.get_or_connect(&host).await {
        Ok(s) => s,
        Err(e) => {
            let err_msg = ServerTerminalMessage::Error {
                message: format!("Failed to connect to host: {}", e),
            };
            if let Ok(json) = serde_json::to_string(&err_msg) {
                let _ = socket.send(Message::Text(json.into())).await;
            }
            return;
        }
    };

    let cols = query.cols.unwrap_or(80);
    let rows = query.rows.unwrap_or(24);
    let (pty_out_tx, mut pty_out_rx) = mpsc::channel::<Vec<u8>>(512);

    let pty_channel = match PtyChannel::new(&session, cols, rows, pty_out_tx).await {
        Ok(c) => Arc::new(c),
        Err(e) => {
            let err_msg = ServerTerminalMessage::Error {
                message: format!("Failed to open PTY channel: {}", e),
            };
            if let Ok(json) = serde_json::to_string(&err_msg) {
                let _ = socket.send(Message::Text(json.into())).await;
            }
            return;
        }
    };

    let (mut ws_tx, mut ws_rx) = socket.split();
    let (server_tx, mut server_rx) = mpsc::channel::<ServerTerminalMessage>(128);

    // WebSocket Outgoing Writer Task
    let ws_writer = tokio::spawn(async move {
        while let Some(msg) = server_rx.recv().await {
            if let Ok(json) = serde_json::to_string(&msg)
                && ws_tx.send(Message::Text(json.into())).await.is_err()
            {
                break;
            }
        }
    });

    // Task 1: Forward PTY output -> WebSocket Client (with Agent HUD Detection)
    let server_tx_pty = server_tx.clone();
    let pty_out_forwarder = tokio::spawn(async move {
        let mut buffer_history = String::new();

        while let Some(bytes) = pty_out_rx.recv().await {
            // Forward raw text output to web client
            let text = String::from_utf8_lossy(&bytes).to_string();

            let out_msg = ServerTerminalMessage::Output { data: text.clone() };
            if server_tx_pty.send(out_msg).await.is_err() {
                break;
            }

            // Real-time AI Agent detection on terminal output
            buffer_history.push_str(&text);
            if buffer_history.len() > 8192 {
                buffer_history = buffer_history[buffer_history.len() - 4096..].to_string();
            }

            if let Some(agent) = AgentDetector::detect(None, None, &buffer_history) {
                let agent_msg = ServerTerminalMessage::Agent {
                    name: agent.name,
                    state: agent.status.label().to_string(),
                    cost_usd: agent.cost_usd,
                    tokens: agent.tokens,
                };
                let _ = server_tx_pty.send(agent_msg).await;
            }
        }
    });

    // Task 2: Forward WebSocket Client -> PTY Input
    let pty_channel_clone = Arc::clone(&pty_channel);
    let ws_in_forwarder = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            match msg {
                Message::Text(text) => {
                    // Try parsing structured JSON command
                    if let Ok(client_cmd) = serde_json::from_str::<ClientTerminalMessage>(&text) {
                        match client_cmd {
                            ClientTerminalMessage::Input { data } => {
                                let _ = pty_channel_clone.send_data(data.as_bytes()).await;
                            }
                            ClientTerminalMessage::Resize { cols, rows } => {
                                let _ = pty_channel_clone.resize(cols, rows).await;
                            }
                            ClientTerminalMessage::Ping => {
                                let _ = server_tx.send(ServerTerminalMessage::Pong).await;
                            }
                        }
                    } else {
                        // Raw text fallback
                        let _ = pty_channel_clone.send_data(text.as_bytes()).await;
                    }
                }
                Message::Binary(bin) => {
                    let _ = pty_channel_clone.send_data(&bin).await;
                }
                Message::Close(_) => {
                    let _ = pty_channel_clone.close().await;
                    break;
                }
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = ws_writer => {},
        _ = pty_out_forwarder => {},
        _ = ws_in_forwarder => {},
    }
}
