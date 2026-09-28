use crate::control_plane::registry::ControlPlaneRegistry;
use crate::state::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use log::{debug, info, warn};
use redash_types::{AgentToHubMessage, HubToAgentMessage};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};

pub async fn agent_ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let registry = state.control_plane.clone();
    ws.on_upgrade(move |socket| handle_agent_socket(socket, addr, registry))
}

async fn handle_agent_socket(
    mut socket: WebSocket,
    addr: SocketAddr,
    registry: ControlPlaneRegistry,
) {
    info!("Incoming agent connection attempt from {}", addr);

    // 1. Await Handshake
    let handshake = match socket.recv().await {
        Some(Ok(Message::Text(text))) => {
            match serde_json::from_str::<AgentToHubMessage>(&text) {
                Ok(AgentToHubMessage::Handshake(hs)) => hs,
                other => {
                    warn!("Expected handshake frame from {}, got: {:?}", addr, other);
                    return;
                }
            }
        }
        other => {
            warn!("Failed to read initial handshake from {}: {:?}", addr, other);
            return;
        }
    };

    let node_id = handshake.node_id.clone();
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<HubToAgentMessage>(64);
    let pending_actions = Arc::new(Mutex::new(HashMap::<String, oneshot::Sender<redash_types::ActionResult>>::new()));

    // 2. Register with Registry
    registry.register_agent(
        handshake.clone(),
        addr.to_string(),
        cmd_tx,
        pending_actions.clone(),
    );

    // 3. Send HandshakeAck
    let ack = HubToAgentMessage::HandshakeAck {
        success: true,
        message: format!("Node '{}' authenticated and bound to control plane", node_id),
        heartbeat_interval_secs: 5,
    };
    if let Ok(ack_json) = serde_json::to_string(&ack)
        && let Err(e) = socket.send(Message::Text(ack_json.into())).await
    {
        warn!("Failed to send HandshakeAck to {}: {}", node_id, e);
        registry.unregister_agent(&node_id);
        return;
    }

    let (mut ws_sink, mut ws_stream) = socket.split();

    // 4. Outbound dispatch task (Hub -> Agent)
    let nid_out = node_id.clone();
    let writer_task = tokio::spawn(async move {
        while let Some(hub_msg) = cmd_rx.recv().await {
            if let Ok(json) = serde_json::to_string(&hub_msg)
                && let Err(e) = ws_sink.send(Message::Text(json.into())).await
            {
                debug!("Agent {} writer sink closed: {}", nid_out, e);
                break;
            }
        }
    });

    // 5. Inbound frame loop (Agent -> Hub)
    let nid_in = node_id.clone();
    let reg_in = registry.clone();
    let actions_in = pending_actions.clone();

    while let Some(msg_res) = ws_stream.next().await {
        match msg_res {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<AgentToHubMessage>(&text) {
                    Ok(AgentToHubMessage::Telemetry(telemetry)) => {
                        reg_in.record_telemetry(telemetry);
                    }
                    Ok(AgentToHubMessage::ActionResult(result)) => {
                        info!(
                            "Received action result for {} from node {}: success={}, code={:?}",
                            result.action_id, result.node_id, result.success, result.exit_code
                        );
                        let mut pending = actions_in.lock().await;
                        if let Some(sender) = pending.remove(&result.action_id) {
                            let _ = sender.send(result);
                        }
                    }
                    Ok(AgentToHubMessage::Heartbeat) => {
                        reg_in.update_heartbeat(&nid_in);
                    }
                    Ok(AgentToHubMessage::TtyOutput { session_id, data }) => {
                        debug!("TTY output frame from node {} ({}: {} bytes)", nid_in, session_id, data.len());
                    }
                    other => {
                        debug!("Unhandled agent message from {}: {:?}", nid_in, other);
                    }
                }
            }
            Ok(Message::Ping(_)) => {
                reg_in.update_heartbeat(&nid_in);
            }
            Ok(Message::Close(_)) => {
                info!("Agent {} closed connection", nid_in);
                break;
            }
            Err(e) => {
                warn!("WebSocket error from agent {}: {}", nid_in, e);
                break;
            }
            _ => {}
        }
    }

    writer_task.abort();
    registry.unregister_agent(&node_id);
    info!("Cleaned up agent session for '{}'", node_id);
}
