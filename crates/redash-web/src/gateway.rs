//! WebSocket and REST API Gateway Client for ReDash Web
//! Connects from the browser WASM runtime to `redash-server`.

use crate::models::{HostConfig, MetricsMessage, ServerTerminalMessage};
use redash_types::sftp::RemoteFileItem;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{MessageEvent, WebSocket, console};

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
        let host = location
            .host()
            .unwrap_or_else(|_| "127.0.0.1:8080".to_string());
        let ws_url = format!("ws://{}/ws/metrics/{}", host, host_id);

        console::log_1(&format!("Connecting to metrics stream: {}", ws_url).into());
        let ws = WebSocket::new(&ws_url)?;

        let onmessage_callback = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
            let Some(text) = e.data().as_string() else {
                return;
            };
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
        cols: u32,
        rows: u32,
        on_output: Rc<dyn Fn(String)>,
        on_agent: Rc<dyn Fn(crate::models::DetectedAgent)>,
    ) -> Result<(), JsValue> {
        let location = web_sys::window().unwrap().location();
        let host = location
            .host()
            .unwrap_or_else(|_| "127.0.0.1:8080".to_string());
        let c = if cols == 0 { 120 } else { cols };
        let r = if rows == 0 { 40 } else { rows };
        let ws_url = format!(
            "ws://{}/ws/terminal/{}?cols={}&rows={}",
            host, host_id, c, r
        );

        console::log_1(&format!("Connecting to terminal stream: {}", ws_url).into());
        let ws = WebSocket::new(&ws_url)?;

        let onmessage_callback = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
            let Some(text) = e.data().as_string() else {
                return;
            };
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

    pub fn send_terminal_resize(&self, cols: u16, rows: u16) {
        if let Some(ws) = &self.active_terminal_ws {
            let cmd = crate::models::ClientTerminalMessage::Resize {
                cols: cols as u32,
                rows: rows as u32,
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
        let Ok(json_val) = wasm_bindgen_futures::JsFuture::from(json_prom).await else {
            return;
        };
        if let Ok(api_resp) = serde_wasm_bindgen_compat(&json_val) {
            (on_loaded.borrow_mut())(api_resp);
        }
    });
}

fn serde_wasm_bindgen_compat(val: &JsValue) -> Result<Vec<HostConfig>, ()> {
    if let Some(json_str) = js_sys::JSON::stringify(val)
        .ok()
        .and_then(|s| s.as_string())
    {
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

#[allow(clippy::type_complexity)]
pub fn async_save_host(
    host: HostConfig,
    on_done: Rc<RefCell<dyn FnMut(Result<HostConfig, String>)>>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        let Some(window) = web_sys::window() else {
            (on_done.borrow_mut())(Err("Window not found".to_string()));
            return;
        };

        let opts = web_sys::RequestInit::new();
        opts.set_method("POST");

        let payload_str = match serde_json::to_string(&host) {
            Ok(s) => s,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to serialize host: {}", e)));
                return;
            }
        };
        opts.set_body(&wasm_bindgen::JsValue::from_str(&payload_str));

        let headers = match web_sys::Headers::new() {
            Ok(h) => h,
            Err(_) => {
                (on_done.borrow_mut())(Err("Failed to construct Headers".to_string()));
                return;
            }
        };
        let _ = headers.set("Content-Type", "application/json");
        opts.set_headers(&headers);

        let resp_val = match wasm_bindgen_futures::JsFuture::from(
            window.fetch_with_str_and_init("/api/hosts", &opts),
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Network request failed: {:?}", e)));
                return;
            }
        };
        let resp: web_sys::Response = resp_val.unchecked_into();
        let json_prom = match resp.json() {
            Ok(p) => p,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to parse response: {:?}", e)));
                return;
            }
        };
        let json_val = match wasm_bindgen_futures::JsFuture::from(json_prom).await {
            Ok(v) => v,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to await JSON: {:?}", e)));
                return;
            }
        };
        if let Some(json_str) = js_sys::JSON::stringify(&json_val)
            .ok()
            .and_then(|s| s.as_string())
        {
            #[derive(serde::Deserialize)]
            struct ApiResp {
                success: bool,
                data: Option<HostConfig>,
                message: Option<String>,
            }
            match serde_json::from_str::<ApiResp>(&json_str) {
                Ok(res) if res.success => {
                    let saved = res.data.unwrap_or(host);
                    (on_done.borrow_mut())(Ok(saved));
                }
                Ok(res) => {
                    let msg = res
                        .message
                        .unwrap_or_else(|| "Failed to save host".to_string());
                    (on_done.borrow_mut())(Err(msg));
                }
                Err(e) => {
                    (on_done.borrow_mut())(Err(format!("Failed to deserialize response: {}", e)));
                }
            }
        } else {
            (on_done.borrow_mut())(Err("Failed to stringify JSON response".to_string()));
        }
    });
}

