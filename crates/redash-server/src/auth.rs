//! Authentication for human management clients; Agent enrollment is a separate boundary.
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;

const COOKIE: &str = "redash_session";
const SESSION_TTL: Duration = Duration::from_secs(12 * 3600);
#[derive(Clone, Default)]
pub struct GatewayAuth {
    token_hash: Option<[u8; 32]>,
    sessions: Arc<Mutex<HashMap<String, Instant>>>,
    origins: Vec<String>,
}
impl GatewayAuth {
    pub fn from_env() -> Self {
        std::env::var("REDASH_GATEWAY_TOKEN")
            .ok()
            .and_then(|token| {
                Self::new(
                    &token,
                    std::env::var("REDASH_ALLOWED_ORIGINS")
                        .unwrap_or_default()
                        .split(',')
                        .filter(|s| !s.trim().is_empty())
                        .map(|s| s.trim().trim_end_matches('/').into())
                        .collect(),
                )
                .ok()
            })
            .unwrap_or_default()
    }
    pub fn new(token: &str, origins: Vec<String>) -> Result<Self, &'static str> {
        if token.len() < 32
            || token
                .bytes()
                .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
        {
            return Err("REDASH_GATEWAY_TOKEN must contain at least 32 non-whitespace characters");
        }
        Ok(Self {
            token_hash: Some(Sha256::digest(token).into()),
            origins,
            ..Default::default()
        })
    }
    pub fn configured(&self) -> bool {
        self.token_hash.is_some()
    }
    fn bearer(&self, headers: &HeaderMap) -> bool {
        let Some(value) = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
        else {
            return false;
        };
        let actual: [u8; 32] = Sha256::digest(value).into();
        self.token_hash
            .is_some_and(|expected| bool::from(expected.ct_eq(&actual)))
    }
    fn origin_allowed(&self, headers: &HeaderMap) -> bool {
        let Some(origin) = headers.get(header::ORIGIN) else {
            return true;
        };
        let Ok(origin) = origin.to_str() else {
            return false;
        };
        if !self.origins.is_empty() {
            return self.origins.iter().any(|o| o == origin);
        }
        // Browser same-origin requests use the public Host forwarded by the proxy.
        let Ok(uri) = origin.parse::<axum::http::Uri>() else {
            return false;
        };
        matches!(uri.scheme_str(), Some("http" | "https"))
            && uri.authority().is_some_and(|a| {
                headers
                    .get(header::HOST)
                    .is_some_and(|host| host.as_bytes() == a.as_str().as_bytes())
            })
            && uri.path() == "/"
            && uri.query().is_none()
    }
    fn session(&self, headers: &HeaderMap) -> bool {
        let Some(id) = cookie_id(headers) else {
            return false;
        };
        let mut sessions = self.sessions.lock().unwrap();
        sessions.retain(|_, created| created.elapsed() < SESSION_TTL);
        sessions.contains_key(id)
    }
}
fn cookie_id(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|item| {
            let (name, value) = item.trim().split_once('=')?;
            (name == COOKIE).then_some(value)
        })
}
pub async fn authorize(State(auth): State<GatewayAuth>, request: Request, next: Next) -> Response {
    if !auth.configured() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Gateway management authentication is not configured",
        )
            .into_response();
    }
    if !auth.origin_allowed(request.headers()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !auth.bearer(request.headers()) && !auth.session(request.headers()) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(request).await
}
pub async fn login(State(state): State<crate::state::AppState>, headers: HeaderMap) -> Response {
    let auth = state.gateway_auth;
    if !auth.configured() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if !auth.origin_allowed(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !auth.bearer(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // Browser sessions are same-origin only. Native clients use the bearer header.
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    let secure = origin.starts_with("https://");
    let local_http = origin
        .parse::<axum::http::Uri>()
        .ok()
        .is_some_and(|u| matches!(u.host(), Some("localhost" | "127.0.0.1" | "[::1]")));
    if !secure && !local_http {
        return (
            StatusCode::FORBIDDEN,
            "Browser login requires HTTPS outside localhost",
        )
            .into_response();
    }
    let mut sessions = auth.sessions.lock().unwrap();
    sessions.retain(|_, created| created.elapsed() < SESSION_TTL);
    if sessions.len() >= 128 {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let id = redash_ui_core::e2ee::random_hex();
    sessions.insert(id.clone(), Instant::now());
    (
        StatusCode::NO_CONTENT,
        [
            (
                header::SET_COOKIE,
                format!(
                    "{COOKIE}={id}; HttpOnly; SameSite=Strict; Path=/; Max-Age=43200{}",
                    if secure { "; Secure" } else { "" }
                ),
            ),
            (header::CACHE_CONTROL, "no-store".into()),
        ],
    )
        .into_response()
}
pub async fn logout(State(state): State<crate::state::AppState>, headers: HeaderMap) -> Response {
    if let Some(id) = cookie_id(&headers) {
        state.gateway_auth.sessions.lock().unwrap().remove(id);
    }
    (
        StatusCode::NO_CONTENT,
        [(
            header::SET_COOKIE,
            format!("{COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"),
        )],
    )
        .into_response()
}
