use gpui::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::components::chart::SparklineChart;
use crate::components::icon::Icon;
use crate::components::status_led::{HostLedState, StatusLed};
use crate::components::theme::DarkTechTheme;
use crate::terminal::split::{SplitDirection, SplitLayoutManager, SplitNode};
use crate::terminal::view::TerminalView;
use crate::views::sftp_view::SftpView;
use crate::views::workbench::{
    DockerPanel, NetworkPanel, ProcessPanel, SnippetsPanel, TunnelPanel,
};
use redash_core::config::{AppSettings, HostConfig};
use redash_core::probe::NodeMetrics;
use redash_core::session::SessionManager;
use redash_core::session::tunnel::TunnelManager;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkbenchMode {
    Terminal,
    Docker,
    Processes,
    Network,
    Tunnels,
    Snippets,
    Sftp,
}

#[derive(Debug, Clone)]
pub struct ActiveSplitDrag {
    pub branch_path: Vec<usize>,
    pub is_vertical: bool,
    pub start_pos: f32,
    pub start_ratio: f32,
}

pub struct WorkbenchView {
    pub probe_error: Option<String>,
    settings: AppSettings,
    pub host: HostConfig,
    pub session_mgr: Arc<SessionManager>,
    pub terminal_view: Entity<TerminalView>,
    pub split_manager: SplitLayoutManager,
    pub terminal_views: HashMap<String, Entity<TerminalView>>,
    pub docker_panel: Entity<DockerPanel>,
    pub process_panel: Entity<ProcessPanel>,
    pub network_panel: Entity<NetworkPanel>,
    pub tunnel_panel: Entity<TunnelPanel>,
    pub snippets_panel: Entity<SnippetsPanel>,
    pub sftp_view: Option<Entity<SftpView>>,
    pub metrics: Option<NodeMetrics>,
    pub cpu_history: Vec<f32>,
    pub active_mode: WorkbenchMode,
    pub active_split_drag: Option<ActiveSplitDrag>,
}

impl WorkbenchView {
    pub fn new(host: HostConfig, session_mgr: Arc<SessionManager>, cx: &mut Context<Self>) -> Self {
        let (output_tx, output_rx) = mpsc::channel(64);
        let host_clone = host.clone();
        let session_mgr_clone = Arc::clone(&session_mgr);

        // 1. Terminal Entity
        let host_name = host.name.clone();
        let terminal_view =
            cx.new(|cx| TerminalView::new(120, 35, None, output_rx, cx).with_host_name(host_name));

        let term_entity = terminal_view.clone();
        cx.spawn(async move |_this, cx| {
            let result = session_mgr_clone
                .open_pty(&host_clone, 120, 35, output_tx)
                .await;
            let _ = term_entity.update(cx, |view, cx| match result {
                Ok(pty) => view.attach_pty(Arc::new(pty), cx),
                Err(error) => {
                    view.connection_error = Some(crate::t_fmt!(
                        "workbench.term_conn_failed",
                        error = format!("{error:#}")
                    ));
                    cx.notify();
                }
            });
        })
        .detach();

        // 2. Tunnel Manager & Tunnel Panel
        let tunnel_mgr = Arc::new(TunnelManager::new());
        let tunnel_panel = cx.new(|_cx| {
            TunnelPanel::new(host.clone(), Arc::clone(&tunnel_mgr))
                .with_session_mgr(Arc::clone(&session_mgr))
        });

        // 3. Docker Panel
        let docker_panel = cx.new(|_cx| DockerPanel::new(host.clone(), Arc::clone(&session_mgr)));

        // 4. Process Panel
        let process_panel = cx.new(|_cx| ProcessPanel::new(host.clone(), Arc::clone(&session_mgr)));

        // 5. Network Panel
        let network_panel = cx.new(|_cx| NetworkPanel::new(host.clone(), Arc::clone(&session_mgr)));

        // 6. Snippets Panel
        let snippets_panel =
            cx.new(|_cx| SnippetsPanel::new().with_session(host.clone(), Arc::clone(&session_mgr)));

        // Wire snippet terminal injection
        let this_weak = cx.entity().downgrade();
        snippets_panel.update(cx, |p, _cx| {
            p.set_on_inject_terminal(move |cmd, window, cx| {
                if let Some(this) = this_weak.upgrade() {
                    this.update(cx, |wb, cx| {
                        wb.terminal_view.update(cx, |term, _cx| {
                            term.send_input(format!("{}\n", cmd).as_bytes());
                        });
                        wb.active_mode = WorkbenchMode::Terminal;
                        wb.focus_terminal(window, cx);
                        cx.notify();
                    });
                }
            });
        });

        let initial_pane_id = "pane_1".to_string();
        let mut terminal_views = HashMap::new();
        terminal_views.insert(initial_pane_id.clone(), terminal_view.clone());
        let split_manager = SplitLayoutManager::new(initial_pane_id);

        Self {
            probe_error: None,
            settings: AppSettings::default(),
            host,
            session_mgr,
            terminal_view,
            split_manager,
            terminal_views,
            docker_panel,
            process_panel,
            network_panel,
            tunnel_panel,
            snippets_panel,
            sftp_view: None,
            metrics: None,
            cpu_history: Vec::new(),
            active_mode: WorkbenchMode::Terminal,
            active_split_drag: None,
        }
    }

