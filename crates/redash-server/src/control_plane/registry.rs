use dashmap::DashMap;
use log::{info, warn};
use redash_types::{
    ActionResult, AgentHandshake, AgentTelemetry, HubToAgentMessage, ManagedNodeDetail,
    NodeOnlineStatus, SignedAction,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};

pub struct ActiveAgentSession {
    pub handshake: AgentHandshake,
    pub remote_ip: String,
    pub connected_at: u64,
    pub last_heartbeat_at: u64,
    pub status: NodeOnlineStatus,
    pub latest_telemetry: Option<AgentTelemetry>,
    pub command_tx: mpsc::Sender<HubToAgentMessage>,
    pub pending_actions: Arc<Mutex<HashMap<String, oneshot::Sender<ActionResult>>>>,
}

#[derive(Clone)]
pub struct ControlPlaneRegistry {
    agents: Arc<DashMap<String, ActiveAgentSession>>,
    telemetry_bus: broadcast::Sender<AgentTelemetry>,
    tty_subscribers: Arc<DashMap<String, mpsc::Sender<Vec<u8>>>>,
}

impl Default for ControlPlaneRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ControlPlaneRegistry {
    pub fn new() -> Self {
        let (bus, _) = broadcast::channel(1024);
        Self {
            agents: Arc::new(DashMap::new()),
            telemetry_bus: bus,
            tty_subscribers: Arc::new(DashMap::new()),
        }
    }

    pub fn subscribe_telemetry(&self) -> broadcast::Receiver<AgentTelemetry> {
        self.telemetry_bus.subscribe()
    }

    pub fn register_agent(
        &self,
        handshake: AgentHandshake,
        remote_ip: String,
        command_tx: mpsc::Sender<HubToAgentMessage>,
        pending_actions: Arc<Mutex<HashMap<String, oneshot::Sender<ActionResult>>>>,
    ) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        info!(
            "Registered control-plane agent '{}' ({}) from {}",
            handshake.node_id, handshake.hostname, remote_ip
        );

        let session = ActiveAgentSession {
            handshake: handshake.clone(),
            remote_ip,
            connected_at: now,
            last_heartbeat_at: now,
            status: NodeOnlineStatus::Online,
            latest_telemetry: None,
            command_tx,
            pending_actions,
        };