#[allow(clippy::type_complexity)]
pub fn async_delete_host(host_id: String, on_done: Rc<RefCell<dyn FnMut(Result<(), String>)>>) {
    wasm_bindgen_futures::spawn_local(async move {
        let Some(window) = web_sys::window() else {
            (on_done.borrow_mut())(Err("Window not found".to_string()));
            return;
        };

        let opts = web_sys::RequestInit::new();
        opts.set_method("DELETE");

        let url = format!("/api/hosts/{}", host_id);
        let resp_val =
            match wasm_bindgen_futures::JsFuture::from(window.fetch_with_str_and_init(&url, &opts))
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    (on_done.borrow_mut())(Err(format!("Network request failed: {:?}", e)));
                    return;
                }
            };
        let resp: web_sys::Response = resp_val.unchecked_into();
        if resp.ok() {
            (on_done.borrow_mut())(Ok(()));
        } else {
            (on_done.borrow_mut())(Err(format!("Server returned HTTP {}", resp.status())));
        }
    });
}

#[derive(serde::Serialize)]
struct TestDraftPayload {
    hostname: String,
    port: u16,
    user: String,
    password: Option<String>,
}

#[allow(clippy::type_complexity)]
pub fn async_test_draft_host(
    hostname: String,
    port: u16,
    user: String,
    password: Option<String>,
    on_done: Rc<RefCell<dyn FnMut(Result<String, String>)>>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        let Some(window) = web_sys::window() else {
            (on_done.borrow_mut())(Err("Window not found".to_string()));
            return;
        };

        let opts = web_sys::RequestInit::new();
        opts.set_method("POST");

        let payload = TestDraftPayload {
            hostname,
            port,
            user,
            password,
        };

        let payload_str = match serde_json::to_string(&payload) {
            Ok(s) => s,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Serialization error: {}", e)));
                return;
            }
        };
        opts.set_body(&wasm_bindgen::JsValue::from_str(&payload_str));

        let headers = match web_sys::Headers::new() {
            Ok(h) => h,
            Err(_) => {
                (on_done.borrow_mut())(Err("Failed to construct Headers".to_string()));
                return;
            }
        };
        let _ = headers.set("Content-Type", "application/json");
        opts.set_headers(&headers);

        let resp_val = match wasm_bindgen_futures::JsFuture::from(
            window.fetch_with_str_and_init("/api/hosts/test", &opts),
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Network request failed: {:?}", e)));
                return;
            }
        };
        let resp: web_sys::Response = resp_val.unchecked_into();
        let json_prom = match resp.json() {
            Ok(p) => p,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to parse JSON response: {:?}", e)));
                return;
            }
        };
        let json_val = match wasm_bindgen_futures::JsFuture::from(json_prom).await {
            Ok(v) => v,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to await JSON promise: {:?}", e)));
                return;
            }
        };

        if let Some(json_str) = js_sys::JSON::stringify(&json_val)
            .ok()
            .and_then(|s| s.as_string())
        {
            #[derive(serde::Deserialize)]
            struct ApiResponse {
                success: bool,
                message: Option<String>,
            }

            match serde_json::from_str::<ApiResponse>(&json_str) {
                Ok(res) if res.success => {
                    let msg = res.message.unwrap_or_else(|| "连接测试成功".to_string());
                    (on_done.borrow_mut())(Ok(msg));
                }
                Ok(res) => {
                    let msg = res.message.unwrap_or_else(|| "连接失败".to_string());
                    (on_done.borrow_mut())(Err(msg));
                }
                Err(e) => {
                    (on_done.borrow_mut())(Err(format!("Deserialize error: {}", e)));
                }
            }
        } else {
            (on_done.borrow_mut())(Err("Failed to stringify JSON response".to_string()));
        }
    });
}

