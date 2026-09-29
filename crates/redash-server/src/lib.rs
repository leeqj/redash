pub mod alert_monitor;
pub mod api;
pub mod auth;
pub mod control_plane;
pub mod state;
pub mod web_assets;
pub mod ws;

use axum::Router;
use axum::routing::get;
use axum::{http::StatusCode, middleware, routing::post};

use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    let management = Router::new()
        .merge(api::api_router())
        .merge(ws::ws_router())
        .merge(control_plane::control_plane_router())
        .route("/auth/session", get(|| async { StatusCode::NO_CONTENT }))
        .route("/auth/logout", post(auth::logout))
        .layer(middleware::from_fn_with_state(
            state.gateway_auth.clone(),
            auth::authorize,
        ));
    Router::new()
        // Embedded GPUI Web Frontend Routes
        .route("/", get(web_assets::serve_index))
        .route("/pkg/redash_web.js", get(web_assets::serve_wasm_js))
        .route("/pkg/redash_web_bg.wasm", get(web_assets::serve_wasm_bin))
        .route("/pkg/build.json", get(web_assets::serve_build))
        .route("/assets/style.css", get(web_assets::serve_css))
        // API and WebSocket Routes
        .merge(management)
        .merge(control_plane::agent_router())
        .route("/auth/login", post(auth::login))
        .fallback(web_assets::not_found)
        .with_state(state)
}