        self.agents.insert(handshake.node_id, session);
    }

    pub fn unregister_agent(&self, node_id: &str) {
        info!("Unregistering control-plane agent '{}'", node_id);
        self.agents.remove(node_id);
    }

    pub fn update_heartbeat(&self, node_id: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Some(mut session) = self.agents.get_mut(node_id) {
            session.last_heartbeat_at = now;
            session.status = NodeOnlineStatus::Online;
        }
    }

    pub fn record_telemetry(&self, telemetry: AgentTelemetry) {
        let node_id = telemetry.node_id.clone();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Some(mut session) = self.agents.get_mut(&node_id) {
            session.last_heartbeat_at = now;
            session.status = NodeOnlineStatus::Online;
            session.latest_telemetry = Some(telemetry.clone());
        }

        let _ = self.telemetry_bus.send(telemetry);
    }

    pub async fn dispatch_action(
        &self,
        signed_action: SignedAction,
        timeout_secs: u64,
    ) -> Result<ActionResult, String> {
        let node_id = signed_action.node_id.clone();
        let action_id = signed_action.action_id.clone();

        let (cmd_tx, pending_actions) = {
            let session = self
                .agents
                .get(&node_id)
                .ok_or_else(|| format!("Node '{}' is currently offline or not registered", node_id))?;

            if session.status == NodeOnlineStatus::Offline {
                return Err(format!("Node '{}' is offline", node_id));
            }

            (session.command_tx.clone(), session.pending_actions.clone())
        };

        let (resp_tx, resp_rx) = oneshot::channel();
        pending_actions.lock().await.insert(action_id.clone(), resp_tx);

        cmd_tx
            .send(HubToAgentMessage::ExecuteAction(signed_action))
            .await
            .map_err(|e| format!("Failed to send action to node {}: {}", node_id, e))?;

        match tokio::time::timeout(Duration::from_secs(timeout_secs), resp_rx).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_)) => {
                pending_actions.lock().await.remove(&action_id);
                Err("Agent dropped connection before responding".to_string())
            }
            Err(_) => {
                pending_actions.lock().await.remove(&action_id);
                Err(format!("Action timed out after {}s", timeout_secs))
            }
        }
    }

    pub fn list_nodes(&self) -> Vec<ManagedNodeDetail> {
        self.agents
            .iter()
            .map(|entry| {
                let s = entry.value();
                ManagedNodeDetail {
                    node_id: s.handshake.node_id.clone(),
                    hostname: s.handshake.hostname.clone(),
                    os: s.handshake.os.clone(),
                    arch: s.handshake.arch.clone(),
                    version: s.handshake.version.clone(),
                    remote_ip: s.remote_ip.clone(),
                    status: s.status,
                    connected_at: s.connected_at,
                    last_heartbeat_at: s.last_heartbeat_at,
                    latest_telemetry: s.latest_telemetry.clone(),
                }
            })
            .collect()
    }

    pub fn get_node(&self, node_id: &str) -> Option<ManagedNodeDetail> {
        self.agents.get(node_id).map(|entry| {
            let s = entry.value();
            ManagedNodeDetail {
                node_id: s.handshake.node_id.clone(),
                hostname: s.handshake.hostname.clone(),
                os: s.handshake.os.clone(),
                arch: s.handshake.arch.clone(),
                version: s.handshake.version.clone(),
                remote_ip: s.remote_ip.clone(),
                status: s.status,
                connected_at: s.connected_at,
                last_heartbeat_at: s.last_heartbeat_at,
                latest_telemetry: s.latest_telemetry.clone(),
            }
        })
    }

    /// Evaluates all agents' heartbeat timeouts. Returns a list of newly offline node IDs.
    pub fn sweep_health(&self) -> Vec<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut newly_offline = Vec::new();

        for mut entry in self.agents.iter_mut() {
            let session = entry.value_mut();
            let elapsed = now.saturating_sub(session.last_heartbeat_at);

            if elapsed > 30 {
                if session.status != NodeOnlineStatus::Offline {
                    session.status = NodeOnlineStatus::Offline;
                    warn!(
                        "Agent '{}' ({}) timed out after {}s -> MARKED OFFLINE",
                        session.handshake.node_id, session.handshake.hostname, elapsed
                    );
                    newly_offline.push(session.handshake.node_id.clone());
                }
            } else if elapsed > 15 && session.status == NodeOnlineStatus::Online {
                session.status = NodeOnlineStatus::Stale;
            }
        }

        newly_offline
    }

    pub fn register_tty_subscriber(&self, session_id: String, tx: mpsc::Sender<Vec<u8>>) {
        self.tty_subscribers.insert(session_id, tx);
    }

    pub fn unregister_tty_subscriber(&self, session_id: &str) {
        self.tty_subscribers.remove(session_id);
    }

    pub fn forward_tty_output(&self, session_id: &str, data: Vec<u8>) {
        if let Some(subscriber) = self.tty_subscribers.get(session_id) {
            let _ = subscriber.try_send(data);
        }
    }

    pub async fn send_tty_input_to_node(&self, node_id: &str, session_id: &str, data: Vec<u8>) {
        if let Some(session) = self.agents.get(node_id) {
            let _ = session
                .command_tx
                .send(HubToAgentMessage::TtyInput {
                    session_id: session_id.to_string(),
                    data,
                })
                .await;
        }
    }

    pub async fn open_node_tty(&self, node_id: &str, session_id: &str, rows: u16, cols: u16) {
        if let Some(session) = self.agents.get(node_id) {
            let _ = session
                .command_tx
                .send(HubToAgentMessage::ExecuteAction(SignedAction {
                    action_id: format!("open-{}", session_id),
                    node_id: node_id.to_string(),
                    action: redash_types::RemediationAction::TtyOpen {
                        session_id: session_id.to_string(),
                        rows,
                        cols,
                    },
                    timestamp: 0,
                    nonce: String::new(),
                    public_key_hex: String::new(),
                    signature_hex: String::new(),
                }))
                .await;
        }
    }

    pub async fn close_node_tty(&self, node_id: &str, session_id: &str) {
        if let Some(session) = self.agents.get(node_id) {
            let _ = session
                .command_tx
                .send(HubToAgentMessage::TtyClose {
                    session_id: session_id.to_string(),
                })
                .await;
        }
        self.unregister_tty_subscriber(session_id);
    }
}
