use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use redash_server::{auth::GatewayAuth, build_router, state::AppState};
use tower::ServiceExt;
const TOKEN: &str = "test-gateway-secret-at-least-32-characters";
fn app() -> axum::Router {
    let mut state = AppState::new();
    state.gateway_auth = GatewayAuth::new(TOKEN, vec![]).unwrap();
    build_router(state)
}
#[tokio::test]
async fn every_management_surface_requires_authentication() {
    for (method, url) in [
        ("GET", "/api/hosts"),
        ("POST", "/api/batch/exec"),
        ("POST", "/api/sftp/fixture/write"),
        ("PUT", "/api/settings"),
        ("GET", "/ws/terminal/fixture"),
        ("GET", "/ws/metrics/fixture"),
        ("GET", "/v1/control/nodes"),
        ("GET", "/v1/control/tty/fixture"),
        ("POST", "/v1/control/actions"),
    ] {
        let response = app()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(url)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{url}");
        assert!(
            !response
                .headers()
                .contains_key("access-control-allow-origin")
        );
    }
    let mut state = AppState::new();
    state.gateway_auth = GatewayAuth::default();
    let response = build_router(state)
        .oneshot(
            Request::builder()
                .uri("/api/hosts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}
#[tokio::test]
async fn bearer_and_browser_sessions_reject_foreign_origins_and_logout() {
    let app = app();
    for (token, origin, expected) in [
        ("wrong", "https://hub.test", StatusCode::UNAUTHORIZED),
        (TOKEN, "https://evil.test", StatusCode::FORBIDDEN),
        (TOKEN, "https://hub.test", StatusCode::OK),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/control/nodes")
                    .header("Host", "hub.test")
                    .header("Origin", origin)
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header("Host", "hub.test")
                .header("Origin", "https://hub.test")
                .header("Authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let set_cookie = response.headers()["set-cookie"].to_str().unwrap();
    assert!(
        set_cookie.contains("HttpOnly")
            && set_cookie.contains("Secure")
            && set_cookie.contains("SameSite=Strict")
    );
    assert!(!set_cookie.contains(TOKEN));
    let cookie = set_cookie.split(';').next().unwrap();
    for (origin, expected) in [
        ("https://hub.test", StatusCode::OK),
        ("https://evil.test", StatusCode::FORBIDDEN),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/control/nodes")
                    .header("Host", "hub.test")
                    .header("Origin", origin)
                    .header("Cookie", cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/logout")
                .header("Cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/session")
                .header("Cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
#[tokio::test]
async fn ssh_websocket_checks_credentials_and_origin_before_upgrade() {
    use tokio_tungstenite::{
        connect_async,
        tungstenite::{Error, client::IntoClientRequest},
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app()).await.unwrap() });
    let url = format!("ws://{addr}/ws/terminal/unknown-auth-test-fixture");
    for (token, origin, status) in [
        (None, None, 401),
        (Some(TOKEN), Some("https://evil.test"), 403),
    ] {
        let mut request = url.clone().into_client_request().unwrap();
        if let Some(token) = token {
            request
                .headers_mut()
                .insert("Authorization", format!("Bearer {token}").parse().unwrap());
        }
        if let Some(origin) = origin {
            request
                .headers_mut()
                .insert("Origin", origin.parse().unwrap());
        }
        let Error::Http(response) = connect_async(request).await.unwrap_err() else {
            panic!("expected auth rejection")
        };
        assert_eq!(response.status().as_u16(), status);
    }
    let mut request = url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {TOKEN}").parse().unwrap());
    let (mut socket, response) = connect_async(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
    socket.close(None).await.unwrap();
    server.abort();
}
