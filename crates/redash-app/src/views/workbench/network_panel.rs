use gpui::prelude::FluentBuilder;
use gpui::*;
use std::sync::Arc;
use std::time::Duration;

use crate::components::icon::Icon;
use crate::components::theme::DarkTechTheme;
use redash_core::config::HostConfig;
use redash_core::probe::network::{ListeningPort, NetworkDiagnostics};
use redash_core::session::SessionManager;

#[allow(dead_code)]
pub struct NetworkPanel {
    pub rates_available: bool,
    pub totals_available: bool,
    pub ports_available: bool,
    pub host: HostConfig,
    pub session_mgr: Arc<SessionManager>,
    pub rtt_ms: Option<u32>,
    pub listening_ports: Vec<ListeningPort>,
    pub filter_query: String,
    pub rx_bytes_sec: u64,
    pub tx_bytes_sec: u64,
    pub total_rx_bytes: u64,
    pub total_tx_bytes: u64,
    pub scroll_handle: ScrollHandle,
}

#[allow(dead_code)]
impl NetworkPanel {
    pub fn new(host: HostConfig, session_mgr: Arc<SessionManager>) -> Self {
        Self {
            rates_available: false,
            totals_available: false,
            ports_available: false,
            host,
            session_mgr,
            rtt_ms: None,
            listening_ports: Vec::new(),
            filter_query: String::new(),
            rx_bytes_sec: 0,
            tx_bytes_sec: 0,
            total_rx_bytes: 0,
            total_tx_bytes: 0,
            scroll_handle: ScrollHandle::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_telemetry(
        &mut self,
        rtt: Option<u32>,
        ports: Vec<ListeningPort>,
        rx_sec: u64,
        tx_sec: u64,
        tot_rx: u64,
        tot_tx: u64,
        cx: &mut Context<Self>,
    ) {
        self.rtt_ms = rtt;
        self.listening_ports = ports;
        self.rx_bytes_sec = rx_sec;
        self.tx_bytes_sec = tx_sec;
        self.total_rx_bytes = tot_rx;
        self.total_tx_bytes = tot_tx;
        cx.notify();
    }

    pub fn refresh_ports(&mut self, cx: &mut Context<Self>) {
        let cmd = NetworkDiagnostics::listening_ports_cmd();
        let session_mgr = Arc::clone(&self.session_mgr);
        let host = self.host.clone();

        cx.spawn(async move |this, cx| {
            let res = session_mgr.exec(&host, cmd, Duration::from_secs(10)).await;
            if let Ok(r) = res
                && r.exit_code == 0
            {
                let ports = NetworkDiagnostics::parse_listening_ports(&r.stdout);
                let _ = this.update(cx, |view, cx| {
                    view.listening_ports = ports;
                    view.ports_available = true;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub fn format_speed(bytes_per_sec: u64) -> String {
        if bytes_per_sec >= 1024 * 1024 {
            format!("{:.1} MB/s", bytes_per_sec as f64 / 1048576.0)
        } else if bytes_per_sec >= 1024 {
            format!("{:.1} KB/s", bytes_per_sec as f64 / 1024.0)
        } else {
            format!("{} B/s", bytes_per_sec)
        }
    }

    pub fn format_bytes(bytes: u64) -> String {
        redash_types::formatters::format_bytes(bytes)
    }
}

impl Render for NetworkPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let q = self.filter_query.to_lowercase().trim().to_string();
        let filtered_ports: Vec<ListeningPort> = self
            .listening_ports
            .iter()
            .filter(|p| {
                if q.is_empty() {
                    true
                } else {
                    p.proto.to_lowercase().contains(&q)
                        || p.bind_ip.to_lowercase().contains(&q)
                        || p.port.to_string().contains(&q)
                        || p.process_name
                            .as_deref()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&q)
                        || p.pid
                            .map(|pid| pid.to_string())
                            .unwrap_or_default()
                            .contains(&q)
                }
            })
            .cloned()
            .collect();

        // RTT Quality Card
        let (rtt_label, rtt_color, rtt_text) = match self.rtt_ms {
            Some(ms) if ms <= 50 => (
                "极佳 (<=50ms)",
                DarkTechTheme::status_online(),
                format!("{} ms", ms),
            ),
            Some(ms) if ms <= 150 => (
                "良好 (50-150ms)",
                DarkTechTheme::status_warn(),
                format!("{} ms", ms),
            ),
            Some(ms) => (
                "高延迟 (>150ms)",
                DarkTechTheme::status_crit(),
                format!("{} ms", ms),
            ),
            None => (
                "未测速 / 离线",
                DarkTechTheme::status_offline(),
                "-- ms".to_string(),
            ),
        };

        let row_elements: Vec<_> = if filtered_ports.is_empty() {
            vec![
                div()
                    .id("net_empty_placeholder")
                    .p_8()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .child(
                        Icon::network()
                            .with_size(px(32.0))
                            .with_color(DarkTechTheme::text_muted()),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(DarkTechTheme::text_muted())
                            .child(if self.ports_available {
                                crate::t!("network.no_ports").to_string()
                            } else {
                                "监听端口未采集".into()
                            }),
                    ),
            ]
        } else {
            filtered_ports
                .into_iter()
                .enumerate()
                .map(|(idx, p)| {
                    let proto_upper = p.proto.to_uppercase();
                    let is_tcp = proto_upper.starts_with("TCP");
                    let proc_name = p.process_name.unwrap_or_else(|| "-".to_string());
                    let pid_str = p
                        .pid
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "-".to_string());

                    div()
                        .id(ElementId::NamedInteger("net_port_row".into(), idx as u64))
                        .h(px(38.0))
                        .w_full()
                        .px_3()
                        .border_b_1()
                        .border_color(DarkTechTheme::border_muted())
                        .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()))
                        .flex()
                        .flex_row()
                        .items_center()
                        .text_size(px(12.0))
                        // Proto Badge
                        .child(
                            div().w(px(80.0)).flex_shrink_0().child(
                                div()
                                    .w(px(50.0))
                                    .py_0p5()
                                    .rounded_sm()
                                    .bg(if is_tcp {
                                        rgba(0x38bdf822)
                                    } else {
                                        rgba(0xf59e0b22)
                                    })
                                    .border_1()
                                    .border_color(if is_tcp {
                                        DarkTechTheme::accent_cyan()
                                    } else {
                                        DarkTechTheme::status_warn()
                                    })
                                    .text_align(TextAlign::Center)
                                    .text_size(px(10.5))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(if is_tcp {
                                        DarkTechTheme::accent_cyan()
                                    } else {
                                        DarkTechTheme::status_warn()
                                    })
                                    .child(proto_upper),
                            ),
                        )
                        // Bind IP
                        .child(
                            div()
                                .w(px(180.0))
                                .flex_shrink_0()
                                .font_family("Menlo")
                                .truncate()
                                .text_color(DarkTechTheme::text_secondary())
                                .child(p.bind_ip),
                        )
                        // Port
                        .child(
                            div()
                                .w(px(100.0))
                                .flex_shrink_0()
                                .font_family("Menlo")
                                .font_weight(FontWeight::BOLD)
                                .text_color(DarkTechTheme::text_accent())
                                .child(p.port.to_string()),
                        )
                        // Process Name
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(120.0))
                                .font_family("Menlo")
                                .truncate()
                                .text_color(DarkTechTheme::text_primary())
                                .child(proc_name),
                        )
                        // PID
                        .child(
                            div()
                                .w(px(100.0))
                                .flex_shrink_0()
                                .text_align(TextAlign::Right)
                                .font_family("Menlo")
                                .text_color(DarkTechTheme::text_muted())
                                .child(pid_str),
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
            // Top Metric Cards Row
            .child(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .flex()
                    .flex_row()
                    .gap_3()
                    // Card 1: RTT
                    .child(
                        div()
                            .flex_1()
                            .h(px(90.0))
                            .bg(DarkTechTheme::bg_panel())
                            .border_1()
                            .border_color(DarkTechTheme::border_default())
                            .rounded_lg()
                            .p_3()
                            .flex()
                            .flex_col()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(11.5))
                                            .text_color(DarkTechTheme::text_secondary())
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_1()
                                                    .child(
                                                        Icon::zap().with_size(px(11.0)).with_color(
                                                            DarkTechTheme::accent_emerald(),
                                                        ),
                                                    )
                                                    .child(crate::t!("network.rtt_label")),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .px_2()
                                            .py_0p5()
                                            .rounded_full()
                                            .bg(DarkTechTheme::bg_input())
                                            .text_size(px(10.5))
                                            .text_color(rtt_color)
                                            .child(rtt_label),
                                    ),
                            )
                            .child(
                                div()
                                    .font_family("Menlo")
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(22.0))
                                    .text_color(rtt_color)
                                    .child(rtt_text),
                            ),
                    )
                    // Card 2: Realtime Speed (RX / TX)
                    .child(
                        div()
                            .flex_1()
                            .h(px(90.0))
                            .bg(DarkTechTheme::bg_panel())
                            .border_1()
                            .border_color(DarkTechTheme::border_default())
                            .rounded_lg()
                            .p_3()
                            .flex()
                            .flex_col()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .child(
                                        Icon::network()
                                            .with_size(px(12.0))
                                            .with_color(DarkTechTheme::text_secondary()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.5))
                                            .text_color(DarkTechTheme::text_secondary())
                                            .child(crate::t!("network.throughput")),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .text_size(px(10.0))
                                                    .text_color(DarkTechTheme::text_muted())
                                                    .child(crate::t!("network.downlink")),
                                            )
                                            .child(
                                                div()
                                                    .font_family("Menlo")
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_size(px(15.0))
                                                    .text_color(DarkTechTheme::accent_emerald())
                                                    .child(if self.rates_available {
                                                        Self::format_speed(self.rx_bytes_sec)
                                                    } else {
                                                        "未采集".into()
                                                    }),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .text_size(px(10.0))
                                                    .text_color(DarkTechTheme::text_muted())
                                                    .child(crate::t!("network.uplink")),
                                            )
                                            .child(
                                                div()
                                                    .font_family("Menlo")
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_size(px(15.0))
                                                    .text_color(DarkTechTheme::accent_cyan())
                                                    .child(if self.rates_available {
                                                        Self::format_speed(self.tx_bytes_sec)
                                                    } else {
                                                        "未采集".into()
                                                    }),
                                            ),
                                    ),
                            ),
                    )
                    // Card 3: Total Accumulated Traffic
                    .child(
                        div()
                            .flex_1()
                            .h(px(90.0))
                            .bg(DarkTechTheme::bg_panel())
                            .border_1()
                            .border_color(DarkTechTheme::border_default())
                            .rounded_lg()
                            .p_3()
                            .flex()
                            .flex_col()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(DarkTechTheme::text_secondary())
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                Icon::network()
                                                    .with_size(px(11.0))
                                                    .with_color(DarkTechTheme::accent_cyan()),
                                            )
                                            .child(crate::t!("network.total_bandwidth")),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .text_size(px(10.0))
                                                    .text_color(DarkTechTheme::text_muted())
                                                    .child("累计下载"),
                                            )
                                            .child(
                                                div()
                                                    .font_family("Menlo")
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_size(px(14.0))
                                                    .text_color(DarkTechTheme::text_primary())
                                                    .child(if self.totals_available {
                                                        Self::format_bytes(self.total_rx_bytes)
                                                    } else {
                                                        "未采集".into()
                                                    }),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .text_size(px(10.0))
                                                    .text_color(DarkTechTheme::text_muted())
                                                    .child("累计上传"),
                                            )
                                            .child(
                                                div()
                                                    .font_family("Menlo")
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_size(px(14.0))
                                                    .text_color(DarkTechTheme::text_primary())
                                                    .child(if self.totals_available {
                                                        Self::format_bytes(self.total_tx_bytes)
                                                    } else {
                                                        "未采集".into()
                                                    }),
                                            ),
                                    ),
                            ),
                    ),
            )
            // Mid Toolbar: Search Input and Refresh Button
            .child(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    // Search box
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id("net_search_container")
                                    .px_3()
                                    .py_1p5()
                                    .bg(DarkTechTheme::bg_input())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_default())
                                    .rounded_md()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        Icon::search()
                                            .with_size(px(12.0))
                                            .with_color(DarkTechTheme::text_muted()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(if self.filter_query.is_empty() {
                                                DarkTechTheme::text_muted()
                                            } else {
                                                DarkTechTheme::text_primary()
                                            })
                                            .child(if self.filter_query.is_empty() {
                                                crate::t!("network.filter_ports").to_string()
                                            } else {
                                                self.filter_query.clone()
                                            }),
                                    ),
                            )
                            // Preset filter pills
                            .child(
                                div()
                                    .id("net_filter_all")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(if self.filter_query.is_empty() {
                                        DarkTechTheme::bg_panel_hover()
                                    } else {
                                        DarkTechTheme::bg_panel()
                                    })
                                    .border_1()
                                    .border_color(if self.filter_query.is_empty() {
                                        DarkTechTheme::border_active()
                                    } else {
                                        DarkTechTheme::border_default()
                                    })
                                    .text_color(if self.filter_query.is_empty() {
                                        DarkTechTheme::text_accent()
                                    } else {
                                        DarkTechTheme::text_secondary()
                                    })
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.filter_query.clear();
                                        cx.notify();
                                    }))
                                    .child(crate::t!("common.all")),
                            )
                            .child(
                                div()
                                    .id("net_filter_tcp")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(if self.filter_query == "tcp" {
                                        DarkTechTheme::bg_panel_hover()
                                    } else {
                                        DarkTechTheme::bg_panel()
                                    })
                                    .border_1()
                                    .border_color(if self.filter_query == "tcp" {
                                        DarkTechTheme::border_active()
                                    } else {
                                        DarkTechTheme::border_default()
                                    })
                                    .text_color(if self.filter_query == "tcp" {
                                        DarkTechTheme::text_accent()
                                    } else {
                                        DarkTechTheme::text_secondary()
                                    })
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.filter_query = "tcp".to_string();
                                        cx.notify();
                                    }))
                                    .child("TCP"),
                            )
                            .child(
                                div()
                                    .id("net_filter_udp")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(if self.filter_query == "udp" {
                                        DarkTechTheme::bg_panel_hover()
                                    } else {
                                        DarkTechTheme::bg_panel()
                                    })
                                    .border_1()
                                    .border_color(if self.filter_query == "udp" {
                                        DarkTechTheme::border_active()
                                    } else {
                                        DarkTechTheme::border_default()
                                    })
                                    .text_color(if self.filter_query == "udp" {
                                        DarkTechTheme::text_accent()
                                    } else {
                                        DarkTechTheme::text_secondary()
                                    })
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.filter_query = "udp".to_string();
                                        cx.notify();
                                    }))
                                    .child("UDP"),
                            )
                            .when(!self.filter_query.is_empty(), |d| {
                                d.child(
                                    div()
                                        .id("net_filter_clear")
                                        .px_2()
                                        .py_1()
                                        .rounded_md()
                                        .bg(DarkTechTheme::bg_input())
                                        .border_1()
                                        .border_color(DarkTechTheme::border_default())
                                        .text_color(DarkTechTheme::text_muted())
                                        .text_size(px(11.0))
                                        .cursor_pointer()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_1()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.filter_query.clear();
                                            cx.notify();
                                        }))
                                        .child(
                                            Icon::close()
                                                .with_size(px(10.0))
                                                .with_color(DarkTechTheme::text_muted()),
                                        )
                                        .child(crate::t!("common.clear")),
                                )
                            }),
                    )
                    // Refresh Ports Button
                    .child(
                        div()
                            .id("btn_refresh_ports")
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .bg(DarkTechTheme::bg_panel())
                            .border_1()
                            .border_color(DarkTechTheme::border_default())
                            .text_color(DarkTechTheme::text_primary())
                            .text_size(px(11.5))
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.refresh_ports(cx);
                            }))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        Icon::refresh()
                                            .with_size(px(11.0))
                                            .with_color(DarkTechTheme::text_primary()),
                                    )
                                    .child(crate::t!("common.refresh")),
                            ),
                    ),
            )
            // Listening Ports Table
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
                    // Table Header
                    .child(
                        div()
                            .h(px(36.0))
                            .flex_shrink_0()
                            .w_full()
                            .bg(DarkTechTheme::bg_input())
                            .border_b_1()
                            .border_color(DarkTechTheme::border_default())
                            .px_3()
                            .flex()
                            .flex_row()
                            .items_center()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::BOLD)
                            .text_color(DarkTechTheme::text_secondary())
                            .child(
                                div()
                                    .w(px(80.0))
                                    .flex_shrink_0()
                                    .child(crate::t!("network.col_proto")),
                            )
                            .child(
                                div()
                                    .w(px(180.0))
                                    .flex_shrink_0()
                                    .child(crate::t!("network.col_bind")),
                            )
                            .child(
                                div()
                                    .w(px(100.0))
                                    .flex_shrink_0()
                                    .child(crate::t!("network.col_port")),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(120.0))
                                    .child(crate::t!("network.col_process")),
                            )
                            .child(
                                div()
                                    .w(px(100.0))
                                    .flex_shrink_0()
                                    .text_align(TextAlign::Right)
                                    .child("PID"),
                            ),
                    )
                    // Table Rows
                    .child(
                        div()
                            .id("net_ports_table_rows")
                            .track_scroll(&self.scroll_handle)
                            .flex_1()
                            .w_full()
                            .min_h(px(0.0))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .children(row_elements),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use redash_core::config::{AuthMethod, HostConfig, HostId, TargetOs};

    fn make_test_host() -> HostConfig {
        HostConfig {
            id: HostId("host-test-net".into()),
            name: "Network Test Node".into(),
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
    fn test_network_panel_initial_state() {
        let host = make_test_host();
        let session_mgr = Arc::new(SessionManager::new());
        let panel = NetworkPanel::new(host, session_mgr);

        assert_eq!(panel.host.name, "Network Test Node");
        assert!(panel.listening_ports.is_empty());
        assert!(panel.filter_query.is_empty());
        assert_eq!(panel.rtt_ms, None);
        assert_eq!(panel.rx_bytes_sec, 0);
        assert_eq!(panel.tx_bytes_sec, 0);
        assert_eq!(panel.total_rx_bytes, 0);
        assert_eq!(panel.total_tx_bytes, 0);
    }

    #[test]
    fn test_network_panel_format_speed_and_bytes() {
        assert_eq!(NetworkPanel::format_speed(500), "500 B/s");
        assert_eq!(NetworkPanel::format_speed(2048), "2.0 KB/s");
        assert_eq!(NetworkPanel::format_speed(1048576 * 5), "5.0 MB/s");

        assert_eq!(NetworkPanel::format_bytes(100), "100 B");
        assert_eq!(NetworkPanel::format_bytes(1024), "1.0 KB");
        assert_eq!(NetworkPanel::format_bytes(1048576 * 10), "10.0 MB");
        assert_eq!(NetworkPanel::format_bytes(1073741824 * 3), "3.00 GB");
    }

    #[test]
    fn test_network_panel_listening_ports_filter() {
        let p1 = ListeningPort {
            proto: "tcp".into(),
            bind_ip: "0.0.0.0".into(),
            port: 22,
            pid: Some(1234),
            process_name: Some("sshd".into()),
        };
        let p2 = ListeningPort {
            proto: "udp".into(),
            bind_ip: "127.0.0.1".into(),
            port: 53,
            pid: Some(567),
            process_name: Some("dnsmasq".into()),
        };

        let list = [p1, p2];
        let q = "22";
        let filtered: Vec<_> = list
            .iter()
            .filter(|p| p.port.to_string().contains(q))
            .collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].process_name.as_deref(), Some("sshd"));
    }
}
