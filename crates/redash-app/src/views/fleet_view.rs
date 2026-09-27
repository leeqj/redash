use gpui::prelude::FluentBuilder;
use gpui::*;
use std::collections::{HashMap, HashSet};

use crate::components::chart::{SparklineChart, SparklineSeries};
use crate::components::icon::Icon;
use crate::components::micro_meter::MicroMeter;
use crate::components::status_led::{HostLedState, StatusLed};
use crate::components::theme::DarkTechTheme;
use crate::{t, t_fmt};
use redash_core::config::{HostConfig, HostId, MissingCredential, TargetOs};
use redash_core::probe::NodeMetrics;

#[derive(Debug, Clone)]
pub struct ProbeFailure {
    pub message: &'static str,
    pub details: String,
    pub needs_credentials: bool,
}

impl ProbeFailure {
    pub fn from_error(error: &anyhow::Error) -> Self {
        let needs_credentials = error.downcast_ref::<MissingCredential>().is_some();
        Self {
            message: if needs_credentials {
                t!("fleet.needs_credentials")
            } else {
                t!("fleet.probe_failed")
            },
            details: format!("{error:#}"),
            needs_credentials,
        }
    }
}

#[derive(Debug, Clone)]
pub enum FleetAction {
    OpenTerminal(HostConfig),
    OpenSftp(HostConfig),
    OpenBatch(Vec<HostConfig>),
    AddNewHost,
    EditHost(HostConfig),
    CloneHost(HostConfig),
    DeleteHost(HostId),
    DeleteBatch(Vec<HostId>),
    MoveHostUp(HostId),
    MoveHostDown(HostId),
    #[allow(dead_code)]
    MoveHostToTop(HostId),
    ReorderHosts(Vec<HostId>),
}

pub type FleetActionCallback =
    Box<dyn Fn(FleetAction, &mut Window, &mut Context<FleetView>) + 'static>;

#[derive(Clone, Debug)]
pub struct DraggedHost {
    pub id: HostId,
    pub name: String,
    pub group: String,
}

