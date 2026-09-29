pub mod api;
pub mod health_sentinel;
pub mod registry;
pub mod ws_agent;

use crate::state::AppState;
use axum::Router;
use axum::routing::{get, post};

pub fn control_plane_router() -> Router<AppState> {
    Router::new()
        // Control Plane Client REST & SSE
        .route("/v1/control/nodes", get(api::list_nodes))
        .route("/v1/control/nodes/{id}", get(api::get_node))
        .route("/v1/control/actions", post(api::dispatch_action))
        .route("/v1/control/actions/dispatch", post(api::dispatch_action))
        .route("/v1/control/telemetry/sse", get(api::telemetry_sse))
        // Reverse WebTTY Bridge
        .route("/v1/control/tty/{node_id}", get(api::client_tty_handler))
}

pub fn agent_router() -> Router<AppState> {
    Router::new()
        .route("/v1/agent/ws", get(ws_agent::agent_ws_handler))
        .route("/v1/control/ws-agent", get(ws_agent::agent_ws_handler))
}
