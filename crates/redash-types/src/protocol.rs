use crate::metrics::NodeMetrics;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum ClientTerminalMessage {
    #[serde(rename = "input")]
    Input { data: String },
    #[serde(rename = "resize")]
    Resize { cols: u32, rows: u32 },
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
