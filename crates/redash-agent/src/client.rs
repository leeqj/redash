use crate::collector::TelemetryCollector;
use crate::remediation::RemediationEngine;
use crate::tty::TtyManager;
use futures::{SinkExt, StreamExt};
use log::{error, info, warn};
use redash_types::{
    AgentHandshake, AgentToHubMessage, HubToAgentMessage, RemediationAction,
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
}

pub struct AgentClient {
    config: AgentConfig,
    remediation: Arc<RemediationEngine>,
}

impl AgentClient {
    pub fn new(config: AgentConfig) -> anyhow::Result<Self> {
        let remediation = Arc::new(RemediationEngine::new(
            config.trusted_public_key.as_deref(),
        )?);

        Ok(Self {
            config,
            remediation,
        })
    }

    /// Runs the agent event loop with auto-reconnection.
    pub async fn run_forever(&self) {
        let mut backoff_secs = 1u64;

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

    async fn connect_and_serve(&self) -> anyhow::Result<()> {
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

        // 3. Telemetry reporting task
        let node_id = self.config.node_id.clone();
        let interval_secs = self.config.telemetry_interval_secs.max(1);
        let tx_telemetry = outbound_tx.clone();

        let telemetry_task = tokio::spawn(async move {
            let mut collector = TelemetryCollector::new(node_id);
            loop {
                let snapshot = collector.collect().await;
                if tx_telemetry
                    .send(AgentToHubMessage::Telemetry(snapshot))
                    .await
                    .is_err()
                {
                    break;
                }
                sleep(Duration::from_secs(interval_secs)).await;
            }
        });

        // 4. Inbound message loop (Actions, TTY, Ping)
        let tty_mgr = Arc::new(TtyManager::new(outbound_tx.clone()));
        let remediation = self.remediation.clone();
        let tx_actions = outbound_tx.clone();

        while let Some(msg_res) = read_half.next().await {
            match msg_res {
                Ok(Message::Text(text)) => {
                    match serde_json::from_str::<HubToAgentMessage>(&text) {
                        Ok(HubToAgentMessage::ExecuteAction(signed_action)) => {
                            let rem = remediation.clone();
                            let tx = tx_actions.clone();
                            let tty = tty_mgr.clone();

                            tokio::spawn(async move {
                                match signed_action.action {
                                    RemediationAction::TtyOpen {
                                        session_id,
                                        rows,
                                        cols,
                                    } => {
                                        tty.open_session(session_id, rows, cols).await;
                                    }
                                    _ => {
                                        let result = rem.execute(signed_action).await;
                                        let _ = tx.send(AgentToHubMessage::ActionResult(result)).await;
                                    }
                                }
                            });
                        }
                        Ok(HubToAgentMessage::TtyInput { session_id, data }) => {
                            tty_mgr.write_input(&session_id, &data).await;
                        }
                        Ok(HubToAgentMessage::TtyClose { session_id }) => {
                            tty_mgr.close_session(&session_id).await;
                        }
                        Ok(HubToAgentMessage::Ping) => {
                            let _ = outbound_tx.send(AgentToHubMessage::Heartbeat).await;
                        }
                        Ok(HubToAgentMessage::HandshakeAck { success, message, .. }) => {
                            if success {
                                info!("Hub handshake acknowledged: {}", message);
                            } else {
                                error!("Hub handshake rejected: {}", message);
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Message::Ping(_)) => {
                    let _ = outbound_tx.send(AgentToHubMessage::Heartbeat).await;
                }
                Ok(Message::Close(_)) => {
                    info!("Server sent close frame");
                    break;
                }
                Err(e) => {
                    warn!("WebSocket error from agent {}: {}", nid_err(&self.config.node_id), e);
                    break;
                }
                _ => {}
            }
        }

        // Abort background loops on disconnect
        write_task.abort();
        telemetry_task.abort();

        Ok(())
    }
}

fn nid_err(id: &str) -> &str {
    id
}
