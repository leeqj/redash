use axum::http::StatusCode;
use axum::http::header::{CONTENT_TYPE, HeaderValue};
use axum::response::{IntoResponse, Response};

pub const INDEX_HTML: &str = include_str!("../web/index.html");
pub const STYLE_CSS: &str = include_str!("../web/style.css");
pub const WASM_JS: &str = include_str!("../web/pkg/redash_web.js");
pub const WASM_BIN: &[u8] = include_bytes!("../web/pkg/redash_web_bg.wasm");
pub const WEB_BUILD: &str = include_str!("../web/pkg/build.json");

pub async fn serve_build() -> impl IntoResponse {
    (
        [(CONTENT_TYPE, HeaderValue::from_static("application/json"))],
        WEB_BUILD,
    )
}

pub async fn serve_index() -> Response {
    (
        [(
            CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        )],
        INDEX_HTML,
    )
        .into_response()
}

pub async fn serve_wasm_js() -> Response {
    (
        [(
            CONTENT_TYPE,
            HeaderValue::from_static("application/javascript; charset=utf-8"),
        )],
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
        [(
            CONTENT_TYPE,
            HeaderValue::from_static("text/css; charset=utf-8"),
        )],
        STYLE_CSS,
    )
        .into_response()
}

pub async fn not_found() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "404 Not Found - ReDash Web")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn embedded_web_manifest_matches_binary() {
        let manifest: serde_json::Value = serde_json::from_str(WEB_BUILD).unwrap();
        assert_eq!(
            manifest["wasm_sha256"],
            format!("{:x}", Sha256::digest(WASM_BIN))
        );
        assert!(!manifest["revision"].as_str().unwrap().is_empty());
        if let Ok(revision) = std::env::var("GITHUB_SHA") {
            assert_eq!(
                manifest["revision"], revision,
                "Embedded Web resources must come from this workflow revision"
            );
        }
    }
}