    pub fn focus_terminal(&self, window: &mut Window, cx: &App) {
        if let Some(active) = self.terminal_views.get(&self.split_manager.active_pane_id) {
            active.read(cx).focus(window);
        } else {
            self.terminal_view.read(cx).focus(window);
        }
    }

    pub fn split_terminal(&mut self, direction: SplitDirection, cx: &mut Context<Self>) {
        let (init_cols, init_rows) = self
            .terminal_views
            .get(&self.split_manager.active_pane_id)
            .map(|t| {
                let r = t.read(cx);
                (r.cols(), r.rows())
            })
            .unwrap_or((120, 35));

        let new_pane_id = self.split_manager.split_active(direction);
        let (output_tx, output_rx) = mpsc::channel(64);
        let host_clone = self.host.clone();
        let session_mgr_clone = Arc::clone(&self.session_mgr);

        let host_name = self.host.name.clone();
        let settings = self.settings.clone();
        let new_term = cx.new(|cx| {
            let mut tv = TerminalView::new(init_cols, init_rows, None, output_rx, cx)
                .with_host_name(host_name);
            tv.apply_settings(&settings, cx);
            tv
        });

        let term_entity = new_term.clone();
        cx.spawn(async move |_this, cx| {
            let result = session_mgr_clone
                .open_pty(&host_clone, init_cols as u32, init_rows as u32, output_tx)
                .await;
            let _ = term_entity.update(cx, |view, cx| match result {
                Ok(pty) => view.attach_pty(Arc::new(pty), cx),
                Err(error) => {
                    view.connection_error = Some(crate::t_fmt!(
                        "workbench.term_conn_failed",
                        error = format!("{error:#}")
                    ));
                    cx.notify();
                }
            });
        })
        .detach();

        self.terminal_views.insert(new_pane_id, new_term.clone());
        self.terminal_view = new_term;
        cx.notify();
    }

    pub fn reconnect_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (init_cols, init_rows) = self
            .terminal_views
            .get(&self.split_manager.active_pane_id)
            .map(|t| {
                let r = t.read(cx);
                (r.cols(), r.rows())
            })
            .unwrap_or((120, 35));

