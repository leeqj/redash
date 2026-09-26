//! WebSocket and REST API Gateway Client for ReDash Web
//! Connects from the browser WASM runtime to `redash-server`.

use crate::models::{HostConfig, MetricsMessage, ServerTerminalMessage};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{console, MessageEvent, WebSocket};

pub struct GatewayClient {
    pub active_terminal_ws: Option<WebSocket>,
    pub active_metrics_ws: Option<WebSocket>,
}

impl GatewayClient {
    pub fn new() -> Self {
        Self {
            active_terminal_ws: None,
            active_metrics_ws: None,
        }
    }

    pub fn connect_metrics(
        &mut self,
        host_id: &str,
        on_metrics: Rc<dyn Fn(crate::models::NodeMetrics)>,
    ) -> Result<(), JsValue> {
        let location = web_sys::window().unwrap().location();
        let host = location.host().unwrap_or_else(|_| "127.0.0.1:8080".to_string());
        let ws_url = format!("ws://{}/ws/metrics/{}", host, host_id);

        console::log_1(&format!("Connecting to metrics stream: {}", ws_url).into());
        let ws = WebSocket::new(&ws_url)?;

        let onmessage_callback = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
            let Some(text) = e.data().as_string() else { return };
            if let Ok(MetricsMessage::Metrics { data, .. }) =
                serde_json::from_str::<MetricsMessage>(&text)
            {
                on_metrics(data);
            }
        });
        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        onmessage_callback.forget();

        self.active_metrics_ws = Some(ws);
        Ok(())
    }

    pub fn connect_terminal(
        &mut self,
        host_id: &str,
        on_output: Rc<dyn Fn(String)>,
        on_agent: Rc<dyn Fn(crate::models::DetectedAgent)>,
    ) -> Result<(), JsValue> {
        let location = web_sys::window().unwrap().location();
        let host = location.host().unwrap_or_else(|_| "127.0.0.1:8080".to_string());
        let ws_url = format!("ws://{}/ws/terminal/{}?cols=120&rows=40", host, host_id);

        console::log_1(&format!("Connecting to terminal stream: {}", ws_url).into());
        let ws = WebSocket::new(&ws_url)?;

        let onmessage_callback = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
            let Some(text) = e.data().as_string() else { return };
            if let Ok(msg) = serde_json::from_str::<ServerTerminalMessage>(&text) {
                match msg {
                    ServerTerminalMessage::Output { data } => {
                        on_output(data);
                    }
                    ServerTerminalMessage::Agent {
                        name,
                        state,
                        cost_usd,
                        tokens,
                    } => {
                        let status = if state.contains("Thinking") || state.contains("思考") {
                            redash_types::AgentStatus::Thinking
                        } else if state.contains("NeedsInput") || state.contains("等待") {
                            redash_types::AgentStatus::NeedsInput
                        } else if state.contains("Done") || state.contains("完成") {
                            redash_types::AgentStatus::Done
                        } else {
                            redash_types::AgentStatus::Idle
                        };
                        on_agent(crate::models::DetectedAgent {
                            id: "active-agent".to_string(),
                            name,
                            category: "AI".to_string(),
                            status,
                            detail: state,
                            cost_usd,
                            tokens,
                        });
                    }
                    _ => {}
                }
            }
        });
        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        onmessage_callback.forget();

        self.active_terminal_ws = Some(ws);
        Ok(())
    }

    pub fn send_terminal_input(&self, data: &str) {
        if let Some(ws) = &self.active_terminal_ws {
            let cmd = crate::models::ClientTerminalMessage::Input {
                data: data.to_string(),
            };
            if let Ok(json) = serde_json::to_string(&cmd) {
                let _ = ws.send_with_str(&json);
            }
        }
    }
}

impl Default for GatewayClient {
    fn default() -> Self {
        Self::new()
    }
}

pub fn async_load_hosts(on_loaded: Rc<RefCell<dyn FnMut(Vec<HostConfig>)>>) {
    wasm_bindgen_futures::spawn_local(async move {
        let Ok(resp_val) = wasm_bindgen_futures::JsFuture::from(
            web_sys::window().unwrap().fetch_with_str("/api/hosts"),
        )
        .await
        else {
            return;
        };
        let resp: web_sys::Response = resp_val.unchecked_into();
        let Ok(json_prom) = resp.json() else { return };
        let Ok(json_val) = wasm_bindgen_futures::JsFuture::from(json_prom).await else { return };
        if let Ok(api_resp) = serde_wasm_bindgen_compat(&json_val) {
            (on_loaded.borrow_mut())(api_resp);
        }
    });
}

fn serde_wasm_bindgen_compat(val: &JsValue) -> Result<Vec<HostConfig>, ()> {
    if let Some(json_str) = js_sys::JSON::stringify(val).ok().and_then(|s| s.as_string()) {
        #[derive(serde::Deserialize)]
        struct Resp {
            data: Option<Vec<HostConfig>>,
        }
        if let Ok(r) = serde_json::from_str::<Resp>(&json_str) {
            return Ok(r.data.unwrap_or_default());
        }
    }
    Err(())
}