#[allow(clippy::type_complexity)]
pub fn async_test_saved_host(
    host_id: String,
    on_done: Rc<RefCell<dyn FnMut(Result<String, String>)>>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        let Some(window) = web_sys::window() else {
            (on_done.borrow_mut())(Err("Window not found".to_string()));
            return;
        };

        let opts = web_sys::RequestInit::new();
        opts.set_method("POST");

        let url = format!("/api/hosts/{}/test", host_id);
        let resp_val =
            match wasm_bindgen_futures::JsFuture::from(window.fetch_with_str_and_init(&url, &opts))
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    (on_done.borrow_mut())(Err(format!("Network request failed: {:?}", e)));
                    return;
                }
            };
        let resp: web_sys::Response = resp_val.unchecked_into();
        let json_prom = match resp.json() {
            Ok(p) => p,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to parse JSON response: {:?}", e)));
                return;
            }
        };
        let json_val = match wasm_bindgen_futures::JsFuture::from(json_prom).await {
            Ok(v) => v,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to await JSON promise: {:?}", e)));
                return;
            }
        };

        if let Some(json_str) = js_sys::JSON::stringify(&json_val)
            .ok()
            .and_then(|s| s.as_string())
        {
            #[derive(serde::Deserialize)]
            struct ApiResponse {
                success: bool,
                message: Option<String>,
            }

            match serde_json::from_str::<ApiResponse>(&json_str) {
                Ok(res) if res.success => {
                    let msg = res.message.unwrap_or_else(|| "连接测试成功".to_string());
                    (on_done.borrow_mut())(Ok(msg));
                }
                Ok(res) => {
                    let msg = res.message.unwrap_or_else(|| "连接失败".to_string());
                    (on_done.borrow_mut())(Err(msg));
                }
                Err(e) => {
                    (on_done.borrow_mut())(Err(format!("Deserialize error: {}", e)));
                }
            }
        } else {
            (on_done.borrow_mut())(Err("Failed to stringify JSON response".to_string()));
        }
    });
}

pub type SftpListCallback = Rc<RefCell<dyn FnMut(Result<Vec<RemoteFileItem>, String>)>>;
pub type SftpReadCallback = Rc<RefCell<dyn FnMut(Result<String, String>)>>;
pub type SftpWriteCallback = Rc<RefCell<dyn FnMut(Result<(), String>)>>;

