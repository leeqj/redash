use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use redash_core::probe::ProbeScheduler;
use serde::Serialize;
use std::time::Duration;

use crate::state::AppState;

#[derive(Serialize)]
#[serde(tag = "type")]
pub enum MetricsMessage {
    #[serde(rename = "metrics")]
    Metrics {
        host_id: String,
        data: Box<redash_core::probe::NodeMetrics>,
    },
    #[serde(rename = "error")]
    Error { message: String },
}

pub async fn metrics_ws_handler(
    ws: WebSocketUpgrade,
    Path(host_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_metrics_socket(socket, host_id, state))
}

async fn handle_metrics_socket(mut socket: WebSocket, host_id: String, state: AppState) {
    let scheduler = ProbeScheduler::new(state.session_mgr.clone());

    loop {
        let host = match state.get_host(&host_id).await {
            Some(h) => h,
            None => {
                let err_msg = MetricsMessage::Error {
                    message: format!("Host not found: {}", host_id),
                };
                if let Ok(json) = serde_json::to_string(&err_msg) {
                    let _ = socket.send(Message::Text(json.into())).await;
                }
                break;
            }
        };

        match scheduler.poll_host(&host).await {
            Ok(metrics) => {
                // Update server cache
                {
                    let mut cache = state.metrics_cache.write().await;
                    cache.insert(host.id.clone(), metrics.clone());
                }

                let msg = MetricsMessage::Metrics {
                    host_id: host_id.clone(),
                    data: Box::new(metrics),
                };
                if let Ok(json) = serde_json::to_string(&msg)
                    && socket.send(Message::Text(json.into())).await.is_err()
                {
                    break;
                }
            }
            Err(e) => {
                let err_msg = MetricsMessage::Error {
                    message: format!("Probe failed: {}", e),
                };
                if let Ok(json) = serde_json::to_string(&err_msg)
                    && socket.send(Message::Text(json.into())).await.is_err()
                {
                    break;
                }
            }
        }

        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}
