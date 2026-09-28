pub mod api;
pub mod health_sentinel;
pub mod registry;
pub mod ws_agent;

use crate::state::AppState;
use axum::Router;
use axum::routing::{get, post};

pub fn control_plane_router() -> Router<AppState> {
    Router::new()
        // Agent Inbound WebSocket
        .route("/v1/agent/ws", get(ws_agent::agent_ws_handler))
        // Control Plane Client REST & SSE
        .route("/v1/control/nodes", get(api::list_nodes))
        .route("/v1/control/nodes/{id}", get(api::get_node))
        .route("/v1/control/actions/dispatch", post(api::dispatch_action))
        .route("/v1/control/telemetry/sse", get(api::telemetry_sse))
}
