/// Pure function determining the next telemetry sampling cadence based on real-time load volatility.
/// - Returns 1s under critical stress (CPU >= 80%, RAM >= 85%), rapid metric shifts (ΔRAM >= 2%, ΔCPU >= 5%),
///   or container state transitions/unhealthy states.
/// - Returns base_interval (typically 5s~10s) during steady state to minimize CPU & bandwidth usage.
pub fn calculate_adaptive_cadence(
    base_interval_secs: u64,
    current: &redash_types::AgentTelemetry,
    last_cpu: Option<f32>,
    last_mem_pct: Option<f32>,
    last_containers_snapshot: Option<&[(String, String)]>,
) -> Duration {
    let cur_cpu = current.cpu_usage_pct;
    let cur_mem_pct = current.memory_usage_pct();

    // 1. Critical threshold stress (> 80% CPU or > 85% RAM)
    if cur_cpu >= 80.0 || cur_mem_pct >= 85.0 {
        return Duration::from_secs(1);
    }

    // 2. Metric rapid delta (CPU change >= 5.0% or RAM change >= 2.0%)
    if let Some(prev_cpu) = last_cpu
        && (cur_cpu - prev_cpu).abs() >= 5.0
    {
        return Duration::from_secs(1);
    }
    if let Some(prev_mem) = last_mem_pct
        && (cur_mem_pct - prev_mem).abs() >= 2.0
    {
        return Duration::from_secs(1);
    }

    // 3. Container state transitions or unhealthy containers
    let cur_containers: Vec<(String, String)> = current
        .containers
        .iter()
        .map(|c| (c.id.clone(), c.state.clone()))
        .collect();

    // If any container is not in running state, trigger high frequency
    if cur_containers.iter().any(|(_, s)| s != "running") {
        return Duration::from_secs(1);
    }

    // If container list or state transitioned compared to previous sample
    if let Some(prev) = last_containers_snapshot
        && prev != cur_containers.as_slice()
    {
        return Duration::from_secs(1);
    }

    // Steady state: low load & stable metrics
    Duration::from_secs(base_interval_secs.max(1))
}

