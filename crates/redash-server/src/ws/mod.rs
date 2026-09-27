pub mod metrics;
pub mod terminal;

use axum::Router;
use axum::routing::get;

use crate::state::AppState;

pub fn ws_router() -> Router<AppState> {
    Router::new()
        .route("/ws/terminal/{host_id}", get(terminal::terminal_ws_handler))
        .route("/ws/metrics/{host_id}", get(metrics::metrics_ws_handler))
}
