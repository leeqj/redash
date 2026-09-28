use crate::state::AppState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
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
