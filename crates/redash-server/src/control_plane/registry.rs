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
use tokio::sync::{Mutex, Notify, broadcast, mpsc, oneshot};

pub struct ActiveAgentSession {
    pub telemetry_expires_at: u64,
    shutdown: Arc<Notify>,
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
    binding: String,
    command_tx: mpsc::Sender<HubToAgentMessage>,
    shutdown: Arc<Notify>,
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
            telemetry_expires_at: now
                + redash_types::telemetry_max_age(handshake.telemetry_interval_secs),
            shutdown: Arc::new(Notify::new()),
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
            session.telemetry_expires_at = now_secs()
                + redash_types::telemetry_max_age(session.handshake.telemetry_interval_secs);
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

        let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
        tokio::time::timeout_at(deadline, async {
            // Reserve capacity before creating a pending response. Cancellation while
            // waiting for capacity cannot leave state or enqueue a late command.
            let permit = cmd_tx
                .reserve()
                .await
                .map_err(|_| "Agent disconnected".to_string())?;
            let (sender, response) = oneshot::channel();
            let mut pending = pending_actions.lock().await;
            if pending.contains_key(&action_id) {
                return Err("Action ID is already pending".into());
            }
            pending.insert(action_id.clone(), sender);
            let mut guard = PendingAction {
                id: action_id.clone(),
                map: pending_actions.clone(),
                response: Some(response),
            };
            drop(pending);
            if tokio::time::Instant::now() >= deadline {
                return Err("Action timed out before dispatch".into());
            }
            permit.send(HubToAgentMessage::ExecuteAction(signed_action));
            guard
                .response
                .as_mut()
                .unwrap()
                .await
                .map_err(|_| "Agent dropped connection before responding".to_string())
        })
        .await
        .map_err(|_| format!("Action timed out after {timeout_secs}s"))?
    }

    pub fn connection_shutdown(&self, node_id: &str, connection_id: &str) -> Option<Arc<Notify>> {
        self.agents
            .get(node_id)
            .filter(|s| s.connection_id == connection_id)
            .map(|s| s.shutdown.clone())
    }

    pub fn list_nodes(&self) -> Vec<ManagedNodeDetail> {
        self.agents
            .iter()
            .map(|entry| {
                let s = entry.value();
                let mut node = ManagedNodeDetail {
                    telemetry_expires_at: s.telemetry_expires_at,
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
                };
                node.expire_telemetry(now_secs());
                node
            })
            .collect()
    }

    pub fn get_node(&self, node_id: &str) -> Option<ManagedNodeDetail> {
        self.agents.get(node_id).map(|entry| {
            let s = entry.value();
            let mut node = ManagedNodeDetail {
                telemetry_expires_at: s.telemetry_expires_at,
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
            };
            node.expire_telemetry(now_secs());
            node
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
    ) -> Result<String, String> {
        let (connection_id, command_tx, shutdown) = {
            let agent = self.agents.get(node_id).ok_or("Unknown node")?;
            if !agent.connected {
                return Err("Node offline".into());
            }
            (
                agent.connection_id.clone(),
                agent.command_tx.clone(),
                agent.shutdown.clone(),
            )
        };
        match self.tty_subscribers.entry(session_id) {
            Entry::Vacant(entry) => {
                let binding = uuid::Uuid::new_v4().to_string();
                entry.insert(TtySubscriber {
                    binding: binding.clone(),
                    command_tx,
                    shutdown,
                    node_id: node_id.into(),
                    connection_id,
                    tx,
                });
                Ok(binding)
            }
            Entry::Occupied(_) => Err("Terminal session already registered".into()),
        }
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
                let binding = subscriber.binding.clone();
                drop(subscriber);
                if let Some((_, subscriber)) = self.tty_subscribers.remove_if(sid, |_, s| {
                    s.binding == binding && s.node_id == node_id && s.connection_id == connection_id
                }) {
                    let sid = sid.to_string();
                    tokio::spawn(close_subscriber(subscriber, sid));
                }
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
            let connection_id = {
                let subscriber = self.tty_subscribers.get(sid).ok_or("Unknown session")?;
                if subscriber.node_id != node_id {
                    return Err("Wrong terminal node".into());
                }
                subscriber.connection_id.clone()
            };
            let agent = self.agents.get(node_id).ok_or("Unknown node")?;
            if !agent.connected || connection_id != agent.connection_id {
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
    pub async fn close_node_tty(&self, sid: &str, binding: &str) {
        if let Some((_, subscriber)) = self
            .tty_subscribers
            .remove_if(sid, |_, s| s.binding == binding)
        {
            close_subscriber(subscriber, sid.into()).await;
        }
    }
}

// Cleanup keeps the original connection's sender. It must never resolve a new
// Agent connection by node ID after the subscription has been removed.
async fn close_subscriber(subscriber: TtySubscriber, session_id: String) {
    drop(subscriber.tx);
    if !tokio::time::timeout(
        Duration::from_secs(5),
        subscriber
            .command_tx
            .send(HubToAgentMessage::TtyClose { session_id }),
    )
    .await
    .is_ok_and(|r| r.is_ok())
    {
        // A stuck writer cannot carry a close. Disconnect that transport so the
        // Agent's route lease reaps its PTYs instead of leaving orphan processes.
        subscriber.shutdown.notify_one();
    }
}

type PendingMap = Arc<Mutex<HashMap<String, oneshot::Sender<ActionResult>>>>;
struct PendingAction {
    id: String,
    map: PendingMap,
    response: Option<oneshot::Receiver<ActionResult>>,
}
impl Drop for PendingAction {
    fn drop(&mut self) {
        drop(self.response.take());
        let remove = |map: &mut HashMap<String, oneshot::Sender<ActionResult>>, id: &str| {
            if map.get(id).is_some_and(|sender| sender.is_closed()) {
                map.remove(id);
            }
        };
        if let Ok(mut map) = self.map.try_lock() {
            remove(&mut map, &self.id);
        } else {
            let map = self.map.clone();
            let id = self.id.clone();
            tokio::spawn(async move {
                remove(&mut *map.lock().await, &id);
            });
        }
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

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use redash_ui_core::control_plane::ClientSigner;
    const TOKEN: &str = "test-enrollment-token-at-least-32-characters";
    fn setup(
        capacity: usize,
    ) -> (
        ControlPlaneRegistry,
        String,
        mpsc::Receiver<HubToAgentMessage>,
        PendingMap,
    ) {
        let registry = ControlPlaneRegistry::with_enrollments(HashMap::from([(
            "node".into(),
            Enrollment {
                auth_token: TOKEN.into(),
                trusted_public_key: String::new(),
            },
        )]));
        let (tx, rx) = mpsc::channel(capacity);
        let pending: PendingMap = Arc::default();
        let connection = registry
            .register_agent(handshake(), "local".into(), tx, pending.clone())
            .unwrap();
        (registry, connection, rx, pending)
    }
    fn handshake() -> AgentHandshake {
        AgentHandshake {
            telemetry_interval_secs: 3,
            node_id: "node".into(),
            hostname: "node".into(),
            os: "test".into(),
            arch: "test".into(),
            version: "2".into(),
            auth_token: TOKEN.into(),
            trusted_public_key: String::new(),
        }
    }
    fn action() -> SignedAction {
        let (_, private) = ClientSigner::generate_keypair();
        ClientSigner::sign_fresh_action(
            &private,
            "node",
            redash_types::RemediationAction::PruneContainers,
        )
        .unwrap()
    }
    fn ack(sid: &str) -> redash_types::E2eeHandshakeAck {
        redash_types::E2eeHandshakeAck {
            session_id: sid.into(),
            agent_ephemeral_pubkey_hex: String::new(),
            signature_hex: String::new(),
            success: true,
            error_msg: None,
        }
    }
    #[tokio::test]
    async fn slow_terminal_sends_close_to_original_connection() {
        let (registry, conn, mut commands, _) = setup(8);
        let (tx, mut rx) = mpsc::channel(1);
        let binding = registry
            .register_tty_subscriber("node", "sid".into(), tx)
            .unwrap();
        registry.forward_e2ee_ack("node", &conn, ack("sid"));
        registry.forward_e2ee_ack("node", &conn, ack("sid"));
        assert!(rx.recv().await.is_some());
        assert!(rx.recv().await.is_none());
        registry.close_node_tty("sid", &binding).await;
        assert!(
            matches!(tokio::time::timeout(Duration::from_secs(1), commands.recv()).await.unwrap(), Some(HubToAgentMessage::TtyClose { session_id }) if session_id == "sid")
        );
    }
    #[tokio::test]
    async fn old_client_cleanup_cannot_close_reconnected_subscription() {
        let (registry, conn, _old_commands, _) = setup(8);
        let (tx, _) = mpsc::channel(1);
        let old = registry
            .register_tty_subscriber("node", "sid".into(), tx)
            .unwrap();
        registry.unregister_agent("node", &conn);
        let (commands, mut rx) = mpsc::channel(8);
        registry
            .register_agent(handshake(), "local".into(), commands, Arc::default())
            .unwrap();
        let (tx, _) = mpsc::channel(1);
        let new = registry
            .register_tty_subscriber("node", "sid".into(), tx)
            .unwrap();
        registry.close_node_tty("sid", &old).await;
        assert_eq!(registry.tty_subscribers.get("sid").unwrap().binding, new);
        assert!(rx.try_recv().is_err());
        registry.close_node_tty("sid", &new).await;
        assert!(matches!(
            rx.recv().await,
            Some(HubToAgentMessage::TtyClose { .. })
        ));
    }
    #[tokio::test]
    async fn full_command_queue_deadline_never_enqueues_late_action() {
        let (registry, _, mut rx, pending) = setup(1);
        registry
            .agents
            .get("node")
            .unwrap()
            .command_tx
            .try_send(HubToAgentMessage::Ping)
            .unwrap();
        assert!(
            registry
                .dispatch_action(action(), 0)
                .await
                .unwrap_err()
                .contains("timed out")
        );
        assert!(pending.lock().await.is_empty());
        assert!(matches!(rx.recv().await, Some(HubToAgentMessage::Ping)));
        assert!(rx.try_recv().is_err());
    }
    #[tokio::test]
    async fn cancelling_response_wait_removes_pending_entry() {
        let (registry, _, mut rx, pending) = setup(1);
        let task = tokio::spawn(async move { registry.dispatch_action(action(), 30).await });
        assert!(matches!(
            rx.recv().await,
            Some(HubToAgentMessage::ExecuteAction(_))
        ));
        assert_eq!(pending.lock().await.len(), 1);
        task.abort();
        let _ = task.await;
        assert!(pending.lock().await.is_empty());
    }
    #[tokio::test]
    async fn failed_close_delivery_disconnects_only_its_agent_transport() {
        let (registry, conn, _rx, _) = setup(1);
        let shutdown = registry.connection_shutdown("node", &conn).unwrap();
        registry
            .agents
            .get("node")
            .unwrap()
            .command_tx
            .try_send(HubToAgentMessage::Ping)
            .unwrap();
        let (tx, _) = mpsc::channel(1);
        let binding = registry
            .register_tty_subscriber("node", "sid".into(), tx)
            .unwrap();
        registry.close_node_tty("sid", &binding).await;
        tokio::time::timeout(Duration::from_millis(100), shutdown.notified())
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn heartbeat_does_not_refresh_telemetry_and_sample_recovery_clears_stale_alert() {
        let (registry, conn, _commands, _) = setup(8);
        let telemetry = AgentTelemetry {
            node_id: "node".into(),
            timestamp: now_secs(),
            ..Default::default()
        };
        assert!(registry.record_telemetry("node", &conn, telemetry.clone()));
        registry
            .agents
            .get_mut("node")
            .unwrap()
            .telemetry_expires_at = now_secs() - 1;
        registry.update_heartbeat("node", &conn);
        let node = registry.get_node("node").unwrap();
        assert_eq!(node.status, NodeOnlineStatus::Online);
        assert!(node.latest_telemetry.is_none());
        assert!(registry.list_nodes()[0].latest_telemetry.is_none());
        assert_eq!(
            super::super::health_sentinel::health_alert_kind(&node, now_secs()),
            Some("telemetry_stale")
        );
        assert!(registry.record_telemetry("node", &conn, telemetry));
        let node = registry.get_node("node").unwrap();
        assert!(node.latest_telemetry.is_some());
        assert_eq!(
            super::super::health_sentinel::health_alert_kind(&node, now_secs()),
            None
        );
        registry.unregister_agent("node", &conn);
        assert_eq!(
            super::super::health_sentinel::health_alert_kind(
                &registry.get_node("node").unwrap(),
                now_secs()
            ),
            Some("offline")
        );
    }
    #[tokio::test]
    async fn initial_collector_failure_expires_and_reported_interval_controls_deadline() {
        let (registry, conn, _commands, _) = setup(8);
        registry
            .agents
            .get_mut("node")
            .unwrap()
            .telemetry_expires_at = now_secs() - 1;
        let node = registry.get_node("node").unwrap();
        assert_eq!(
            super::super::health_sentinel::health_alert_kind(&node, now_secs()),
            Some("telemetry_stale")
        );
        registry
            .agents
            .get_mut("node")
            .unwrap()
            .handshake
            .telemetry_interval_secs = 60;
        assert!(registry.record_telemetry(
            "node",
            &conn,
            AgentTelemetry {
                node_id: "node".into(),
                timestamp: now_secs(),
                ..Default::default()
            }
        ));
        let node = registry.get_node("node").unwrap();
        assert!(!node.telemetry_is_stale(now_secs() + 179));
        assert!(node.telemetry_is_stale(now_secs() + 181));
    }
}
