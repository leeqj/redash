use serde::{Deserialize, Serialize};

/// Unsupported or failed samples must never masquerade as healthy readings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryValidity {
    pub cpu: bool,
    pub memory: bool,
    pub disk: bool,
    pub network: bool,
}

/// High-frequency telemetry snapshot emitted by the `redash-agent`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentTelemetry {
    #[serde(default)]
    pub validity: TelemetryValidity,
    pub node_id: String,
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub timestamp: u64,
    pub uptime_secs: u64,
    pub cpu_usage_pct: f32,
    pub cpu_cores: usize,
    pub mem_used_bytes: u64,
    pub mem_total_bytes: u64,
    pub disk_used_bytes: u64,
    pub disk_total_bytes: u64,
    pub net_rx_rate: u64,
    pub net_tx_rate: u64,
    #[serde(default)]
    pub containers: Vec<ContainerSummary>,
}

impl AgentTelemetry {
    pub fn memory_usage_pct(&self) -> f32 {
        if self.mem_total_bytes == 0 {
            0.0
        } else {
            (self.mem_used_bytes as f64 / self.mem_total_bytes as f64 * 100.0) as f32
        }
    }

    pub fn disk_usage_pct(&self) -> f32 {
        if self.disk_total_bytes == 0 {
            0.0
        } else {
            (self.disk_used_bytes as f64 / self.disk_total_bytes as f64 * 100.0) as f32
        }
    }
}

/// Brief summary of a single container (Docker / Podman).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerSummary {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String, // "running", "exited", "restarting", "paused"
    pub status: String,
    pub created: i64,
    #[serde(default)]
    pub ports: Vec<String>,
}

/// Node health/online state as tracked by the hub.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeOnlineStatus {
    Online,
    Stale,
    Offline,
}

impl NodeOnlineStatus {
    pub fn label(&self) -> &'static str {
        match self {
            NodeOnlineStatus::Online => "在线",
            NodeOnlineStatus::Stale => "失联警告",
            NodeOnlineStatus::Offline => "离线",
        }
    }

    pub fn color_hex(&self) -> &'static str {
        match self {
            NodeOnlineStatus::Online => "#10b981",  // Emerald
            NodeOnlineStatus::Stale => "#f59e0b",   // Amber
            NodeOnlineStatus::Offline => "#ef4444", // Rose
        }
    }
}

/// Action types supported by the remediation engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RemediationAction {
    RestartContainer {
        container_id: String,
    },
    StopContainer {
        container_id: String,
    },
    PruneContainers,
    VacuumLogs {
        max_size_mb: u32,
    },
    KillProcess {
        pid: u32,
        signal: i32,
    },
    ExecuteRecipe {
        name: String,
        script: String,
    },
    RestartService {
        service_name: String,
    },
    DiagnosePort {
        port: u16,
    },
    KillPortConflict {
        port: u16,
    },
    TtyOpen {
        session_id: String,
        rows: u16,
        cols: u16,
    },
    TtyInput {
        session_id: String,
        data: Vec<u8>,
    },
    TtyResize {
        session_id: String,
        rows: u16,
        cols: u16,
    },
    TtyClose {
        session_id: String,
    },
}

/// Digitally signed action payload sent from Client to Hub to Agent.
/// Enforces end-to-end zero-trust: Hub cannot forge or alter the action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedAction {
    pub action_id: String,
    pub node_id: String,
    pub action: RemediationAction,
    pub timestamp: u64,
    pub nonce: String,
    pub public_key_hex: String,
    pub signature_hex: String,
}

impl SignedAction {
    /// Constructs the canonical message bytes to sign / verify.
    pub fn canonical_signable_bytes(
        action_id: &str,
        node_id: &str,
        action: &RemediationAction,
        timestamp: u64,
        nonce: &str,
    ) -> Result<Vec<u8>, serde_json::Error> {
        #[derive(Serialize)]
        struct CanonicalMsg<'a> {
            action_id: &'a str,
            node_id: &'a str,
            action: &'a RemediationAction,
            timestamp: u64,
            nonce: &'a str,
        }