        let (output_tx, output_rx) = mpsc::channel(64);
        let settings = self.settings.clone();
        let host = self.host.clone();
        let name = host.name.clone();
        let terminal = cx.new(|cx| {
            let mut terminal =
                TerminalView::new(init_cols, init_rows, None, output_rx, cx).with_host_name(name);
            terminal.apply_settings(&settings, cx);
            terminal
        });
        self.terminal_views
            .insert(self.split_manager.active_pane_id.clone(), terminal.clone());
        self.terminal_view = terminal.clone();
        let manager = Arc::clone(&self.session_mgr);
        cx.spawn(async move |_, cx| {
            let result = manager
                .open_pty(&host, init_cols as u32, init_rows as u32, output_tx)
                .await;
            let _ = terminal.update(cx, |view, cx| match result {
                Ok(pty) => view.attach_pty(Arc::new(pty), cx),
                Err(error) => {
                    view.connection_error = Some(crate::t_fmt!(
                        "workbench.term_conn_failed",
                        error = format!("{error:#}")
                    ));
                    cx.notify();
                }
            });
        })
        .detach();
        self.focus_terminal(window, cx);
        cx.notify();
    }

    pub fn update_settings(&mut self, settings: &AppSettings, cx: &mut Context<Self>) {
        self.settings = settings.clone();

        for tv in self.terminal_views.values() {
            tv.update(cx, |tv, cx| {
                tv.apply_settings(settings, cx);
            });
        }
        if let Some(ref sftp) = self.sftp_view {
            sftp.update(cx, |sftp, cx| {
                sftp.show_hidden = settings.sftp_show_hidden_files;
                sftp.confirm_delete = settings.sftp_confirm_delete;
                cx.notify();
            });
        }
        cx.notify();
    }

    pub fn close_pane_by_id(&mut self, pane_id: &str, cx: &mut Context<Self>) {
        if self.split_manager.close_pane(pane_id) {
            self.terminal_views.remove(pane_id);
            if let Some(next_active) = self.terminal_views.get(&self.split_manager.active_pane_id) {
                self.terminal_view = next_active.clone();
            }
            cx.notify();
        }
    }

    pub fn close_active_pane(&mut self, cx: &mut Context<Self>) {
        let active_id = self.split_manager.active_pane_id.clone();
        self.close_pane_by_id(&active_id, cx);
    }

    pub fn swap_active_pane(&mut self, cx: &mut Context<Self>) {
        let active_id = self.split_manager.active_pane_id.clone();
        let panes = self.split_manager.panes();
        if panes.len() >= 2
            && let Some(pos) = panes.iter().position(|p| p == &active_id)
        {
            let other_idx = (pos + 1) % panes.len();
            self.split_manager.swap_panes(&active_id, &panes[other_idx]);
            cx.notify();
        }
    }

    pub fn move_pane_forward(&mut self, pane_id: &str, cx: &mut Context<Self>) {
        if self.split_manager.move_pane_forward(pane_id) {
            cx.notify();
        }
    }

    pub fn move_pane_backward(&mut self, pane_id: &str, cx: &mut Context<Self>) {
        if self.split_manager.move_pane_backward(pane_id) {
            cx.notify();
        }
    }

    pub fn set_split_ratio(&mut self, path: &[usize], ratio: f32, cx: &mut Context<Self>) {
        if self.split_manager.set_ratio_at_path(path, ratio) {
            cx.notify();
        }
    }

    pub fn set_mode(&mut self, mode: WorkbenchMode, window: &mut Window, cx: &mut Context<Self>) {
        self.active_mode = mode;
        match mode {
            WorkbenchMode::Terminal => {
                self.focus_terminal(window, cx);
            }
            WorkbenchMode::Sftp => {
                self.ensure_sftp_initialized(cx);
            }
            WorkbenchMode::Docker => {
                self.docker_panel.update(cx, |p, cx| {
                    p.refresh(cx);
                });
            }
            WorkbenchMode::Processes => {
                self.process_panel.update(cx, |p, cx| {
                    p.refresh(cx);
                });
            }
            WorkbenchMode::Network => {
                self.network_panel.update(cx, |p, cx| {
                    p.refresh_ports(cx);
                });
            }
            _ => {}
        }
        cx.notify();
    }

    pub fn set_metrics(&mut self, metric: NodeMetrics, cx: &mut Context<Self>) {
        self.probe_error = None;
        let cpu = metric.cpu.usage_percent;
        self.metrics = Some(metric.clone());
        while self.cpu_history.len() >= self.settings.history_points.clamp(1, 3600) {
            self.cpu_history.remove(0);
        }
        self.cpu_history.push(cpu);

        // Forward telemetry to panels
        if metric.containers_available {
            self.docker_panel.update(cx, |p, cx| {
                p.set_containers(metric.containers_detail.clone(), cx);
            });
        }

        if metric.processes_available {
            let procs = metric.processes_detail.clone();
            self.process_panel.update(cx, |p, cx| {
                p.set_processes(procs.clone(), cx);
            });
        }

        self.network_panel.update(cx, |p, cx| {
            p.rates_available = metric.net_rates_available;
            p.totals_available = metric.net_available;
            p.ports_available |= metric.listening_ports_available;
            p.update_telemetry(
                metric.rtt_ms,
                if metric.listening_ports_available {
                    metric.listening_ports.clone()
                } else {
                    p.listening_ports.clone()
                },
                metric.net.rx_bytes_per_sec,
                metric.net.tx_bytes_per_sec,
                metric.net.total_rx_bytes,
                metric.net.total_tx_bytes,
                cx,
            );
        });

        cx.notify();
    }

    pub fn ensure_sftp_initialized(&mut self, cx: &mut Context<Self>) {
        if self.sftp_view.is_none() {
            let host = self.host.clone();
            let default_path = SftpView::default_path_for_host(&host);
            let session_mgr = Arc::clone(&self.session_mgr);

            let sftp_view = cx.new(|cx| SftpView::new(host, session_mgr, cx));
            sftp_view.update(cx, |view, cx| {
                view.show_hidden = self.settings.sftp_show_hidden_files;
                view.confirm_delete = self.settings.sftp_confirm_delete;
                view.init_connection_and_load(default_path, cx);
            });

            self.sftp_view = Some(sftp_view);
        }
    }

    fn render_mode_tab(
        &self,
        mode: WorkbenchMode,
        label: &'static str,
        icon: Icon,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_active = self.active_mode == mode;
        let icon_color = if is_active {
            DarkTechTheme::text_accent()
        } else {
            DarkTechTheme::text_secondary()
        };

        let mut tooltip_text = label.to_string();
        if mode == WorkbenchMode::Terminal {
            let agent = self
                .terminal_views
                .get(&self.split_manager.active_pane_id)
                .and_then(|t| t.read(cx).detected_agent.clone())
                .or_else(|| {
                    self.terminal_views
                        .values()
                        .find_map(|t| t.read(cx).detected_agent.clone())
                });
            if let Some(a) = agent {
                let badge_text = match a.status {
                    redash_core::probe::agent::AgentStatus::NeedsInput => {
                        crate::t!("agent.status_needs_input")
                    }
                    redash_core::probe::agent::AgentStatus::Thinking => {
                        crate::t!("agent.status_thinking")
                    }
                    redash_core::probe::agent::AgentStatus::Done => crate::t!("agent.status_done"),
                    redash_core::probe::agent::AgentStatus::Idle => crate::t!("agent.status_idle"),
                };
                tooltip_text = format!("{} ({})", label, badge_text);
            }
        }

        div()
            .id(ElementId::Name(format!("wb_mode_{}", label).into()))
            .relative()
            .h(px(26.0))
            .w(px(28.0))
            .rounded_md()
            .bg(if is_active {
                DarkTechTheme::bg_panel_hover()
            } else {
                DarkTechTheme::bg_input()
            })
            .border_1()
            .border_color(if is_active {
                DarkTechTheme::border_active()
            } else {
                DarkTechTheme::border_default()
            })
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|s| {
                if is_active {
                    s
                } else {
                    s.border_color(DarkTechTheme::border_muted())
                        .bg(DarkTechTheme::bg_panel_hover())
                }
            })
            .on_click(cx.listener(move |this, _e: &ClickEvent, window, cx| {
                this.set_mode(mode, window, cx);
            }))
            .tooltip(crate::components::tooltip::tooltip(tooltip_text))
            .child(icon.with_size(px(13.0)).with_color(icon_color))
            .children({
                if mode == WorkbenchMode::Terminal {
                    let agent = self
                        .terminal_views
                        .get(&self.split_manager.active_pane_id)
                        .and_then(|t| t.read(cx).detected_agent.clone())
                        .or_else(|| {
                            self.terminal_views
                                .values()
                                .find_map(|t| t.read(cx).detected_agent.clone())
                        });
                    agent.as_ref().map(|a| {
                        let badge_color = match a.status {
                            redash_core::probe::agent::AgentStatus::NeedsInput => 0xf59e0b,
                            redash_core::probe::agent::AgentStatus::Thinking => 0x38bdf8,
                            redash_core::probe::agent::AgentStatus::Done => 0x10b981,
                            redash_core::probe::agent::AgentStatus::Idle => 0x64748b,
                        };
                        div()
                            .id("wb_tab_agent_dot")
                            .absolute()
                            .top(px(3.0))
                            .right(px(3.0))
                            .w(px(5.0))
                            .h(px(5.0))
                            .rounded_full()
                            .bg(rgb(badge_color))
                    })
                } else {
                    None
                }
            })
    }

    pub fn render_split_node(
        &self,
        node: &SplitNode,
        path: Vec<usize>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match node {
            SplitNode::Leaf { pane_id } => {
                let is_active = self.split_manager.active_pane_id == *pane_id;
                let active_border_color = if is_active {
                    DarkTechTheme::accent_cyan().opacity(0.8)
                } else {
                    DarkTechTheme::border_default()
                };

                let term_elem = if let Some(view) = self.terminal_views.get(pane_id) {
                    view.clone().into_any_element()
                } else {
                    div().into_any_element()
                };

                let total_panes = self.split_manager.panes().len();
                let pane_idx = self
                    .split_manager
                    .panes()
                    .iter()
                    .position(|p| p == pane_id)
                    .unwrap_or(0);

                let header_elem = if total_panes > 1 {
                    let pane_id_close = pane_id.clone();
                    let pane_id_fwd = pane_id.clone();
                    let pane_id_bwd = pane_id.clone();

                    Some(
                        div()
                            .id(ElementId::Name(format!("pane_hdr_{}", pane_id).into()))
                            .h(px(24.0))
                            .flex_shrink_0()
                            .w_full()
                            .bg(if is_active {
                                DarkTechTheme::bg_panel_hover()
                            } else {
                                DarkTechTheme::bg_panel()
                            })
                            .border_b_1()
                            .border_color(DarkTechTheme::border_muted())
                            .px_2()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .text_size(px(10.5))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_xs()
                                            .bg(if is_active {
                                                DarkTechTheme::accent_cyan().opacity(0.2)
                                            } else {
                                                DarkTechTheme::bg_input()
                                            })
                                            .border_1()
                                            .border_color(if is_active {
                                                DarkTechTheme::accent_cyan()
                                            } else {
                                                DarkTechTheme::border_muted()
                                            })
                                            .text_color(if is_active {
                                                DarkTechTheme::accent_cyan()
                                            } else {
                                                DarkTechTheme::text_muted()
                                            })
                                            .font_weight(FontWeight::BOLD)
                                            .child(crate::t_fmt!(
                                                "workbench.pane",
                                                index = (pane_idx + 1).to_string()
                                            )),
                                    )
                                    .child(
                                        div()
                                            .text_color(if is_active {
                                                DarkTechTheme::text_primary()
                                            } else {
                                                DarkTechTheme::text_muted()
                                            })
                                            .child(self.host.name.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        div()
                                            .id(ElementId::Name(
                                                format!("btn_swap_left_{}", pane_id).into(),
                                            ))
                                            .px_1p5()
                                            .h(px(18.0))
                                            .rounded_xs()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_1()
                                            .border_color(DarkTechTheme::border_default())
                                            .text_color(DarkTechTheme::text_secondary())
                                            .cursor_pointer()
                                            .hover(|s| {
                                                s.bg(DarkTechTheme::bg_panel_hover())
                                                    .text_color(DarkTechTheme::accent_cyan())
                                            })
                                            .on_click(cx.listener(
                                                move |this, _e: &ClickEvent, _window, cx| {
                                                    this.move_pane_backward(&pane_id_bwd, cx);
                                                },
                                            ))
                                            .child(crate::t!("workbench.swap_left")),
                                    )
                                    .child(
                                        div()
                                            .id(ElementId::Name(
                                                format!("btn_swap_right_{}", pane_id).into(),
                                            ))
                                            .px_1p5()
                                            .h(px(18.0))
                                            .rounded_xs()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_1()
                                            .border_color(DarkTechTheme::border_default())
                                            .text_color(DarkTechTheme::text_secondary())
                                            .cursor_pointer()
                                            .hover(|s| {
                                                s.bg(DarkTechTheme::bg_panel_hover())
                                                    .text_color(DarkTechTheme::accent_cyan())
                                            })
                                            .on_click(cx.listener(
                                                move |this, _e: &ClickEvent, _window, cx| {
                                                    this.move_pane_forward(&pane_id_fwd, cx);
                                                },
                                            ))
                                            .child(crate::t!("workbench.swap_right")),
                                    )
                                    .child(
                                        div()
                                            .id(ElementId::Name(
                                                format!("btn_close_pane_{}", pane_id).into(),
                                            ))
                                            .px_1p5()
                                            .h(px(18.0))
                                            .rounded_xs()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_1()
                                            .border_color(DarkTechTheme::status_crit().opacity(0.3))
                                            .text_color(DarkTechTheme::status_crit())
                                            .cursor_pointer()
                                            .hover(|s| {
                                                s.bg(DarkTechTheme::status_crit().opacity(0.15))
                                            })
                                            .on_click(cx.listener(
                                                move |this, _e: &ClickEvent, _window, cx| {
                                                    this.close_pane_by_id(&pane_id_close, cx);
                                                },
                                            ))
                                            .child("✕"),
                                    ),
                            ),
                    )
                } else {
                    None
                };

                let click_pane_id = pane_id.clone();
                div()
                    .id(ElementId::Name(format!("pane_{}", pane_id).into()))
                    .size_full()
                    .flex()
                    .flex_col()
                    .border_1()
                    .border_color(active_border_color)
                    .on_click(cx.listener(move |this, _e: &ClickEvent, window, cx| {
                        this.split_manager.set_active(click_pane_id.clone());
                        if let Some(v) = this.terminal_views.get(&click_pane_id) {
                            this.terminal_view = v.clone();
                            v.read(cx).focus(window);
                        }
                        cx.notify();
                    }))
                    .children(header_elem)
                    .child(
                        div()
                            .flex_1()
                            .min_h(px(0.0))
                            .w_full()
                            .overflow_hidden()
                            .child(term_elem),
                    )
                    .into_any_element()
            }
            SplitNode::Branch {
                direction,
                ratio,
                first,
                second,
            } => {
                let mut first_path = path.clone();
                first_path.push(0);
                let mut second_path = path.clone();
                second_path.push(1);

                let first_elem = self.render_split_node(first, first_path, cx);
                let second_elem = self.render_split_node(second, second_path, cx);
                let r = ratio.clamp(0.1, 0.9);

                let branch_path_drag = path.clone();
                let current_ratio = r;

                match direction {
                    SplitDirection::Vertical => {
                        let divider_id = format!("v_divider_{:?}", path);
                        div()
                            .size_full()
                            .flex()
                            .flex_row()
                            .child(
                                div()
                                    .flex_basis(relative(r))
                                    .flex_shrink_0()
                                    .h_full()
                                    .overflow_hidden()
                                    .child(first_elem),
                            )
                            // Interactive Draggable Divider
                            .child(
                                div()
                                    .id(ElementId::Name(divider_id.into()))
                                    .w(px(8.0))
                                    .h_full()
                                    .flex_shrink_0()
                                    .cursor_col_resize()
                                    .bg(DarkTechTheme::bg_root())
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .hover(|s| s.bg(DarkTechTheme::accent_cyan().opacity(0.2)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(
                                            move |this, e: &MouseDownEvent, _window, cx| {
                                                let start_pos = f32::from(e.position.x);
                                                this.active_split_drag = Some(ActiveSplitDrag {
                                                    branch_path: branch_path_drag.clone(),
                                                    is_vertical: true,
                                                    start_pos,
                                                    start_ratio: current_ratio,
                                                });
                                                cx.notify();
                                            },
                                        ),
                                    )
                                    .child(
                                        div().w(px(2.0)).h_full().bg(DarkTechTheme::border_muted()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_basis(relative(1.0 - r))
                                    .flex_shrink_0()
                                    .h_full()
                                    .overflow_hidden()
                                    .child(second_elem),
                            )
                            .into_any_element()
                    }
                    SplitDirection::Horizontal => {
                        let divider_id = format!("h_divider_{:?}", path);
                        div()
                            .size_full()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex_basis(relative(r))
                                    .flex_shrink_0()
                                    .w_full()
                                    .overflow_hidden()
                                    .child(first_elem),
                            )
                            // Interactive Draggable Divider
                            .child(
                                div()
                                    .id(ElementId::Name(divider_id.into()))
                                    .h(px(8.0))
                                    .w_full()
                                    .flex_shrink_0()
                                    .cursor_row_resize()
                                    .bg(DarkTechTheme::bg_root())
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .hover(|s| s.bg(DarkTechTheme::accent_cyan().opacity(0.2)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(
                                            move |this, e: &MouseDownEvent, _window, cx| {
                                                let start_pos = f32::from(e.position.y);
                                                this.active_split_drag = Some(ActiveSplitDrag {
                                                    branch_path: branch_path_drag.clone(),
                                                    is_vertical: false,
                                                    start_pos,
                                                    start_ratio: current_ratio,
                                                });
                                                cx.notify();
                                            },
                                        ),
                                    )
                                    .child(
                                        div().h(px(2.0)).w_full().bg(DarkTechTheme::border_muted()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_basis(relative(1.0 - r))
                                    .flex_shrink_0()
                                    .w_full()
                                    .overflow_hidden()
                                    .child(second_elem),
                            )
                            .into_any_element()
                    }
                }
            }
        }
    }
}

impl Render for WorkbenchView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (cpu_pct, mem_str, net_str, is_online, cpu_val, mem_val, rtt_text, rtt_color) =
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
                let net = if m.net_rates_available {
                    format!("↓ {:.0}KB/s  ↑ {:.0}KB/s", rx_kb, tx_kb)
                } else {
                    crate::t!("workbench.net_rate_uncollected").into()
                };
                let (rtt_t, rtt_c) = match m.rtt_ms {
                    Some(ms) if ms < 80 => (format!("{}ms", ms), DarkTechTheme::accent_emerald()),
                    Some(ms) if ms < 250 => (format!("{}ms", ms), DarkTechTheme::status_warn()),
                    Some(ms) => (format!("{}ms", ms), DarkTechTheme::status_crit()),
                    None => (
                        crate::t!("workbench.unknown").to_string(),
                        DarkTechTheme::accent_cyan(),
                    ),
                };
                (
                    cpu,
                    mem,
                    net,
                    self.probe_error.is_none(),
                    m.cpu.usage_percent,
                    m.mem.usage_percent,
                    rtt_t,
                    rtt_c,
                )
            } else {
                (
                    crate::t!("workbench.unknown").to_string(),
                    crate::t!("workbench.uncollected").to_string(),
                    crate::t!("workbench.uncollected").to_string(),
                    false,
                    0.0,
                    0.0,
                    "--".to_string(),
                    DarkTechTheme::text_muted(),
                )
            };

        let led_state = HostLedState::from_metrics(is_online, cpu_val, mem_val);
        let cpu_hist = self.cpu_history.clone();
        let active_mode = self.active_mode;

        div()
            .id("workbench_root")
            .children(self.probe_error.as_ref().map(|error| div().p_2().text_sm().text_color(DarkTechTheme::status_warn()).child(crate::t_fmt!("workbench.probe_error_last_data", error = error.to_string()))))
            .children(self.metrics.as_ref().filter(|m| !m.collection_errors.is_empty()).map(|m| div().px_2().text_xs().text_color(DarkTechTheme::text_muted()).child(m.collection_errors.join(" · "))))
            .size_full()
            .bg(DarkTechTheme::bg_root())
            .flex()
            .flex_col()
            .overflow_hidden()
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                if let Some(drag) = &this.active_split_drag {
                    let current_pos = if drag.is_vertical {
                        f32::from(event.position.x)
                    } else {
                        f32::from(event.position.y)
                    };
                    let delta = current_pos - drag.start_pos;
                    let ratio_delta = delta / 900.0;
                    let new_ratio = (drag.start_ratio + ratio_delta).clamp(0.1, 0.9);
                    this.split_manager.set_ratio_at_path(&drag.branch_path, new_ratio);
                    cx.notify();
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                if this.active_split_drag.is_some() {
                    this.active_split_drag = None;
                    cx.notify();
                }
            }))
            // Top Status & Navigation Ribbon (h: 36px)
            .child(
                div()
                    .h(px(36.0))
                    .flex_shrink_0()
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
                    // Left Telemetry Ribbon
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            .child(StatusLed::new(led_state).with_size(px(10.0)))
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(DarkTechTheme::text_primary())
                                    .child(self.host.name.clone()),
                            )
                            .child(
                                div()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_xs()
                                    .bg(DarkTechTheme::bg_input())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_muted())
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        Icon::zap()
                                            .with_size(px(8.5))
                                            .with_color(rtt_color),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(9.0))
                                            .font_family("Menlo")
                                            .text_color(rtt_color)
                                            .child(rtt_text),
                                    ),
                            )
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
                                    .child(
                                        div()
                                            .w(px(46.0))
                                            .h(px(15.0))
                                            .child(
                                                SparklineChart::tech(cpu_hist)
                                                    .with_pulse_dot(true),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap_1()
                                    .child("RAM:")
                                    .child(
                                        div()
                                            .text_color(DarkTechTheme::text_primary())
                                            .font_family("Menlo")
                                            .child(mem_str),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap_1()
                                    .child("NET:")
                                    .child(
                                        div()
                                            .text_color(DarkTechTheme::text_primary())
                                            .font_family("Menlo")
                                            .child(net_str),
                                    ),
                            ),
                    )
                    // Right Sub-View Navigation Tabs
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1p5()
                            .children({
                                if active_mode == WorkbenchMode::Terminal {
                                    Some(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .mr_2()
                                            .child(
                                                div()
                                                    .id("reconnect_terminal")
                                                    .h(px(26.0))
                                                    .w(px(28.0))
                                                    .rounded_md()
                                                    .bg(DarkTechTheme::bg_input())
                                                    .border_1()
                                                    .border_color(DarkTechTheme::border_default())
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .cursor_pointer()
                                                    .hover(|s| {
                                                        s.bg(DarkTechTheme::bg_panel_hover())
                                                            .border_color(DarkTechTheme::border_muted())
                                                    })
                                                    .on_click(cx.listener(|this, _e: &ClickEvent, window, cx| {
                                                        this.reconnect_terminal(window, cx);
                                                    }))
                                                    .tooltip(crate::components::tooltip::tooltip(crate::t!("workbench.reconnect_terminal")))
                                                    .child(Icon::refresh().with_size(px(13.0)).with_color(DarkTechTheme::text_secondary()))
                                            )
                                            .child(
                                                div()
                                                    .id("search_terminal")
                                                    .h(px(26.0))
                                                    .w(px(28.0))
                                                    .rounded_md()
                                                    .bg(DarkTechTheme::bg_input())
                                                    .border_1()
                                                    .border_color(DarkTechTheme::border_default())
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .cursor_pointer()
                                                    .hover(|s| {
                                                        s.bg(DarkTechTheme::bg_panel_hover())
                                                            .border_color(DarkTechTheme::border_muted())
                                                    })
                                                    .on_click(cx.listener(|this, _e: &ClickEvent, _window, cx| {
                                                        this.terminal_view.update(cx, |term, cx| {
                                                            term.toggle_search(cx);
                                                        });
                                                    }))
                                                    .tooltip(crate::components::tooltip::tooltip(crate::t!("term.search")))
                                                    .child(Icon::search().with_size(px(13.0)).with_color(DarkTechTheme::text_secondary()))
                                            )
                                            .child(
                                                div()
                                                    .id("clear_terminal")
                                                    .h(px(26.0))
                                                    .w(px(28.0))
                                                    .rounded_md()
                                                    .bg(DarkTechTheme::bg_input())
                                                    .border_1()
                                                    .border_color(DarkTechTheme::border_default())
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .cursor_pointer()
                                                    .hover(|s| {
                                                        s.bg(DarkTechTheme::bg_panel_hover())
                                                            .border_color(DarkTechTheme::border_muted())
                                                    })
                                                    .on_click(cx.listener(|this, _e: &ClickEvent, window, cx| {
                                                        this.terminal_view.update(cx, |term, _cx| {
                                                            term.focus(window);
                                                            term.clear_screen();
                                                        });
                                                    }))
                                                    .tooltip(crate::components::tooltip::tooltip(crate::t!("term.clear")))
                                                    .child(Icon::clear().with_size(px(13.0)).with_color(DarkTechTheme::text_secondary()))
                                            )
                                            .child(
                                                div()
                                                    .id("wb_split_v")
                                                    .h(px(26.0))
                                                    .w(px(28.0))
                                                    .rounded_md()
                                                    .bg(DarkTechTheme::bg_input())
                                                    .border_1()
                                                    .border_color(DarkTechTheme::border_default())
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .cursor_pointer()
                                                    .hover(|s| {
                                                        s.bg(DarkTechTheme::bg_panel_hover())
                                                            .border_color(DarkTechTheme::border_muted())
                                                    })
                                                    .on_click(cx.listener(|this, _e: &ClickEvent, _window, cx| {
                                                        this.split_terminal(SplitDirection::Vertical, cx);
                                                    }))
                                                    .tooltip(crate::components::tooltip::tooltip(crate::t!("workbench.btn_split_v")))
                                                    .child(Icon::split_vertical().with_size(px(13.0)).with_color(DarkTechTheme::text_secondary()))
                                            )
                                            .child(
                                                div()
                                                    .id("wb_split_h")
                                                    .h(px(26.0))
                                                    .w(px(28.0))
                                                    .rounded_md()
                                                    .bg(DarkTechTheme::bg_input())
                                                    .border_1()
                                                    .border_color(DarkTechTheme::border_default())
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .cursor_pointer()
                                                    .hover(|s| {
                                                        s.bg(DarkTechTheme::bg_panel_hover())
                                                            .border_color(DarkTechTheme::border_muted())
                                                    })
                                                    .on_click(cx.listener(|this, _e: &ClickEvent, _window, cx| {
                                                        this.split_terminal(SplitDirection::Horizontal, cx);
                                                    }))
                                                    .tooltip(crate::components::tooltip::tooltip(crate::t!("workbench.btn_split_h")))
                                                    .child(Icon::split_horizontal().with_size(px(13.0)).with_color(DarkTechTheme::text_secondary()))
                                            )
                                             .children({
                                                if self.split_manager.panes().len() > 1 {
                                                    Some(
                                                        div()
                                                            .flex()
                                                            .flex_row()
                                                            .items_center()
                                                            .gap_1()
                                                            .child(
                                                                div()
                                                                    .id("wb_ratio_30")
                                                                    .h(px(24.0))
                                                                    .px_1p5()
                                                                    .rounded_sm()
                                                                    .bg(DarkTechTheme::bg_input())
                                                                    .border_1()
                                                                    .border_color(DarkTechTheme::border_default())
                                                                    .text_color(DarkTechTheme::text_secondary())
                                                                    .text_size(px(10.0))
                                                                    .cursor_pointer()
                                                                    .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()).text_color(DarkTechTheme::accent_cyan()))
                                                                    .on_click(cx.listener(|this, _e: &ClickEvent, _window, cx| {
                                                                        this.set_split_ratio(&[], 0.3, cx);
                                                                    }))
                                                                    .tooltip(crate::components::tooltip::tooltip("30% : 70%"))
                                                                    .child("30:70")
                                                            )
                                                            .child(
                                                                div()
                                                                    .id("wb_ratio_50")
                                                                    .h(px(24.0))
                                                                    .px_1p5()
                                                                    .rounded_sm()
                                                                    .bg(DarkTechTheme::bg_input())
                                                                    .border_1()
                                                                    .border_color(DarkTechTheme::border_default())
                                                                    .text_color(DarkTechTheme::text_secondary())
                                                                    .text_size(px(10.0))
                                                                    .cursor_pointer()
                                                                    .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()).text_color(DarkTechTheme::accent_cyan()))
                                                                    .on_click(cx.listener(|this, _e: &ClickEvent, _window, cx| {
                                                                        this.set_split_ratio(&[], 0.5, cx);
                                                                    }))
                                                                    .tooltip(crate::components::tooltip::tooltip("50% : 50%"))
                                                                    .child("50:50")
                                                            )
                                                            .child(
                                                                div()
                                                                    .id("wb_ratio_70")
                                                                    .h(px(24.0))
                                                                    .px_1p5()
                                                                    .rounded_sm()
                                                                    .bg(DarkTechTheme::bg_input())
                                                                    .border_1()
                                                                    .border_color(DarkTechTheme::border_default())
                                                                    .text_color(DarkTechTheme::text_secondary())
                                                                    .text_size(px(10.0))
                                                                    .cursor_pointer()
                                                                    .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()).text_color(DarkTechTheme::accent_cyan()))
                                                                    .on_click(cx.listener(|this, _e: &ClickEvent, _window, cx| {
                                                                        this.set_split_ratio(&[], 0.7, cx);
                                                                    }))
                                                                    .tooltip(crate::components::tooltip::tooltip("70% : 30%"))
                                                                    .child("70:30")
                                                            )
                                                            .child(
                                                                div()
                                                                    .id("wb_close_split")
                                                                    .h(px(26.0))
                                                                    .w(px(28.0))
                                                                    .rounded_md()
                                                                    .bg(DarkTechTheme::bg_input())
                                                                    .border_1()
                                                                    .border_color(DarkTechTheme::status_crit().opacity(0.4))
                                                                    .flex()
                                                                    .items_center()
                                                                    .justify_center()
                                                                    .cursor_pointer()
                                                                    .hover(|s| s.bg(DarkTechTheme::status_crit().opacity(0.1)).border_color(DarkTechTheme::status_crit()))
                                                                    .on_click(cx.listener(|this, _e: &ClickEvent, _window, cx| {
                                                                        this.close_active_pane(cx);
                                                                    }))
                                                                    .tooltip(crate::components::tooltip::tooltip(crate::t!("workbench.btn_close_pane")))
                                                                    .child(Icon::close().with_size(px(11.0)).with_color(DarkTechTheme::status_crit()))
                                                            )
                                                    )
                                                } else {
                                                    None
                                                }
                                            })
                                    )
                                } else {
                                    None
                                }
                            })
                            .child(self.render_mode_tab(WorkbenchMode::Terminal, crate::t!("workbench.tab_terminal"), Icon::terminal(), cx))
                            .child(self.render_mode_tab(WorkbenchMode::Docker, crate::t!("workbench.tab_docker"), Icon::docker(), cx))
                            .child(self.render_mode_tab(WorkbenchMode::Processes, crate::t!("workbench.tab_process"), Icon::cpu(), cx))
                            .child(self.render_mode_tab(WorkbenchMode::Network, crate::t!("workbench.tab_network"), Icon::network(), cx))
                            .child(self.render_mode_tab(WorkbenchMode::Tunnels, crate::t!("workbench.tab_tunnel"), Icon::tunnel(), cx))
                            .child(self.render_mode_tab(WorkbenchMode::Snippets, crate::t!("workbench.tab_snippets"), Icon::snippet(), cx))
                            .child(self.render_mode_tab(WorkbenchMode::Sftp, crate::t!("workbench.tab_sftp"), Icon::folder(), cx)),
                    ),
            )
            // Main Content Area: Switches between Terminal, Docker, Processes, Network, Tunnels, Snippets, and SFTP
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .h_full()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .child(match active_mode {
                        WorkbenchMode::Terminal => {
                            let root = self.split_manager.root.clone();
                            self.render_split_node(&root, Vec::new(), cx)
                        }
                        WorkbenchMode::Docker => self.docker_panel.clone().into_any_element(),
                        WorkbenchMode::Processes => self.process_panel.clone().into_any_element(),
                        WorkbenchMode::Network => self.network_panel.clone().into_any_element(),
                        WorkbenchMode::Tunnels => self.tunnel_panel.clone().into_any_element(),
                        WorkbenchMode::Snippets => self.snippets_panel.clone().into_any_element(),
                        WorkbenchMode::Sftp => {
                            if let Some(ref sftp) = self.sftp_view {
                                sftp.clone().into_any_element()
                            } else {
                                div()
                                    .size_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(12.0))
                                    .text_color(DarkTechTheme::text_muted())
                                    .child(crate::t!("workbench.sftp_connecting"))
                                    .into_any_element()
                            }
                        }
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn test_workbench_mode_transitions() {
        assert_eq!(WorkbenchMode::Terminal, WorkbenchMode::Terminal);
        assert_ne!(WorkbenchMode::Docker, WorkbenchMode::Processes);
    }
}
