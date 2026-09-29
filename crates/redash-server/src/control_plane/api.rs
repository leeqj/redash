use crate::state::AppState;
use axum::Json;
use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use redash_types::{ManagedNodeDetail, SignedAction};
use std::convert::Infallible;

pub async fn list_nodes(State(state): State<AppState>) -> Json<Vec<ManagedNodeDetail>> {
    Json(state.control_plane.list_nodes())
}

pub async fn get_node(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ManagedNodeDetail>, (StatusCode, String)> {
    state
        .control_plane
        .get_node(&id)
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Node '{}' not found", id)))
}

pub async fn dispatch_action(
    State(state): State<AppState>,
    Json(signed_action): Json<SignedAction>,
) -> Result<Json<redash_types::ActionResult>, (StatusCode, String)> {
    match state.control_plane.dispatch_action(signed_action, 25).await {
        Ok(result) => Ok(Json(result)),
        Err(err) => Err((StatusCode::BAD_REQUEST, err)),
    }
}

pub async fn telemetry_sse(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.control_plane.subscribe_telemetry();

    let stream = futures::stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(telemetry) => {
                    if let Ok(data) = serde_json::to_string(&telemetry) {
                        let event = Event::default().event("telemetry").data(data);
                        return Some((Ok(event), rx));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    return None;
                }
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

pub async fn client_tty_handler(
    Path(node_id): Path<String>,
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_client_tty(socket, node_id, state))
}

async fn handle_client_tty(mut socket: WebSocket, node_id: String, state: AppState) {
    use crate::control_plane::registry::TtyDownstreamMsg;
    use std::time::Duration;
    let Ok(Some(Ok(WsMessage::Text(first)))) =
        tokio::time::timeout(Duration::from_secs(5), socket.recv()).await
    else {
        return;
    };
    let Ok(init) = serde_json::from_str::<redash_types::E2eeHandshakeInit>(&first) else {
        return;
    };
    if init.node_id != node_id || init.session_id.is_empty() || init.session_id.len() > 128 {
        return;
    }
    let sid = init.session_id.clone();
    let registry = state.control_plane;
    let (tx, mut rx) = tokio::sync::mpsc::channel(128);
    let Ok(binding) = registry.register_tty_subscriber(&node_id, sid.clone(), tx) else {
        return;
    };
    let result: anyhow::Result<()> = async {
        registry.send_e2ee_init_to_node(&node_id, init).await.map_err(anyhow::Error::msg)?;
        // Do not route client commands until this connection's Agent acknowledges the same session.
        let Some(TtyDownstreamMsg::Text(text)) = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await? else { anyhow::bail!("Agent disconnected"); };
        let ack: redash_types::E2eeHandshakeAck = serde_json::from_str(&text)?;
        tokio::time::timeout(Duration::from_secs(5), socket.send(WsMessage::Text(text.into()))).await??;
        anyhow::ensure!(ack.success, "Agent rejected terminal authentication");
        loop {
            tokio::select! {
                incoming = socket.recv() => match incoming {
                    Some(Ok(WsMessage::Text(text))) => {
                        let env: redash_types::EncryptedEnvelope = serde_json::from_str(&text)?;
                        anyhow::ensure!(env.session_id == sid, "Cross-session input rejected");
                        registry.send_tty_encrypted_to_node(&node_id, env).await.map_err(anyhow::Error::msg)?;
                    }
                    Some(Ok(WsMessage::Ping(data))) => { tokio::time::timeout(Duration::from_secs(5), socket.send(WsMessage::Pong(data))).await??; }
                    _ => break,
                },
                outgoing = rx.recv() => match outgoing {
                    Some(TtyDownstreamMsg::Text(text)) => { tokio::time::timeout(Duration::from_secs(5), socket.send(WsMessage::Text(text.into()))).await??; }
                    _ => break,
                }
            }
        }
        Ok(())
    }.await;
    if let Err(err) = result {
        log::debug!("Terminal relay closed: {}", err);
    }
    registry.close_node_tty(&sid, &binding).await;
}
