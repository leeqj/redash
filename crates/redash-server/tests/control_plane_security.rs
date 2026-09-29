use futures::{SinkExt, StreamExt};
use redash_agent::{AgentClient, AgentConfig};
use redash_core::session::pty::{PtyChannel, TerminalIdentity};
use redash_server::{
    build_router,
    control_plane::registry::{ControlPlaneRegistry, Enrollment},
    state::AppState,
};
use redash_types::*;
use redash_ui_core::{
    control_plane::ClientSigner,
    e2ee::{ClientE2eeSession, now_secs, random_hex},
};
use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::mpsc};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const NODE: &str = "security-test-node";

async fn output_until(rx: &mut mpsc::Receiver<Vec<u8>>, needle: &str) -> String {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut output = String::new();
        loop {
            let data = rx.recv().await.expect("terminal unexpectedly closed");
            output.push_str(&String::from_utf8_lossy(&data));
            if output.contains(needle) {
                return output;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {needle}"))
}

async fn exercise_pty(url: &str, identity: &TerminalIdentity) {
    let (tx, mut rx) = mpsc::channel(128);
    let channel = PtyChannel::new_reverse_ws_with_auth(url, 80, 24, tx, Some(identity))
        .await
        .unwrap();
    channel
        .send_data(b"stty -echo; test -t 0 && printf 'REAL_%s\\n' PTY\n")
        .await
        .unwrap();
    output_until(&mut rx, "REAL_PTY").await;
    channel.resize(91, 37).await.unwrap();
    channel.send_data(b"stty size\n").await.unwrap();
    output_until(&mut rx, "37 91").await;
    channel.send_data(b"sleep 30\n").await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    channel
        .send_data(b"\x03printf 'INTERRUPT_%s\\n' OK\n")
        .await
        .unwrap();
    output_until(&mut rx, "INTERRUPT_OK").await;
    channel.send_data(b"exit\n").await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !channel.is_closed() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

async fn reject_plaintext(url: &str) {
    let (mut ws, _) = connect_async(url).await.unwrap();
    ws.send(Message::Text("printf 'UNAUTHORISED_EXECUTION\\n'\n".into()))
        .await
        .unwrap();
    let reply = tokio::time::timeout(Duration::from_secs(2), ws.next())
        .await
        .unwrap();
    assert!(
        matches!(reply, None | Some(Err(_)) | Some(Ok(Message::Close(_)))),
        "anonymous input received a response: {reply:?}"
    );
}

#[tokio::test]
async fn real_client_hub_agent_pty_and_anonymous_rejection() {
    let (client_public, client_private) = ClientSigner::generate_keypair();
    let (agent_public, agent_private) = ClientSigner::generate_keypair();
    let token = random_hex();
    let mut state = AppState::new();
    state.control_plane = ControlPlaneRegistry::with_enrollments(HashMap::from([(
        NODE.into(),
        Enrollment {
            auth_token: token.clone(),
            trusted_public_key: client_public.clone(),
        },
    )]));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = build_router(state.clone());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let agent = Arc::new(
        AgentClient::new(AgentConfig {
            hub_url: format!("ws://{addr}/v1/agent/ws"),
            node_id: NODE.into(),
            auth_token: token.clone(),
            trusted_public_key: Some(client_public.clone()),
            identity_private_key: Some(agent_private),
            telemetry_interval_secs: 1,
        })
        .unwrap(),
    );
    let running_agent = agent.clone();
    let agent_task = tokio::spawn(async move {
        running_agent.connect_and_serve().await.unwrap();
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        while state.control_plane.get_node(NODE).is_none() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    for (id, credential) in [
        ("unknown-node", token.clone()),
        (NODE, random_hex()),
        (NODE, token.clone()),
    ] {
        let (mut duplicate, _) = connect_async(format!("ws://{addr}/v1/agent/ws"))
            .await
            .unwrap();
        let hs = AgentToHubMessage::Handshake(AgentHandshake {
            node_id: id.into(),
            hostname: "impostor".into(),
            os: "test".into(),
            arch: "test".into(),
            version: "2".into(),
            auth_token: credential,
            trusted_public_key: client_public.clone(),
        });
        duplicate
            .send(Message::Text(serde_json::to_string(&hs).unwrap().into()))
            .await
            .unwrap();
        let reply = duplicate
            .next()
            .await
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap();
        assert!(matches!(
            serde_json::from_str::<HubToAgentMessage>(&reply).unwrap(),
            HubToAgentMessage::HandshakeAck { success: false, .. }
        ));
    }
    let url = format!("ws://{addr}/v1/control/tty/{NODE}");
    reject_plaintext(&url).await;
    let identity = TerminalIdentity {
        node_id: NODE.into(),
        client_private_key: client_private.clone(),
        agent_public_key: agent_public,
    };
    exercise_pty(&url, &identity).await;

    // Legacy unsigned open must be rejected even though the node has a trusted key.
    let result = state
        .control_plane
        .dispatch_action(
            SignedAction {
                action_id: "unsigned-open".into(),
                node_id: NODE.into(),
                action: RemediationAction::TtyOpen {
                    session_id: "legacy".into(),
                    rows: 24,
                    cols: 80,
                },
                timestamp: now_secs(),
                nonce: random_hex(),
                public_key_hex: String::new(),
                signature_hex: String::new(),
            },
            3,
        )
        .await
        .unwrap();
    assert!(!result.success);

    // Signed remediation still works, with unique IDs/nonces for two actions in one second.
    for _ in 0..2 {
        let action = ClientSigner::sign_fresh_action(
            &client_private,
            NODE,
            RemediationAction::ExecuteRecipe {
                name: "test".into(),
                script: "printf governance_ok".into(),
            },
        )
        .unwrap();
        let result = state
            .control_plane
            .dispatch_action(action, 3)
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.stdout, "governance_ok");
    }
    agent_task.abort();
    tokio::time::timeout(Duration::from_secs(3), async {
        while state.control_plane.get_node(NODE).unwrap().status != NodeOnlineStatus::Offline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(state.control_plane.sweep_health(), vec![NODE]);
    let reconnected = tokio::spawn(async move {
        agent.connect_and_serve().await.unwrap();
    });
    tokio::time::timeout(Duration::from_secs(3), async {
        while state.control_plane.get_node(NODE).unwrap().status != NodeOnlineStatus::Online {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    exercise_pty(&url, &identity).await;
    reconnected.abort();
    server.abort();
}

#[tokio::test]
async fn direct_pty_rejects_bad_identity_replay_and_cross_session_frames() {
    let (cp, cs) = ClientSigner::generate_keypair();
    let (ap, ass) = ClientSigner::generate_keypair();
    let manager = Arc::new(redash_agent::tty::TtyManager::new());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let mgr = manager.clone();
    let server = tokio::spawn(async move {
        while let Ok((stream, addr)) = listener.accept().await {
            let (cp, ass, mgr) = (cp.clone(), ass.clone(), mgr.clone());
            tokio::spawn(async move {
                let _ = redash_agent::discovery::handle_direct_connection(
                    stream,
                    addr,
                    mgr,
                    cp,
                    ass,
                    NODE.into(),
                )
                .await;
            });
        }
    });
    reject_plaintext(&url).await;
    assert_eq!(manager.session_count().await, 0);
    let identity = TerminalIdentity {
        node_id: NODE.into(),
        client_private_key: cs.clone(),
        agent_public_key: ap.clone(),
    };
    exercise_pty(&url, &identity).await;
    let (_, wrong_private) = ClientSigner::generate_keypair();
    let bad = TerminalIdentity {
        client_private_key: wrong_private,
        ..identity.clone()
    };
    let (tx, _) = mpsc::channel(10);
    assert!(
        PtyChannel::new_reverse_ws_with_auth(&url, 80, 24, tx, Some(&bad))
            .await
            .is_err()
    );

    let (mut client, init) = ClientE2eeSession::initiate("manual-session", NODE, &cs, &ap).unwrap();
    let (mut ws, _) = connect_async(&url).await.unwrap();
    ws.send(Message::Text(serde_json::to_string(&init).unwrap().into()))
        .await
        .unwrap();
    let ack = ws.next().await.unwrap().unwrap().into_text().unwrap();
    client
        .complete_handshake(&serde_json::from_str(&ack).unwrap())
        .unwrap();
    let (mut replay, _) = connect_async(&url).await.unwrap();
    replay
        .send(Message::Text(serde_json::to_string(&init).unwrap().into()))
        .await
        .unwrap();
    assert!(matches!(
        replay.next().await,
        None | Some(Err(_)) | Some(Ok(Message::Close(_)))
    ));
    let mut env = client
        .seal(&serde_json::to_vec(&TtyClientFrame::Open { rows: 24, cols: 80 }).unwrap())
        .unwrap();
    env.session_id = "other-session".into();
    ws.send(Message::Text(serde_json::to_string(&env).unwrap().into()))
        .await
        .unwrap();
    assert!(matches!(
        ws.next().await,
        None | Some(Err(_)) | Some(Ok(Message::Close(_)))
    ));
    tokio::time::timeout(Duration::from_secs(2), async {
        while manager.session_count().await != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    server.abort();
}

#[tokio::test]
async fn registration_identity_disconnect_and_reconnect_are_bound() {
    let registry = ControlPlaneRegistry::with_enrollments(HashMap::from([(
        NODE.into(),
        Enrollment {
            auth_token: random_hex(),
            trusted_public_key: String::new(),
        },
    )]));
    let (tx, _rx) = mpsc::channel(10);
    let mut hs = AgentHandshake {
        node_id: NODE.into(),
        hostname: "test".into(),
        os: "test".into(),
        arch: "test".into(),
        version: "2".into(),
        auth_token: "wrong-token".into(),
        trusted_public_key: String::new(),
    };
    let pending = Arc::default();
    assert!(
        registry
            .register_agent(hs.clone(), "local".into(), tx.clone(), pending)
            .is_err()
    );
    let token = random_hex();
    let registry = ControlPlaneRegistry::with_enrollments(HashMap::from([(
        NODE.into(),
        Enrollment {
            auth_token: token.clone(),
            trusted_public_key: String::new(),
        },
    )]));
    hs.auth_token = token;
    let old = registry
        .register_agent(hs.clone(), "local".into(), tx.clone(), Arc::default())
        .unwrap();
    assert!(
        registry
            .register_agent(hs.clone(), "local".into(), tx.clone(), Arc::default())
            .is_err()
    );
    registry.unregister_agent(NODE, &old);
    assert_eq!(registry.list_nodes().len(), 1);
    assert_eq!(
        registry.get_node(NODE).unwrap().status,
        NodeOnlineStatus::Offline
    );
    assert_eq!(registry.sweep_health(), vec![NODE]);
    assert!(registry.sweep_health().is_empty());
    let new = registry
        .register_agent(hs, "local".into(), tx, Arc::default())
        .unwrap();
    registry.unregister_agent(NODE, &old);
    assert_eq!(
        registry.get_node(NODE).unwrap().status,
        NodeOnlineStatus::Online
    );
    assert_ne!(new, old);
}

fn sample(node: &str) -> AgentTelemetry {
    AgentTelemetry {
        node_id: node.into(),
        hostname: node.into(),
        timestamp: now_secs(),
        validity: TelemetryValidity {
            cpu: true,
            memory: true,
            disk: true,
            network: true,
        },
        cpu_usage_pct: 95.0,
        mem_used_bytes: 95,
        mem_total_bytes: 100,
        disk_used_bytes: 95,
        disk_total_bytes: 100,
        ..Default::default()
    }
}

#[tokio::test]
async fn telemetry_and_terminal_output_cannot_cross_connection_identity() {
    let token = random_hex();
    let registry = ControlPlaneRegistry::with_enrollments(HashMap::from([(
        NODE.into(),
        Enrollment {
            auth_token: token.clone(),
            trusted_public_key: String::new(),
        },
    )]));
    let (tx, _rx) = mpsc::channel(10);
    let hs = AgentHandshake {
        node_id: NODE.into(),
        hostname: NODE.into(),
        os: "linux".into(),
        arch: "test".into(),
        version: "2".into(),
        auth_token: token,
        trusted_public_key: String::new(),
    };
    let id = registry
        .register_agent(hs, "local".into(), tx, Arc::default())
        .unwrap();
    let mut telemetry = registry.subscribe_telemetry();
    assert!(!registry.record_telemetry(NODE, &id, sample("victim")));
    assert!(!registry.record_telemetry(NODE, "old-connection", sample(NODE)));
    assert!(telemetry.try_recv().is_err());
    assert!(registry.record_telemetry(NODE, &id, sample(NODE)));
    assert_eq!(telemetry.recv().await.unwrap().node_id, NODE);
    let (tx, mut rx) = mpsc::channel(10);
    registry
        .register_tty_subscriber(NODE, "bound-session".into(), tx)
        .unwrap();
    let env = EncryptedEnvelope::new("bound-session", 1, "", "", "");
    registry.forward_tty_encrypted("victim", &id, env.clone());
    registry.forward_tty_encrypted(NODE, "old-connection", env.clone());
    assert!(rx.try_recv().is_err());
    registry.forward_tty_encrypted(NODE, &id, env);
    assert!(rx.recv().await.is_some());
}

#[tokio::test]
async fn agent_only_samples_dispatch_real_webhooks_with_validity_cooldown_and_recovery() {
    use redash_core::config::{HostStore, alert::AlertEvent};
    use tokio::sync::RwLock;
    let (events, mut received) = mpsc::channel::<AlertEvent>(32);
    let router = axum::Router::new().route(
        "/alert",
        axum::routing::post(move |axum::Json(event): axum::Json<AlertEvent>| {
            let events = events.clone();
            async move {
                events.send(event).await.unwrap();
                axum::http::StatusCode::OK
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let webhook = format!("http://{}/alert", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let token = random_hex();
    let mut state = AppState::new();
    state.host_store = Arc::new(RwLock::new(HostStore::new()));
    state.control_plane = ControlPlaneRegistry::with_enrollments(HashMap::from([(
        NODE.into(),
        Enrollment {
            auth_token: token.clone(),
            trusted_public_key: String::new(),
        },
    )]));
    {
        let mut settings = state.app_settings.write().await;
        settings.alert_webhook_url = Some(webhook);
        settings.alert_cpu_threshold = 80.0;
        settings.alert_mem_threshold = 80.0;
        settings.alert_disk_threshold = 80.0;
    }
    let (tx, _rx) = mpsc::channel(10);
    let id = state
        .control_plane
        .register_agent(
            AgentHandshake {
                node_id: NODE.into(),
                hostname: NODE.into(),
                os: "test".into(),
                arch: "test".into(),
                version: "2".into(),
                auth_token: token,
                trusted_public_key: String::new(),
            },
            "local".into(),
            tx,
            Arc::default(),
        )
        .unwrap();
    let monitor = redash_server::alert_monitor::start_alert_monitor(state.clone());
    let mut invalid = sample(NODE);
    invalid.validity = Default::default();
    assert!(state.control_plane.record_telemetry(NODE, &id, invalid));
    assert!(
        tokio::time::timeout(Duration::from_millis(150), received.recv())
            .await
            .is_err()
    );
    state
        .control_plane
        .record_telemetry(NODE, &id, sample(NODE));
    let mut kinds = Vec::new();
    for _ in 0..3 {
        kinds.push(
            tokio::time::timeout(Duration::from_secs(3), received.recv())
                .await
                .unwrap()
                .unwrap()
                .alert_type,
        );
    }
    kinds.sort();
    assert_eq!(kinds, ["cpu", "disk", "mem"]);
    state
        .control_plane
        .record_telemetry(NODE, &id, sample(NODE));
    assert!(
        tokio::time::timeout(Duration::from_millis(150), received.recv())
            .await
            .is_err()
    );
    let mut healthy = sample(NODE);
    healthy.cpu_usage_pct = 1.0;
    healthy.mem_used_bytes = 1;
    healthy.disk_used_bytes = 1;
    state.control_plane.record_telemetry(NODE, &id, healthy);
    state
        .control_plane
        .record_telemetry(NODE, &id, sample(NODE));
    for _ in 0..3 {
        assert!(
            tokio::time::timeout(Duration::from_secs(3), received.recv())
                .await
                .unwrap()
                .is_some()
        );
    }
    monitor.abort();
    server.abort();
}

#[tokio::test]
async fn wss_connects_with_trusted_certificate_and_rejects_untrusted_certificate() {
    use tokio_rustls::{
        TlsAcceptor,
        rustls::{self, pki_types::PrivatePkcs8KeyDer},
    };
    let _ = rustls::crypto::ring::default_provider().install_default();
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.der().clone()],
            PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into(),
        )
        .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(server_config));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("wss://localhost:{}", listener.local_addr().unwrap().port());
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (stream, _) = listener.accept().await.unwrap();
            if let Ok(stream) = acceptor.accept(stream).await {
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                let msg = ws.next().await.unwrap().unwrap();
                ws.send(msg).await.unwrap();
            }
        }
    });
    let err = connect_async(&url).await.unwrap_err();
    assert!(
        format!("{err:?}").contains("UnknownIssuer"),
        "expected certificate rejection, got {err:?}"
    );
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert.der().clone()).unwrap();
    let client = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let (mut ws, _) = tokio_tungstenite::connect_async_tls_with_config(
        &url,
        None,
        false,
        Some(tokio_tungstenite::Connector::Rustls(Arc::new(client))),
    )
    .await
    .unwrap();
    ws.send(Message::Text("TLS verified".into())).await.unwrap();
    assert_eq!(
        ws.next().await.unwrap().unwrap().into_text().unwrap(),
        "TLS verified"
    );
    server.await.unwrap();
}

#[tokio::test]
async fn http_executor_distinguishes_http_and_business_failures() {
    let router = axum::Router::new()
        .route(
            "/http-failure",
            axum::routing::post(|| async { (axum::http::StatusCode::FORBIDDEN, "forbidden") }),
        )
        .route(
            "/action-failure",
            axum::routing::post(|axum::Json(action): axum::Json<SignedAction>| async move {
                axum::Json(ActionResult {
                    action_id: action.action_id,
                    node_id: action.node_id,
                    success: false,
                    exit_code: Some(17),
                    stdout: String::new(),
                    stderr: "actual command error".into(),
                    duration_ms: 1,
                })
            }),
        );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let (_, key) = ClientSigner::generate_keypair();
    let action =
        ClientSigner::sign_fresh_action(&key, NODE, RemediationAction::DiagnosePort { port: 9090 })
            .unwrap();
    assert!(
        redash_core::control_plane::dispatch_action(&format!("{base}/http-failure"), &action)
            .await
            .unwrap_err()
            .to_string()
            .contains("403")
    );
    let result =
        redash_core::control_plane::dispatch_action(&format!("{base}/action-failure"), &action)
            .await
            .unwrap();
    assert!(!result.success);
    assert_eq!(result.exit_code, Some(17));
    assert_eq!(result.stderr, "actual command error");
    server.abort();
}
