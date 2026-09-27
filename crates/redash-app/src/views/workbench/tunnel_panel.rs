use gpui::prelude::FluentBuilder;
use gpui::*;
use std::collections::HashMap;
use std::sync::Arc;

use crate::components::icon::Icon;
use crate::components::theme::DarkTechTheme;
use redash_core::config::HostConfig;
use redash_core::session::SessionManager;
use redash_core::session::tunnel::{
    TunnelConfig, TunnelHealthState, TunnelManager, TunnelStats, TunnelType,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelModalField {
    Name,
    LocalPort,
    RemoteHost,
    RemotePort,
}

pub struct TunnelPanel {
    pub host: HostConfig,
    pub tunnel_mgr: Arc<TunnelManager>,
    pub tunnels: Vec<TunnelConfig>,
    pub tunnel_stats: HashMap<String, TunnelStats>,
    pub show_create_modal: bool,
    pub new_name: String,
    pub new_type_is_socks: bool,
    pub new_local_port: String,
    pub new_remote_host: String,
    pub new_remote_port: String,
    pub active_field: TunnelModalField,
    pub focus_handle: Option<FocusHandle>,
    pub status_message: Option<(String, bool)>,
    pub session_mgr: Option<Arc<SessionManager>>,
    pub scroll_handle: ScrollHandle,
}

impl TunnelPanel {
    pub fn new(host: HostConfig, tunnel_mgr: Arc<TunnelManager>) -> Self {
        Self {
            host,
            tunnel_mgr,
            tunnels: Vec::new(),
            tunnel_stats: HashMap::new(),
            show_create_modal: false,
            new_name: String::new(),
            new_type_is_socks: false,
            new_local_port: "1080".to_string(),
            new_remote_host: "127.0.0.1".to_string(),
            new_remote_port: "80".to_string(),
            active_field: TunnelModalField::LocalPort,
            focus_handle: None,
            status_message: None,
            session_mgr: None,
            scroll_handle: ScrollHandle::new(),
        }
    }

    pub fn with_session_mgr(mut self, session_mgr: Arc<SessionManager>) -> Self {
        self.session_mgr = Some(session_mgr);
        self
    }

    pub fn get_active_field_text_mut(&mut self) -> &mut String {
        match self.active_field {
            TunnelModalField::Name => &mut self.new_name,
            TunnelModalField::LocalPort => &mut self.new_local_port,
            TunnelModalField::RemoteHost => &mut self.new_remote_host,
            TunnelModalField::RemotePort => &mut self.new_remote_port,
        }
    }

    pub fn insert_char(&mut self, ch: char) {
        let text = self.get_active_field_text_mut();
        text.push(ch);
    }

    pub fn insert_str(&mut self, s: &str) {
        let text = self.get_active_field_text_mut();
        text.push_str(s);
    }

    pub fn backspace(&mut self) {
        let text = self.get_active_field_text_mut();
        text.pop();
    }

    pub fn cycle_field(&mut self, forward: bool) {
        let fields = if self.new_type_is_socks {
            vec![TunnelModalField::Name, TunnelModalField::LocalPort]
        } else {
            vec![
                TunnelModalField::Name,
                TunnelModalField::LocalPort,
                TunnelModalField::RemoteHost,
                TunnelModalField::RemotePort,
            ]
        };
        if let Some(pos) = fields.iter().position(|f| *f == self.active_field) {
            let next_pos = if forward {
                (pos + 1) % fields.len()
            } else if pos == 0 {
                fields.len().saturating_sub(1)
            } else {
                pos - 1
            };
            self.active_field = fields[next_pos];
        } else if let Some(&first) = fields.first() {
            self.active_field = first;
        }
    }

    pub fn handle_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = &event.keystroke.modifiers;

        if key == "escape" {
            self.show_create_modal = false;
            cx.notify();
            return;
        }

        if key == "tab" {
            let forward = !modifiers.shift;
            self.cycle_field(forward);
            cx.notify();
            return;
        }

        if key == "enter" {
            self.create_tunnel(cx);
            return;
        }

        if (modifiers.platform || modifiers.control) && (key == "v" || key == "V") {
            if let Some(item) = cx.read_from_clipboard()
                && let Some(text) = item.text()
            {
                let clean = text.replace("\r\n", "").replace(['\n', '\r'], "");
                self.insert_str(&clean);
                cx.notify();
            }
            return;
        }

        if key == "backspace" {
            self.backspace();
            cx.notify();
            return;
        }

        if key == "space" && !modifiers.control && !modifiers.platform {
            self.insert_char(' ');
            cx.notify();
            return;
        }

        // Printable characters
        if !modifiers.control && !modifiers.platform {
            if let Some(ref kc) = event.keystroke.key_char {
                if !kc.is_empty() && kc != "\n" && kc != "\r" && kc != "\t" {
                    self.insert_str(kc);
                    cx.notify();
                }
            } else if key.chars().count() == 1
                && let Some(ch) = key.chars().next()
            {
                self.insert_char(ch);
                cx.notify();
            }
        }
    }

    pub fn toggle_tunnel(&mut self, tunnel_id: String, cx: &mut Context<Self>) {
        let pos = self.tunnels.iter().position(|t| t.id == tunnel_id);
        if let Some(idx) = pos {
            let tunnel = &self.tunnels[idx];
            let is_currently_active = tunnel.active;
            let tunnel_mgr = Arc::clone(&self.tunnel_mgr);
            let tid = tunnel_id.clone();
            let tname = tunnel.name.clone();

            if is_currently_active {
                cx.spawn(async move |this, cx| {
                    let res = tunnel_mgr.stop_tunnel(&tid).await;
                    let _ = this.update(cx, |view, cx| {
                        if let Some(t) = view.tunnels.iter_mut().find(|t| t.id == tid) {
                            t.active = false;
                        }
                        view.tunnel_stats.remove(&tid);
                        match res {
                            Ok(_) => {
                                view.status_message =
                                    Some((format!("隧道 [{}] 已成功停止", tname), true))
                            }
                            Err(e) => {
                                view.status_message = Some((format!("停止隧道失败: {}", e), false))
                            }
                        }
                        cx.notify();
                    });
                })
                .detach();
            } else if let Some(session_mgr) = &self.session_mgr {
                let session_mgr = Arc::clone(session_mgr);
                let host = self.host.clone();
                let config = tunnel.clone();

                cx.spawn(async move |this, cx| {
                    let handle_res = session_mgr.get_or_connect(&host).await;
                    match handle_res {
                        Ok(handle) => {
                            let start_res = tunnel_mgr.start_tunnel(config, handle).await;
                            let _ = this.update(cx, |view, cx| {
                                match start_res {
                                    Ok(_) => {
                                        if let Some(t) =
                                            view.tunnels.iter_mut().find(|t| t.id == tid)
                                        {
                                            t.active = true;
                                        }
                                        view.status_message =
                                            Some((format!("隧道 [{}] 启动成功", tname), true));
                                        view.refresh_health(cx);
                                    }
                                    Err(e) => {
                                        view.status_message =
                                            Some((format!("启动隧道失败: {}", e), false));
                                    }
                                }
                                cx.notify();
                            });
                        }
                        Err(e) => {
                            let _ = this.update(cx, |view, cx| {
                                view.status_message = Some((format!("SSH 连接失败: {}", e), false));
                                cx.notify();
                            });
                        }
                    }
                })
                .detach();
            } else {
                // If session manager is not set, toggle in memory
                self.tunnels[idx].active = true;
                self.status_message = Some((format!("隧道 [{}] 状态已激活", tname), true));
                cx.notify();
            }
        }
    }

    pub fn refresh_health(&mut self, cx: &mut Context<Self>) {
        let tunnel_mgr = Arc::clone(&self.tunnel_mgr);
        cx.spawn(async move |this, cx| {
            let stats_list = tunnel_mgr.list_tunnel_stats().await;
            let _ = this.update(cx, |view, cx| {
                for st in stats_list {
                    view.tunnel_stats.insert(st.tunnel_id.clone(), st);
                }
                view.status_message =
                    Some(("已完成隧道连接池健康状态同步与心跳诊断".to_string(), true));
                cx.notify();
            });
        })
        .detach();
    }

    pub fn create_tunnel(&mut self, cx: &mut Context<Self>) {
        let local_port = match self.new_local_port.trim().parse::<u16>() {
            Ok(p) if p > 0 => p,
            _ => {
                self.status_message =
                    Some(("本地端口无效，请输入 1-65535 整数".to_string(), false));
                cx.notify();
                return;
            }
        };

        let name = if self.new_name.trim().is_empty() {
            if self.new_type_is_socks {
                format!("SOCKS5-{}", local_port)
            } else {
                format!("TCP-{}", local_port)
            }
        } else {
            self.new_name.trim().to_string()
        };

        let tunnel_type = if self.new_type_is_socks {
            TunnelType::DynamicSocks5 { local_port }
        } else {
            let remote_port = match self.new_remote_port.trim().parse::<u16>() {
                Ok(p) if p > 0 => p,
                _ => {
                    self.status_message =
                        Some(("远程目标端口无效，请输入 1-65535 整数".to_string(), false));
                    cx.notify();
                    return;
                }
            };
            let remote_host = if self.new_remote_host.trim().is_empty() {
                "127.0.0.1".to_string()
            } else {
                self.new_remote_host.trim().to_string()
            };
            TunnelType::Local {
                local_port,
                remote_host,
                remote_port,
            }
        };

        let id = format!(
            "tun-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );

        self.tunnels.push(TunnelConfig {
            id,
            name: name.clone(),
            tunnel_type,
            active: false,
        });

        self.show_create_modal = false;
        self.new_name.clear();
        self.status_message = Some((format!("已创建新隧道配置 [{}]", name), true));
        cx.notify();
    }

    pub fn delete_tunnel(&mut self, tunnel_id: String, cx: &mut Context<Self>) {
        if let Some(pos) = self.tunnels.iter().position(|t| t.id == tunnel_id) {
            let tunnel = self.tunnels.remove(pos);
            if tunnel.active {
                let tunnel_mgr = Arc::clone(&self.tunnel_mgr);
                let tid = tunnel_id.clone();
                cx.spawn(async move |_this, _cx| {
                    let _ = tunnel_mgr.stop_tunnel(&tid).await;
                })
                .detach();
            }
            self.status_message = Some((format!("已删除隧道 [{}]", tunnel.name), true));
            cx.notify();
        }
    }
}

impl Render for TunnelPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self
            .focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone();

        let total_tunnels = self.tunnels.len();
        let active_count = self.tunnels.iter().filter(|t| t.active).count();

        let card_elements: Vec<_> = if self.tunnels.is_empty() {
            vec![div()
                .id("tunnel_empty_placeholder")
                .p_8()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .child(Icon::tunnel().with_size(px(32.0)).with_color(DarkTechTheme::text_muted()))
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(DarkTechTheme::text_muted())
                        .child("暂无配置的隧道，点击右上角【+ 新建隧道】创建端口映射或 SOCKS5 代理"),
                )]
        } else {
            self.tunnels
                .iter()
                .enumerate()
                .map(|(idx, t)| {
                    let t_id_toggle = t.id.clone();
                    let t_id_del = t.id.clone();
                    let is_active = t.active;

                    let (type_badge, is_socks, endpoint_desc) = match &t.tunnel_type {
                        TunnelType::Local {
                            local_port,
                            remote_host,
                            remote_port,
                        } => (
                            "Local TCP",
                            false,
                            format!(
                                "127.0.0.1:{} -> {}:{}",
                                local_port, remote_host, remote_port
                            ),
                        ),
                        TunnelType::DynamicSocks5 { local_port } => (
                            "Dynamic SOCKS5",
                            true,
                            format!("127.0.0.1:{} (SOCKS5 代理网关)", local_port),
                        ),
                    };

                    let stats_opt = self.tunnel_stats.get(&t.id);
                    let (health_color, health_label, conn_stats_str) = if is_active {
                        if let Some(st) = stats_opt {
                            let (color, lbl) = match &st.health {
                                TunnelHealthState::Healthy { latency_ms } => (
                                    DarkTechTheme::status_online(),
                                    format!("健康 ({}ms)", latency_ms),
                                ),
                                TunnelHealthState::Degraded { reason } => {
                                    (DarkTechTheme::status_warn(), format!("降级 ({})", reason))
                                }
                                TunnelHealthState::Unhealthy { error } => {
                                    (DarkTechTheme::status_crit(), format!("异常 ({})", error))
                                }
                                TunnelHealthState::Stopped => {
                                    (DarkTechTheme::text_muted(), "已停止".to_string())
                                }
                            };
                            let conns = format!(
                                "活跃连接: {} | 累计: {}",
                                st.active_connections, st.total_connections
                            );
                            (color, lbl, conns)
                        } else {
                            (
                                DarkTechTheme::status_online(),
                                "运行中".to_string(),
                                "活跃: 0".to_string(),
                            )
                        }
                    } else {
                        (
                            DarkTechTheme::text_muted(),
                            "已停止".to_string(),
                            String::new(),
                        )
                    };

                    div()
                        .id(ElementId::NamedInteger("tunnel_card".into(), idx as u64))
                        .w_full()
                        .bg(DarkTechTheme::bg_input())
                        .border_1()
                        .border_color(if is_active {
                            DarkTechTheme::border_active()
                        } else {
                            DarkTechTheme::border_default()
                        })
                        .rounded_lg()
                        .p_3()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        // Left: Badge, Name, Endpoint
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                // Type Badge
                                .child(
                                    div()
                                        .px_2()
                                        .py_1()
                                        .rounded_sm()
                                        .bg(if is_socks {
                                            rgba(0x38bdf822)
                                        } else {
                                            rgba(0x818cf822)
                                        })
                                        .border_1()
                                        .border_color(if is_socks {
                                            DarkTechTheme::accent_cyan()
                                        } else {
                                            DarkTechTheme::accent_indigo()
                                        })
                                        .text_size(px(10.5))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(if is_socks {
                                            DarkTechTheme::accent_cyan()
                                        } else {
                                            DarkTechTheme::accent_indigo()
                                        })
                                        .child(type_badge),
                                )
                                // Info
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_0p5()
                                        .child(
                                            div()
                                                .font_weight(FontWeight::BOLD)
                                                .text_size(px(13.0))
                                                .text_color(DarkTechTheme::text_primary())
                                                .child(t.name.clone()),
                                        )
                                        .child(
                                            div()
                                                .font_family("Menlo")
                                                .text_size(px(11.5))
                                                .text_color(DarkTechTheme::text_secondary())
                                                .child(endpoint_desc),
                                        )
                                        .children(if is_active && !conn_stats_str.is_empty() {
                                            vec![
                                                div()
                                                    .font_family("Menlo")
                                                    .text_size(px(10.5))
                                                    .text_color(DarkTechTheme::accent_cyan())
                                                    .child(conn_stats_str),
                                            ]
                                        } else {
                                            vec![]
                                        }),
                                ),
                        )
                        // Right: Status LED, Toggle Button, Delete Button
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                // Status Indicator
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_1p5()
                                        .child(div().size(px(8.0)).rounded_full().bg(health_color))
                                        .child(
                                            div()
                                                .text_size(px(11.5))
                                                .font_family("Menlo")
                                                .text_color(health_color)
                                                .child(health_label),
                                        ),
                                )
                                // Toggle Start/Stop Button
                                .child(
                                    div()
                                        .id(ElementId::Name(
                                            format!("btn_toggle_{}", t_id_toggle).into(),
                                        ))
                                        .px_3()
                                        .py_1()
                                        .rounded_md()
                                        .bg(if is_active {
                                            rgba(0xef444422)
                                        } else {
                                            rgba(0x10b98122)
                                        })
                                        .border_1()
                                        .border_color(if is_active {
                                            DarkTechTheme::status_crit()
                                        } else {
                                            DarkTechTheme::status_online()
                                        })
                                        .text_color(if is_active {
                                            DarkTechTheme::status_crit()
                                        } else {
                                            DarkTechTheme::status_online()
                                        })
                                        .text_size(px(11.5))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.toggle_tunnel(t_id_toggle.clone(), cx);
                                        }))
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_1()
                                                .child(if is_active {
                                                    Icon::stop()
                                                        .with_size(px(10.0))
                                                        .with_color(DarkTechTheme::status_warn())
                                                } else {
                                                    Icon::play()
                                                        .with_size(px(10.0))
                                                        .with_color(DarkTechTheme::status_online())
                                                })
                                                .child(if is_active {
                                                    crate::t!("docker.stop")
                                                } else {
                                                    crate::t!("docker.start")
                                                }),
                                        ),
                                )
                                // Delete Button
                                .child(
                                    div()
                                        .id(ElementId::Name(format!("btn_del_{}", t_id_del).into()))
                                        .px_2()
                                        .py_1()
                                        .rounded_md()
                                        .bg(DarkTechTheme::bg_panel())
                                        .border_1()
                                        .border_color(DarkTechTheme::border_default())
                                        .text_color(DarkTechTheme::text_muted())
                                        .text_size(px(11.5))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.delete_tunnel(t_id_del.clone(), cx);
                                        }))
                                        .child(
                                            Icon::trash()
                                                .with_size(px(10.0))
                                                .with_color(DarkTechTheme::status_crit()),
                                        ),
                                ),
                        )
                })
                .collect()
        };

        div()
            .size_full()
            .overflow_hidden()
            .bg(DarkTechTheme::bg_root())
            .flex()
            .flex_col()
            .p_4()
            .gap_3()
            // Header Bar
            .child(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(Icon::tunnel().with_size(px(16.0)).with_color(DarkTechTheme::accent_cyan()))
                                    .child(
                                        div()
                                            .font_weight(FontWeight::BOLD)
                                            .text_size(px(14.0))
                                            .text_color(DarkTechTheme::text_primary())
                                            .child(crate::t!("tunnel.title")),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(DarkTechTheme::text_muted())
                                    .child(format!("活动: {} / 共计: {}", active_count, total_tunnels)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id("btn_refresh_tunnel_health")
                                    .px_2p5()
                                    .py_1p5()
                                    .rounded_md()
                                    .bg(rgba(0x38bdf81a))
                                    .border_1()
                                    .border_color(DarkTechTheme::accent_cyan())
                                    .hover(|s| s.bg(rgba(0x38bdf833)))
                                    .text_size(px(12.0))
                                    .text_color(DarkTechTheme::accent_cyan())
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.refresh_health(cx);
                                    }))
                                    .child("💓 心跳自愈与诊断")
                            )
                            .child(
                                div()
                                    .id("btn_open_create_tunnel")
                                    .px_3()
                                    .py_1p5()
                                    .rounded_md()
                                    .bg(DarkTechTheme::accent_cyan())
                                    .text_color(DarkTechTheme::bg_root())
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(12.0))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.show_create_modal = true;
                                        this.active_field = TunnelModalField::LocalPort;
                                        if let Some(ref fh) = this.focus_handle {
                                            window.focus(fh);
                                        }
                                        cx.notify();
                                    }))
                                    .child(crate::t!("tunnel.new_btn"))
                            )
                    ),
            )
            // Status Message Banner
            .when_some(self.status_message.as_ref(), |d, (msg, is_success)| {
                let is_ok = *is_success;
                let msg_cloned = msg.clone();
                d.child(
                    div()
                        .id("tunnel_status_banner")
                        .w_full()
                        .flex_shrink_0()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .bg(if is_ok {
                            rgba(0x10b98122)
                        } else {
                            rgba(0xef444422)
                        })
                        .border_1()
                        .border_color(if is_ok {
                            DarkTechTheme::status_online()
                        } else {
                            DarkTechTheme::status_crit()
                        })
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1p5()
                                .child(
                                    if is_ok {
                                        Icon::check().with_size(px(12.0)).with_color(DarkTechTheme::status_online())
                                    } else {
                                        Icon::close().with_size(px(12.0)).with_color(DarkTechTheme::status_crit())
                                    }
                                )
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(if is_ok {
                                            DarkTechTheme::status_online()
                                        } else {
                                            DarkTechTheme::status_crit()
                                        })
                                        .child(msg_cloned),
                                ),
                        )
                        .child(
                            div()
                                .id("btn_dismiss_tunnel_status")
                                .cursor_pointer()
                                .text_size(px(11.0))
                                .text_color(DarkTechTheme::text_muted())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.status_message = None;
                                    cx.notify();
                                }))
                                .child(Icon::close().with_size(px(9.0)).with_color(DarkTechTheme::text_muted())),
                        ),
                )
            })
            // Tunnel List Area
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .min_h(px(0.0))
                    .bg(DarkTechTheme::bg_panel())
                    .border_1()
                    .border_color(DarkTechTheme::border_default())
                    .rounded_lg()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(
                        div()
                            .id("tunnel_list_container")
                            .track_scroll(&self.scroll_handle)
                            .flex_1()
                            .w_full()
                            .min_h(px(0.0))
                            .overflow_y_scroll()
                            .p_3()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .children(card_elements),
                    ),
            )
            // Modal Dialog for Creating New Tunnel
            .when(self.show_create_modal, |d| {
                let is_socks = self.new_type_is_socks;
                d.child(
                    div()
                        .id("modal_create_tunnel_backdrop")
                        .absolute()
                        .inset_0()
                        .bg(rgba(0x000000aa))
                        .flex()
                        .items_center()
                        .justify_center()
                        .p_6()
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _window, cx| {
                            this.show_create_modal = false;
                            cx.notify();
                        }))
                        .child(
                            div()
                                .id("modal_create_tunnel_box")
                                .track_focus(&focus_handle)
                                .w(px(460.0))
                                .bg(DarkTechTheme::bg_panel())
                                .border_1()
                                .border_color(DarkTechTheme::border_active())
                                .rounded_xl()
                                .p_5()
                                .flex()
                                .flex_col()
                                .gap_4()
                                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                                    if let Some(ref fh) = this.focus_handle {
                                        window.focus(fh);
                                    }
                                    cx.notify();
                                    cx.stop_propagation();
                                }))
                                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                                    this.handle_key_down(event, cx);
                                }))
                                // Title
                                .child(
                                    div()
                                        .font_weight(FontWeight::BOLD)
                                        .text_size(px(14.0))
                                        .text_color(DarkTechTheme::text_primary())
                                        .child("新建 SSH 端口转发 / 动态代理隧道"),
                                )
                                // Type Selection Tabs
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .gap_2()
                                        .child(
                                            div()
                                                .id("tab_type_local")
                                                .flex_1()
                                                .py_1p5()
                                                .rounded_md()
                                                .bg(if !is_socks {
                                                    DarkTechTheme::bg_panel_hover()
                                                } else {
                                                    DarkTechTheme::bg_input()
                                                })
                                                .border_1()
                                                .border_color(if !is_socks {
                                                    DarkTechTheme::border_active()
                                                } else {
                                                    DarkTechTheme::border_default()
                                                })
                                                .text_align(TextAlign::Center)
                                                .text_size(px(12.0))
                                                .text_color(if !is_socks {
                                                    DarkTechTheme::text_accent()
                                                } else {
                                                    DarkTechTheme::text_secondary()
                                                })
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.new_type_is_socks = false;
                                                    cx.notify();
                                                }))
                                                .child(crate::t!("tunnel.local_type")),
                                        )
                                        .child(
                                            div()
                                                .id("tab_type_socks")
                                                .flex_1()
                                                .py_1p5()
                                                .rounded_md()
                                                .bg(if is_socks {
                                                    DarkTechTheme::bg_panel_hover()
                                                } else {
                                                    DarkTechTheme::bg_input()
                                                })
                                                .border_1()
                                                .border_color(if is_socks {
                                                    DarkTechTheme::border_active()
                                                } else {
                                                    DarkTechTheme::border_default()
                                                })
                                                .text_align(TextAlign::Center)
                                                .text_size(px(12.0))
                                                .text_color(if is_socks {
                                                    DarkTechTheme::text_accent()
                                                } else {
                                                    DarkTechTheme::text_secondary()
                                                })
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.new_type_is_socks = true;
                                                    if this.active_field == TunnelModalField::RemoteHost
                                                        || this.active_field == TunnelModalField::RemotePort
                                                    {
                                                        this.active_field = TunnelModalField::LocalPort;
                                                    }
                                                    cx.notify();
                                                }))
                                                .child(crate::t!("tunnel.socks_type")),
                                        ),
                                )
                                // Input: Name
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_size(px(11.0))
                                                .text_color(DarkTechTheme::text_secondary())
                                                .child("隧道标识名称 (可选):"),
                                        )
                                        .child(
                                            div()
                                                .id("input_new_tunnel_name")
                                                .px_3()
                                                .py_1p5()
                                                .rounded_md()
                                                .bg(DarkTechTheme::bg_input())
                                                .border_1()
                                                .border_color(if self.active_field == TunnelModalField::Name {
                                                    DarkTechTheme::border_active()
                                                } else {
                                                    DarkTechTheme::border_default()
                                                })
                                                .text_size(px(12.0))
                                                .text_color(DarkTechTheme::text_primary())
                                                .cursor_text()
                                                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                                                    this.active_field = TunnelModalField::Name;
                                                    if let Some(ref fh) = this.focus_handle {
                                                        window.focus(fh);
                                                    }
                                                    cx.notify();
                                                    cx.stop_propagation();
                                                }))
                                                .child(if self.new_name.is_empty() {
                                                    if self.active_field == TunnelModalField::Name {
                                                        "|".to_string()
                                                    } else {
                                                        "默认根据端口自动命名".to_string()
                                                    }
                                                } else if self.active_field == TunnelModalField::Name {
                                                    format!("{}|", self.new_name)
                                                } else {
                                                    self.new_name.clone()
                                                }),
                                        ),
                                )
                                // Input: Local Port
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_size(px(11.0))
                                                .text_color(DarkTechTheme::text_secondary())
                                                .child(crate::t!("tunnel.local_port")),
                                        )
                                        .child(
                                            div()
                                                .id("input_new_tunnel_local_port")
                                                .px_3()
                                                .py_1p5()
                                                .rounded_md()
                                                .bg(DarkTechTheme::bg_input())
                                                .border_1()
                                                .border_color(if self.active_field == TunnelModalField::LocalPort {
                                                    DarkTechTheme::border_active()
                                                } else {
                                                    DarkTechTheme::border_default()
                                                })
                                                .font_family("Menlo")
                                                .text_size(px(12.0))
                                                .text_color(DarkTechTheme::text_primary())
                                                .cursor_text()
                                                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                                                    this.active_field = TunnelModalField::LocalPort;
                                                    if let Some(ref fh) = this.focus_handle {
                                                        window.focus(fh);
                                                    }
                                                    cx.notify();
                                                    cx.stop_propagation();
                                                }))
                                                .child(if self.active_field == TunnelModalField::LocalPort {
                                                    format!("{}|", self.new_local_port)
                                                } else {
                                                    self.new_local_port.clone()
                                                }),
                                        ),
                                )
                                // Inputs: Remote Host & Port (if Local TCP)
                                .when(!is_socks, |d| {
                                    d.child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .flex()
                                                    .flex_col()
                                                    .gap_1()
                                                    .child(
                                                        div()
                                                            .text_size(px(11.0))
                                                            .text_color(DarkTechTheme::text_secondary())
                                                            .child(crate::t!("tunnel.remote_host")),
                                                    )
                                                    .child(
                                                        div()
                                                            .id("input_new_tunnel_remote_host")
                                                            .px_3()
                                                            .py_1p5()
                                                            .rounded_md()
                                                            .bg(DarkTechTheme::bg_input())
                                                            .border_1()
                                                            .border_color(if self.active_field == TunnelModalField::RemoteHost {
                                                                DarkTechTheme::border_active()
                                                            } else {
                                                                DarkTechTheme::border_default()
                                                            })
                                                            .font_family("Menlo")
                                                            .text_size(px(12.0))
                                                            .text_color(DarkTechTheme::text_primary())
                                                            .cursor_text()
                                                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                                                                this.active_field = TunnelModalField::RemoteHost;
                                                                if let Some(ref fh) = this.focus_handle {
                                                                    window.focus(fh);
                                                                }
                                                                cx.notify();
                                                                cx.stop_propagation();
                                                            }))
                                                            .child(if self.active_field == TunnelModalField::RemoteHost {
                                                                format!("{}|", self.new_remote_host)
                                                            } else {
                                                                self.new_remote_host.clone()
                                                            }),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .w(px(120.0))
                                                    .flex()
                                                    .flex_col()
                                                    .gap_1()
                                                    .child(
                                                        div()
                                                            .text_size(px(11.0))
                                                            .text_color(DarkTechTheme::text_secondary())
                                                            .child(crate::t!("tunnel.remote_port")),
                                                    )
                                                    .child(
                                                        div()
                                                            .id("input_new_tunnel_remote_port")
                                                            .px_3()
                                                            .py_1p5()
                                                            .rounded_md()
                                                            .bg(DarkTechTheme::bg_input())
                                                            .border_1()
                                                            .border_color(if self.active_field == TunnelModalField::RemotePort {
                                                                DarkTechTheme::border_active()
                                                            } else {
                                                                DarkTechTheme::border_default()
                                                            })
                                                            .font_family("Menlo")
                                                            .text_size(px(12.0))
                                                            .text_color(DarkTechTheme::text_primary())
                                                            .cursor_text()
                                                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                                                                this.active_field = TunnelModalField::RemotePort;
                                                                if let Some(ref fh) = this.focus_handle {
                                                                    window.focus(fh);
                                                                }
                                                                cx.notify();
                                                                cx.stop_propagation();
                                                            }))
                                                            .child(if self.active_field == TunnelModalField::RemotePort {
                                                                format!("{}|", self.new_remote_port)
                                                            } else {
                                                                self.new_remote_port.clone()
                                                            }),
                                                    ),
                                            ),
                                    )
                                })
                                // Modal Actions: Cancel & Confirm
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .justify_end()
                                        .gap_2()
                                        .mt_2()
                                        .child(
                                            div()
                                                .id("btn_cancel_create_tunnel")
                                                .px_3()
                                                .py_1p5()
                                                .rounded_md()
                                                .bg(DarkTechTheme::bg_input())
                                                .border_1()
                                                .border_color(DarkTechTheme::border_default())
                                                .text_size(px(12.0))
                                                .text_color(DarkTechTheme::text_secondary())
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.show_create_modal = false;
                                                    cx.notify();
                                                }))
                                                .child(crate::t!("common.cancel")),
                                        )
                                        .child(
                                            div()
                                                .id("btn_confirm_create_tunnel")
                                                .px_4()
                                                .py_1p5()
                                                .rounded_md()
                                                .bg(DarkTechTheme::accent_cyan())
                                                .text_size(px(12.0))
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(DarkTechTheme::bg_root())
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.create_tunnel(cx);
                                                }))
                                                .child(crate::t!("common.confirm")),
                                        ),
                                ),
                        ),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use redash_core::config::{AuthMethod, HostConfig, HostId, TargetOs};

    fn make_test_host() -> HostConfig {
        HostConfig {
            id: HostId("host-test-tunnel".into()),
            name: "Tunnel Test Node".into(),
            hostname: "127.0.0.1".into(),
            port: 22,
            user: "root".into(),
            auth: AuthMethod::Password {
                credential_id: "pwd-1".into(),
            },
            group: "Default".into(),
            target_os: TargetOs::Linux,
            tags: vec![],
            jump_host: None,
            proxy_jump_id: None,
            bandwidth_limit_gb: None,
            bandwidth_reset_day: Some(1),
        }
    }

    #[test]
    fn test_tunnel_panel_initial_state() {
        let host = make_test_host();
        let tunnel_mgr = Arc::new(TunnelManager::new());
        let panel = TunnelPanel::new(host, tunnel_mgr);

        assert_eq!(panel.host.name, "Tunnel Test Node");
        assert!(panel.tunnels.is_empty());
        assert!(!panel.show_create_modal);
        assert_eq!(panel.new_local_port, "1080");
        assert!(!panel.new_type_is_socks);
        assert!(panel.status_message.is_none());
    }

    #[test]
    fn test_create_and_delete_tunnel_logic() {
        let host = make_test_host();
        let tunnel_mgr = Arc::new(TunnelManager::new());
        let mut panel = TunnelPanel::new(host, tunnel_mgr);

        // Manually push a local tunnel
        let t1 = TunnelConfig {
            id: "tun-1".into(),
            name: "Postgres Forward".into(),
            tunnel_type: TunnelType::Local {
                local_port: 15432,
                remote_host: "127.0.0.1".into(),
                remote_port: 5432,
            },
            active: false,
        };
        panel.tunnels.push(t1.clone());
        assert_eq!(panel.tunnels.len(), 1);
        assert_eq!(panel.tunnels[0].name, "Postgres Forward");

        // Delete it
        let pos = panel.tunnels.iter().position(|t| t.id == "tun-1").unwrap();
        panel.tunnels.remove(pos);
        assert!(panel.tunnels.is_empty());
    }

    #[test]
    fn test_socks5_tunnel_configuration() {
        let host = make_test_host();
        let tunnel_mgr = Arc::new(TunnelManager::new());
        let mut panel = TunnelPanel::new(host, tunnel_mgr);

        let t_socks = TunnelConfig {
            id: "tun-socks".into(),
            name: "SOCKS Proxy".into(),
            tunnel_type: TunnelType::DynamicSocks5 { local_port: 10808 },
            active: false,
        };
        panel.tunnels.push(t_socks);
        assert_eq!(panel.tunnels.len(), 1);
        match &panel.tunnels[0].tunnel_type {
            TunnelType::DynamicSocks5 { local_port } => assert_eq!(*local_port, 10808),
            _ => panic!("Expected SOCKS5 tunnel"),
        }
    }

    #[test]
    fn test_tunnel_panel_text_input_and_field_cycling() {
        let host = make_test_host();
        let tunnel_mgr = Arc::new(TunnelManager::new());
        let mut panel = TunnelPanel::new(host, tunnel_mgr);

        // Initial field is LocalPort
        assert_eq!(panel.active_field, TunnelModalField::LocalPort);
        assert_eq!(panel.new_local_port, "1080");

        // Backspace removes last char
        panel.backspace();
        assert_eq!(panel.new_local_port, "108");

        // Insert char
        panel.insert_char('9');
        assert_eq!(panel.new_local_port, "1089");

        // Cycle field forward
        panel.cycle_field(true);
        assert_eq!(panel.active_field, TunnelModalField::RemoteHost);

        // Clear and insert string into RemoteHost
        panel.new_remote_host.clear();
        panel.insert_str("192.168.1.1");
        assert_eq!(panel.new_remote_host, "192.168.1.1");

        // Cycle field backward
        panel.cycle_field(false);
        assert_eq!(panel.active_field, TunnelModalField::LocalPort);
    }
}
