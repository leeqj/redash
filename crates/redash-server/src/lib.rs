pub mod api;
pub mod state;
pub mod web_assets;
pub mod ws;

use axum::routing::get;
use axum::Router;
use tower_http::cors::CorsLayer;

use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        // Embedded Web Frontend Routes
        .route("/", get(web_assets::serve_index))
        .route("/assets/app.js", get(web_assets::serve_js))
        .route("/assets/style.css", get(web_assets::serve_css))
        // API and WebSocket Routes
        .merge(api::api_router())
        .merge(ws::ws_router())
        .fallback(web_assets::not_found)
        .layer(CorsLayer::permissive())
        .with_state(state)
}