        let msg = CanonicalMsg {
            action_id,
            node_id,
            action,
            timestamp,
            nonce,
        };

        serde_json::to_vec(&msg)
    }
}

/// Execution result of a remediation action returned by the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionResult {
    pub action_id: String,
    pub node_id: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

/// Initial handshake data sent by the agent upon connecting to the hub.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHandshake {
    pub node_id: String,
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub version: String,
    pub auth_token: String,
    pub trusted_public_key: String,
}

/// Encrypted data envelope for end-to-end encrypted (E2EE) blind relay.
/// Transmitted across the Hub without the Hub having access to plaintext or keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedEnvelope {
    pub session_id: String,
    pub seq_num: u64,
    pub nonce_hex: String,
    pub ciphertext_hex: String,
    pub tag_hex: String,
}

impl EncryptedEnvelope {
    pub fn new(
        session_id: impl Into<String>,
        seq_num: u64,
        nonce_hex: impl Into<String>,
        ciphertext_hex: impl Into<String>,
        tag_hex: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            seq_num,
            nonce_hex: nonce_hex.into(),
            ciphertext_hex: ciphertext_hex.into(),
            tag_hex: tag_hex.into(),
        }
    }
}

/// Protocol v2: both signatures bind the node, session, nonce and ephemeral keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct E2eeHandshakeInit {
    pub version: u8,
    pub node_id: String,
    pub session_id: String,
    pub client_ephemeral_pubkey_hex: String,
    pub timestamp: u64,
    pub nonce: String,
    pub signature_hex: String,
}

impl E2eeHandshakeInit {
    pub fn signable_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(&(
            "redash-tty-v2-client",
            self.version,
            &self.node_id,
            &self.session_id,
            &self.client_ephemeral_pubkey_hex,
            self.timestamp,
            &self.nonce,
        ))
        .expect("fixed handshake schema")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct E2eeHandshakeAck {
    pub session_id: String,
    pub agent_ephemeral_pubkey_hex: String,
    pub signature_hex: String,
    pub success: bool,
    pub error_msg: Option<String>,
}

impl E2eeHandshakeAck {
    pub fn signable_bytes(&self, init: &E2eeHandshakeInit) -> Vec<u8> {
        serde_json::to_vec(&(
            "redash-tty-v2-agent",
            init,
            &self.session_id,
            &self.agent_ephemeral_pubkey_hex,
            self.success,
            &self.error_msg,
        ))
        .expect("fixed handshake schema")
    }
}

/// All terminal operations travel inside authenticated encryption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TtyClientFrame {
    Open { rows: u16, cols: u16 },
    Input { data: Vec<u8> },
    Resize { rows: u16, cols: u16 },
    Close,
}

/// Frames sent from Agent to Hub over WebSocket.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "msg_type", rename_all = "snake_case")]
pub enum AgentToHubMessage {
    Handshake(AgentHandshake),
    Telemetry(AgentTelemetry),
    ActionResult(ActionResult),
    TtyOutput { session_id: String, data: Vec<u8> },
    TtyClosed { session_id: String },
    TtyEncrypted(EncryptedEnvelope),
    E2eeHandshakeAck(E2eeHandshakeAck),
    Heartbeat,
}

/// Frames sent from Hub to Agent over WebSocket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "msg_type", rename_all = "snake_case")]
pub enum HubToAgentMessage {
    HandshakeAck {
        success: bool,
        message: String,
        heartbeat_interval_secs: u64,
    },
    ExecuteAction(SignedAction),
    TtyInput {
        session_id: String,
        data: Vec<u8>,
    },
    TtyEncrypted(EncryptedEnvelope),
    E2eeHandshakeInit(E2eeHandshakeInit),
    TtyResize {
        session_id: String,
        rows: u16,
        cols: u16,
    },
    TtyClose {
        session_id: String,
    },
    Ping,
}

