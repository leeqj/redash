use crate::state::AppState;
use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::Json;
use futures::stream::Stream;
use futures::{SinkExt, StreamExt};
use redash_types::{ManagedNodeDetail, SignedAction};
use std::convert::Infallible;

pub async fn list_nodes(
    State(state): State<AppState>,
) -> Json<Vec<ManagedNodeDetail>> {
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

async fn handle_client_tty(
    socket: WebSocket,
    node_id: String,
    state: AppState,
) {
    let session_id = format!("tty-{}", uuid::Uuid::new_v4());
    let (tty_tx, mut tty_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(128);

    state.control_plane.register_tty_subscriber(session_id.clone(), tty_tx);
    state.control_plane.open_node_tty(&node_id, &session_id, 24, 80).await;

    let (mut client_sink, mut client_stream) = socket.split();

    // Task forwarding output from Agent -> Browser client
    let reader_task = tokio::spawn(async move {
        while let Some(bytes) = tty_rx.recv().await {
            if client_sink.send(WsMessage::Binary(bytes.into())).await.is_err() {
                break;
            }
        }
    });

    // Inbound keystrokes from Browser client -> Agent
    let reg_in = state.control_plane.clone();
    let nid_in = node_id.clone();
    let sid_in = session_id.clone();

    while let Some(msg_res) = client_stream.next().await {
        match msg_res {
            Ok(WsMessage::Text(text)) => {
                reg_in.send_tty_input_to_node(&nid_in, &sid_in, text.as_bytes().to_vec()).await;
            }
            Ok(WsMessage::Binary(bytes)) => {
                reg_in.send_tty_input_to_node(&nid_in, &sid_in, bytes.to_vec()).await;
            }
            _ => break,
        }
    }

    reader_task.abort();
    state.control_plane.close_node_tty(&node_id, &session_id).await;
}
