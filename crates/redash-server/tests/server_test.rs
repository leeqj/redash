use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use redash_core::config::{AppSettings, HostConfig, HostId};
use redash_server::build_router;
use redash_server::state::AppState;
use tower::ServiceExt;

#[tokio::test]
async fn test_web_assets_serving() {
    let state = test_state();
    let app = build_router(state);

    // Test GET /
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .header(
                    "Authorization",
                    "Bearer test-management-token-at-least-32-characters",
                )
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
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
                .header(
                    "Authorization",
                    "Bearer test-management-token-at-least-32-characters",
                )
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
                .header(
                    "Authorization",
                    "Bearer test-management-token-at-least-32-characters",
                )
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
                .header(
                    "Authorization",
                    "Bearer test-management-token-at-least-32-characters",
                )
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
    let test_dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test_config");
    let _ = std::fs::create_dir_all(&test_dir);
    unsafe {
        std::env::set_var("REDASH_CONFIG_DIR", &test_dir);
    }

    let state = test_state();

    // Insert a test host into state directly
    let mut test_host = HostConfig::new("Web Unit Test Host", "127.0.0.1", "testuser");
    test_host.id = HostId("test_host_123".to_string());
    test_host.tags = vec!["web".to_string(), "testing".to_string()];

    {
        let mut store = state.host_store.write().unwrap();
        store.hosts.push(test_host.clone());
    }

    let app = build_router(state);

    // 1. GET /api/hosts
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .header(
                    "Authorization",
                    "Bearer test-management-token-at-least-32-characters",
                )
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
    assert!(!json["data"].as_array().unwrap().is_empty());

    // 2. GET /api/hosts/test_host_123
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .header(
                    "Authorization",
                    "Bearer test-management-token-at-least-32-characters",
                )
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
                .header(
                    "Authorization",
                    "Bearer test-management-token-at-least-32-characters",
                )
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
                .header(
                    "Authorization",
                    "Bearer test-management-token-at-least-32-characters",
                )
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
    let test_dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test_config");
    let _ = std::fs::create_dir_all(&test_dir);
    unsafe {
        std::env::set_var("REDASH_CONFIG_DIR", &test_dir);
    }

    let state = test_state();
    let app = build_router(state);

    // 1. GET /api/settings
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .header(
                    "Authorization",
                    "Bearer test-management-token-at-least-32-characters",
                )
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
    let updated_settings = AppSettings {
        theme_name: "CyberpunkNeon".to_string(),
        probe_interval_secs: 5,
        ..AppSettings::default()
    };

    let res = app
        .oneshot(
            Request::builder()
                .header(
                    "Authorization",
                    "Bearer test-management-token-at-least-32-characters",
                )
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
    let state = test_state();
    let app = build_router(state);

    let batch_req = redash_types::batch::BatchRunRequest {
        host_ids: vec!["non-existent".to_string()],
        command: "uptime".to_string(),
    };

    let res = app
        .oneshot(
            Request::builder()
                .header(
                    "Authorization",
                    "Bearer test-management-token-at-least-32-characters",
                )
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

#[tokio::test]
async fn test_webhook_test_api() {
    let state = test_state();
    state.app_settings.write().await.alert_webhook_url = None;
    let app = build_router(state);

    // 1. Without webhook configured -> returns error message
    let req = serde_json::json!({
        "webhook_url": null,
    });
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .header(
                    "Authorization",
                    "Bearer test-management-token-at-least-32-characters",
                )
                .method("POST")
                .uri("/api/settings/test-webhook")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], false);
    assert!(json["message"].as_str().unwrap().contains("未配置 Webhook"));

    // 2. With invalid / mock target url -> triggers send_webhook and returns outcome
    let req_mock = serde_json::json!({
        "webhook_url": "http://127.0.0.1:9/invalid-webhook",
    });
    let res2 = app
        .oneshot(
            Request::builder()
                .header(
                    "Authorization",
                    "Bearer test-management-token-at-least-32-characters",
                )
                .method("POST")
                .uri("/api/settings/test-webhook")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&req_mock).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res2.status(), StatusCode::OK);
    let body2 = res2.into_body().collect().await.unwrap().to_bytes();
    let json2: serde_json::Value = serde_json::from_slice(&body2).unwrap();
    assert_eq!(json2["success"], false);
    assert!(json2["message"].as_str().unwrap().contains("推送失败"));
}

#[tokio::test]
async fn test_hosts_test_connection_api() {
    let state = test_state();
    let app = build_router(state);

    // 1. POST /api/hosts/test with empty hostname -> 400 Bad Request
    let req_empty = serde_json::json!({
        "hostname": "   ",
        "port": 22,
        "user": "root"
    });
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .header(
                    "Authorization",
                    "Bearer test-management-token-at-least-32-characters",
                )
                .method("POST")
                .uri("/api/hosts/test")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&req_empty).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], false);
    assert!(json["message"].as_str().unwrap().contains("不能为空"));

    // 2. POST /api/hosts/test with dummy unreachable host -> BAD_GATEWAY error handled cleanly
    let req_unreachable = serde_json::json!({
        "hostname": "127.0.0.1",
        "port": 54321,
        "user": "nonexistent_user",
        "password": "wrong_password"
    });
    let res2 = app
        .oneshot(
            Request::builder()
                .header(
                    "Authorization",
                    "Bearer test-management-token-at-least-32-characters",
                )
                .method("POST")
                .uri("/api/hosts/test")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&req_unreachable).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res2.status(), StatusCode::BAD_GATEWAY);
    let body2 = res2.into_body().collect().await.unwrap().to_bytes();
    let json2: serde_json::Value = serde_json::from_slice(&body2).unwrap();
    assert_eq!(json2["success"], false);
    assert!(
        json2["message"]
            .as_str()
            .unwrap()
            .contains("SSH connection failed")
    );
}

fn test_state() -> AppState {
    let mut state = AppState::new();
    state.gateway_auth = redash_server::auth::GatewayAuth::new(
        "test-management-token-at-least-32-characters",
        vec![],
    )
    .unwrap();
    state
}