impl Render for DraggedHost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(240.0))
            .h(px(50.0))
            .bg(DarkTechTheme::bg_panel().opacity(0.95))
            .border_2()
            .border_color(DarkTechTheme::accent_cyan())
            .rounded_lg()
            .shadow_lg()
            .px_3()
            .py_2()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(
                Icon::server()
                    .with_size(px(16.0))
                    .with_color(DarkTechTheme::accent_cyan()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(DarkTechTheme::text_primary())
                            .child(self.name.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(
                                Icon::folder()
                                    .with_size(px(10.0))
                                    .with_color(DarkTechTheme::accent_cyan()),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(DarkTechTheme::accent_cyan())
                                    .child(self.group.clone()),
                            ),
                    ),
            )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CardMetricType {
    #[default]
    Cpu,
    Memory,
    Disk,
    All,
}

pub struct FleetView {
    pub probe_errors: HashMap<HostId, ProbeFailure>,
    pub history_limit: usize,
    pub hosts: Vec<HostConfig>,
    pub metrics: HashMap<HostId, NodeMetrics>,
    pub cpu_histories: HashMap<HostId, Vec<f32>>,
    pub mem_histories: HashMap<HostId, Vec<f32>>,
    pub disk_histories: HashMap<HostId, Vec<f32>>,
    pub active_card_metrics: HashMap<HostId, CardMetricType>,
    pub selected_hosts: HashSet<HostId>,
    pub search_query: String,
    pub on_action: Option<FleetActionCallback>,
    pub scroll_handle: ScrollHandle,
}

impl FleetView {
    pub fn new(hosts: Vec<HostConfig>) -> Self {
        Self {
            hosts,
            probe_errors: HashMap::new(),
            history_limit: 30,
            metrics: HashMap::new(),
            cpu_histories: HashMap::new(),
            mem_histories: HashMap::new(),
            disk_histories: HashMap::new(),
            active_card_metrics: HashMap::new(),
            selected_hosts: HashSet::new(),
            search_query: String::new(),
            on_action: None,
            scroll_handle: ScrollHandle::new(),
        }
    }

    pub fn set_hosts(&mut self, hosts: Vec<HostConfig>, cx: &mut Context<Self>) {
        self.hosts = hosts;
        let valid_ids: HashSet<HostId> = self.hosts.iter().map(|h| h.id.clone()).collect();
        self.selected_hosts.retain(|id| valid_ids.contains(id));
        cx.notify();
    }

    pub fn move_host_internal(&mut self, host_id: &HostId, move_up: bool) -> bool {
        if let Some(pos) = self.hosts.iter().position(|h| &h.id == host_id) {
            if move_up && pos > 0 {
                self.hosts.swap(pos, pos - 1);
                return true;
            } else if !move_up && pos + 1 < self.hosts.len() {
                self.hosts.swap(pos, pos + 1);
                return true;
            }
        }
        false
    }

    pub fn move_host(&mut self, host_id: &HostId, move_up: bool, cx: &mut Context<Self>) -> bool {
        let changed = self.move_host_internal(host_id, move_up);
        if changed {
            cx.notify();
        }
        changed
    }

    #[allow(dead_code)]
    pub fn move_host_to_top_internal(&mut self, host_id: &HostId) -> bool {
        if let Some(pos) = self.hosts.iter().position(|h| &h.id == host_id)
            && pos > 0
        {
            let host = self.hosts.remove(pos);
            self.hosts.insert(0, host);
            return true;
        }
        false
    }

    #[allow(dead_code)]
    pub fn move_host_to_top(&mut self, host_id: &HostId, cx: &mut Context<Self>) -> bool {
        let changed = self.move_host_to_top_internal(host_id);
        if changed {
            cx.notify();
        }
        changed
    }

    pub fn sort_by_name_internal(&mut self) -> Vec<HostId> {
        self.hosts.sort_by_key(|a| a.name.to_lowercase());
        self.hosts.iter().map(|h| h.id.clone()).collect()
    }

    pub fn sort_by_name(&mut self, cx: &mut Context<Self>) -> Vec<HostId> {
        let res = self.sort_by_name_internal();
        cx.notify();
        res
    }

    pub fn sort_by_group_internal(&mut self) -> Vec<HostId> {
        self.hosts.sort_by(|a, b| {
            a.group
                .to_lowercase()
                .cmp(&b.group.to_lowercase())
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        self.hosts.iter().map(|h| h.id.clone()).collect()
    }

    pub fn sort_by_group(&mut self, cx: &mut Context<Self>) -> Vec<HostId> {
        let res = self.sort_by_group_internal();
        cx.notify();
        res
    }

    pub fn sort_by_status_internal(&mut self) -> Vec<HostId> {
        self.hosts.sort_by(|a, b| {
            let online_a = self.metrics.contains_key(&a.id);
            let online_b = self.metrics.contains_key(&b.id);
            online_b
                .cmp(&online_a)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        self.hosts.iter().map(|h| h.id.clone()).collect()
    }

    pub fn sort_by_status(&mut self, cx: &mut Context<Self>) -> Vec<HostId> {
        let res = self.sort_by_status_internal();
        cx.notify();
        res
    }

    pub fn reorder_drag_drop_internal(&mut self, source_id: &HostId, target_id: &HostId) -> bool {
        if source_id == target_id {
            return false;
        }
        let source_pos = self.hosts.iter().position(|h| &h.id == source_id);
        let target_pos = self.hosts.iter().position(|h| &h.id == target_id);
        if let (Some(s_idx), Some(t_idx)) = (source_pos, target_pos) {
            let item = self.hosts.remove(s_idx);
            self.hosts.insert(t_idx, item);
            return true;
        }
        false
    }

    pub fn reorder_drag_drop(
        &mut self,
        source_id: &HostId,
        target_id: &HostId,
        cx: &mut Context<Self>,
    ) -> bool {
        let changed = self.reorder_drag_drop_internal(source_id, target_id);
        if changed {
            cx.notify();
        }
        changed
    }

    pub fn set_on_action<F>(&mut self, callback: F)
    where
        F: Fn(FleetAction, &mut Window, &mut Context<Self>) + 'static,
    {
        self.on_action = Some(Box::new(callback));
    }

    pub fn clear_host_metrics(&mut self, id: &HostId) {
        self.metrics.remove(id);
        self.probe_errors.remove(id);
        self.cpu_histories.remove(id);
        self.mem_histories.remove(id);
        self.disk_histories.remove(id);
    }
    pub fn set_probe_error(&mut self, id: HostId, failure: ProbeFailure, cx: &mut Context<Self>) {
        self.probe_errors.insert(id, failure);
        cx.notify();
    }

    pub fn update_metrics(&mut self, host_id: HostId, metric: NodeMetrics, cx: &mut Context<Self>) {
        self.probe_errors.remove(&host_id);
        let cpu = metric.cpu.usage_percent;
        let mem = metric.mem.usage_percent;
        let disk = metric.disks.first().map(|d| d.usage_percent).unwrap_or(0.0);
        self.metrics.insert(host_id.clone(), metric);

        // CPU ring buffer (30 points)
        let cpu_hist = self.cpu_histories.entry(host_id.clone()).or_default();
        while cpu_hist.len() >= self.history_limit {
            cpu_hist.remove(0);
        }
        cpu_hist.push(cpu);

        // Memory ring buffer (30 points)
        let mem_hist = self.mem_histories.entry(host_id.clone()).or_default();
        while mem_hist.len() >= self.history_limit {
            mem_hist.remove(0);
        }
        mem_hist.push(mem);

        // Disk ring buffer (30 points)
        let disk_hist = self.disk_histories.entry(host_id).or_default();
        while disk_hist.len() >= self.history_limit {
            disk_hist.remove(0);
        }
        disk_hist.push(disk);

        cx.notify();
    }

    pub fn set_card_metric(
        &mut self,
        host_id: &HostId,
        metric: CardMetricType,
        cx: &mut Context<Self>,
    ) {
        self.active_card_metrics.insert(host_id.clone(), metric);
        cx.notify();
    }

    pub fn toggle_selection(&mut self, host_id: &HostId, cx: &mut Context<Self>) {
        if self.selected_hosts.contains(host_id) {
            self.selected_hosts.remove(host_id);
        } else {
            self.selected_hosts.insert(host_id.clone());
        }
        cx.notify();
    }

    pub fn select_all(&mut self, filtered_ids: &[HostId], cx: &mut Context<Self>) {
        let all_filtered_selected = !filtered_ids.is_empty()
            && filtered_ids
                .iter()
                .all(|id| self.selected_hosts.contains(id));
        if all_filtered_selected {
            for id in filtered_ids {
                self.selected_hosts.remove(id);
            }
        } else {
            for id in filtered_ids {
                self.selected_hosts.insert(id.clone());
            }
        }
        cx.notify();
    }
}

impl Render for FleetView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let total_hosts = self.hosts.len();
        let selected_count = self.selected_hosts.len();

        let filtered_hosts: Vec<HostConfig> = self
            .hosts
            .iter()
            .filter(|h| {
                if self.search_query.is_empty() {
                    true
                } else {
                    let q = self.search_query.to_lowercase();
                    h.name.to_lowercase().contains(&q)
                        || h.hostname.to_lowercase().contains(&q)
                        || h.group.to_lowercase().contains(&q)
                        || h.tags.iter().any(|t| t.to_lowercase().contains(&q))
                }
            })
            .cloned()
            .collect();
        let filtered_ids: Vec<HostId> = filtered_hosts.iter().map(|h| h.id.clone()).collect();

        div()
            .size_full()
            .bg(DarkTechTheme::bg_root())
            .flex()
            .flex_col()
            .overflow_hidden()
            .p_4()
            .gap_4()
            // 1. Top Header Bar
            .child(
                div()
                    .w_full()
                    .h(px(40.0))
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
                                    .child(
                                        Icon::server()
                                            .with_size(px(18.0))
                                            .with_color(DarkTechTheme::accent_cyan()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(18.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(DarkTechTheme::text_primary())
                                            .child(crate::t!("fleet.title")),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(DarkTechTheme::text_muted())
                                    .child(crate::t_fmt!(
                                        "fleet.hosts_count",
                                        total = total_hosts.to_string(),
                                        selected = selected_count.to_string(),
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            // Prominent "+ 添加主机" Button
                            .child(
                                div()
                                    .id("btn_fleet_add_host")
                                    .px_3()
                                    .py_1p5()
                                    .rounded_md()
                                    .bg(DarkTechTheme::border_active())
                                    .text_color(DarkTechTheme::bg_root())
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(12.0))
                                    .hover(|s| s.bg(DarkTechTheme::accent_cyan()))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _event: &ClickEvent, window, cx| {
                                        if let Some(cb) = &this.on_action {
                                            cb(FleetAction::AddNewHost, window, cx);
                                        }
                                    }))
                                    .child(format!("+ {}", crate::t!("fleet.add_host"))),
                            )
                            // Filter: All
                            .child(
                                div()
                                    .id("filter_all")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(if self.search_query.is_empty() {
                                        DarkTechTheme::bg_panel_hover()
                                    } else {
                                        DarkTechTheme::bg_panel()
                                    })
                                    .border_1()
                                    .border_color(if self.search_query.is_empty() {
                                        DarkTechTheme::border_active()
                                    } else {
                                        DarkTechTheme::border_default()
                                    })
                                    .text_color(if self.search_query.is_empty() {
                                        DarkTechTheme::text_accent()
                                    } else {
                                        DarkTechTheme::text_secondary()
                                    })
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.search_query = "".to_string();
                                        cx.notify();
                                    }))
                                    .child(crate::t!("fleet.filter_all")),
                            )
                            // Filter: Production
                            .child(
                                div()
                                    .id("filter_prod")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(if self.search_query == "production" {
                                        DarkTechTheme::bg_panel_hover()
                                    } else {
                                        DarkTechTheme::bg_panel()
                                    })
                                    .border_1()
                                    .border_color(if self.search_query == "production" {
                                        DarkTechTheme::border_active()
                                    } else {
                                        DarkTechTheme::border_default()
                                    })
                                    .text_color(if self.search_query == "production" {
                                        DarkTechTheme::text_accent()
                                    } else {
                                        DarkTechTheme::text_secondary()
                                    })
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.search_query = "production".to_string();
                                        cx.notify();
                                    }))
                                    .child("Production"),
                            )
                            // Filter: Staging
                            .child(
                                div()
                                    .id("filter_staging")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(if self.search_query == "staging" {
                                        DarkTechTheme::bg_panel_hover()
                                    } else {
                                        DarkTechTheme::bg_panel()
                                    })
                                    .border_1()
                                    .border_color(if self.search_query == "staging" {
                                        DarkTechTheme::border_active()
                                    } else {
                                        DarkTechTheme::border_default()
                                    })
                                    .text_color(if self.search_query == "staging" {
                                        DarkTechTheme::text_accent()
                                    } else {
                                        DarkTechTheme::text_secondary()
                                    })
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.search_query = "staging".to_string();
                                        cx.notify();
                                    }))
                                    .child("Staging"),
                            )
                            // Select All / Deselect All
                            .child({
                                let ids = filtered_ids.clone();
                                let is_all_filtered_selected = !ids.is_empty()
                                    && ids.iter().all(|id| self.selected_hosts.contains(id));
                                div()
                                    .id("btn_select_all")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_panel())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_default())
                                    .text_color(DarkTechTheme::text_secondary())
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .on_click(cx.listener(
                                        move |this, _event: &ClickEvent, _window, cx| {
                                            this.select_all(&ids, cx);
                                        },
                                    ))
                                     .child(if is_all_filtered_selected {
                                         crate::t!("batch.deselect_all")
                                     } else {
                                         crate::t!("batch.select_all")
                                     })
                            })
                            // Sort by Name
                            .child(
                                div()
                                    .id("btn_sort_name")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_panel())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_default())
                                    .text_color(DarkTechTheme::text_secondary())
                                    .text_size(px(11.0))
                                    .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()).text_color(DarkTechTheme::accent_cyan()))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _event: &ClickEvent, window, cx| {
                                        let new_order = this.sort_by_name(cx);
                                        if let Some(cb) = &this.on_action {
                                            cb(FleetAction::ReorderHosts(new_order), window, cx);
                                        }
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                Icon::sort()
                                                    .with_size(px(11.0))
                                                    .with_color(DarkTechTheme::text_secondary()),
                                            )
                                            .child(crate::t!("fleet.sort_name")),
                                    ),
                            )
                            // Sort by Group
                            .child(
                                div()
                                    .id("btn_sort_group")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_panel())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_default())
                                    .text_color(DarkTechTheme::text_secondary())
                                    .text_size(px(11.0))
                                    .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()).text_color(DarkTechTheme::accent_cyan()))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _event: &ClickEvent, window, cx| {
                                        let new_order = this.sort_by_group(cx);
                                        if let Some(cb) = &this.on_action {
                                             cb(FleetAction::ReorderHosts(new_order), window, cx);
                                        }
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                Icon::sort()
                                                    .with_size(px(11.0))
                                                    .with_color(DarkTechTheme::text_secondary()),
                                            )
                                            .child(crate::t!("fleet.sort_group")),
                                    ),
                            )
                            // Sort by Status
                            .child(
                                div()
                                    .id("btn_sort_status")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_panel())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_default())
                                    .text_color(DarkTechTheme::text_secondary())
                                    .text_size(px(11.0))
                                    .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()).text_color(DarkTechTheme::accent_cyan()))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _event: &ClickEvent, window, cx| {
                                        let new_order = this.sort_by_status(cx);
                                        if let Some(cb) = &this.on_action {
                                             cb(FleetAction::ReorderHosts(new_order), window, cx);
                                        }
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                Icon::sort()
                                                    .with_size(px(11.0))
                                                    .with_color(DarkTechTheme::text_secondary()),
                                            )
                                            .child(crate::t!("fleet.sort_status")),
                                    ),
                            )
                    )
            )
            // 2. Grid of High-Tech Host Cards
            .child(
                div()
                    .id("fleet_card_scroll")
                    .track_scroll(&self.scroll_handle)
                    .flex_1()
                    .w_full()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .id("fleet_card_grid")
                            .w_full()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap_3()
                            .pb_6()
                            .children(filtered_hosts.into_iter().enumerate().map(|(idx, host)| {
                        let host_id = host.id.clone();
                        let is_selected = self.selected_hosts.contains(&host_id);
                        let metric = self.metrics.get(&host_id);
                        let cpu_history =
                            self.cpu_histories.get(&host_id).cloned().unwrap_or_default();
                        let mem_history =
                            self.mem_histories.get(&host_id).cloned().unwrap_or_default();
                        let disk_history =
                            self.disk_histories.get(&host_id).cloned().unwrap_or_default();
                        let active_metric =
                            self.active_card_metrics.get(&host_id).copied().unwrap_or_default();

                        let (cpu_val, mem_val, disk_val, mem_used_gb, mem_total_gb, rtt_ms_val, net_used_gb) =
                            if let Some(m) = metric {
                                let used_g = m.mem.used_bytes as f32 / (1024.0 * 1024.0 * 1024.0);
                                let total_g =
                                    m.mem.total_bytes as f32 / (1024.0 * 1024.0 * 1024.0);
                                let net_gb = (m.net.total_rx_bytes + m.net.total_tx_bytes) as f32
                                    / (1024.0 * 1024.0 * 1024.0);
                                (
                                    m.cpu.usage_percent,
                                    m.mem.usage_percent,
                                    m.disks
                                        .first()
                                        .map(|d| d.usage_percent)
                                        .unwrap_or(0.0),
                                    used_g,
                                    total_g,
                                    m.rtt_ms,
                                    net_gb,
                                )
                            } else {
                                (0.0, 0.0, 0.0, 0.0, 0.0, None, 0.0)
                            };

                        let is_online = metric.is_some() && !self.probe_errors.contains_key(&host_id);
                        let led_state = HostLedState::from_metrics(is_online, cpu_val, mem_val);

                        let (rtt_text, rtt_color, rtt_bg) = match rtt_ms_val {
                            Some(ms) if ms < 80 => (
                                format!("{}ms", ms),
                                DarkTechTheme::accent_emerald(),
                                rgba(0x10b98122),
                            ),
                            Some(ms) if ms < 250 => (
                                format!("{}ms", ms),
                                DarkTechTheme::status_warn(),
                                rgba(0xf59e0b22),
                            ),
                            Some(ms) => (
                                format!("{}ms", ms),
                                DarkTechTheme::status_crit(),
                                rgba(0xef444422),
                            ),
                            None if is_online => (
                                t!("fleet.status_unknown").to_string(),
                                DarkTechTheme::accent_cyan(),
                                rgba(0x06b6d422),
                            ),
                            None => (
                                "--".to_string(),
                                DarkTechTheme::text_muted(),
                                rgba(0x181a20ff),
                            ),
                        };

                        let (os_icon, os_name) = match host.target_os {
                            TargetOs::Linux => (Icon::linux(), "Linux"),
                            TargetOs::Darwin => (Icon::apple(), "macOS"),
                            TargetOs::Windows => (Icon::windows(), "Windows"),
                            TargetOs::Unknown => (Icon::server(), "Host"),
                        };

                        let host_for_term = host.clone();
                        let host_for_sftp = host.clone();
                        let host_for_edit = host.clone();
                        let host_for_clone = host.clone();
                        let host_id_for_delete = host_id.clone();
                        let host_id_for_select = host_id.clone();
                        let host_id_for_move_up = host_id.clone();
                        let host_id_for_move_down = host_id.clone();

                        div()
                            .id(ElementId::Name(format!("host_card_{}", host_id.0).into()))
                            .w(px(320.0))
                            .min_h(px(220.0))
                            .bg(DarkTechTheme::bg_panel())
                            .border_1()
                            .border_color(if is_selected {
                                DarkTechTheme::border_active()
                            } else {
                                DarkTechTheme::border_default()
                            })
                            .rounded_lg()
                            .p_3()
                            .flex()
                            .flex_col()
                            .justify_between()
                            .hover(|s| {
                                s.bg(DarkTechTheme::bg_panel_hover())
                                    .border_color(DarkTechTheme::border_active())
                            })
                            .drag_over::<DraggedHost>(|style, _dragged, _window, _cx| {
                                style.border_color(DarkTechTheme::accent_cyan())
                            })
                            .on_drop(cx.listener({
                                let target_id = host_id.clone();
                                move |this, dragged: &DraggedHost, window, cx| {
                                    if dragged.id != target_id {
                                        this.reorder_drag_drop(&dragged.id, &target_id, cx);
                                        if let Some(cb) = &this.on_action {
                                            let new_order = this.hosts.iter().map(|h| h.id.clone()).collect();
                                            cb(FleetAction::ReorderHosts(new_order), window, cx);
                                        }
                                    }
                                }
                            }))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    // Row 1: Status LED, Name, Group Badge, RTT badge, Selection checkbox
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .id(ElementId::Name(format!("drag_handle_{}", host_id.0).into()))
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_1p5()
                                                    .cursor_grab()
                                                    .on_drag(
                                                        DraggedHost {
                                                            id: host_id.clone(),
                                                            name: host.name.clone(),
                                                            group: host.group.clone(),
                                                        },
                                                        |info: &DraggedHost, _position, _window, cx| {
                                                            let info = info.clone();
                                                            cx.new(|_| info)
                                                        },
                                                    )
                                                    .child(
                                                        Icon::grip()
                                                            .with_size(px(12.0))
                                                            .with_color(DarkTechTheme::text_muted()),
                                                    )
                                                    .child(
                                                        StatusLed::new(led_state)
                                                            .with_size(px(12.0)),
                                                    )
                                                    .child(
                                                        div()
                                                            .font_weight(FontWeight::BOLD)
                                                            .text_size(px(13.5))
                                                            .text_color(
                                                                DarkTechTheme::text_primary(),
                                                            )
                                                            .child(host.name.clone()),
                                                    )
                                                    .child(
                                                        div()
                                                            .px_1p5()
                                                            .py_0p5()
                                                            .rounded_xs()
                                                            .bg(DarkTechTheme::bg_input())
                                                            .border_1()
                                                            .border_color(
                                                                DarkTechTheme::border_muted(),
                                                            )
                                                            .text_size(px(9.0))
                                                            .text_color(
                                                                DarkTechTheme::text_muted(),
                                                            )
                                                            .child(host.group.clone()),
                                                    )
                                                    .child(
                                                        div()
                                                            .px_1p5()
                                                            .py_0p5()
                                                            .rounded_xs()
                                                            .bg(rtt_bg)
                                                            .border_1()
                                                            .border_color(
                                                                DarkTechTheme::border_muted(),
                                                            )
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
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .id(ElementId::NamedInteger(
                                                        "btn_select".into(),
                                                        idx as u64,
                                                    ))
                                                    .px_2()
                                                    .py_0p5()
                                                    .rounded_md()
                                                    .bg(if is_selected {
                                                        DarkTechTheme::border_active()
                                                    } else {
                                                        DarkTechTheme::bg_input()
                                                    })
                                                    .border_1()
                                                    .border_color(if is_selected {
                                                        DarkTechTheme::border_active()
                                                    } else {
                                                        DarkTechTheme::border_default()
                                                    })
                                                    .text_color(if is_selected {
                                                        DarkTechTheme::bg_root()
                                                    } else {
                                                        DarkTechTheme::text_muted()
                                                    })
                                                    .text_size(px(10.0))
                                                    .cursor_pointer()
                                                    .on_click(cx.listener(
                                                        move |this,
                                                              _event: &ClickEvent,
                                                              _window,
                                                              cx| {
                                                            this.toggle_selection(
                                                                &host_id_for_select,
                                                                cx,
                                                            );
                                                        },
                                                    ))
                                                    .child(if is_selected {
                                                        div()
                                                            .flex()
                                                            .flex_row()
                                                            .items_center()
                                                            .gap_1()
                                                            .child(Icon::check().with_size(px(9.0)).with_color(DarkTechTheme::bg_root()))
                                                            .child(crate::t!("fleet.selected"))
                                                    } else {
                                                        div()
                                                            .flex()
                                                            .flex_row()
                                                            .items_center()
                                                            .gap_1()
                                                            .child(Icon::plus().with_size(px(9.0)).with_color(DarkTechTheme::text_muted()))
                                                            .child(crate::t!("fleet.select"))
                                                    }),
                                            ),
                                    )
                                    // Row 2: Address & Target OS
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .justify_between()
                                            .items_center()
                                            .text_size(px(11.0))
                                            .child(
                                                div()
                                                    .font_family("Menlo")
                                                    .text_color(DarkTechTheme::text_muted())
                                                    .child(format!(
                                                        "{}@{}:{}",
                                                        host.user, host.hostname, host.port
                                                    )),
                                             )
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_1()
                                                    .text_color(DarkTechTheme::text_secondary())
                                                    .child(
                                                        os_icon
                                                            .with_size(px(11.0))
                                                            .with_color(DarkTechTheme::text_secondary()),
                                                    )
                                                    .child(div().text_size(px(11.0)).child(os_name)),
                                            ),
                                    )
                                    .children(self.probe_errors.get(&host_id).map(|failure| {
                                        let details = failure.details.clone();
                                        let host = host.clone();
                                        let now = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default().as_secs();
                                        div().w_full().flex().flex_col().gap_1().p_2().rounded_md()
                                            .bg(DarkTechTheme::bg_input())
                                            .text_size(px(10.0)).text_color(DarkTechTheme::status_warn())
                                            .child(failure.message)
                                            .child(div().text_color(DarkTechTheme::text_muted()).child(
                                                metric.map(|m| t_fmt!("fleet.last_probe_success", secs = now.saturating_sub(m.timestamp)))
                                                    .unwrap_or_else(|| t!("fleet.no_probe_yet").into())
                                            ))
                                            .child(div().flex().gap_3()
                                                .when(failure.needs_credentials, |row| row.child(
                                                    div().id(ElementId::Name(format!("repair_credentials_{}", host_id.0).into()))
                                                        .cursor_pointer().text_color(DarkTechTheme::accent_cyan())
                                                        .on_click(cx.listener(move |this, _, window, cx| {
                                                            if let Some(callback) = &this.on_action {
                                                                callback(FleetAction::EditHost(host.clone()), window, cx);
                                                            }
                                                        }))
                                                        .child(t!("fleet.edit_credentials"))
                                                ))
                                                .child(div().id(ElementId::Name(format!("probe_details_{}", host_id.0).into()))
                                                    .cursor_pointer().text_color(DarkTechTheme::text_secondary())
                                                    .on_click(cx.listener(move |_, _, window, cx| {
                                                        let answer = window.prompt(PromptLevel::Warning, t!("fleet.probe_failed_title"), Some(&details), &[t!("common.close")], cx);
                                                        cx.spawn(async move |_, _| { let _ = answer.await; }).detach();
                                                    }))
                                                    .child(t!("common.details"))))
                                    }))
                                    // Row 3: Precision Telemetry MicroMeters & Interactive Sparkline HUD
                                    .child({
                                        let is_cpu_active = active_metric == CardMetricType::Cpu;
                                        let is_mem_active = active_metric == CardMetricType::Memory;
                                        let is_disk_active = active_metric == CardMetricType::Disk;
                                        let is_all_active = active_metric == CardMetricType::All;

                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .justify_between()
                                            .gap_2()
                                            // Left Column: Interactive MicroMeters
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .flex()
                                                    .flex_col()
                                                    .gap_1()
                                                    // CPU row
                                                    .child(
                                                        div()
                                                            .id(ElementId::Name(format!("meter_cpu_{}", host_id.0).into()))
                                                            .cursor_pointer()
                                                            .rounded_xs()
                                                            .px_1()
                                                            .py_0p5()
                                                            .when(is_cpu_active, |this| {
                                                                this.bg(DarkTechTheme::bg_panel_hover())
                                                                    .border_l_2()
                                                                    .border_color(DarkTechTheme::accent_cyan())
                                                            })
                                                            .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()))
                                                            .on_click(cx.listener({
                                                                let host_id = host_id.clone();
                                                                move |this, _, _, cx| {
                                                                    this.set_card_metric(&host_id, CardMetricType::Cpu, cx);
                                                                }
                                                            }))
                                                            .child(if metric.is_some() { MicroMeter::cpu(cpu_val).into_any_element() } else { div().text_sm().text_color(DarkTechTheme::text_secondary()).child(t!("fleet.cpu_not_collected")).into_any_element() }),
                                                    )
                                                    // RAM row
                                                    .child(
                                                        div()
                                                            .id(ElementId::Name(format!("meter_mem_{}", host_id.0).into()))
                                                            .cursor_pointer()
                                                            .rounded_xs()
                                                            .px_1()
                                                            .py_0p5()
                                                            .when(is_mem_active, |this| {
                                                                this.bg(DarkTechTheme::bg_panel_hover())
                                                                    .border_l_2()
                                                                    .border_color(DarkTechTheme::accent_indigo())
                                                            })
                                                            .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()))
                                                            .on_click(cx.listener({
                                                                let host_id = host_id.clone();
                                                                move |this, _, _, cx| {
                                                                    this.set_card_metric(&host_id, CardMetricType::Memory, cx);
                                                                }
                                                            }))
                                                            .child(if metric.is_some() {
                                                                MicroMeter::memory(mem_used_gb, mem_total_gb, mem_val).into_any_element()
                                                            } else {
                                                                div().text_sm().text_color(DarkTechTheme::text_secondary()).child(t!("fleet.mem_not_collected")).into_any_element()
                                                            }),
                                                    )
                                                    // DISK row
                                                    .child(
                                                        div()
                                                            .id(ElementId::Name(format!("meter_disk_{}", host_id.0).into()))
                                                            .cursor_pointer()
                                                            .rounded_xs()
                                                            .px_1()
                                                            .py_0p5()
                                                            .when(is_disk_active, |this| {
                                                                this.bg(DarkTechTheme::bg_panel_hover())
                                                                    .border_l_2()
                                                                    .border_color(DarkTechTheme::status_warn())
                                                            })
                                                            .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()))
                                                            .on_click(cx.listener({
                                                                let host_id = host_id.clone();
                                                                move |this, _, _, cx| {
                                                                    this.set_card_metric(&host_id, CardMetricType::Disk, cx);
                                                                }
                                                            }))
                                                            .child(if metric.is_some_and(|m| !m.disks.is_empty()) { MicroMeter::disk(disk_val).into_any_element() } else { div().text_sm().text_color(DarkTechTheme::text_secondary()).child(t!("fleet.disk_not_collected")).into_any_element() }),
                                                    ),
                                            )
                                            // Right Column: Interactive Sparkline HUD Box
                                            .child(
                                                div()
                                                    .w(px(144.0))
                                                    .h(px(84.0))
                                                    .flex()
                                                    .flex_col()
                                                    .justify_between()
                                                    .bg(DarkTechTheme::bg_input())
                                                    .border_1()
                                                    .border_color(DarkTechTheme::border_muted())
                                                    .rounded_sm()
                                                    .p_1()
                                                    // 1. Metric Switcher Mini Tabs
                                                    .child(
                                                        div()
                                                            .w_full()
                                                            .flex()
                                                            .flex_row()
                                                            .justify_between()
                                                            .items_center()
                                                            .mb_1()
                                                            .child(
                                                                div()
                                                                    .flex()
                                                                    .flex_row()
                                                                    .gap_1()
                                                                    .child(
                                                                        div()
                                                                            .id(ElementId::Name(format!("tab_cpu_{}", host_id.0).into()))
                                                                            .cursor_pointer()
                                                                            .px_1()
                                                                            .py_0p5()
                                                                            .rounded_xs()
                                                                            .text_size(px(8.5))
                                                                            .font_family("Menlo")
                                                                            .font_weight(FontWeight::BOLD)
                                                                            .border_1()
                                                                            .border_color(if is_cpu_active {
                                                                                DarkTechTheme::accent_cyan()
                                                                            } else {
                                                                                DarkTechTheme::border_muted()
                                                                            })
                                                                            .bg(if is_cpu_active {
                                                                                DarkTechTheme::bg_panel_hover()
                                                                            } else {
                                                                                hsla(0.0, 0.0, 0.0, 0.0)
                                                                            })
                                                                            .text_color(if is_cpu_active {
                                                                                DarkTechTheme::accent_cyan()
                                                                            } else {
                                                                                DarkTechTheme::text_muted()
                                                                            })
                                                                            .on_click(cx.listener({
                                                                                let host_id = host_id.clone();
                                                                                move |this, _, _, cx| {
                                                                                    this.set_card_metric(&host_id, CardMetricType::Cpu, cx);
                                                                                }
                                                                            }))
                                                                            .child("CPU"),
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .id(ElementId::Name(format!("tab_mem_{}", host_id.0).into()))
                                                                            .cursor_pointer()
                                                                            .px_1()
                                                                            .py_0p5()
                                                                            .rounded_xs()
                                                                            .text_size(px(8.5))
                                                                            .font_family("Menlo")
                                                                            .font_weight(FontWeight::BOLD)
                                                                            .border_1()
                                                                            .border_color(if is_mem_active {
                                                                                DarkTechTheme::accent_indigo()
                                                                            } else {
                                                                                DarkTechTheme::border_muted()
                                                                            })
                                                                            .bg(if is_mem_active {
                                                                                DarkTechTheme::bg_panel_hover()
                                                                            } else {
                                                                                hsla(0.0, 0.0, 0.0, 0.0)
                                                                            })
                                                                            .text_color(if is_mem_active {
                                                                                DarkTechTheme::accent_indigo()
                                                                            } else {
                                                                                DarkTechTheme::text_muted()
                                                                            })
                                                                            .on_click(cx.listener({
                                                                                let host_id = host_id.clone();
                                                                                move |this, _, _, cx| {
                                                                                    this.set_card_metric(&host_id, CardMetricType::Memory, cx);
                                                                                }
                                                                            }))
                                                                            .child(t!("metric.memory")),
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .id(ElementId::Name(format!("tab_disk_{}", host_id.0).into()))
                                                                            .cursor_pointer()
                                                                            .px_1()
                                                                            .py_0p5()
                                                                            .rounded_xs()
                                                                            .text_size(px(8.5))
                                                                            .font_family("Menlo")
                                                                            .font_weight(FontWeight::BOLD)
                                                                            .border_1()
                                                                            .border_color(if is_disk_active {
                                                                                DarkTechTheme::status_warn()
                                                                            } else {
                                                                                DarkTechTheme::border_muted()
                                                                            })
                                                                            .bg(if is_disk_active {
                                                                                DarkTechTheme::bg_panel_hover()
                                                                            } else {
                                                                                hsla(0.0, 0.0, 0.0, 0.0)
                                                                            })
                                                                            .text_color(if is_disk_active {
                                                                                DarkTechTheme::status_warn()
                                                                            } else {
                                                                                DarkTechTheme::text_muted()
                                                                            })
                                                                            .on_click(cx.listener({
                                                                                let host_id = host_id.clone();
                                                                                move |this, _, _, cx| {
                                                                                    this.set_card_metric(&host_id, CardMetricType::Disk, cx);
                                                                                }
                                                                            }))
                                                                            .child(t!("metric.disk")),
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .id(ElementId::Name(format!("tab_all_{}", host_id.0).into()))
                                                                            .cursor_pointer()
                                                                            .px_1()
                                                                            .py_0p5()
                                                                            .rounded_xs()
                                                                            .text_size(px(8.5))
                                                                            .font_family("Menlo")
                                                                            .font_weight(FontWeight::BOLD)
                                                                            .border_1()
                                                                            .border_color(if is_all_active {
                                                                                DarkTechTheme::accent_emerald()
                                                                            } else {
                                                                                DarkTechTheme::border_muted()
                                                                            })
                                                                            .bg(if is_all_active {
                                                                                DarkTechTheme::bg_panel_hover()
                                                                            } else {
                                                                                hsla(0.0, 0.0, 0.0, 0.0)
                                                                            })
                                                                            .text_color(if is_all_active {
                                                                                DarkTechTheme::accent_emerald()
                                                                            } else {
                                                                                DarkTechTheme::text_muted()
                                                                            })
                                                                            .on_click(cx.listener({
                                                                                let host_id = host_id.clone();
                                                                                move |this, _, _, cx| {
                                                                                    this.set_card_metric(&host_id, CardMetricType::All, cx);
                                                                                }
                                                                            }))
                                                                            .child(t!("metric.all")),
                                                                    ),
                                                            ),
                                                    )
                                                    // 2. Interactive HUD Sparkline Chart with Scale & Value
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .w_full()
                                                            .min_h(px(0.0))
                                                            .child(match active_metric {
                                                                CardMetricType::Cpu => SparklineChart::tech(cpu_history)
                                                                    .with_value(if metric.is_some() { format!("{:.1}%", cpu_val) } else { t!("fleet.not_collected").into() })
                                                                    .with_scale_labels(true)
                                                                    .with_range(true)
                                                                    .with_pulse_dot(true),
                                                                CardMetricType::Memory => SparklineChart::new(mem_history, DarkTechTheme::accent_indigo())
                                                                    .with_value(if metric.is_some() { format!("{:.1}G ({:.0}%)", mem_used_gb, mem_val) } else { t!("fleet.not_collected").into() })
                                                                    .with_scale_labels(true)
                                                                    .with_range(true)
                                                                    .with_pulse_dot(true),
                                                                CardMetricType::Disk => SparklineChart::new(disk_history, DarkTechTheme::status_warn())
                                                                    .with_value(if metric.is_some_and(|m| !m.disks.is_empty()) { format!("{:.0}%", disk_val) } else { t!("fleet.not_collected").into() })
                                                                    .with_scale_labels(true)
                                                                    .with_range(true)
                                                                    .with_pulse_dot(true),
                                                                CardMetricType::All => SparklineChart::multi(vec![
                                                                    SparklineSeries::new("CPU", cpu_history, DarkTechTheme::accent_cyan()),
                                                                    SparklineSeries::new("RAM", mem_history, DarkTechTheme::accent_indigo()),
                                                                    SparklineSeries::new("DISK", disk_history, DarkTechTheme::status_warn()),
                                                                ])
                                                                .with_value(if metric.is_some() { format!("{:.0}%/{:.0}%/{:.0}%", cpu_val, mem_val, disk_val) } else { t!("fleet.not_collected").into() })
                                                                .with_scale_labels(true)
                                                                .with_pulse_dot(true),
                                                            }),
                                                    ),
                                            )
                                    })
                                    .child(div().text_xs().text_color(DarkTechTheme::text_muted())
                                        .child(if metric.is_some_and(|m| m.net_available) { t_fmt!("fleet.traffic_total", gb = format!("{net_used_gb:.1}")) } else { t!("fleet.traffic_not_collected").into() })),
                            )
                            // Row 4: Card Action Toolbar (Terminal, SFTP, Edit, Clone, Move Up, Move Down, Delete)
                            .child(
                                div()
                                    .w_full()
                                    .pt_1()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    // Terminal Button
                                    .child(
                                        div()
                                            .id(ElementId::Name(format!("btn_term_{}", host_id.0).into()))
                                            .flex_1()
                                            .h(px(24.0))
                                            .rounded_md()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_1()
                                            .border_color(DarkTechTheme::border_default())
                                            .text_color(DarkTechTheme::text_primary())
                                            .text_size(px(10.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()).border_color(DarkTechTheme::border_active()))
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _event: &ClickEvent, window, cx| {
                                                if let Some(cb) = &this.on_action {
                                                    cb(FleetAction::OpenTerminal(host_for_term.clone()), window, cx);
                                                }
                                            }))
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_1()
                                                    .child(
                                                        Icon::terminal()
                                                            .with_size(px(11.0))
                                                            .with_color(DarkTechTheme::text_primary()),
                                                    )
                                                    .child(crate::t!("fleet.open_terminal")),
                                            ),
                                    )
                                    // SFTP Button
                                    .child(
                                        div()
                                            .id(ElementId::Name(format!("btn_sftp_{}", host_id.0).into()))
                                            .flex_1()
                                            .h(px(24.0))
                                            .rounded_md()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_1()
                                            .border_color(DarkTechTheme::border_default())
                                            .text_color(DarkTechTheme::text_primary())
                                            .text_size(px(10.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()).border_color(DarkTechTheme::border_active()))
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _event: &ClickEvent, window, cx| {
                                                if let Some(cb) = &this.on_action {
                                                    cb(FleetAction::OpenSftp(host_for_sftp.clone()), window, cx);
                                                }
                                            }))
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_1()
                                                    .child(
                                                        Icon::folder()
                                                            .with_size(px(11.0))
                                                            .with_color(DarkTechTheme::text_primary()),
                                                    )
                                                    .child("SFTP"),
                                            ),
                                    )
                                    // Edit Button
                                    .child(
                                        div()
                                            .id(ElementId::Name(format!("btn_edit_{}", host_id.0).into()))
                                            .w(px(24.0))
                                            .h(px(24.0))
                                            .rounded_md()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_1()
                                            .border_color(DarkTechTheme::border_default())
                                            .text_color(DarkTechTheme::text_secondary())
                                            .text_size(px(11.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()).text_color(DarkTechTheme::accent_indigo()))
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _event: &ClickEvent, window, cx| {
                                                if let Some(cb) = &this.on_action {
                                                    cb(FleetAction::EditHost(host_for_edit.clone()), window, cx);
                                                }
                                            }))
                                            .child(
                                                Icon::edit()
                                                    .with_size(px(11.0))
                                                    .with_color(DarkTechTheme::text_secondary()),
                                            ),
                                    )
                                    // Clone Button
                                    .child(
                                        div()
                                            .id(ElementId::Name(format!("btn_clone_{}", host_id.0).into()))
                                            .w(px(24.0))
                                            .h(px(24.0))
                                            .rounded_md()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_1()
                                            .border_color(DarkTechTheme::border_default())
                                            .text_color(DarkTechTheme::text_secondary())
                                            .text_size(px(11.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()).text_color(DarkTechTheme::accent_cyan()))
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _event: &ClickEvent, window, cx| {
                                                if let Some(cb) = &this.on_action {
                                                    cb(FleetAction::CloneHost(host_for_clone.clone()), window, cx);
                                                }
                                            }))
                                            .child(
                                                Icon::copy()
                                                    .with_size(px(11.0))
                                                    .with_color(DarkTechTheme::text_secondary()),
                                            ),
                                    )
                                    // Move Up Button ▲
                                    .child(
                                        div()
                                            .id(ElementId::Name(format!("btn_move_up_{}", host_id.0).into()))
                                            .w(px(22.0))
                                            .h(px(24.0))
                                            .rounded_md()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_1()
                                            .border_color(DarkTechTheme::border_default())
                                            .text_color(DarkTechTheme::text_muted())
                                            .text_size(px(10.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()).text_color(DarkTechTheme::accent_cyan()))
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _event: &ClickEvent, window, cx| {
                                                this.move_host(&host_id_for_move_up, true, cx);
                                                if let Some(cb) = &this.on_action {
                                                    cb(FleetAction::MoveHostUp(host_id_for_move_up.clone()), window, cx);
                                                }
                                            }))
                                            .child(
                                                Icon::arrow_up()
                                                    .with_size(px(10.0))
                                                    .with_color(DarkTechTheme::text_muted()),
                                            ),
                                    )
                                    // Move Down Button ▼
                                    .child(
                                        div()
                                            .id(ElementId::Name(format!("btn_move_down_{}", host_id.0).into()))
                                            .w(px(22.0))
                                            .h(px(24.0))
                                            .rounded_md()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_1()
                                            .border_color(DarkTechTheme::border_default())
                                            .text_color(DarkTechTheme::text_muted())
                                            .text_size(px(10.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()).text_color(DarkTechTheme::accent_cyan()))
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _event: &ClickEvent, window, cx| {
                                                this.move_host(&host_id_for_move_down, false, cx);
                                                if let Some(cb) = &this.on_action {
                                                    cb(FleetAction::MoveHostDown(host_id_for_move_down.clone()), window, cx);
                                                }
                                            }))
                                            .child(
                                                Icon::arrow_down()
                                                    .with_size(px(10.0))
                                                    .with_color(DarkTechTheme::text_muted()),
                                            ),
                                    )
                                    // Delete Button
                                    .child(
                                        div()
                                            .id(ElementId::Name(format!("btn_delete_{}", host_id.0).into()))
                                            .w(px(24.0))
                                            .h(px(24.0))
                                            .rounded_md()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_1()
                                            .border_color(DarkTechTheme::border_default())
                                            .text_color(DarkTechTheme::text_muted())
                                            .text_size(px(11.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()).text_color(DarkTechTheme::status_crit()))
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _event: &ClickEvent, window, cx| {
                                                this.hosts.retain(|h| h.id != host_id_for_delete);
                                                this.selected_hosts.remove(&host_id_for_delete);
                                                cx.notify();
                                                if let Some(cb) = &this.on_action {
                                                    cb(FleetAction::DeleteHost(host_id_for_delete.clone()), window, cx);
                                                }
                                            }))
                                            .child(
                                                Icon::trash()
                                                    .with_size(px(11.0))
                                                    .with_color(DarkTechTheme::text_muted()),
                                            ),
                                    ),
                            )
                    }))
                )
            )
            // 3. Floating Bottom Action Bar when hosts are selected
            .children(if selected_count > 0 {
                let selected_hosts_list: Vec<HostConfig> = self
                    .hosts
                    .iter()
                    .filter(|h| self.selected_hosts.contains(&h.id))
                    .cloned()
                    .collect();
                let selected_ids: Vec<HostId> = selected_hosts_list.iter().map(|h| h.id.clone()).collect();

                Some(
                    div()
                        .w_full()
                        .h(px(48.0))
                        .flex_shrink_0()
                        .bg(DarkTechTheme::bg_panel())
                        .border_1()
                        .border_color(DarkTechTheme::border_active())
                        .rounded_lg()
                        .px_4()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(DarkTechTheme::text_primary())
                                .font_weight(FontWeight::BOLD)
                                .child(t_fmt!("fleet.selected_hosts", count = selected_count)),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                // Danger Batch Delete Button
                                .child({
                                    let ids_for_delete = selected_ids.clone();
                                    div()
                                        .id("btn_batch_delete")
                                        .px_3()
                                        .py_1p5()
                                        .bg(DarkTechTheme::status_crit())
                                        .text_color(DarkTechTheme::text_primary())
                                        .rounded_md()
                                        .font_weight(FontWeight::BOLD)
                                        .text_size(px(12.0))
                                        .cursor_pointer()
                                        .hover(|s| s.bg(rgb(0xdc2626)))
                                        .on_click(cx.listener(
                                            move |this, _event: &ClickEvent, window, cx| {
                                                if let Some(cb) = &this.on_action {
                                                    cb(
                                                        FleetAction::DeleteBatch(
                                                            ids_for_delete.clone(),
                                                        ),
                                                        window,
                                                        cx,
                                                    );
                                                }
                                            },
                                        ))
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_1p5()
                                                .child(
                                                    Icon::trash()
                                                        .with_size(px(12.0))
                                                        .with_color(DarkTechTheme::text_primary()),
                                                )
                                                .child(t_fmt!("fleet.batch_delete_selected", count = selected_count)),
                                        )
                                })
                                // Batch Run Command Button
                                .child(
                                    div()
                                        .id("btn_batch_run")
                                        .px_4()
                                        .py_1p5()
                                        .bg(DarkTechTheme::border_active())
                                        .text_color(DarkTechTheme::bg_root())
                                        .rounded_md()
                                        .font_weight(FontWeight::BOLD)
                                        .text_size(px(12.0))
                                        .hover(|s| s.bg(DarkTechTheme::accent_cyan()))
                                        .cursor_pointer()
                                        .on_click(cx.listener(
                                            move |this, _event: &ClickEvent, window, cx| {
                                                if let Some(cb) = &this.on_action {
                                                    cb(
                                                        FleetAction::OpenBatch(
                                                            selected_hosts_list.clone(),
                                                        ),
                                                        window,
                                                        cx,
                                                    );
                                                }
                                            },
                                        ))
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_1p5()
                                                .child(
                                                    Icon::zap()
                                                        .with_size(px(12.0))
                                                        .with_color(DarkTechTheme::bg_root()),
                                                )
                                                .child(crate::t!("fleet.batch_exec")),
                                        ),
                                ),
                        ),
                )
            } else {
                None
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn missing_credentials_have_actionable_summary_and_separate_details() {
        let error =
            anyhow::Error::new(MissingCredential).context("credential_id: internal-test-id");
        let failure = ProbeFailure::from_error(&error);
        assert!(failure.needs_credentials);
        assert!(!failure.message.contains("internal-test-id"));
        assert!(failure.details.contains("internal-test-id"));
        assert!(
            !ProbeFailure::from_error(&anyhow::anyhow!("Connection timed out")).needs_credentials
        );
    }

    #[core::prelude::v1::test]
    fn test_fleet_view_set_hosts() {
        let h1 = HostConfig::new("h1", "10.0.0.1", "root");
        let h2 = HostConfig::new("h2", "10.0.0.2", "root");
        let id1 = h1.id.clone();
        let id2 = h2.id.clone();

        let mut fleet = FleetView::new(vec![h1.clone(), h2.clone()]);
        fleet.selected_hosts.insert(id1.clone());
        fleet.selected_hosts.insert(id2.clone());
        assert_eq!(fleet.selected_hosts.len(), 2);

        // Update hosts keeping only h1
        let valid_ids: HashSet<HostId> = [h1.clone()].iter().map(|h| h.id.clone()).collect();
        fleet.hosts = vec![h1];
        fleet.selected_hosts.retain(|id| valid_ids.contains(id));

        assert_eq!(fleet.hosts.len(), 1);
        assert_eq!(fleet.selected_hosts.len(), 1);
        assert!(fleet.selected_hosts.contains(&id1));
        assert!(!fleet.selected_hosts.contains(&id2));
    }

    #[core::prelude::v1::test]
    fn test_fleet_view_selection_logic() {
        let h1 = HostConfig::new("h1", "10.0.0.1", "root");
        let h2 = HostConfig::new("h2", "10.0.0.2", "root");
        let id1 = h1.id.clone();
        let id2 = h2.id.clone();

        let mut fleet = FleetView::new(vec![h1, h2]);
        let filtered_ids = vec![id1.clone(), id2.clone()];

        // Select all
        let all_filtered_selected = !filtered_ids.is_empty()
            && filtered_ids
                .iter()
                .all(|id| fleet.selected_hosts.contains(id));
        if !all_filtered_selected {
            for id in &filtered_ids {
                fleet.selected_hosts.insert(id.clone());
            }
        }
        assert_eq!(fleet.selected_hosts.len(), 2);

        // Deselect all
        let all_filtered_selected2 = !filtered_ids.is_empty()
            && filtered_ids
                .iter()
                .all(|id| fleet.selected_hosts.contains(id));
        if all_filtered_selected2 {
            for id in &filtered_ids {
                fleet.selected_hosts.remove(id);
            }
        }
        assert_eq!(fleet.selected_hosts.len(), 0);
    }

    #[core::prelude::v1::test]
    fn test_fleet_view_reordering_and_sorting() {
        let mut h_a = HostConfig::new("Alpha", "10.0.0.1", "root");
        h_a.group = "Backend".to_string();
        let mut h_b = HostConfig::new("Bravo", "10.0.0.2", "root");
        h_b.group = "Frontend".to_string();
        let mut h_c = HostConfig::new("Charlie", "10.0.0.3", "root");
        h_c.group = "Backend".to_string();

        let id_a = h_a.id.clone();
        let id_b = h_b.id.clone();
        let id_c = h_c.id.clone();

        let mut fleet = FleetView::new(vec![h_a, h_b, h_c]);

        // 1. Move Charlie up (from index 2 to 1)
        assert!(fleet.move_host_internal(&id_c, true));
        assert_eq!(fleet.hosts[0].id, id_a);
        assert_eq!(fleet.hosts[1].id, id_c);
        assert_eq!(fleet.hosts[2].id, id_b);

        // 2. Move Alpha down (from index 0 to 1)
        assert!(fleet.move_host_internal(&id_a, false));
        assert_eq!(fleet.hosts[0].id, id_c);
        assert_eq!(fleet.hosts[1].id, id_a);
        assert_eq!(fleet.hosts[2].id, id_b);

        // 3. Move Bravo to top
        assert!(fleet.move_host_to_top_internal(&id_b));
        assert_eq!(fleet.hosts[0].id, id_b);
        assert_eq!(fleet.hosts[1].id, id_c);
        assert_eq!(fleet.hosts[2].id, id_a);

        // 4. Sort by name A-Z
        let sorted_ids = fleet.sort_by_name_internal();
        assert_eq!(sorted_ids, vec![id_a.clone(), id_b.clone(), id_c.clone()]);
        assert_eq!(fleet.hosts[0].name, "Alpha");
        assert_eq!(fleet.hosts[1].name, "Bravo");
        assert_eq!(fleet.hosts[2].name, "Charlie");

        // 5. Sort by group
        let group_sorted = fleet.sort_by_group_internal();
        // Backend (Alpha, Charlie) then Frontend (Bravo)
        assert_eq!(group_sorted, vec![id_a.clone(), id_c.clone(), id_b.clone()]);

        // 6. Drag & Drop Reordering
        // State after group sort: Alpha (0), Charlie (1), Bravo (2)
        // Drag Bravo (2) and drop onto Alpha (0)
        assert!(fleet.reorder_drag_drop_internal(&id_b, &id_a));
        assert_eq!(fleet.hosts[0].id, id_b);
        assert_eq!(fleet.hosts[1].id, id_a);
        assert_eq!(fleet.hosts[2].id, id_c);

        // Drag Bravo (0) and drop onto Charlie (2)
        assert!(fleet.reorder_drag_drop_internal(&id_b, &id_c));
        assert_eq!(fleet.hosts[0].id, id_a);
        assert_eq!(fleet.hosts[1].id, id_c);
        assert_eq!(fleet.hosts[2].id, id_b);

        // Dragging onto itself should be a no-op
        assert!(!fleet.reorder_drag_drop_internal(&id_a, &id_a));
    }

    #[core::prelude::v1::test]
    fn test_fleet_view_multi_metric_tracking_and_switching() {
        let h1 = HostConfig::new("H1", "10.0.0.1", "root");
        let id1 = h1.id.clone();
        let mut fleet = FleetView::new(vec![h1]);

        assert_eq!(fleet.active_card_metrics.get(&id1), None);

        // Switch to Memory
        fleet
            .active_card_metrics
            .insert(id1.clone(), CardMetricType::Memory);
        assert_eq!(
            fleet.active_card_metrics.get(&id1),
            Some(&CardMetricType::Memory)
        );

        // Switch to Disk
        fleet
            .active_card_metrics
            .insert(id1.clone(), CardMetricType::Disk);
        assert_eq!(
            fleet.active_card_metrics.get(&id1),
            Some(&CardMetricType::Disk)
        );

        // Switch to All
        fleet
            .active_card_metrics
            .insert(id1.clone(), CardMetricType::All);
        assert_eq!(
            fleet.active_card_metrics.get(&id1),
            Some(&CardMetricType::All)
        );

        // Test ring buffer limits for all three metrics
        let cpu_hist = fleet.cpu_histories.entry(id1.clone()).or_default();
        for i in 0..35 {
            while cpu_hist.len() >= 30 {
                cpu_hist.remove(0);
            }
            cpu_hist.push(i as f32);
        }
        assert_eq!(cpu_hist.len(), 30);
        assert_eq!(cpu_hist[0], 5.0);
        assert_eq!(cpu_hist[29], 34.0);

        let mem_hist = fleet.mem_histories.entry(id1.clone()).or_default();
        for i in 0..35 {
            while mem_hist.len() >= 30 {
                mem_hist.remove(0);
            }
            mem_hist.push(i as f32 * 2.0);
        }
        assert_eq!(mem_hist.len(), 30);
        assert_eq!(mem_hist[0], 10.0);
        assert_eq!(mem_hist[29], 68.0);

        let disk_hist = fleet.disk_histories.entry(id1.clone()).or_default();
        for i in 0..35 {
            while disk_hist.len() >= 30 {
                disk_hist.remove(0);
            }
            disk_hist.push(i as f32 + 10.0);
        }
        assert_eq!(disk_hist.len(), 30);
        assert_eq!(disk_hist[0], 15.0);
        assert_eq!(disk_hist[29], 44.0);
    }
}