#[allow(clippy::type_complexity)]
pub fn async_fetch_sftp_list(
    host_id: String,
    path: String,
    on_done: Rc<RefCell<dyn FnMut(Result<Vec<RemoteFileItem>, String>)>>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        let Some(window) = web_sys::window() else {
            (on_done.borrow_mut())(Err("Window not found".to_string()));
            return;
        };
        let encoded_path = js_sys::encode_uri_component(&path)
            .as_string()
            .unwrap_or_else(|| path.clone());
        let url = format!("/api/sftp/{}/list?path={}", host_id, encoded_path);

        let resp_val = match wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(&url)).await
        {
            Ok(v) => v,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Network request failed: {:?}", e)));
                return;
            }
        };
        let resp: web_sys::Response = resp_val.unchecked_into();
        let json_prom = match resp.json() {
            Ok(p) => p,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to parse response: {:?}", e)));
                return;
            }
        };
        let json_val = match wasm_bindgen_futures::JsFuture::from(json_prom).await {
            Ok(v) => v,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to await JSON: {:?}", e)));
                return;
            }
        };
        if let Some(json_str) = js_sys::JSON::stringify(&json_val)
            .ok()
            .and_then(|s| s.as_string())
        {
            #[derive(serde::Deserialize)]
            struct ApiResp {
                success: bool,
                data: Option<redash_types::sftp::PagedFileResult>,
                message: Option<String>,
            }
            match serde_json::from_str::<ApiResp>(&json_str) {
                Ok(res) if res.success => {
                    let items = res.data.map(|d| d.items).unwrap_or_default();
                    (on_done.borrow_mut())(Ok(items));
                }
                Ok(res) => {
                    let msg = res
                        .message
                        .unwrap_or_else(|| "Failed to fetch files".to_string());
                    (on_done.borrow_mut())(Err(msg));
                }
                Err(e) => {
                    (on_done.borrow_mut())(Err(format!("Failed to deserialize response: {}", e)));
                }
            }
        } else {
            (on_done.borrow_mut())(Err("Failed to stringify JSON response".to_string()));
        }
    });
}

#[allow(clippy::type_complexity)]
pub fn async_read_sftp_file(
    host_id: String,
    path: String,
    on_done: Rc<RefCell<dyn FnMut(Result<String, String>)>>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        let Some(window) = web_sys::window() else {
            (on_done.borrow_mut())(Err("Window not found".to_string()));
            return;
        };
        let encoded_path = js_sys::encode_uri_component(&path)
            .as_string()
            .unwrap_or_else(|| path.clone());
        let url = format!("/api/sftp/{}/read?path={}", host_id, encoded_path);

        let resp_val = match wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(&url)).await
        {
            Ok(v) => v,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Network request failed: {:?}", e)));
                return;
            }
        };
        let resp: web_sys::Response = resp_val.unchecked_into();
        let json_prom = match resp.json() {
            Ok(p) => p,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to parse response: {:?}", e)));
                return;
            }
        };
        let json_val = match wasm_bindgen_futures::JsFuture::from(json_prom).await {
            Ok(v) => v,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to await JSON: {:?}", e)));
                return;
            }
        };
        if let Some(json_str) = js_sys::JSON::stringify(&json_val)
            .ok()
            .and_then(|s| s.as_string())
        {
            #[derive(serde::Deserialize)]
            struct ApiResp {
                success: bool,
                data: Option<String>,
                message: Option<String>,
            }
            match serde_json::from_str::<ApiResp>(&json_str) {
                Ok(res) if res.success => {
                    let content = res.data.unwrap_or_default();
                    (on_done.borrow_mut())(Ok(content));
                }
                Ok(res) => {
                    let msg = res
                        .message
                        .unwrap_or_else(|| "Failed to read file".to_string());
                    (on_done.borrow_mut())(Err(msg));
                }
                Err(e) => {
                    (on_done.borrow_mut())(Err(format!("Failed to deserialize response: {}", e)));
                }
            }
        } else {
            (on_done.borrow_mut())(Err("Failed to stringify JSON response".to_string()));
        }
    });
}

