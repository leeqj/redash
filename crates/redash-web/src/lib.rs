//! ReDash Web Client - GPUI WebAssembly Canvas Engine
//!
//! 100% Rust WASM Canvas runtime bringing GPUI desktop fidelity and performance
//! to the browser without rewriting UI components in JavaScript.

pub mod app;
pub mod gateway;
pub mod input;
pub use redash_types as models;
pub mod notifications;
pub mod render;
pub mod theme;

use app::{ActiveView, AppState, UiEffect, WorkbenchTab};
use gateway::GatewayClient;
use input::{UiAction, handle_key_down, handle_mouse_click};
use render::render_frame;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{HtmlCanvasElement, KeyboardEvent, MouseEvent, Window, console};

#[wasm_bindgen]
pub fn start_web_app(canvas_id: &str) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    console::log_1(&"⚡ ReDash GPUI Web WASM Engine Starting...".into());

    let window = web_sys::window().expect("global window not found");
    let document = window.document().expect("document not found");
    let canvas: HtmlCanvasElement = document
        .get_element_by_id(canvas_id)
        .expect("Canvas element not found")
        .dyn_into::<HtmlCanvasElement>()?;

    let ctx = canvas
        .get_context("2d")?
        .expect("Failed to get 2D canvas context")
        .dyn_into::<web_sys::CanvasRenderingContext2d>()?;

    let app_state = Rc::new(RefCell::new(AppState::new()));
    let gateway = Rc::new(RefCell::new(GatewayClient::new()));

    // Auto-resize canvas to fill browser window
    let resize_canvas = {
        let canvas = canvas.clone();
        let window = window.clone();
        Rc::new(move || {
            let width = window.inner_width().unwrap().as_f64().unwrap();
            let height = window.inner_height().unwrap().as_f64().unwrap();
            let dpr = window.device_pixel_ratio();
            canvas.set_width((width * dpr) as u32);
            canvas.set_height((height * dpr) as u32);
            let html_el: &web_sys::HtmlElement = canvas.unchecked_ref();
            let _ = html_el
                .style()
                .set_property("width", &format!("{}px", width));
            let _ = html_el
                .style()
                .set_property("height", &format!("{}px", height));
        })
    };

    resize_canvas();
    let on_resize = {
        let resize_canvas = resize_canvas.clone();
        Closure::<dyn FnMut()>::new(move || {
            resize_canvas();
        })
    };
    window.set_onresize(Some(on_resize.as_ref().unchecked_ref()));
    on_resize.forget();

    // Async Fetch Hosts from Server
    {
        let app_state_clone = app_state.clone();
        let gateway_clone = gateway.clone();
        let on_loaded = Rc::new(RefCell::new(move |hosts: Vec<models::HostConfig>| {
            console::log_1(&format!("Loaded {} hosts from gateway", hosts.len()).into());
            let first_host_id = hosts.first().map(|h| h.id.0.clone());
            app_state_clone.borrow_mut().hosts = hosts;

            // Connect live metrics for first host
            if let Some(host_id) = first_host_id {
                let app_state_metrics = app_state_clone.clone();
                let hid = host_id.clone();
                let alert_cooldowns: Rc<RefCell<std::collections::HashMap<String, f64>>> =
                    Rc::new(RefCell::new(std::collections::HashMap::new()));
                let cds = alert_cooldowns.clone();
                let _ = gateway_clone.borrow_mut().connect_metrics(
                    &host_id,
                    Rc::new(move |metrics| {
                        let mut sm = app_state_metrics.borrow_mut();
                        let now = js_sys::Date::now();
                        let host_name = sm
                            .hosts
                            .iter()
                            .find(|h| h.id.0 == hid)
                            .map(|h| h.name.clone())
                            .unwrap_or_else(|| hid.clone());

                        // 1. CPU Threshold Check
                        if sm.settings.alert_cpu_threshold > 0.0
                            && metrics.cpu.usage_percent >= sm.settings.alert_cpu_threshold
                        {
                            let key = format!("{hid}:cpu");
                            let mut map = cds.borrow_mut();
                            let last = map.get(&key).copied().unwrap_or(0.0);
                            if now - last > 300_000.0 {
                                map.insert(key, now);
                                let _ = notifications::show_browser_notification(
                                    &format!("⚠️ CPU 过载告警: {host_name}"),
                                    &format!(
                                        "主机 {host_name} CPU 利用率已达 {:.1}% (设定阈值: {:.0}%)",
                                        metrics.cpu.usage_percent, sm.settings.alert_cpu_threshold
                                    ),
                                );
                            }
                        }

                        // 2. Memory Threshold Check
                        if sm.settings.alert_mem_threshold > 0.0
                            && metrics.mem.usage_percent >= sm.settings.alert_mem_threshold
                        {
                            let key = format!("{hid}:mem");
                            let mut map = cds.borrow_mut();
                            let last = map.get(&key).copied().unwrap_or(0.0);
                            if now - last > 300_000.0 {
                                map.insert(key, now);
                                let _ = notifications::show_browser_notification(
                                    &format!("⚠️ 内存过载告警: {host_name}"),
                                    &format!(
                                        "主机 {host_name} 内存利用率已达 {:.1}% (设定阈值: {:.0}%)",
                                        metrics.mem.usage_percent, sm.settings.alert_mem_threshold
                                    ),
                                );
                            }
                        }

                        // 3. Disk Threshold Check
                        let max_disk_usage = metrics
                            .disks
                            .iter()
                            .map(|d| d.usage_percent)
                            .fold(0.0f32, f32::max);
                        if sm.settings.alert_disk_threshold > 0.0
                            && max_disk_usage >= sm.settings.alert_disk_threshold
                        {
                            let key = format!("{hid}:disk");
                            let mut map = cds.borrow_mut();
                            let last = map.get(&key).copied().unwrap_or(0.0);
                            if now - last > 300_000.0 {
                                map.insert(key, now);
                                let _ = notifications::show_browser_notification(
                                    &format!("⚠️ 磁盘空间告警: {host_name}"),
                                    &format!(
                                        "主机 {host_name} 磁盘空间已使用 {:.1}% (设定阈值: {:.0}%)",
                                        max_disk_usage, sm.settings.alert_disk_threshold
                                    ),
                                );
                            }
                        }

                        sm.update_metrics(hid.clone(), metrics);
                    }),
                );
            }
        }));
        gateway::async_load_hosts(on_loaded);
    }

    // Async Fetch Settings from Server on Boot
    {
        let app_state_clone = app_state.clone();
        let on_settings_loaded = Rc::new(RefCell::new(
            move |res: Result<redash_types::settings::AppSettings, String>| match res {
                Ok(settings) => {
                    console::log_1(
                        &format!(
                            "Loaded settings from server: theme={}, lang={}, interval={}s",
                            settings.theme_name, settings.language, settings.probe_interval_secs
                        )
                        .into(),
                    );
                    app_state_clone.borrow_mut().settings = settings;
                }
                Err(err) => {
                    console::log_1(&format!("Failed to load settings from server: {}", err).into());
                }
            },
        ));
        gateway::async_load_settings(on_settings_loaded);
    }

    // Auto-focus canvas on startup
    {
        let html_el: &web_sys::HtmlElement = canvas.unchecked_ref();
        let _ = html_el.focus();
    }

    // Register Mouse Event Listener on Canvas
    {
        let app_state_clone = app_state.clone();
        let gateway_clone = gateway.clone();
        let canvas_clone = canvas.clone();
        let window = window.clone();

        let on_mousedown = Closure::<dyn FnMut(_)>::new(move |e: MouseEvent| {
            let rect = canvas_clone.get_bounding_client_rect();
            let x = e.client_x() as f64 - rect.left();
            let y = e.client_y() as f64 - rect.top();
            let width = window.inner_width().unwrap().as_f64().unwrap();
            let height = window.inner_height().unwrap().as_f64().unwrap();

            let html_el: &web_sys::HtmlElement = canvas_clone.unchecked_ref();
            let _ = html_el.focus();

            let mut state = app_state_clone.borrow_mut();
            if let Some(action) = handle_mouse_click(&mut state, x, y, width, height) {
                match action {
                    UiAction::OpenTerminal(host_id) => {
                        let app_state_term = app_state_clone.clone();
                        let app_state_agent = app_state_clone.clone();
                        let (cols, rows) = {
                            let sm = app_state_clone.borrow();
                            (sm.terminal_grid.cols as u32, sm.terminal_grid.rows as u32)
                        };
                        let _ = gateway_clone.borrow_mut().connect_terminal(
                            &host_id,
                            cols,
                            rows,
                            Rc::new(move |data| {
                                app_state_term.borrow_mut().append_terminal_output(&data);
                            }),
                            Rc::new(move |agent| {
                                app_state_agent.borrow_mut().agent = Some(agent);
                            }),
                        );
                    }
                    UiAction::OpenSftp(host_id) => {
                        let app_state_sftp = app_state_clone.clone();
                        let path = app_state_sftp.borrow().sftp_current_path.clone();
                        app_state_sftp.borrow_mut().set_sftp_loading(true);
                        let on_done = Rc::new(RefCell::new(
                            move |res: Result<Vec<redash_types::sftp::RemoteFileItem>, String>| {
                                let mut sm = app_state_sftp.borrow_mut();
                                match res {
                                    Ok(files) => sm.set_sftp_files(files),
                                    Err(err) => sm.set_sftp_error(Some(err)),
                                }
                            },
                        ));
                        gateway::async_fetch_sftp_list(host_id, path, on_done);
                    }
                    UiAction::FetchSftpList { host_id, path } => {
                        let app_state_sftp = app_state_clone.clone();
                        let on_done = Rc::new(RefCell::new(
                            move |res: Result<Vec<redash_types::sftp::RemoteFileItem>, String>| {
                                let mut sm = app_state_sftp.borrow_mut();
                                match res {
                                    Ok(files) => sm.set_sftp_files(files),
                                    Err(err) => sm.set_sftp_error(Some(err)),
                                }
                            },
                        ));
                        gateway::async_fetch_sftp_list(host_id, path, on_done);
                    }
                    UiAction::ReadSftpFile { host_id, path } => {
                        let app_state_sftp = app_state_clone.clone();
                        let read_path = path.clone();
                        let on_done = Rc::new(RefCell::new(move |res: Result<String, String>| {
                            let mut sm = app_state_sftp.borrow_mut();
                            match res {
                                Ok(content) => sm.open_sftp_editor(read_path.clone(), content),
                                Err(err) => sm.set_sftp_error(Some(err)),
                            }
                        }));
                        gateway::async_read_sftp_file(host_id, path, on_done);
                    }
                    UiAction::SaveSftpFile {
                        host_id,
                        path,
                        content,
                    } => {
                        let app_state_sftp = app_state_clone.clone();
                        let on_done = Rc::new(RefCell::new(move |res: Result<(), String>| {
                            let mut sm = app_state_sftp.borrow_mut();
                            match res {
                                Ok(()) => {
                                    sm.sftp_editor_modified = false;
                                    sm.set_sftp_loading(false);
                                }
                                Err(err) => sm.set_sftp_error(Some(err)),
                            }
                        }));
                        gateway::async_write_sftp_file(host_id, path, content, on_done);
                    }
                    UiAction::SaveSettings => {
                        let app_state_save = app_state_clone.clone();
                        let settings = app_state_save.borrow().settings.clone();
                        let on_done = Rc::new(RefCell::new(move |res: Result<(), String>| {
                            let mut sm = app_state_save.borrow_mut();
                            match res {
                                Ok(()) => {
                                    sm.settings_save_status =
                                        Some(("设置已成功保存到云端服务器".to_string(), true));
                                }
                                Err(err) => {
                                    sm.settings_save_status =
                                        Some((format!("设置保存失败: {}", err), false));
                                }
                            }
                        }));
                        gateway::async_save_settings(settings, on_done);
                    }
                    UiAction::ExportSettingsJson => {
                        let json =
                            serde_json::to_string_pretty(&state.settings).unwrap_or_default();
                        console::log_1(&format!("Exported Settings JSON:\n{}", json).into());
                        if let Some(window) = web_sys::window()
                            && let Some(document) = window.document()
                            && let Ok(element) = document.create_element("a")
                        {
                            let href = format!(
                                "data:application/json;charset=utf-8,{}",
                                js_sys::encode_uri_component(&json)
                                    .as_string()
                                    .unwrap_or_default()
                            );
                            let _ = element.set_attribute("href", &href);
                            let _ = element.set_attribute("download", "redash-settings.json");
                            let html_el: web_sys::HtmlElement = element.unchecked_into();
                            html_el.click();
                        }
                        state.settings_save_status =
                            Some(("已成功导出设置 JSON 文件".to_string(), true));
                    }
                    UiAction::PromptPingTarget => {
                        if let Some(window) = web_sys::window() {
                            let current = &state.ping_target;
                            if let Ok(Some(val)) = window.prompt_with_message_and_default(
                                "请输入网络探测 Ping 目标地址 (IP 或域名):",
                                current,
                            ) {
                                let trimmed = val.trim();
                                if !trimmed.is_empty() {
                                    state.set_ping_target(trimmed.to_string());
                                    state.settings_save_status =
                                        Some((format!("探测节点已更新为 {}", trimmed), true));
                                }
                            }
                        }
                    }
                    UiAction::PromptWebhookUrl => {
                        if let Some(window) = web_sys::window() {
                            let current = state.settings.alert_webhook_url.as_deref().unwrap_or("");
                            if let Ok(Some(val)) = window.prompt_with_message_and_default(
                                "请输入 Webhook 机器人告警推送地址 (钉钉 / 企微 / 飞书):",
                                current,
                            ) {
                                let trimmed = val.trim();
                                if trimmed.is_empty() {
                                    state.set_webhook_url(None);
                                    state.settings_save_status =
                                        Some(("已清除 Webhook 推送地址".to_string(), true));
                                } else {
                                    state.set_webhook_url(Some(trimmed.to_string()));
                                    state.settings_save_status =
                                        Some(("已更新 Webhook 推送地址".to_string(), true));
                                }
                                let settings = state.settings.clone();
                                let app_state_save = app_state_clone.clone();
                                let on_done = Rc::new(RefCell::new(move |_| {
                                    app_state_save.borrow_mut().settings_save_status =
                                        Some(("Webhook 地址已成功保存".to_string(), true));
                                }));
                                gateway::async_save_settings(settings, on_done);
                            }
                        }
                    }
                    UiAction::TestWebhookAlert => {
                        let webhook_url = state.settings.alert_webhook_url.clone();
                        let app_state_test = app_state_clone.clone();
                        state.settings_save_status =
                            Some(("正在向 Webhook/飞书 发送测试消息...".to_string(), true));
                        let on_done =
                            Rc::new(RefCell::new(move |res: Result<String, String>| match res {
                                Ok(msg) => {
                                    app_state_test.borrow_mut().settings_save_status =
                                        Some((msg, true));
                                }
                                Err(err) => {
                                    app_state_test.borrow_mut().settings_save_status =
                                        Some((format!("推送失败: {err}"), false));
                                }
                            }));
                        gateway::async_test_webhook(webhook_url, on_done);
                    }
                    UiAction::TestBrowserNotification => {
                        match notifications::show_browser_notification(
                            "ReDash 告警测试",
                            "这是一条来自 ReDash Web 运维工作台的自动化测试通知，指标监控系统运转正常。",
                        ) {
                            Ok(()) => {
                                state.settings_save_status =
                                    Some(("已发送浏览器桌面测试通知！".to_string(), true));
                            }
                            Err(msg) => {
                                state.settings_save_status = Some((msg, false));
                            }
                        }
                    }
                    UiAction::ImportSettingsJson => {
                        if let Some(window) = web_sys::window()
                            && let Some(document) = window.document()
                            && let Ok(element) = document.create_element("input")
                            && let Ok(input_el) = element.dyn_into::<web_sys::HtmlInputElement>()
                        {
                            input_el.set_type("file");
                            let _ = input_el.set_attribute("accept", ".json");
                            let app_state_import = app_state_clone.clone();
                            let input_clone = input_el.clone();
                            let on_change = Closure::<dyn FnMut()>::new(move || {
                                if let Some(files) = input_clone.files()
                                    && let Some(file) = files.get(0)
                                    && let Ok(reader) = web_sys::FileReader::new()
                                {
                                    let reader_clone = reader.clone();
                                    let app_state_reader = app_state_import.clone();
                                    let on_load = Closure::<dyn FnMut()>::new(move || {
                                        if let Ok(val) = reader_clone.result()
                                            && let Some(json_text) = val.as_string()
                                        {
                                            match serde_json::from_str::<
                                                redash_types::settings::AppSettings,
                                            >(
                                                &json_text
                                            ) {
                                                Ok(imported) => {
                                                    let mut sm = app_state_reader.borrow_mut();
                                                    sm.settings = imported.clone();
                                                    sm.settings_save_status = Some((
                                                        "已成功导入设置配置！".to_string(),
                                                        true,
                                                    ));
                                                    let on_done =
                                                        Rc::new(RefCell::new(move |_| {}));
                                                    gateway::async_save_settings(imported, on_done);
                                                }
                                                Err(e) => {
                                                    app_state_reader
                                                        .borrow_mut()
                                                        .settings_save_status = Some((
                                                        format!("JSON 格式解析失败: {}", e),
                                                        false,
                                                    ));
                                                }
                                            }
                                        }
                                    });
                                    reader.set_onload(Some(on_load.as_ref().unchecked_ref()));
                                    on_load.forget();
                                    let _ = reader.read_as_text(&file);
                                }
                            });
                            input_el.set_onchange(Some(on_change.as_ref().unchecked_ref()));
                            on_change.forget();
                            input_el.click();
                        }
                    }
                    UiAction::SaveNewHost => {
                        if let Some(new_host) = state.build_new_host() {
                            state.hosts.push(new_host.clone());
                            let on_done = Rc::new(RefCell::new(
                                move |res: Result<models::HostConfig, String>| match res {
                                    Ok(saved) => {
                                        console::log_1(
                                            &format!("Saved host to backend: {}", saved.name)
                                                .into(),
                                        );
                                    }
                                    Err(err) => {
                                        console::log_1(
                                            &format!("Failed to persist host: {}", err).into(),
                                        );
                                    }
                                },
                            ));
                            gateway::async_save_host(new_host, on_done);
                        }
                    }
                    UiAction::DeleteHost(host_id) => {
                        state.hosts.retain(|h| h.id.0 != host_id);
                        if state.selected_host_id.as_deref() == Some(&host_id) {
                            state.selected_host_id = state.hosts.first().map(|h| h.id.0.clone());
                        }
                        let on_done =
                            Rc::new(RefCell::new(move |res: Result<(), String>| match res {
                                Ok(()) => {
                                    console::log_1(&"Host deleted from backend".into());
                                }
                                Err(err) => {
                                    console::log_1(
                                        &format!("Failed to delete host from backend: {}", err)
                                            .into(),
                                    );
                                }
                            }));
                        gateway::async_delete_host(host_id, on_done);
                    }
                    UiAction::RunBatch { host_ids, command } => {
                        let app_state_batch = app_state_clone.clone();
                        let on_done = Rc::new(RefCell::new(
                            move |res: Result<redash_types::batch::BatchJobResult, String>| {
                                let mut sm = app_state_batch.borrow_mut();
                                sm.set_batch_running(false);
                                match res {
                                    Ok(results) => {
                                        sm.set_batch_results(results);
                                    }
                                    Err(err) => {
                                        console::log_1(
                                            &format!("Batch execution error: {}", err).into(),
                                        );
                                    }
                                }
                            },
                        ));
                        gateway::async_run_batch(host_ids, command, on_done);
                    }
                    UiAction::ApplyAgentSuggestion => {
                        gateway_clone.borrow().send_terminal_input("\r");
                        if let Some(ref mut agent) = state.agent {
                            agent.status = redash_types::agent::AgentStatus::Done;
                        }
                    }
                    UiAction::AbortAgentTask => {
                        gateway_clone.borrow().send_terminal_input("\x03");
                        state.agent = None;
                    }
                    UiAction::TestDraftHost => {
                        let hostname = state.modal_hostname.trim().to_string();
                        let port: u16 = state.modal_port.trim().parse().unwrap_or(22);
                        let user = if state.modal_user.trim().is_empty() {
                            "root".to_string()
                        } else {
                            state.modal_user.trim().to_string()
                        };

                        if hostname.is_empty() {
                            state.set_modal_test_status(Some((
                                "主机名/IP不能为空".to_string(),
                                false,
                            )));
                        } else {
                            state.set_modal_is_testing(true);
                            let app_state_clone2 = app_state_clone.clone();
                            let on_done =
                                Rc::new(RefCell::new(move |res: Result<String, String>| {
                                    let mut sm = app_state_clone2.borrow_mut();
                                    sm.set_modal_is_testing(false);
                                    match res {
                                        Ok(msg) => {
                                            sm.set_modal_test_status(Some((msg, true)));
                                        }
                                        Err(err) => {
                                            sm.set_modal_test_status(Some((err, false)));
                                        }
                                    }
                                }));
                            gateway::async_test_draft_host(hostname, port, user, None, on_done);
                        }
                    }
                    _ => {}
                }
            }
        });

        canvas.set_onmousedown(Some(on_mousedown.as_ref().unchecked_ref()));
        on_mousedown.forget();
    }

    // Register Mouse Move and Hover Event Listener on Canvas
    {
        let app_state_clone = app_state.clone();
        let canvas_clone = canvas.clone();
        let window = window.clone();

        let on_mousemove = Closure::<dyn FnMut(_)>::new(move |e: MouseEvent| {
            let rect = canvas_clone.get_bounding_client_rect();
            let x = e.client_x() as f64 - rect.left();
            let y = e.client_y() as f64 - rect.top();
            let width = window.inner_width().unwrap().as_f64().unwrap();
            let height = window.inner_height().unwrap().as_f64().unwrap();

            let mut state = app_state_clone.borrow_mut();
            let (_, cursor) = render::is_interactive_element(x, y, width, height, &state);
            let html_el: &web_sys::HtmlElement = canvas_clone.unchecked_ref();
            let _ = html_el.style().set_property("cursor", cursor);
            let _ = state.set_hover_pos(Some((x, y)));
        });

        canvas.set_onmousemove(Some(on_mousemove.as_ref().unchecked_ref()));
        on_mousemove.forget();
    }

    // Register Mouse Leave Event Listener on Canvas
    {
        let app_state_clone = app_state.clone();
        let canvas_clone = canvas.clone();

        let on_mouseleave = Closure::<dyn FnMut()>::new(move || {
            let html_el: &web_sys::HtmlElement = canvas_clone.unchecked_ref();
            let _ = html_el.style().set_property("cursor", "default");
            let _ = app_state_clone.borrow_mut().set_hover_pos(None);
        });

        canvas.set_onmouseleave(Some(on_mouseleave.as_ref().unchecked_ref()));
        on_mouseleave.forget();
    }

    // Register Keyboard Event Listener on Canvas
    {
        let app_state_clone = app_state.clone();
        let gateway_clone = gateway.clone();

        let on_keydown = Closure::<dyn FnMut(_)>::new(move |e: KeyboardEvent| {
            let key = e.key();
            let is_ctrl = e.ctrl_key() || e.meta_key();

            let mut state = app_state_clone.borrow_mut();

            // Prevent browser default on application shortcuts
            if (key == "s" || key == "S") && is_ctrl {
                e.prevent_default();
            }
            if (key == "f" || key == "F") && is_ctrl {
                e.prevent_default();
            }
            if key == "Tab" && state.show_add_modal {
                e.prevent_default();
            }
            if key == "Backspace"
                && (state.is_filter_focused || state.show_add_modal || state.terminal_search_active)
            {
                e.prevent_default();
            }
            if key == "Enter" && is_ctrl && state.active_view == ActiveView::Batch {
                e.prevent_default();
            }

            if let Some(action) = handle_key_down(&mut state, &key, is_ctrl) {
                match action {
                    UiAction::SendTerminalInput(input) => {
                        gateway_clone.borrow().send_terminal_input(&input);
                    }
                    UiAction::SaveSftpFile {
                        host_id,
                        path,
                        content,
                    } => {
                        let app_state_sftp = app_state_clone.clone();
                        let on_done = Rc::new(RefCell::new(move |res: Result<(), String>| {
                            let mut sm = app_state_sftp.borrow_mut();
                            match res {
                                Ok(()) => {
                                    sm.sftp_editor_modified = false;
                                    sm.set_sftp_loading(false);
                                }
                                Err(err) => sm.set_sftp_error(Some(err)),
                            }
                        }));
                        gateway::async_write_sftp_file(host_id, path, content, on_done);
                    }
                    UiAction::SaveSettings => {
                        let app_state_save = app_state_clone.clone();
                        let settings = app_state_save.borrow().settings.clone();
                        let on_done = Rc::new(RefCell::new(move |res: Result<(), String>| {
                            let mut sm = app_state_save.borrow_mut();
                            match res {
                                Ok(()) => {
                                    sm.settings_save_status =
                                        Some(("设置已成功保存到云端服务器".to_string(), true));
                                }
                                Err(err) => {
                                    sm.settings_save_status =
                                        Some((format!("设置保存失败: {}", err), false));
                                }
                            }
                        }));
                        gateway::async_save_settings(settings, on_done);
                    }
                    UiAction::ExportSettingsJson => {
                        let json =
                            serde_json::to_string_pretty(&state.settings).unwrap_or_default();
                        console::log_1(&format!("Exported Settings JSON:\n{}", json).into());
                        state.settings_save_status =
                            Some(("已成功导出设置 JSON 文件".to_string(), true));
                    }
                    UiAction::SaveNewHost => {
                        if let Some(new_host) = state.build_new_host() {
                            state.hosts.push(new_host.clone());
                            let on_done = Rc::new(RefCell::new(
                                move |res: Result<models::HostConfig, String>| match res {
                                    Ok(saved) => {
                                        console::log_1(
                                            &format!("Saved host to backend: {}", saved.name)
                                                .into(),
                                        );
                                    }
                                    Err(err) => {
                                        console::log_1(
                                            &format!("Failed to persist host: {}", err).into(),
                                        );
                                    }
                                },
                            ));
                            gateway::async_save_host(new_host, on_done);
                        }
                    }
                    UiAction::DeleteHost(host_id) => {
                        state.hosts.retain(|h| h.id.0 != host_id);
                        if state.selected_host_id.as_deref() == Some(&host_id) {
                            state.selected_host_id = state.hosts.first().map(|h| h.id.0.clone());
                        }
                        let on_done =
                            Rc::new(RefCell::new(move |res: Result<(), String>| match res {
                                Ok(()) => {
                                    console::log_1(&"Host deleted from backend".into());
                                }
                                Err(err) => {
                                    console::log_1(
                                        &format!("Failed to delete host from backend: {}", err)
                                            .into(),
                                    );
                                }
                            }));
                        gateway::async_delete_host(host_id, on_done);
                    }
                    UiAction::RunBatch { host_ids, command } => {
                        let app_state_batch = app_state_clone.clone();
                        let on_done = Rc::new(RefCell::new(
                            move |res: Result<redash_types::batch::BatchJobResult, String>| {
                                let mut sm = app_state_batch.borrow_mut();
                                sm.set_batch_running(false);
                                match res {
                                    Ok(results) => {
                                        sm.set_batch_results(results);
                                    }
                                    Err(err) => {
                                        console::log_1(
                                            &format!("Batch execution error: {}", err).into(),
                                        );
                                    }
                                }
                            },
                        ));
                        gateway::async_run_batch(host_ids, command, on_done);
                    }
                    UiAction::ApplyAgentSuggestion => {
                        gateway_clone.borrow().send_terminal_input("\r");
                        if let Some(ref mut agent) = state.agent {
                            agent.status = redash_types::agent::AgentStatus::Done;
                        }
                    }
                    UiAction::AbortAgentTask => {
                        gateway_clone.borrow().send_terminal_input("\x03");
                        state.agent = None;
                    }
                    UiAction::TestDraftHost => {
                        let hostname = state.modal_hostname.trim().to_string();
                        let port: u16 = state.modal_port.trim().parse().unwrap_or(22);
                        let user = if state.modal_user.trim().is_empty() {
                            "root".to_string()
                        } else {
                            state.modal_user.trim().to_string()
                        };

                        if hostname.is_empty() {
                            state.set_modal_test_status(Some((
                                "主机名/IP不能为空".to_string(),
                                false,
                            )));
                        } else {
                            state.set_modal_is_testing(true);
                            let app_state_clone2 = app_state_clone.clone();
                            let on_done =
                                Rc::new(RefCell::new(move |res: Result<String, String>| {
                                    let mut sm = app_state_clone2.borrow_mut();
                                    sm.set_modal_is_testing(false);
                                    match res {
                                        Ok(msg) => {
                                            sm.set_modal_test_status(Some((msg, true)));
                                        }
                                        Err(err) => {
                                            sm.set_modal_test_status(Some((err, false)));
                                        }
                                    }
                                }));
                            gateway::async_test_draft_host(hostname, port, user, None, on_done);
                        }
                    }
                    _ => {}
                }
            }
        });

        canvas.set_onkeydown(Some(on_keydown.as_ref().unchecked_ref()));
        on_keydown.forget();
    }

    // High Performance 120 FPS Render Loop via requestAnimationFrame
    {
        let f = Rc::new(RefCell::new(None));
        let g = f.clone();

        let app_state_render = app_state.clone();
        let gateway_render = gateway.clone();
        let window_render = window.clone();

        *g.borrow_mut() = Some(Closure::<dyn FnMut()>::new(move || {
            let width = window_render.inner_width().unwrap().as_f64().unwrap();
            let height = window_render.inner_height().unwrap().as_f64().unwrap();
            let dpr = window_render.device_pixel_ratio();

            // Terminal Viewport Auto-Resize (Parity with desktop app)
            if app_state_render.borrow().active_view == ActiveView::Terminal
                && app_state_render.borrow().active_workbench_tab == WorkbenchTab::Terminal
            {
                let content_w = width - render::LAYOUT.sidebar_width;
                let mut term_h =
                    height - render::LAYOUT.topbar_height - render::WORKBENCH_TAB_BAR_HEIGHT;
                if app_state_render.borrow().agent.is_some() {
                    term_h -= 32.0;
                }
                let cols = ((content_w - 32.0) / 7.8).max(20.0) as usize;
                let rows = (term_h / 18.0).max(5.0) as usize;
                let effects = app_state_render.borrow_mut().resize_terminal(cols, rows);
                for effect in effects {
                    if let UiEffect::ResizeTerminal { cols, rows } = effect {
                        gateway_render.borrow().send_terminal_resize(cols, rows);
                    }
                }
            }

            ctx.save();
            ctx.scale(dpr, dpr).unwrap();

            let state = app_state_render.borrow();
            let _ = render_frame(&ctx, &state, width, height);

            ctx.restore();

            request_animation_frame(f.borrow().as_ref().unwrap(), &window_render);
        }));

        request_animation_frame(g.borrow().as_ref().unwrap(), &window);
    }

    console::log_1(&"✨ ReDash GPUI Web Running Successfully on Canvas!".into());
    Ok(())
}

fn request_animation_frame(f: &Closure<dyn FnMut()>, window: &Window) {
    window
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("should register `requestAnimationFrame` OK");
}
