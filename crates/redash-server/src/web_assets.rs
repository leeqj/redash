use axum::http::header::{CONTENT_TYPE, HeaderValue};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

pub const INDEX_HTML: &str = include_str!("../web/index.html");
pub const APP_JS: &str = include_str!("../web/app.js");
pub const STYLE_CSS: &str = include_str!("../web/style.css");

pub async fn serve_index() -> Response {
    (
        [(CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"))],
        INDEX_HTML,
    )
        .into_response()
}

pub async fn serve_js() -> Response {
    (
        [(CONTENT_TYPE, HeaderValue::from_static("application/javascript; charset=utf-8"))],
        APP_JS,
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