#[allow(clippy::type_complexity)]
pub fn async_write_sftp_file(
    host_id: String,
    path: String,
    content: String,
    on_done: Rc<RefCell<dyn FnMut(Result<(), String>)>>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        let Some(window) = web_sys::window() else {
            (on_done.borrow_mut())(Err("Window not found".to_string()));
            return;
        };
        let url = format!("/api/sftp/{}/write", host_id);

        let opts = web_sys::RequestInit::new();
        opts.set_method("POST");

        let payload = serde_json::json!({
            "path": path,
            "content": content,
        });
        let payload_str = match serde_json::to_string(&payload) {
            Ok(s) => s,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to serialize request: {}", e)));
                return;
            }
        };
        opts.set_body(&wasm_bindgen::JsValue::from_str(&payload_str));

        let headers = match web_sys::Headers::new() {
            Ok(h) => h,
            Err(_) => {
                (on_done.borrow_mut())(Err("Failed to construct Headers".to_string()));
                return;
            }
        };
        let _ = headers.set("Content-Type", "application/json");
        opts.set_headers(&headers);

        let resp_val =
            match wasm_bindgen_futures::JsFuture::from(window.fetch_with_str_and_init(&url, &opts))
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    (on_done.borrow_mut())(Err(format!("Network request failed: {:?}", e)));
                    return;
                }
            };
        let resp: web_sys::Response = resp_val.unchecked_into();
        let json_prom = match resp.json() {
            Ok(p) => p,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to parse response: {:?}", e)));
                return;
            }
        };
        let json_val = match wasm_bindgen_futures::JsFuture::from(json_prom).await {
            Ok(v) => v,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to await JSON: {:?}", e)));
                return;
            }
        };
        if let Some(json_str) = js_sys::JSON::stringify(&json_val)
            .ok()
            .and_then(|s| s.as_string())
        {
            #[derive(serde::Deserialize)]
            struct ApiResp {
                success: bool,
                message: Option<String>,
            }
            match serde_json::from_str::<ApiResp>(&json_str) {
                Ok(res) if res.success => {
                    (on_done.borrow_mut())(Ok(()));
                }
                Ok(res) => {
                    let msg = res
                        .message
                        .unwrap_or_else(|| "Failed to save file".to_string());
                    (on_done.borrow_mut())(Err(msg));
                }
                Err(e) => {
                    (on_done.borrow_mut())(Err(format!("Failed to deserialize response: {}", e)));
                }
            }
        } else {
            (on_done.borrow_mut())(Err("Failed to stringify JSON response".to_string()));
        }
    });
}

#[allow(clippy::type_complexity)]
pub fn async_load_settings(
    on_done: Rc<RefCell<dyn FnMut(Result<redash_types::settings::AppSettings, String>)>>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        let Some(window) = web_sys::window() else {
            (on_done.borrow_mut())(Err("Window not found".to_string()));
            return;
        };
        let resp_val = match wasm_bindgen_futures::JsFuture::from(
            window.fetch_with_str("/api/settings"),
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Network request failed: {:?}", e)));
                return;
            }
        };
        let resp: web_sys::Response = resp_val.unchecked_into();
        let json_prom = match resp.json() {
            Ok(p) => p,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to parse response: {:?}", e)));
                return;
            }
        };
        let json_val = match wasm_bindgen_futures::JsFuture::from(json_prom).await {
            Ok(v) => v,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to await JSON: {:?}", e)));
                return;
            }
        };
        if let Some(json_str) = js_sys::JSON::stringify(&json_val)
            .ok()
            .and_then(|s| s.as_string())
        {
            #[derive(serde::Deserialize)]
            struct ApiResp {
                success: bool,
                data: Option<redash_types::settings::AppSettings>,
                message: Option<String>,
            }
            match serde_json::from_str::<ApiResp>(&json_str) {
                Ok(res) if res.success => {
                    if let Some(settings) = res.data {
                        (on_done.borrow_mut())(Ok(settings));
                    } else {
                        (on_done.borrow_mut())(Err("Empty settings data".to_string()));
                    }
                }
                Ok(res) => {
                    let msg = res
                        .message
                        .unwrap_or_else(|| "Failed to load settings".to_string());
                    (on_done.borrow_mut())(Err(msg));
                }
                Err(e) => {
                    (on_done.borrow_mut())(Err(format!("Failed to deserialize settings: {}", e)));
                }
            }
        } else {
            (on_done.borrow_mut())(Err("Failed to stringify JSON response".to_string()));
        }
    });
}