/// Aggregated node details exposed to UI and API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManagedNodeDetail {
    pub node_id: String,
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub version: String,
    pub remote_ip: String,
    pub status: NodeOnlineStatus,
    pub connected_at: u64,
    pub last_heartbeat_at: u64,
    pub latest_telemetry: Option<AgentTelemetry>,
}

/// Standard LAN discovery multicast group address.
pub const DISCOVERY_MULTICAST_ADDR: &str = "239.255.77.88:8765";

/// Standard LAN discovery broadcast address.
pub const DISCOVERY_BROADCAST_ADDR: &str = "255.255.255.255:8765";

/// Standard LAN discovery UDP port.
pub const DISCOVERY_DEFAULT_PORT: u16 = 8765;

/// Normalizes an HTTP/HTTPS or plain base URL to a WebSocket endpoint (`ws://` or `wss://`).
pub fn to_websocket_endpoint(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    if let Some(stripped) = trimmed.strip_prefix("http://") {
        format!("ws://{}", stripped)
    } else if let Some(stripped) = trimmed.strip_prefix("https://") {
        format!("wss://{}", stripped)
    } else {
        trimmed.to_string()
    }
}

/// Normalizes a WebSocket or plain base URL to an HTTP endpoint (`http://` or `https://`).
pub fn to_http_endpoint(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    if let Some(stripped) = trimmed.strip_prefix("ws://") {
        format!("http://{}", stripped)
    } else if let Some(stripped) = trimmed.strip_prefix("wss://") {
        format!("https://{}", stripped)
    } else {
        trimmed.to_string()
    }
}

/// Returns the standard default shell path for UNIX platforms.
pub fn default_system_shell() -> &'static str {
    if std::path::Path::new("/bin/bash").exists() {
        "/bin/bash"
    } else {
        "/bin/sh"
    }
}

/// Formats the standard shell viewport resize command for remote PTY execution.
pub fn format_pty_resize_command(rows: u16, cols: u16) -> String {
    format!(
        "stty rows {} cols {} 2>/dev/null || export COLUMNS={} LINES={}\n",
        rows, cols, cols, rows
    )
}

/// Local Area Network (LAN) discovery broadcast beacon payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanBeacon {
    pub node_id: String,
    pub hostname: String,
    pub direct_port: u16,
    pub version: String,
    pub timestamp: u64,
}

/// Query broadcast by clients searching for local Redash agents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanDiscoveryQuery {
    pub query: String,
}

/// Secure pairing payload exchanged via QR Code or deep link between Desktop & Mobile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevicePairingPayload {
    pub hub_url: String,
    pub client_public_key: String,
    pub device_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub created_at: u64,
}

impl DevicePairingPayload {
    /// Formats the payload into a standard QR code URL string: `redash://pair?data=<hex>`
    pub fn to_uri(&self) -> String {
        let json = serde_json::to_string(self).unwrap_or_default();
        let hex: String = json
            .as_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        format!("redash://pair?data={}", hex)
    }

    /// Parses a `redash://pair?data=<hex>` URI.
    pub fn from_uri(uri: &str) -> Result<Self, String> {
        let data_prefix = "redash://pair?data=";
        let hex_str = uri.strip_prefix(data_prefix).ok_or_else(|| {
            "Invalid pairing URI prefix; expected redash://pair?data=".to_string()
        })?;

        if hex_str.len() % 2 != 0 {
            return Err("Odd length hex data in pairing URI".to_string());
        }

        let bytes: Result<Vec<u8>, _> = (0..hex_str.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex_str[i..i + 2], 16))
            .collect();

        let bytes = bytes.map_err(|e| format!("Invalid hex encoding: {}", e))?;
        serde_json::from_slice::<Self>(&bytes).map_err(|e| format!("Invalid JSON payload: {}", e))
    }
}
