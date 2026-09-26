use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use redash_core::config::{AppSettings, HostConfig, HostId};
use redash_server::build_router;
use redash_server::state::AppState;
use tower::ServiceExt;

#[tokio::test]
async fn test_web_assets_serving() {
    let state = AppState::new();
    let app = build_router(state);

    // Test GET /
    let res = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("GPUI Web"));
    assert!(body_str.contains("redash-gpui-canvas"));

    // Test GET /pkg/redash_web.js
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/pkg/redash_web.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body);
    assert!(body_str.contains("start_web_app"));

    // Test GET /pkg/redash_web_bg.wasm
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/pkg/redash_web_bg.wasm")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert!(body.starts_with(b"\0asm"));

    // Test GET non-existent page -> 404
    let res = app
        .oneshot(
            Request::builder()
                .uri("/random_404_url")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_hosts_api_crud() {
    let state = AppState::new();

    // Insert a test host into state directly
    let mut test_host = HostConfig::new("Web Unit Test Host", "127.0.0.1", "testuser");
    test_host.id = HostId("test_host_123".to_string());
    test_host.tags = vec!["web".to_string(), "testing".to_string()];

    {
        let mut store = state.host_store.write().await;
        store.hosts.push(test_host.clone());
    }

    let app = build_router(state);

    // 1. GET /api/hosts
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/hosts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    assert!(json["data"].as_array().unwrap().len() >= 1);

    // 2. GET /api/hosts/test_host_123
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/hosts/test_host_123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["name"], "Web Unit Test Host");

    // 3. POST /api/hosts (Create new host)
    let new_host = HostConfig::new("Created Via API", "192.168.1.100", "admin");
    let new_host_json = serde_json::to_value(&new_host).unwrap();

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/hosts")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&new_host_json).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    let created_id = json["data"]["id"].as_str().unwrap().to_string();

    // 4. DELETE /api/hosts/:id
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/hosts/{}", created_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_settings_api() {
    let state = AppState::new();
    let app = build_router(state);

    // 1. GET /api/settings
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    assert!(json["data"]["theme_name"].is_string());

    // 2. PUT /api/settings
    let mut updated_settings = AppSettings::default();
    updated_settings.theme_name = "CyberpunkNeon".to_string();
    updated_settings.probe_interval_secs = 5;

    let res = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&updated_settings).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_batch_exec_api() {
    let state = AppState::new();
    let app = build_router(state);

    let batch_req = redash_types::batch::BatchRunRequest {
        host_ids: vec!["non-existent".to_string()],
        command: "uptime".to_string(),
    };

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/batch/exec")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&batch_req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["command"], "uptime");
    assert!(json["data"]["job_id"].is_string());
}
