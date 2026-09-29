//! WebSocket and REST API Gateway Client for ReDash Web.
use crate::models::{HostConfig, MetricsMessage, NodeMetrics, ServerTerminalMessage};
use redash_types::sftp::RemoteFileItem;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{CloseEvent, Event, MessageEvent, WebSocket};

/// Keep handlers alive exactly as long as their socket. Replacing a subscription
/// closes the previous socket and detaches callbacks into obsolete UI state.
struct LiveSocket {
    ws: WebSocket,
    last_activity: Rc<Cell<f64>>,
    _message: Closure<dyn FnMut(MessageEvent)>,
    _error: Closure<dyn FnMut(Event)>,
    _close: Closure<dyn FnMut(CloseEvent)>,
}
impl LiveSocket {
    fn new(
        url: &str,
        on_message: Rc<dyn Fn(String)>,
        on_error: Rc<dyn Fn(String)>,
    ) -> Result<Self, JsValue> {
        let ws = WebSocket::new(url)?;
        let last_activity = Rc::new(Cell::new(js_sys::Date::now()));
        let activity = last_activity.clone();
        let message = Closure::new(move |e: MessageEvent| {
            activity.set(js_sys::Date::now());
            if let Some(text) = e.data().as_string() {
                on_message(text);
            }
        });
        let error_cb = on_error.clone();
        let error = Closure::new(move |_: Event| {
            error_cb("连接失败，请检查 Gateway、登录凭据和网络".into())
        });
        let close = Closure::new(move |_: CloseEvent| on_error("连接已断开".into()));
        ws.set_onmessage(Some(message.as_ref().unchecked_ref()));
        ws.set_onerror(Some(error.as_ref().unchecked_ref()));
        ws.set_onclose(Some(close.as_ref().unchecked_ref()));
        Ok(Self {
            ws,
            last_activity,
            _message: message,
            _error: error,
            _close: close,
        })
    }
    fn healthy(&self) -> bool {
        matches!(
            self.ws.ready_state(),
            WebSocket::CONNECTING | WebSocket::OPEN
        ) && js_sys::Date::now() - self.last_activity.get() < 45_000.0
    }
}
impl Drop for LiveSocket {
    fn drop(&mut self) {
        self.ws.set_onmessage(None);
        self.ws.set_onerror(None);
        self.ws.set_onclose(None);
        let _ = self.ws.close();
    }
}

pub fn websocket_url(protocol: &str, host: &str, path: &str) -> Result<String, String> {
    let scheme = match protocol {
        "https:" => "wss",
        "http:" => "ws",
        _ => return Err("Gateway 必须通过 HTTP 或 HTTPS 访问".into()),
    };
    Ok(format!("{scheme}://{host}{path}"))
}
fn current_ws_url(path: &str) -> Result<String, JsValue> {
    let location = web_sys::window()
        .ok_or_else(|| JsValue::from_str("Window not found"))?
        .location();
    websocket_url(&location.protocol()?, &location.host()?, path).map_err(|e| JsValue::from_str(&e))
}
fn path_id(id: &str) -> String {
    js_sys::encode_uri_component(id)
        .as_string()
        .unwrap_or_default()
}

