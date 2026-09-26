use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostConfig {
    pub id: HostId,
    pub name: String,
    pub hostname: String,
    pub port: u16,
    pub user: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeMetrics {
    pub cpu_percent: f32,
    pub mem_percent: f32,
    pub mem_used_bytes: u64,
    pub mem_total_bytes: u64,
    pub disk_percent: f32,
    pub disk_used_bytes: u64,
    pub disk_total_bytes: u64,
    pub net_rx_bytes_sec: u64,
    pub net_tx_bytes_sec: u64,
    pub uptime_secs: u64,
    pub load_1m: f32,
    pub load_5m: f32,
    pub load_15m: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub theme_name: String,
    pub probe_interval_secs: u64,
    pub history_points: usize,
    pub terminal_font_size: f32,
    pub language: String,
    pub glow_effects_enabled: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme_name: "DarkTech".to_string(),
            probe_interval_secs: 2,
            history_points: 60,
            terminal_font_size: 13.0,
            language: "zh-CN".to_string(),
            glow_effects_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    Idle,
    Thinking,
    NeedsInput,
    Done,
}

impl AgentStatus {
    pub fn label(&self) -> &'static str {
        match self {
            AgentStatus::Idle => "就绪",
            AgentStatus::Thinking => "思考中",
            AgentStatus::NeedsInput => "等待授权",
            AgentStatus::Done => "完成",
        }
    }

    pub fn color_hex(&self) -> &'static str {
        match self {
            AgentStatus::Idle => "#64748b",
            AgentStatus::Thinking => "#38bdf8",
            AgentStatus::NeedsInput => "#f59e0b",
            AgentStatus::Done => "#10b981",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedAgent {
    pub name: String,
    pub state: String,
    pub cost_usd: Option<f64>,
    pub tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientTerminalMessage {
    #[serde(rename = "input")]
    Input { data: String },
    #[serde(rename = "resize")]
    Resize { cols: u32, rows: u32 },
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerTerminalMessage {
    #[serde(rename = "output")]
    Output { data: String },
    #[serde(rename = "agent")]
    Agent {
        name: String,
        state: String,
        cost_usd: Option<f64>,
        tokens: Option<u64>,
    },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "pong")]
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MetricsMessage {
    #[serde(rename = "metrics")]
    Metrics {
        host_id: String,
        data: NodeMetrics,
    },
    #[serde(rename = "error")]
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_terminal_message_serde() {
        let msg = ClientTerminalMessage::Input {
            data: "ls -la\n".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"input\""));
        assert!(json.contains("ls -la"));

        let de: ClientTerminalMessage = serde_json::from_str(&json).unwrap();
        match de {
            ClientTerminalMessage::Input { data } => assert_eq!(data, "ls -la\n"),
            _ => panic!("wrong message type"),
        }
    }

    #[test]
    fn test_server_terminal_message_serde() {
        let msg = ServerTerminalMessage::Agent {
            name: "Claude Code".to_string(),
            state: "Thinking".to_string(),
            cost_usd: Some(0.042),
            tokens: Some(1520),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"agent\""));

        let de: ServerTerminalMessage = serde_json::from_str(&json).unwrap();
        match de {
            ServerTerminalMessage::Agent {
                name,
                cost_usd,
                tokens,
                ..
            } => {
                assert_eq!(name, "Claude Code");
                assert_eq!(cost_usd, Some(0.042));
                assert_eq!(tokens, Some(1520));
            }
            _ => panic!("wrong message type"),
        }
    }

    #[test]
    fn test_node_metrics_serde() {
        let mut metrics = NodeMetrics::default();
        metrics.cpu_percent = 45.2;
        metrics.mem_percent = 68.1;
        let json = serde_json::to_string(&metrics).unwrap();
        let de: NodeMetrics = serde_json::from_str(&json).unwrap();
        assert!((de.cpu_percent - 45.2).abs() < 0.01);
    }
}
