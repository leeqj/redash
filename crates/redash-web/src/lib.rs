//! ReDash Web Client & GPUI Web Adapter
//!
//! Provides the browser WebAssembly client runtime and WebSocket bridge
//! connecting to the `redash-server` gateway.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebTerminalConfig {
    pub host_id: String,
    pub cols: u32,
    pub rows: u32,
    pub gateway_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WebClientMessage {
    #[serde(rename = "input")]
    Input { data: String },
    #[serde(rename = "resize")]
    Resize { cols: u32, rows: u32 },
    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WebServerMessage {
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

/// Web Session Bridge for WebSocket Terminal
pub struct WebTerminalBridge {
    pub config: WebTerminalConfig,
    pub is_connected: bool,
}

impl WebTerminalBridge {
    pub fn new(config: WebTerminalConfig) -> Self {
        Self {
            config,
            is_connected: false,
        }
    }

    pub fn ws_url(&self) -> String {
        format!(
            "{}/ws/terminal/{}?cols={}&rows={}",
            self.config.gateway_url, self.config.host_id, self.config.cols, self.config.rows
        )
    }

    pub fn metrics_url(&self) -> String {
        format!(
            "{}/ws/metrics/{}",
            self.config.gateway_url, self.config.host_id
        )
    }
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_init() {
    console_error_panic_hook::set_once();
    web_sys::console::log_1(&"ReDash GPUI Web WASM Runtime Initialized".into());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_terminal_bridge_urls() {
        let config = WebTerminalConfig {
            host_id: "host_1".to_string(),
            cols: 120,
            rows: 40,
            gateway_url: "ws://127.0.0.1:8080".to_string(),
        };

        let bridge = WebTerminalBridge::new(config);
        assert_eq!(
            bridge.ws_url(),
            "ws://127.0.0.1:8080/ws/terminal/host_1?cols=120&rows=40"
        );
        assert_eq!(
            bridge.metrics_url(),
            "ws://127.0.0.1:8080/ws/metrics/host_1"
        );
    }

    #[test]
    fn test_message_serde() {
        let msg = WebClientMessage::Resize { cols: 80, rows: 24 };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"resize\""));
        assert!(json.contains("\"cols\":80"));

        let server_msg = WebServerMessage::Output {
            data: "hello".to_string(),
        };
        let s_json = serde_json::to_string(&server_msg).unwrap();
        assert!(s_json.contains("\"type\":\"output\""));
        assert!(s_json.contains("\"data\":\"hello\""));
    }
}
