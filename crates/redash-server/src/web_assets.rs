use axum::http::header::{CONTENT_TYPE, HeaderValue};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

pub const INDEX_HTML: &str = include_str!("../web/index.html");
pub const STYLE_CSS: &str = include_str!("../web/style.css");
pub const WASM_JS: &str = include_str!("../web/pkg/redash_web.js");
pub const WASM_BIN: &[u8] = include_bytes!("../web/pkg/redash_web_bg.wasm");

pub async fn serve_index() -> Response {
    (
        [(CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"))],
        INDEX_HTML,
    )
        .into_response()
}

pub async fn serve_wasm_js() -> Response {
    (
        [(CONTENT_TYPE, HeaderValue::from_static("application/javascript; charset=utf-8"))],
        WASM_JS,
    )
        .into_response()
}

pub async fn serve_wasm_bin() -> Response {
    (
        [(CONTENT_TYPE, HeaderValue::from_static("application/wasm"))],
        WASM_BIN,
    )
        .into_response()
}

pub async fn serve_css() -> Response {
    (
        [(CONTENT_TYPE, HeaderValue::from_static("text/css; charset=utf-8"))],
        STYLE_CSS,
    )
        .into_response()
}

pub async fn not_found() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "404 Not Found - ReDash Web")
}