#[allow(clippy::type_complexity)]
pub fn async_save_settings(
    settings: redash_types::settings::AppSettings,
    on_done: Rc<RefCell<dyn FnMut(Result<(), String>)>>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        let Some(window) = web_sys::window() else {
            (on_done.borrow_mut())(Err("Window not found".to_string()));
            return;
        };
        let opts = web_sys::RequestInit::new();
        opts.set_method("POST");

        let payload_str = match serde_json::to_string(&settings) {
            Ok(s) => s,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to serialize settings: {}", e)));
                return;
            }
        };
        opts.set_body(&wasm_bindgen::JsValue::from_str(&payload_str));

        let headers = match web_sys::Headers::new() {
            Ok(h) => h,
            Err(_) => {
                (on_done.borrow_mut())(Err("Failed to construct Headers".to_string()));
                return;
            }
        };
        let _ = headers.set("Content-Type", "application/json");
        opts.set_headers(&headers);

        let resp_val = match wasm_bindgen_futures::JsFuture::from(
            window.fetch_with_str_and_init("/api/settings", &opts),
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Network request failed: {:?}", e)));
                return;
            }
        };
        let resp: web_sys::Response = resp_val.unchecked_into();
        let json_prom = match resp.json() {
            Ok(p) => p,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to parse response: {:?}", e)));
                return;
            }
        };
        let json_val = match wasm_bindgen_futures::JsFuture::from(json_prom).await {
            Ok(v) => v,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to await JSON: {:?}", e)));
                return;
            }
        };
        if let Some(json_str) = js_sys::JSON::stringify(&json_val)
            .ok()
            .and_then(|s| s.as_string())
        {
            #[derive(serde::Deserialize)]
            struct ApiResp {
                success: bool,
                message: Option<String>,
            }
            match serde_json::from_str::<ApiResp>(&json_str) {
                Ok(res) if res.success => {
                    (on_done.borrow_mut())(Ok(()));
                }
                Ok(res) => {
                    let msg = res
                        .message
                        .unwrap_or_else(|| "Failed to save settings".to_string());
                    (on_done.borrow_mut())(Err(msg));
                }
                Err(e) => {
                    (on_done.borrow_mut())(Err(format!("Failed to deserialize response: {}", e)));
                }
            }
        } else {
            (on_done.borrow_mut())(Err("Failed to stringify JSON response".to_string()));
        }
    });
}

pub type BatchRunCallback =
    Rc<RefCell<dyn FnMut(Result<redash_types::batch::BatchJobResult, String>)>>;

#[allow(clippy::type_complexity)]
pub fn async_run_batch(
    host_ids: Vec<String>,
    command: String,
    on_done: Rc<RefCell<dyn FnMut(Result<redash_types::batch::BatchJobResult, String>)>>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        let Some(window) = web_sys::window() else {
            (on_done.borrow_mut())(Err("Window not found".to_string()));
            return;
        };

        let opts = web_sys::RequestInit::new();
        opts.set_method("POST");

        let req = redash_types::batch::BatchRunRequest { host_ids, command };
        let payload_str = match serde_json::to_string(&req) {
            Ok(s) => s,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to serialize request: {}", e)));
                return;
            }
        };
        opts.set_body(&wasm_bindgen::JsValue::from_str(&payload_str));

        let headers = match web_sys::Headers::new() {
            Ok(h) => h,
            Err(_) => {
                (on_done.borrow_mut())(Err("Failed to construct Headers".to_string()));
                return;
            }
        };
        let _ = headers.set("Content-Type", "application/json");
        opts.set_headers(&headers);

        let resp_val = match wasm_bindgen_futures::JsFuture::from(
            window.fetch_with_str_and_init("/api/batch/exec", &opts),
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Network request failed: {:?}", e)));
                return;
            }
        };
        let resp: web_sys::Response = resp_val.unchecked_into();
        let json_prom = match resp.json() {
            Ok(p) => p,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to parse response: {:?}", e)));
                return;
            }
        };
        let json_val = match wasm_bindgen_futures::JsFuture::from(json_prom).await {
            Ok(v) => v,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to await JSON: {:?}", e)));
                return;
            }
        };
        if let Some(json_str) = js_sys::JSON::stringify(&json_val)
            .ok()
            .and_then(|s| s.as_string())
        {
            #[derive(serde::Deserialize)]
            struct ApiResp {
                success: bool,
                data: Option<redash_types::batch::BatchJobResult>,
                message: Option<String>,
            }
            match serde_json::from_str::<ApiResp>(&json_str) {
                Ok(res) if res.success => {
                    if let Some(result) = res.data {
                        (on_done.borrow_mut())(Ok(result));
                    } else {
                        (on_done.borrow_mut())(Err("Empty batch response data".to_string()));
                    }
                }
                Ok(res) => {
                    let msg = res
                        .message
                        .unwrap_or_else(|| "Failed to execute batch job".to_string());
                    (on_done.borrow_mut())(Err(msg));
                }
                Err(e) => {
                    (on_done.borrow_mut())(Err(format!("Failed to deserialize response: {}", e)));
                }
            }
        } else {
            (on_done.borrow_mut())(Err("Failed to stringify JSON response".to_string()));
        }
    });
}