use crate::collector::TelemetryCollector;
use crate::remediation::RemediationEngine;
use crate::tty::TtyManager;
use futures::{SinkExt, StreamExt};
use log::{info, warn};
use redash_types::{
    AgentHandshake, AgentToHubMessage, HubToAgentMessage,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

pub struct AgentConfig {
    pub hub_url: String,
    pub node_id: String,
    pub auth_token: String,
    pub trusted_public_key: Option<String>,
    pub telemetry_interval_secs: u64,
    pub identity_private_key: Option<String>,
}

pub struct AgentClient {
    config: AgentConfig,
    remediation: Arc<RemediationEngine>,
    tty_mgr: Arc<TtyManager>,
}

impl AgentClient {
    pub fn new(mut config: AgentConfig) -> anyhow::Result<Self> {
        config.node_id = config.node_id.trim().to_string();
        anyhow::ensure!(!config.node_id.is_empty() && config.node_id.len() <= 128 && config.node_id.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)), "Node ID must be nonempty ASCII letters, digits, '-', '_' or '.'");
        anyhow::ensure!(!config.auth_token.trim().is_empty() && config.auth_token != "default-token", "A provisioned per-node enrollment token is required");
        config.trusted_public_key = config.trusted_public_key.filter(|s| !s.trim().is_empty());
        config.identity_private_key = config.identity_private_key.filter(|s| !s.trim().is_empty());
        if let Some(identity) = &config.identity_private_key {
            anyhow::ensure!(hex::decode(identity)?.len() == 32, "Agent identity private key must be 32 bytes");
        }
        let remediation = Arc::new(RemediationEngine::new(
            config.trusted_public_key.as_deref(),
        )?);
        let tty_mgr = Arc::new(TtyManager::new());

        Ok(Self {
            config,
            remediation,
            tty_mgr,
        })
    }

    /// Runs the agent event loop with auto-reconnection.
    pub async fn run_forever(&self) {
        let mut backoff_secs = 1u64;

        let hostname = gethostname::gethostname()
            .to_string_lossy()
            .into_owned();

        if let (Some(trusted), Some(identity)) = (&self.config.trusted_public_key, &self.config.identity_private_key) {
            crate::discovery::LanDiscoveryAgent::start(
                self.config.node_id.clone(), hostname, env!("CARGO_PKG_VERSION").to_string(),
                Some(trusted.clone()), identity.clone(), self.tty_mgr.clone(),
            ).await.unwrap_or_else(|err| { warn!("LAN listener failed: {}", err); (0, tokio::spawn(async {})) });
        }
        let mgr = self.tty_mgr.clone();
        tokio::spawn(async move {
            loop { sleep(Duration::from_secs(5)).await; mgr.expire_sessions().await; }
        });

        loop {
            info!("Dialing ReDash Control Hub at {}", self.config.hub_url);

            match self.connect_and_serve().await {
                Ok(_) => {
                    info!("Connection closed cleanly, reconnecting in 2s...");
                    backoff_secs = 1;
                    sleep(Duration::from_secs(2)).await;
                }
                Err(e) => {
                    warn!("Connection error: {}. Backoff {}s...", e, backoff_secs);
                    sleep(Duration::from_secs(backoff_secs)).await;
                    backoff_secs = (backoff_secs * 2).min(30);
                }
            }
        }
    }

    pub async fn connect_and_serve(&self) -> anyhow::Result<()> {
        let (ws_stream, _) = connect_async(&self.config.hub_url).await?;
        info!("WebSocket connection established with Hub");

        let (mut write_half, mut read_half) = ws_stream.split();
        let (outbound_tx, mut outbound_rx) = mpsc::channel::<AgentToHubMessage>(128);

        // 1. Initial Handshake
        let hostname = gethostname::gethostname()
            .to_string_lossy()
            .into_owned();

        let handshake = AgentHandshake {
            node_id: self.config.node_id.clone(),
            hostname,
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            auth_token: self.config.auth_token.clone(),
            trusted_public_key: self.config.trusted_public_key.clone().unwrap_or_default(),
        };

        let handshake_json = serde_json::to_string(&AgentToHubMessage::Handshake(handshake))?;
        write_half.send(Message::Text(handshake_json.into())).await?;

        let reply = tokio::time::timeout(Duration::from_secs(5), read_half.next()).await?
            .ok_or_else(|| anyhow::anyhow!("Hub closed during authentication"))??;
        let Message::Text(reply) = reply else { anyhow::bail!("Expected Hub authentication acknowledgement"); };
        anyhow::ensure!(matches!(serde_json::from_str::<HubToAgentMessage>(&reply)?, HubToAgentMessage::HandshakeAck { success: true, .. }), "Hub rejected enrollment credentials");

        // 2. Outbound message sink task
        let write_task = tokio::spawn(async move {
            while let Some(msg) = outbound_rx.recv().await {
                if let Ok(json) = serde_json::to_string(&msg) {
                    let text_msg = Message::Text(json.into());
                    if let Err(e) = write_half.send(text_msg).await {
                        warn!("Failed to send frame to Hub: {}", e);
                        break;
                    }
                }
            }
        });

        // 3. Telemetry reporting task with Adaptive Cadence
        let node_id = self.config.node_id.clone();
        let interval_secs = self.config.telemetry_interval_secs.max(1);
        let tx_telemetry = outbound_tx.clone();

        let telemetry_task = tokio::spawn(async move {
            let mut collector = TelemetryCollector::new(node_id);
            let mut last_cpu: Option<f32> = None;
            let mut last_mem_pct: Option<f32> = None;
            let mut last_containers: Option<Vec<(String, String)>> = None;

            loop {
                let snapshot = collector.collect().await;
                let cadence = calculate_adaptive_cadence(
                    interval_secs,
                    &snapshot,
                    last_cpu,
                    last_mem_pct,
                    last_containers.as_deref(),
                );

                last_cpu = Some(snapshot.cpu_usage_pct);
                last_mem_pct = Some(snapshot.memory_usage_pct());
                last_containers = Some(
                    snapshot
                        .containers
                        .iter()
                        .map(|c| (c.id.clone(), c.state.clone()))
                        .collect(),
                );

                if tx_telemetry
                    .send(AgentToHubMessage::Telemetry(snapshot))
                    .await
                    .is_err()
                {
                    break;
                }
                sleep(cadence).await;
            }
        });

        let route = format!("hub-{}", uuid::Uuid::new_v4());
        while let Some(msg_res) = read_half.next().await {
            match msg_res {
                Ok(Message::Text(text)) => match serde_json::from_str::<HubToAgentMessage>(&text) {
                    Ok(HubToAgentMessage::ExecuteAction(action)) => {
                        if action.node_id != self.config.node_id { continue; }
                        // Legacy TTY actions are rejected by RemediationEngine; they cannot bypass E2EE.
                        let rem = self.remediation.clone();
                        let tx = outbound_tx.clone();
                        tokio::spawn(async move {
                            let result = rem.execute(action).await;
                            let _ = tx.send(AgentToHubMessage::ActionResult(result)).await;
                        });
                    }
                    Ok(HubToAgentMessage::E2eeHandshakeInit(init)) => {
                        let result = match (&self.config.trusted_public_key, &self.config.identity_private_key) {
                            (Some(trusted), Some(identity)) => self.tty_mgr.accept_handshake(&init, trusted, identity, &self.config.node_id, &route, outbound_tx.clone()).await,
                            _ => Err("Terminal identity and client trust key must be provisioned".into()),
                        };
                        let ack = result.unwrap_or_else(|err| redash_types::E2eeHandshakeAck {
                            session_id: init.session_id, agent_ephemeral_pubkey_hex: String::new(), signature_hex: String::new(), success: false, error_msg: Some(err),
                        });
                        let _ = outbound_tx.send(AgentToHubMessage::E2eeHandshakeAck(ack)).await;
                    }
                    Ok(HubToAgentMessage::TtyEncrypted(env)) => {
                        if let Err(err) = self.tty_mgr.receive(&env, &route).await {
                            warn!("Rejected terminal frame: {}", err);
                            self.tty_mgr.close_session(&env.session_id, &route).await;
                        }
                    }
                    // Relay disconnect is allowed to release resources, never to open or resize a shell.
                    Ok(HubToAgentMessage::TtyClose { session_id }) => self.tty_mgr.close_session(&session_id, &route).await,
                    Ok(HubToAgentMessage::Ping) => { let _ = outbound_tx.send(AgentToHubMessage::Heartbeat).await; }
                    _ => {} // Plaintext input/resize and legacy open are never executed.
                },
                Ok(Message::Close(_)) | Err(_) => break,
                _ => {}
            }
        }

        // Abort background loops on disconnect
        write_task.abort();
        telemetry_task.abort();
        self.tty_mgr.close_route(&route).await;

        Ok(())
    }
}
