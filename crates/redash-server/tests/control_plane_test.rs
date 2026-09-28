use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use redash_server::build_router;
use redash_server::state::AppState;
use redash_types::{
    ActionResult, AgentHandshake, AgentTelemetry, HubToAgentMessage, ManagedNodeDetail,
    NodeOnlineStatus, RemediationAction, SignedAction,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc, oneshot};
use tower::ServiceExt;

#[tokio::test]
async fn test_control_plane_nodes_api() {
    let state = AppState::new();
    let app = build_router(state.clone());

    // 1. Initially empty
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/control/nodes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let nodes: Vec<ManagedNodeDetail> = serde_json::from_slice(&body).unwrap();
    assert!(nodes.is_empty());

    // 2. Register a simulated node in control plane
    let (cmd_tx, _cmd_rx) = mpsc::channel(16);
    let pending_actions = Arc::new(Mutex::new(HashMap::new()));
    let handshake = AgentHandshake {
        node_id: "node-vps-1".to_string(),
        hostname: "vps-fra-1".to_string(),
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        version: "0.1.1-beta".to_string(),
        auth_token: "secret".to_string(),
        trusted_public_key: "abc123key".to_string(),
    };

    state.control_plane.register_agent(
        handshake,
        "192.168.1.50:44300".to_string(),
        cmd_tx,
        pending_actions,
    );

    // 3. Record telemetry
    let telemetry = AgentTelemetry {
        node_id: "node-vps-1".to_string(),
        hostname: "vps-fra-1".to_string(),
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        timestamp: 1700000000,
        uptime_secs: 3600,
        cpu_usage_pct: 12.5,
        cpu_cores: 4,
        mem_used_bytes: 1024 * 1024 * 512,
        mem_total_bytes: 1024 * 1024 * 2048,
        disk_used_bytes: 1024 * 1024 * 1024 * 10,
        disk_total_bytes: 1024 * 1024 * 1024 * 50,
        net_rx_rate: 512,
        net_tx_rate: 256,
        containers: vec![],
    };
    state.control_plane.record_telemetry(telemetry);

    // 4. Query GET /v1/control/nodes
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/control/nodes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let nodes: Vec<ManagedNodeDetail> = serde_json::from_slice(&body).unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].node_id, "node-vps-1");
    assert_eq!(nodes[0].status, NodeOnlineStatus::Online);
    assert!(nodes[0].latest_telemetry.is_some());

    // 5. Query GET /v1/control/nodes/node-vps-1
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/control/nodes/node-vps-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let node: ManagedNodeDetail = serde_json::from_slice(&body).unwrap();
    assert_eq!(node.node_id, "node-vps-1");
    assert_eq!(node.latest_telemetry.unwrap().cpu_usage_pct, 12.5);
}

#[tokio::test]
async fn test_control_plane_action_dispatch_roundtrip() {
    let state = AppState::new();

    let (cmd_tx, mut cmd_rx) = mpsc::channel(16);
    let pending_actions = Arc::new(Mutex::new(
        HashMap::<String, oneshot::Sender<ActionResult>>::new(),
    ));
    let handshake = AgentHandshake {
        node_id: "node-homelab-1".to_string(),
        hostname: "nas-home".to_string(),
        os: "linux".to_string(),
        arch: "aarch64".to_string(),
        version: "0.1.1-beta".to_string(),
        auth_token: "secret".to_string(),
        trusted_public_key: "key-123".to_string(),
    };

    state.control_plane.register_agent(
        handshake,
        "10.0.0.12:55432".to_string(),
        cmd_tx,
        pending_actions.clone(),
    );

    // Mock agent loop that listens for commands and replies
    let pending_clone = pending_actions.clone();
    tokio::spawn(async move {
        if let Some(HubToAgentMessage::ExecuteAction(signed_action)) = cmd_rx.recv().await {
            let result = ActionResult {
                action_id: signed_action.action_id.clone(),
                node_id: signed_action.node_id.clone(),
                success: true,
                exit_code: Some(0),
                stdout: "Container restarted successfully".to_string(),
                stderr: String::new(),
                duration_ms: 45,
            };

            // Complete action
            let mut map = pending_clone.lock().await;
            if let Some(tx) = map.remove(&signed_action.action_id) {
                let _ = tx.send(result);
            }
        }
    });

    let action = SignedAction {
        action_id: "act-restart-101".to_string(),
        node_id: "node-homelab-1".to_string(),
        action: RemediationAction::RestartContainer {
            container_id: "c-plex".to_string(),
        },
        timestamp: 1700000000,
        nonce: "nonce-999".to_string(),
        public_key_hex: "pk-hex".to_string(),
        signature_hex: "sig-hex".to_string(),
    };

    let result = state
        .control_plane
        .dispatch_action(action, 5)
        .await
        .expect("Dispatch should succeed");

    assert!(result.success);
    assert_eq!(result.exit_code, Some(0));
    assert!(result.stdout.contains("Container restarted"));
}
