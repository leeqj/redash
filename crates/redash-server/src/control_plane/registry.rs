use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use log::{info, warn};
use redash_types::{
    ActionResult, AgentHandshake, AgentTelemetry, HubToAgentMessage, ManagedNodeDetail,
    NodeOnlineStatus, SignedAction,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, broadcast, mpsc, oneshot};

pub struct ActiveAgentSession {
    pub handshake: AgentHandshake,
    pub connection_id: String,
    pub connected: bool,
    pub remote_ip: String,
    pub connected_at: u64,
    pub last_heartbeat_at: u64,
    pub status: NodeOnlineStatus,
    pub latest_telemetry: Option<AgentTelemetry>,
    pub command_tx: mpsc::Sender<HubToAgentMessage>,
    pub pending_actions: Arc<Mutex<HashMap<String, oneshot::Sender<ActionResult>>>>,
}

#[derive(Debug, Clone)]
pub enum TtyDownstreamMsg {
    Binary(Vec<u8>),
    Text(String),
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Enrollment {
    pub auth_token: String,
    pub trusted_public_key: String,
}

struct TtySubscriber {
    node_id: String,
    connection_id: String,
    tx: mpsc::Sender<TtyDownstreamMsg>,
}

#[derive(Clone)]
pub struct ControlPlaneRegistry {
    enrollments: Arc<HashMap<String, Enrollment>>,
    offline_events: Arc<std::sync::Mutex<Vec<String>>>,
    agents: Arc<DashMap<String, ActiveAgentSession>>,
    telemetry_bus: broadcast::Sender<AgentTelemetry>,
    tty_subscribers: Arc<DashMap<String, TtySubscriber>>,
}

impl Default for ControlPlaneRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ControlPlaneRegistry {
    pub fn new() -> Self {
        let path = std::env::var_os("REDASH_ENROLLMENTS_FILE")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                redash_core::config::HostStore::default_path()
                    .with_file_name("agent_enrollments.json")
            });
        let entries = std::fs::read(&path)
            .ok()
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_else(|| {
                warn!(
                    "No valid enrollment file at {}; Agent registration is disabled",
                    path.display()
                );
                HashMap::new()
            });
        Self::with_enrollments(entries)
    }

    pub fn with_enrollments(enrollments: HashMap<String, Enrollment>) -> Self {
        let (bus, _) = broadcast::channel(1024);
        Self {
            enrollments: Arc::new(enrollments),
            offline_events: Arc::default(),
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
    ) -> Result<String, String> {
        let enrolled = self
            .enrollments
            .get(&handshake.node_id)
            .ok_or("Node is not enrolled")?;
        if handshake.node_id.trim().is_empty()
            || enrolled.auth_token.len() < 32
            || enrolled.auth_token == "default-token"
            || !token_matches(&handshake.auth_token, &enrolled.auth_token)
            || handshake.trusted_public_key != enrolled.trusted_public_key
        {
            return Err("Invalid enrollment credentials or client trust key".into());
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        info!(
            "Registered control-plane agent '{}' ({}) from {}",
            handshake.node_id, handshake.hostname, remote_ip
        );

        let connection_id = uuid::Uuid::new_v4().to_string();
        let mut handshake = handshake;
        handshake.auth_token.clear(); // Never retain the wire credential in node state.
        let session = ActiveAgentSession {
            connection_id: connection_id.clone(),
            connected: true,
            handshake: handshake.clone(),
            remote_ip,
            connected_at: now,
            last_heartbeat_at: now,
            status: NodeOnlineStatus::Online,
            latest_telemetry: None,
            command_tx,
            pending_actions,
        };

        match self.agents.entry(handshake.node_id) {
            Entry::Occupied(mut entry) => {
                if entry.get().connected {
                    return Err("Node already has an active connection".into());
                }
                entry.insert(session);
            }
            Entry::Vacant(entry) => {
                entry.insert(session);
            }
        }
        Ok(connection_id)
    }

    pub fn unregister_agent(&self, node_id: &str, connection_id: &str) {
        if let Some(mut session) = self.agents.get_mut(node_id)
            && session.connection_id == connection_id
            && session.connected
        {
            session.connected = false;
            if session.status != NodeOnlineStatus::Offline {
                session.status = NodeOnlineStatus::Offline;
                self.offline_events.lock().unwrap().push(node_id.into());
            }
            if let Ok(mut pending) = session.pending_actions.try_lock() {
                pending.clear();
            }
        }
        self.tty_subscribers
            .retain(|_, s| s.node_id != node_id || s.connection_id != connection_id);
    }

    pub fn update_heartbeat(&self, node_id: &str, connection_id: &str) {
        if let Some(mut session) = self.agents.get_mut(node_id)
            && session.connection_id == connection_id
            && session.connected
        {
            session.last_heartbeat_at = now_secs();
            session.status = NodeOnlineStatus::Online;
        }
    }

    pub fn record_telemetry(
        &self,
        node_id: &str,
        connection_id: &str,
        telemetry: AgentTelemetry,
    ) -> bool {
        if telemetry.node_id != node_id
            || telemetry.timestamp > now_secs().saturating_add(30)
            || now_secs().saturating_sub(telemetry.timestamp) > 30
        {
            return false;
        }
        if let Some(mut session) = self.agents.get_mut(node_id)
            && session.connection_id == connection_id
            && session.connected
        {
            session.last_heartbeat_at = now_secs();
            session.status = NodeOnlineStatus::Online;
            session.latest_telemetry = Some(telemetry.clone());
            let _ = self.telemetry_bus.send(telemetry);
            return true;
        }
        false
    }

    pub async fn dispatch_action(
        &self,
        signed_action: SignedAction,
        timeout_secs: u64,
    ) -> Result<ActionResult, String> {
        let node_id = signed_action.node_id.clone();
        let action_id = signed_action.action_id.clone();

        let (cmd_tx, pending_actions) = {
            let session = self.agents.get(&node_id).ok_or_else(|| {
                format!("Node '{}' is currently offline or not registered", node_id)
            })?;

            if session.status == NodeOnlineStatus::Offline {
                return Err(format!("Node '{}' is offline", node_id));
            }

            (session.command_tx.clone(), session.pending_actions.clone())
        };

        let (resp_tx, resp_rx) = oneshot::channel();
        {
            let mut pending = pending_actions.lock().await;
            if pending.contains_key(&action_id) {
                return Err("Action ID is already pending".into());
            }
            pending.insert(action_id.clone(), resp_tx);
        }

        if cmd_tx
            .send(HubToAgentMessage::ExecuteAction(signed_action))
            .await
            .is_err()
        {
            pending_actions.lock().await.remove(&action_id);
            return Err(format!("Failed to send action to node {node_id}"));
        }

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

        let mut newly_offline = std::mem::take(&mut *self.offline_events.lock().unwrap());

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

        newly_offline.sort();
        newly_offline.dedup();
        newly_offline
    }

    pub fn register_tty_subscriber(
        &self,
        node_id: &str,
        session_id: String,
        tx: mpsc::Sender<TtyDownstreamMsg>,
    ) -> Result<(), String> {
        let agent = self.agents.get(node_id).ok_or("Unknown node")?;
        if !agent.connected {
            return Err("Node offline".into());
        }
        match self.tty_subscribers.entry(session_id) {
            Entry::Vacant(entry) => {
                entry.insert(TtySubscriber {
                    node_id: node_id.into(),
                    connection_id: agent.connection_id.clone(),
                    tx,
                });
                Ok(())
            }
            Entry::Occupied(_) => Err("Terminal session already registered".into()),
        }
    }
    pub fn unregister_tty_subscriber(&self, sid: &str) {
        self.tty_subscribers.remove(sid);
    }
    fn forward(&self, node_id: &str, connection_id: &str, sid: &str, text: String) {
        if let Some(subscriber) = self.tty_subscribers.get(sid)
            && subscriber.node_id == node_id
            && subscriber.connection_id == connection_id
        {
            // Disconnect a slow consumer rather than silently losing terminal output.
            if subscriber
                .tx
                .try_send(TtyDownstreamMsg::Text(text))
                .is_err()
            {
                drop(subscriber);
                self.unregister_tty_subscriber(sid);
            }
        }
    }
    pub fn forward_tty_closed(&self, node_id: &str, connection_id: &str, sid: &str) {
        self.tty_subscribers.remove_if(sid, |_, s| {
            s.node_id == node_id && s.connection_id == connection_id
        });
    }
    pub fn forward_tty_encrypted(
        &self,
        node_id: &str,
        connection_id: &str,
        env: redash_types::EncryptedEnvelope,
    ) {
        if let Ok(text) = serde_json::to_string(&env) {
            self.forward(node_id, connection_id, &env.session_id, text);
        }
    }
    pub fn forward_e2ee_ack(
        &self,
        node_id: &str,
        connection_id: &str,
        ack: redash_types::E2eeHandshakeAck,
    ) {
        if let Ok(text) = serde_json::to_string(&ack) {
            self.forward(node_id, connection_id, &ack.session_id, text);
        }
    }
    async fn send_terminal(
        &self,
        node_id: &str,
        sid: &str,
        message: HubToAgentMessage,
    ) -> Result<(), String> {
        let tx = {
            let subscriber = self.tty_subscribers.get(sid).ok_or("Unknown session")?;
            let agent = self.agents.get(node_id).ok_or("Unknown node")?;
            if !agent.connected
                || subscriber.node_id != node_id
                || subscriber.connection_id != agent.connection_id
            {
                return Err("Terminal connection changed".into());
            }
            agent.command_tx.clone()
        };
        tokio::time::timeout(Duration::from_secs(5), tx.send(message))
            .await
            .map_err(|_| "Agent queue timeout".to_string())?
            .map_err(|e| e.to_string())
    }
    pub async fn send_e2ee_init_to_node(
        &self,
        node_id: &str,
        init: redash_types::E2eeHandshakeInit,
    ) -> Result<(), String> {
        let sid = init.session_id.clone();
        self.send_terminal(node_id, &sid, HubToAgentMessage::E2eeHandshakeInit(init))
            .await
    }
    pub async fn send_tty_encrypted_to_node(
        &self,
        node_id: &str,
        env: redash_types::EncryptedEnvelope,
    ) -> Result<(), String> {
        let sid = env.session_id.clone();
        self.send_terminal(node_id, &sid, HubToAgentMessage::TtyEncrypted(env))
            .await
    }
    pub async fn close_node_tty(&self, node_id: &str, sid: &str) {
        let _ = self
            .send_terminal(
                node_id,
                sid,
                HubToAgentMessage::TtyClose {
                    session_id: sid.into(),
                },
            )
            .await;
        self.unregister_tty_subscriber(sid);
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn token_matches(actual: &str, expected: &str) -> bool {
    // Compare fixed-size digests in constant time; never compare credential prefixes.
    use sha2::{Digest, Sha256};
    use subtle::ConstantTimeEq;
    bool::from(Sha256::digest(actual.as_bytes()).ct_eq(&Sha256::digest(expected.as_bytes())))
}
