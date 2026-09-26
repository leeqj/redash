use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use redash_core::batch::BatchRunner;
use redash_core::config::HostConfig;
use redash_types::batch::BatchRunRequest;
use std::time::Duration;

use super::hosts::ApiResponse;
use crate::state::AppState;

pub async fn run_batch_job(
    State(state): State<AppState>,
    Json(req): Json<BatchRunRequest>,
) -> impl IntoResponse {
    let store = state.host_store.read().await;
    let hosts: Vec<HostConfig> = store
        .hosts
        .iter()
        .filter(|h| req.host_ids.contains(&h.id.0))
        .cloned()
        .collect();
    drop(store);

    let result = BatchRunner::run_batch(
        hosts,
        req.command,
        state.session_mgr.clone(),
        Duration::from_secs(60),
    )
    .await;

    Json(ApiResponse {
        success: true,
        data: Some(result),
        message: None,
    })
}