type MetricsCallback = Rc<dyn Fn(String, NodeMetrics)>;
type ErrorCallback = Rc<dyn Fn(String, String)>;
#[derive(Default)]
pub struct GatewayClient {
    terminal: Option<LiveSocket>,
    metrics: HashMap<String, LiveSocket>,
}
impl GatewayClient {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn sync_metrics(
        &mut self,
        host_ids: &[String],
        on_metrics: MetricsCallback,
        on_error: ErrorCallback,
    ) {
        let wanted: HashSet<_> = host_ids.iter().collect();
        self.metrics.retain(|id, _| wanted.contains(id));
        for host_id in host_ids {
            if self.metrics.get(host_id).is_some_and(LiveSocket::healthy) {
                continue;
            }
            if self.metrics.remove(host_id).is_some() {
                on_error(host_id.clone(), "指标连接中断，正在重连".into());
            }
            let id = host_id.clone();
            let metrics_cb = on_metrics.clone();
            let errors = on_error.clone();
            let parse_id = id.clone();
            let messages =
                Rc::new(
                    move |text: String| match serde_json::from_str::<MetricsMessage>(&text) {
                        Ok(MetricsMessage::Metrics { host_id, data }) if host_id == parse_id => {
                            metrics_cb(host_id, data)
                        }
                        Ok(MetricsMessage::Error { message }) => errors(parse_id.clone(), message),
                        _ => errors(parse_id.clone(), "指标响应无效".into()),
                    },
                );
            let errors = on_error.clone();
            let error_id = id.clone();
            let result = current_ws_url(&format!("/ws/metrics/{}", path_id(&id))).and_then(|url| {
                LiveSocket::new(
                    &url,
                    messages,
                    Rc::new(move |e| errors(error_id.clone(), e)),
                )
            });
            match result {
                Ok(socket) => {
                    self.metrics.insert(id, socket);
                }
                Err(e) => on_error(id, format!("无法打开指标连接: {e:?}")),
            }
        }
    }
    pub fn connect_terminal(
        &mut self,
        host_id: &str,
        cols: u32,
        rows: u32,
        on_output: Rc<dyn Fn(String)>,
        on_agent: Rc<dyn Fn(crate::models::DetectedAgent)>,
    ) -> Result<(), JsValue> {
        self.terminal = None;
        let url = current_ws_url(&format!(
            "/ws/terminal/{}?cols={}&rows={}",
            path_id(host_id),
            cols.max(1),
            rows.max(1)
        ))?;
        let error_output = on_output.clone();
        let messages = Rc::new(move |text: String| {
            match serde_json::from_str::<ServerTerminalMessage>(&text) {
                Ok(ServerTerminalMessage::Output { data }) => on_output(data),
                Ok(ServerTerminalMessage::Error { message }) => {
                    on_output(format!("\r\n[连接错误] {message}\r\n"))
                }
                Ok(ServerTerminalMessage::Agent {
                    name,
                    state,
                    cost_usd,
                    tokens,
                }) => {
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
                        id: "active-agent".into(),
                        name,
                        category: "AI".into(),
                        status,
                        detail: state,
                        cost_usd,
                        tokens,
                    });
                }
                Ok(ServerTerminalMessage::Pong) => {}
                Err(_) => on_output("\r\n[连接错误] 终端响应无效\r\n".into()),
            }
        });
        self.terminal = Some(LiveSocket::new(
            &url,
            messages,
            Rc::new(move |e| error_output(format!("\r\n[终端] {e}\r\n"))),
        )?);
        Ok(())
    }
    pub fn send_terminal_input(&self, data: &str) {
        if let Some(socket) = &self.terminal {
            let cmd = crate::models::ClientTerminalMessage::Input { data: data.into() };
            if let Ok(json) = serde_json::to_string(&cmd) {
                let _ = socket.ws.send_with_str(&json);
            }
        }
    }
    pub fn send_terminal_resize(&self, cols: u16, rows: u16) {
        if let Some(socket) = &self.terminal {
            let cmd = crate::models::ClientTerminalMessage::Resize {
                cols: cols as u32,
                rows: rows as u32,
            };
            if let Ok(json) = serde_json::to_string(&cmd) {
                let _ = socket.ws.send_with_str(&json);
            }
        }
    }
}

type HostsCallback = Rc<RefCell<dyn FnMut(Result<Vec<HostConfig>, String>)>>;
pub fn async_load_hosts(on_loaded: HostsCallback) {
    wasm_bindgen_futures::spawn_local(async move {
        let result = async {
            let resp = wasm_bindgen_futures::JsFuture::from(
                web_sys::window().unwrap().fetch_with_str("/api/hosts"),
            )
            .await
            .map_err(|e| format!("主机列表请求失败: {e:?}"))?
            .unchecked_into::<web_sys::Response>();
            let status = resp.status();
            let text =
                wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
                    .await
                    .map_err(|e| format!("{e:?}"))?
                    .as_string()
                    .unwrap_or_default();
            #[derive(serde::Deserialize)]
            struct HostsResponse {
                success: bool,
                data: Option<Vec<HostConfig>>,
                message: Option<String>,
            }
            let parsed: HostsResponse = serde_json::from_str(&text)
                .map_err(|_| format!("主机列表响应无效 (HTTP {status})"))?;
            if !resp.ok() || !parsed.success {
                return Err(parsed.message.unwrap_or_else(|| format!("HTTP {status}")));
            }
            parsed.data.ok_or_else(|| "主机列表响应缺少数据".into())
        }
        .await;
        (on_loaded.borrow_mut())(result);
    });
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

#[cfg(test)]
mod socket_url_tests {
    use super::*;
    #[test]
    fn inherits_transport_security_and_port() {
        assert_eq!(
            websocket_url("https:", "gateway.test:8443", "/ws/metrics/a").unwrap(),
            "wss://gateway.test:8443/ws/metrics/a"
        );
        assert_eq!(
            websocket_url("http:", "localhost:8080", "/ws/terminal/a").unwrap(),
            "ws://localhost:8080/ws/terminal/a"
        );
        assert!(websocket_url("file:", "", "/ws/metrics/a").is_err());
    }
}