pub type WebhookTestCallback = Rc<RefCell<dyn FnMut(Result<String, String>)>>;

pub fn async_test_webhook(webhook_url: Option<String>, on_done: WebhookTestCallback) {
    wasm_bindgen_futures::spawn_local(async move {
        let Some(window) = web_sys::window() else {
            (on_done.borrow_mut())(Err("Window not found".to_string()));
            return;
        };

        let payload = serde_json::json!({
            "webhook_url": webhook_url,
        });
        let payload_str = payload.to_string();

        let opts = web_sys::RequestInit::new();
        opts.set_method("POST");
        opts.set_body(&wasm_bindgen::JsValue::from_str(&payload_str));

        let headers = match web_sys::Headers::new() {
            Ok(h) => h,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to create headers: {:?}", e)));
                return;
            }
        };
        let _ = headers.set("Content-Type", "application/json");
        opts.set_headers(&headers);

        let req = match web_sys::Request::new_with_str_and_init("/api/settings/test-webhook", &opts)
        {
            Ok(r) => r,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to build request: {:?}", e)));
                return;
            }
        };

        let resp_val =
            match wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&req)).await {
                Ok(v) => v,
                Err(e) => {
                    (on_done.borrow_mut())(Err(format!("Network request failed: {:?}", e)));
                    return;
                }
            };
        let resp: web_sys::Response = resp_val.unchecked_into();
        let json_prom = match resp.json() {
            Ok(p) => p,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to parse response JSON: {:?}", e)));
                return;
            }
        };
        let json_val = match wasm_bindgen_futures::JsFuture::from(json_prom).await {
            Ok(v) => v,
            Err(e) => {
                (on_done.borrow_mut())(Err(format!("Failed to await JSON: {:?}", e)));
                return;
            }
        };
        if let Some(json_str) = js_sys::JSON::stringify(&json_val)
            .ok()
            .and_then(|s| s.as_string())
        {
            #[derive(serde::Deserialize)]
            struct ApiResp {
                success: bool,
                data: Option<String>,
                message: Option<String>,
            }
            match serde_json::from_str::<ApiResp>(&json_str) {
                Ok(res) if res.success => {
                    let msg = res.data.unwrap_or_else(|| "测试消息推送成功！".to_string());
                    (on_done.borrow_mut())(Ok(msg));
                }
                Ok(res) => {
                    let msg = res
                        .message
                        .unwrap_or_else(|| "测试消息推送失败".to_string());
                    (on_done.borrow_mut())(Err(msg));
                }
                Err(e) => {
                    (on_done.borrow_mut())(Err(format!("Failed to deserialize response: {}", e)));
                }
            }
        } else {
            (on_done.borrow_mut())(Err("Failed to stringify JSON response".to_string()));
        }
    });
}
