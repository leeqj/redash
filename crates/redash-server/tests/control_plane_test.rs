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
    let mut state = AppState::new();

    state.control_plane = registry_for("node-vps-1", "abc123key");
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
        version: "0.2.0-beta".to_string(),
        auth_token: "test-secret-with-at-least-32-bytes!".to_string(),
        trusted_public_key: "abc123key".to_string(),
    };

    let connection_id = state
        .control_plane
        .register_agent(
            handshake,
            "192.168.1.50:44300".to_string(),
            cmd_tx,
            pending_actions,
        )
        .unwrap();

    // 3. Record telemetry
    let telemetry = AgentTelemetry {
        validity: redash_types::TelemetryValidity {
            cpu: true,
            memory: true,
            disk: true,
            network: true,
        },
        node_id: "node-vps-1".to_string(),
        hostname: "vps-fra-1".to_string(),
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        timestamp: redash_ui_core::e2ee::now_secs(),
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
    assert!(
        state
            .control_plane
            .record_telemetry("node-vps-1", &connection_id, telemetry)
    );

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
    let mut state = AppState::new();
    state.control_plane = registry_for("node-homelab-1", "key-123");

    let (cmd_tx, mut cmd_rx) = mpsc::channel(16);
    let pending_actions = Arc::new(Mutex::new(
        HashMap::<String, oneshot::Sender<ActionResult>>::new(),
    ));
    let handshake = AgentHandshake {
        node_id: "node-homelab-1".to_string(),
        hostname: "nas-home".to_string(),
        os: "linux".to_string(),
        arch: "aarch64".to_string(),
        version: "0.2.0-beta".to_string(),
        auth_token: "test-secret-with-at-least-32-bytes!".to_string(),
        trusted_public_key: "key-123".to_string(),
    };

    let _connection_id = state
        .control_plane
        .register_agent(
            handshake,
            "10.0.0.12:55432".to_string(),
            cmd_tx,
            pending_actions.clone(),
        )
        .unwrap();

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
        timestamp: redash_ui_core::e2ee::now_secs(),
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

#[tokio::test]
async fn test_control_plane_tty_e2ee_blind_forwarding() {
    let mut state = AppState::new();
    state.control_plane = registry_for("node-e2ee-box", "trusted-pub");

    // 1. Register agent
    let (cmd_tx, mut cmd_rx) = mpsc::channel(16);
    let pending_actions = Arc::new(Mutex::new(HashMap::new()));
    let handshake = AgentHandshake {
        node_id: "node-e2ee-box".to_string(),
        hostname: "e2ee-host".to_string(),
        os: "linux".to_string(),
        arch: "arm64".to_string(),
        version: "0.2.0-beta".to_string(),
        auth_token: "test-secret-with-at-least-32-bytes!".to_string(),
        trusted_public_key: "trusted-pub".to_string(),
    };

    let connection_id = state
        .control_plane
        .register_agent(
            handshake,
            "10.0.0.99:50000".to_string(),
            cmd_tx,
            pending_actions,
        )
        .unwrap();

    // 2. Client registers TTY subscriber
    let session_id = "session-e2ee-999".to_string();
    let (downstream_tx, mut downstream_rx) = mpsc::channel(16);
    state
        .control_plane
        .register_tty_subscriber("node-e2ee-box", session_id.clone(), downstream_tx)
        .unwrap();

    // 3. Agent sends EncryptedEnvelope to Hub
    let agent_env = redash_types::EncryptedEnvelope::new(
        &session_id,
        1,
        "0102030405060708090a0b0c",
        "deadbeefcafebabe",
        "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899",
    );
    state
        .control_plane
        .forward_tty_encrypted("node-e2ee-box", &connection_id, agent_env.clone());

    // 4. Client receives downstream Text frame with unaltered ciphertext
    let received = downstream_rx
        .recv()
        .await
        .expect("Must receive downstream message");
    match received {
        redash_server::control_plane::registry::TtyDownstreamMsg::Text(json) => {
            let received_env: redash_types::EncryptedEnvelope =
                serde_json::from_str(&json).unwrap();
            assert_eq!(received_env.session_id, session_id);
            assert_eq!(received_env.seq_num, 1);
            assert_eq!(received_env.ciphertext_hex, agent_env.ciphertext_hex);
            assert_eq!(received_env.tag_hex, agent_env.tag_hex);
        }
        _ => panic!("Expected Text message containing encrypted envelope JSON"),
    }

    // 5. Client sends EncryptedEnvelope towards Agent through Hub
    let client_env = redash_types::EncryptedEnvelope::new(
        &session_id,
        2,
        "0c0b0a090807060504030201",
        "1234567890abcdef",
        "99887766554433221100ffeeddccbbaa99887766554433221100ffeeddccbbaa",
    );
    state
        .control_plane
        .send_tty_encrypted_to_node("node-e2ee-box", client_env.clone())
        .await
        .unwrap();

    // 6. Agent receives HubToAgentMessage::TtyEncrypted with exact envelope
    let hub_msg = cmd_rx
        .recv()
        .await
        .expect("Agent must receive forwarded frame");
    match hub_msg {
        HubToAgentMessage::TtyEncrypted(received_from_hub) => {
            assert_eq!(received_from_hub.session_id, session_id);
            assert_eq!(received_from_hub.seq_num, 2);
            assert_eq!(received_from_hub.ciphertext_hex, client_env.ciphertext_hex);
            assert_eq!(received_from_hub.tag_hex, client_env.tag_hex);
        }
        other => panic!("Expected TtyEncrypted, got {:?}", other),
    }
}

fn registry_for(
    node_id: &str,
    public_key: &str,
) -> redash_server::control_plane::registry::ControlPlaneRegistry {
    use redash_server::control_plane::registry::{ControlPlaneRegistry, Enrollment};
    ControlPlaneRegistry::with_enrollments(HashMap::from([(
        node_id.into(),
        Enrollment {
            auth_token: "test-secret-with-at-least-32-bytes!".into(),
            trusted_public_key: public_key.into(),
        },
    )]))
}
