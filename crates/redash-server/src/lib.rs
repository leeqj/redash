pub mod alert_monitor;
pub mod api;
pub mod control_plane;
pub mod state;
pub mod web_assets;
pub mod ws;

use axum::Router;
use axum::routing::get;
use tower_http::cors::CorsLayer;

use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        // Embedded GPUI Web Frontend Routes
        .route("/", get(web_assets::serve_index))
        .route("/pkg/redash_web.js", get(web_assets::serve_wasm_js))
        .route("/pkg/redash_web_bg.wasm", get(web_assets::serve_wasm_bin))
        .route("/assets/style.css", get(web_assets::serve_css))
        // API and WebSocket Routes
        .merge(api::api_router())
        .merge(ws::ws_router())
        .merge(control_plane::control_plane_router())
        .fallback(web_assets::not_found)
        .layer(CorsLayer::permissive())
        .with_state(state)
}
