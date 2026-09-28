use serde::{Deserialize, Serialize};

/// High-frequency telemetry snapshot emitted by the `redash-agent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentTelemetry {
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

/// Frames sent from Agent to Hub over WebSocket.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "msg_type", rename_all = "snake_case")]
pub enum AgentToHubMessage {
    Handshake(AgentHandshake),
    Telemetry(AgentTelemetry),
    ActionResult(ActionResult),
    TtyOutput { session_id: String, data: Vec<u8> },
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
