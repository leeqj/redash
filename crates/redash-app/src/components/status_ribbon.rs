use crate::components::chart::SparklineChart;
use crate::components::icon::Icon;
use crate::components::status_led::{HostLedState, StatusLed};
use crate::components::theme::DarkTechTheme;
use gpui::*;
use redash_core::probe::NodeMetrics;

#[allow(dead_code)]
#[derive(Clone)]
pub struct StatusRibbon {
    pub host_name: String,
    pub metrics: Option<NodeMetrics>,
    pub cpu_history: Vec<f32>,
    pub is_inspector_open: bool,
    pub is_sftp_open: bool,
}

impl StatusRibbon {
    #[allow(dead_code)]
    pub fn new(host_name: impl Into<String>) -> Self {
        Self {
            host_name: host_name.into(),
            metrics: None,
            cpu_history: Vec::new(),
            is_inspector_open: false,
            is_sftp_open: false,
        }
    }
}

#[allow(dead_code)]
pub enum RibbonEvent {
    ToggleInspector,
    ToggleSftp,
}

impl RenderOnce for StatusRibbon {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let (cpu_pct, mem_str, net_str, is_online, cpu_val, mem_val) =
            if let Some(m) = &self.metrics {
                let cpu = format!("{:.1}%", m.cpu.usage_percent);
                let mem_used_gb = m.mem.used_bytes as f32 / (1024.0 * 1024.0 * 1024.0);
                let mem_total_gb = m.mem.total_bytes as f32 / (1024.0 * 1024.0 * 1024.0);
                let mem = format!(
                    "{:.1}G / {:.1}G ({:.0}%)",
                    mem_used_gb, mem_total_gb, m.mem.usage_percent
                );
                let rx_kb = m.net.rx_bytes_per_sec as f32 / 1024.0;
                let tx_kb = m.net.tx_bytes_per_sec as f32 / 1024.0;
                let net = format!("↓ {:.0}KB/s  ↑ {:.0}KB/s", rx_kb, tx_kb);
                (
                    cpu,
                    mem,
                    net,
                    true,
                    m.cpu.usage_percent,
                    m.mem.usage_percent,
                )
            } else {
                (
                    "0.0%".to_string(),
                    "0.0G / 0.0G (0%)".to_string(),
                    "↓ 0KB/s  ↑ 0KB/s".to_string(),
                    false,
                    0.0,
                    0.0,
                )
            };

        let led_state = HostLedState::from_metrics(is_online, cpu_val, mem_val);

        div()
            .h(px(28.0))
            .w_full()
            .bg(DarkTechTheme::bg_panel())
            .border_b_1()
            .border_color(DarkTechTheme::border_default())
            .px_3()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .text_size(px(11.0))
            .text_color(DarkTechTheme::text_secondary())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    // Status LED
                    .child(StatusLed::new(led_state).with_size(px(10.0)))
                    // Host name
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_color(DarkTechTheme::text_primary())
                            .child(self.host_name),
                    )
                    // CPU stats + mini sparkline
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child("CPU:")
                            .child(
                                div()
                                    .text_color(DarkTechTheme::text_primary())
                                    .font_family("Menlo")
                                    .child(cpu_pct),
                            )
                            .child(div().w(px(50.0)).h(px(16.0)).child(
                                SparklineChart::tech(self.cpu_history).with_pulse_dot(true),
                            )),
                    )
                    // Memory stats
                    .child(
                        div().flex().flex_row().gap_1().child("RAM:").child(
                            div()
                                .text_color(DarkTechTheme::text_primary())
                                .font_family("Menlo")
                                .child(mem_str),
                        ),
                    )
                    // Net stats
                    .child(
                        div().flex().flex_row().gap_1().child("NET:").child(
                            div()
                                .text_color(DarkTechTheme::text_primary())
                                .font_family("Menlo")
                                .child(net_str),
                        ),
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
                            .px_2()
                            .py_0p5()
                            .rounded_md()
                            .bg(if self.is_inspector_open {
                                DarkTechTheme::bg_panel_hover()
                            } else {
                                DarkTechTheme::bg_input()
                            })
                            .border_1()
                            .border_color(if self.is_inspector_open {
                                DarkTechTheme::border_active()
                            } else {
                                DarkTechTheme::border_default()
                            })
                            .text_color(if self.is_inspector_open {
                                DarkTechTheme::text_accent()
                            } else {
                                DarkTechTheme::text_secondary()
                            })
                            .cursor_pointer()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(Icon::activity().with_size(px(10.0)).with_color(
                                if self.is_inspector_open {
                                    DarkTechTheme::text_accent()
                                } else {
                                    DarkTechTheme::text_secondary()
                                },
                            ))
                            .child(crate::t!("nav.diagnostics_drawer")),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_md()
                            .bg(if self.is_sftp_open {
                                DarkTechTheme::bg_panel_hover()
                            } else {
                                DarkTechTheme::bg_input()
                            })
                            .border_1()
                            .border_color(if self.is_sftp_open {
                                DarkTechTheme::border_active()
                            } else {
                                DarkTechTheme::border_default()
                            })
                            .text_color(if self.is_sftp_open {
                                DarkTechTheme::text_accent()
                            } else {
                                DarkTechTheme::text_secondary()
                            })
                            .cursor_pointer()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(Icon::folder().with_size(px(10.0)).with_color(
                                if self.is_sftp_open {
                                    DarkTechTheme::text_accent()
                                } else {
                                    DarkTechTheme::text_secondary()
                                },
                            ))
                            .child(crate::t!("nav.remote_files")),
                    ),
            )
    }
}
