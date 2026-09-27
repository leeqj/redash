use gpui::prelude::FluentBuilder;
use gpui::*;
use std::sync::Arc;
use std::time::Duration;

use crate::components::icon::Icon;
use crate::components::theme::DarkTechTheme;
use redash_core::config::HostConfig;
use redash_core::probe::docker::{DockerContainerDetail, DockerManager};
use redash_core::session::SessionManager;
use redash_types::formatters::format_bytes;

pub struct DockerPanel {
    pub host: HostConfig,
    pub session_mgr: Arc<SessionManager>,
    pub containers: Vec<DockerContainerDetail>,
    pub filter_query: String,
    pub selected_log_container: Option<(String, String)>,
    pub log_content: Option<String>,
    pub is_loading: bool,
    pub status_message: Option<(String, bool)>,
    pub scroll_handle: ScrollHandle,
}

impl DockerPanel {
    pub fn new(host: HostConfig, session_mgr: Arc<SessionManager>) -> Self {
        Self {
            host,
            session_mgr,
            containers: Vec::new(),
            filter_query: String::new(),
            selected_log_container: None,
            log_content: None,
            is_loading: false,
            status_message: None,
            scroll_handle: ScrollHandle::new(),
        }
    }

    pub fn set_containers(
        &mut self,
        containers: Vec<DockerContainerDetail>,
        cx: &mut Context<Self>,
    ) {
        self.containers = containers;
        cx.notify();
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.is_loading = true;
        cx.notify();

        let session_mgr = Arc::clone(&self.session_mgr);
        let host = self.host.clone();

        cx.spawn(async move |this, cx| {
            let timeout = Duration::from_secs(12);
            let ps_res = session_mgr
                .exec(&host, DockerManager::list_containers_cmd(), timeout)
                .await;
            let stats_res = session_mgr
                .exec(&host, DockerManager::stats_cmd(), timeout)
                .await;

            let ps_out = ps_res.map(|r| r.stdout).unwrap_or_default();
            let stats_out = stats_res.map(|r| r.stdout).unwrap_or_default();

            let containers = DockerManager::parse_containers(&ps_out, &stats_out);

            let _ = this.update(cx, |view, cx| {
                view.is_loading = false;
                view.containers = containers;
                cx.notify();
            });
        })
        .detach();
    }

    pub fn start_container(&mut self, id: String, cx: &mut Context<Self>) {
        self.is_loading = true;
        let session_mgr = Arc::clone(&self.session_mgr);
        let host = self.host.clone();
        let cmd = DockerManager::start_cmd(&id);

        cx.spawn(async move |this, cx| {
            let res = session_mgr.exec(&host, &cmd, Duration::from_secs(15)).await;
            let (msg, ok) = match res {
                Ok(r) if r.exit_code == 0 => {
                    (format!("容器 {} 启动成功", &id[..id.len().min(12)]), true)
                }
                Ok(r) => (
                    format!("启动失败 (code {}): {}", r.exit_code, r.stderr.trim()),
                    false,
                ),
                Err(e) => (format!("执行错误: {}", e), false),
            };

            let _ = this.update(cx, |view, cx| {
                view.status_message = Some((msg, ok));
                view.refresh(cx);
            });
        })
        .detach();
    }

    pub fn stop_container(&mut self, id: String, cx: &mut Context<Self>) {
        self.is_loading = true;
        let session_mgr = Arc::clone(&self.session_mgr);
        let host = self.host.clone();
        let cmd = DockerManager::stop_cmd(&id);

        cx.spawn(async move |this, cx| {
            let res = session_mgr.exec(&host, &cmd, Duration::from_secs(20)).await;
            let (msg, ok) = match res {
                Ok(r) if r.exit_code == 0 => {
                    (format!("容器 {} 已停止", &id[..id.len().min(12)]), true)
                }
                Ok(r) => (
                    format!("停止失败 (code {}): {}", r.exit_code, r.stderr.trim()),
                    false,
                ),
                Err(e) => (format!("执行错误: {}", e), false),
            };

            let _ = this.update(cx, |view, cx| {
                view.status_message = Some((msg, ok));
                view.refresh(cx);
            });
        })
        .detach();
    }

    pub fn restart_container(&mut self, id: String, cx: &mut Context<Self>) {
        self.is_loading = true;
        let session_mgr = Arc::clone(&self.session_mgr);
        let host = self.host.clone();
        let cmd = DockerManager::restart_cmd(&id);

        cx.spawn(async move |this, cx| {
            let res = session_mgr.exec(&host, &cmd, Duration::from_secs(25)).await;
            let (msg, ok) = match res {
                Ok(r) if r.exit_code == 0 => {
                    (format!("容器 {} 重启完成", &id[..id.len().min(12)]), true)
                }
                Ok(r) => (
                    format!("重启失败 (code {}): {}", r.exit_code, r.stderr.trim()),
                    false,
                ),
                Err(e) => (format!("执行错误: {}", e), false),
            };

            let _ = this.update(cx, |view, cx| {
                view.status_message = Some((msg, ok));
                view.refresh(cx);
            });
        })
        .detach();
    }

    pub fn rm_container(&mut self, id: String, cx: &mut Context<Self>) {
        self.is_loading = true;
        let session_mgr = Arc::clone(&self.session_mgr);
        let host = self.host.clone();
        let cmd = DockerManager::rm_cmd(&id);

        cx.spawn(async move |this, cx| {
            let res = session_mgr.exec(&host, &cmd, Duration::from_secs(15)).await;
            let (msg, ok) = match res {
                Ok(r) if r.exit_code == 0 => {
                    (format!("容器 {} 已删除", &id[..id.len().min(12)]), true)
                }
                Ok(r) => (
                    format!("删除失败 (code {}): {}", r.exit_code, r.stderr.trim()),
                    false,
                ),
                Err(e) => (format!("执行错误: {}", e), false),
            };

            let _ = this.update(cx, |view, cx| {
                view.status_message = Some((msg, ok));
                view.refresh(cx);
            });
        })
        .detach();
    }

    pub fn view_logs(&mut self, id: String, name: String, cx: &mut Context<Self>) {
        self.is_loading = true;
        self.selected_log_container = Some((id.clone(), name));
        self.log_content = Some("正在读取容器最新日志...".to_string());
        cx.notify();

        let session_mgr = Arc::clone(&self.session_mgr);
        let host = self.host.clone();
        let cmd = DockerManager::logs_cmd(&id, 200);

        cx.spawn(async move |this, cx| {
            let res = session_mgr.exec(&host, &cmd, Duration::from_secs(15)).await;
            let content = match res {
                Ok(r) => {
                    if r.stdout.is_empty() && !r.stderr.is_empty() {
                        r.stderr
                    } else if r.stdout.is_empty() {
                        "（该容器无最新标准输出日志）".to_string()
                    } else {
                        r.stdout
                    }
                }
                Err(e) => format!("获取日志异常: {}", e),
            };

            let _ = this.update(cx, |view, cx| {
                view.is_loading = false;
                view.log_content = Some(content);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn close_logs(&mut self, cx: &mut Context<Self>) {
        self.selected_log_container = None;
        self.log_content = None;
        cx.notify();
    }
}

impl Render for DockerPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let q = self.filter_query.to_lowercase().trim().to_string();
        let filtered_containers: Vec<DockerContainerDetail> = self
            .containers
            .iter()
            .filter(|c| {
                if q.is_empty() {
                    true
                } else {
                    c.name.to_lowercase().contains(&q)
                        || c.image.to_lowercase().contains(&q)
                        || c.id.to_lowercase().contains(&q)
                        || c.state.to_lowercase().contains(&q)
                        || c.status.to_lowercase().contains(&q)
                }
            })
            .cloned()
            .collect();

        let total_count = filtered_containers.len();
        let row_elements: Vec<_> = if filtered_containers.is_empty() {
            vec![
                div()
                    .id("docker_empty_placeholder")
                    .p_8()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .child(
                        Icon::docker()
                            .with_size(px(32.0))
                            .with_color(DarkTechTheme::text_muted()),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(DarkTechTheme::text_muted())
                            .child(if self.is_loading {
                                "正在检索 Docker 容器列表..."
                            } else {
                                crate::t!("docker.no_containers")
                            }),
                    ),
            ]
        } else {
            filtered_containers
                .into_iter()
                .enumerate()
                .map(|(idx, c)| {
                    let c_id = c.id.clone();
                    let c_id_short = if c_id.len() >= 12 { &c_id[..12] } else { &c_id }.to_string();
                    let c_name = c.name.clone();
                    let state_lower = c.state.to_ascii_lowercase();

                    let (led_color, state_label) = if state_lower.contains("run") {
                        (DarkTechTheme::status_online(), crate::t!("common.running"))
                    } else if state_lower.contains("exit") {
                        (DarkTechTheme::status_crit(), crate::t!("common.stopped"))
                    } else if state_lower.contains("pause") {
                        (DarkTechTheme::status_warn(), "已暂停")
                    } else {
                        (DarkTechTheme::status_offline(), "未知")
                    };

                    let is_running = state_lower.contains("run");
                    let ports_str = if c.ports.is_empty() {
                        "-".to_string()
                    } else {
                        c.ports.join(", ")
                    };

                    let mem_str = if c.mem_limit_bytes > 0 {
                        format!(
                            "{}/{}",
                            format_bytes(c.mem_usage_bytes),
                            format_bytes(c.mem_limit_bytes)
                        )
                    } else if c.mem_usage_bytes > 0 {
                        format_bytes(c.mem_usage_bytes)
                    } else {
                        "-".to_string()
                    };

                    let id_for_start = c.id.clone();
                    let id_for_stop = c.id.clone();
                    let id_for_restart = c.id.clone();
                    let id_for_logs = c.id.clone();
                    let name_for_logs = c.name.clone();
                    let id_for_rm = c.id.clone();

                    div()
                        .id(ElementId::NamedInteger("docker_row".into(), idx as u64))
                        .h(px(48.0))
                        .w_full()
                        .px_3()
                        .border_b_1()
                        .border_color(DarkTechTheme::border_muted())
                        .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()))
                        .flex()
                        .flex_row()
                        .items_center()
                        .text_size(px(12.0))
                        // State LED
                        .child(
                            div()
                                .w(px(80.0))
                                .flex_shrink_0()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1p5()
                                .child(div().size(px(8.0)).rounded_full().bg(led_color))
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(led_color)
                                        .child(state_label),
                                ),
                        )
                        // Name & ID
                        .child(
                            div()
                                .w(px(220.0))
                                .flex_shrink_0()
                                .overflow_hidden()
                                .flex()
                                .flex_col()
                                .justify_center()
                                .gap_0p5()
                                .child(
                                    div()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(DarkTechTheme::text_primary())
                                        .truncate()
                                        .child(c_name),
                                )
                                .child(
                                    div()
                                        .font_family("Menlo")
                                        .text_size(px(10.0))
                                        .text_color(DarkTechTheme::text_muted())
                                        .truncate()
                                        .child(c_id_short),
                                ),
                        )
                        // Image
                        .child(
                            div()
                                .w(px(200.0))
                                .flex_shrink_0()
                                .font_family("Menlo")
                                .text_size(px(11.0))
                                .text_color(DarkTechTheme::text_secondary())
                                .truncate()
                                .child(c.image.clone()),
                        )
                        // Ports
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(120.0))
                                .font_family("Menlo")
                                .text_size(px(11.0))
                                .text_color(DarkTechTheme::text_muted())
                                .truncate()
                                .child(ports_str),
                        )
                        // CPU%
                        .child(
                            div()
                                .w(px(70.0))
                                .flex_shrink_0()
                                .text_align(TextAlign::Right)
                                .font_family("Menlo")
                                .text_color(if c.cpu_percent > 80.0 {
                                    DarkTechTheme::status_crit()
                                } else if c.cpu_percent > 40.0 {
                                    DarkTechTheme::status_warn()
                                } else {
                                    DarkTechTheme::text_primary()
                                })
                                .child(format!("{:.1}%", c.cpu_percent)),
                        )
                        // Memory
                        .child(
                            div()
                                .w(px(140.0))
                                .flex_shrink_0()
                                .text_align(TextAlign::Right)
                                .font_family("Menlo")
                                .text_size(px(11.0))
                                .text_color(DarkTechTheme::text_secondary())
                                .truncate()
                                .child(mem_str),
                        )
                        // Action Buttons
                        .child(
                            div()
                                .w(px(190.0))
                                .flex_shrink_0()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_end()
                                .gap_1()
                                .when(!is_running, |d| {
                                    d.child(
                                        div()
                                            .id(ElementId::Name(
                                                format!("btn_start_{}", id_for_start).into(),
                                            ))
                                            .px_2()
                                            .py_0p5()
                                            .rounded_md()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_1()
                                            .border_color(DarkTechTheme::status_online())
                                            .text_color(DarkTechTheme::status_online())
                                            .text_size(px(11.0))
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.start_container(id_for_start.clone(), cx);
                                            }))
                                            .child(
                                                Icon::play()
                                                    .with_size(px(10.0))
                                                    .with_color(DarkTechTheme::status_online()),
                                            ),
                                    )
                                })
                                .when(is_running, |d| {
                                    d.child(
                                        div()
                                            .id(ElementId::Name(
                                                format!("btn_stop_{}", id_for_stop).into(),
                                            ))
                                            .px_2()
                                            .py_0p5()
                                            .rounded_md()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_1()
                                            .border_color(DarkTechTheme::status_warn())
                                            .text_color(DarkTechTheme::status_warn())
                                            .text_size(px(11.0))
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.stop_container(id_for_stop.clone(), cx);
                                            }))
                                            .child(
                                                Icon::stop()
                                                    .with_size(px(10.0))
                                                    .with_color(DarkTechTheme::status_warn()),
                                            ),
                                    )
                                })
                                // Restart button
                                .child(
                                    div()
                                        .id(ElementId::Name(
                                            format!("btn_restart_{}", id_for_restart).into(),
                                        ))
                                        .px_2()
                                        .py_0p5()
                                        .rounded_md()
                                        .bg(DarkTechTheme::bg_input())
                                        .border_1()
                                        .border_color(DarkTechTheme::border_default())
                                        .text_color(DarkTechTheme::text_secondary())
                                        .text_size(px(11.0))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.restart_container(id_for_restart.clone(), cx);
                                        }))
                                        .child(
                                            Icon::refresh()
                                                .with_size(px(10.0))
                                                .with_color(DarkTechTheme::text_secondary()),
                                        ),
                                )
                                // View Logs button
                                .child(
                                    div()
                                        .id(ElementId::Name(
                                            format!("btn_logs_{}", id_for_logs).into(),
                                        ))
                                        .px_2()
                                        .py_0p5()
                                        .rounded_md()
                                        .bg(DarkTechTheme::bg_input())
                                        .border_1()
                                        .border_color(DarkTechTheme::border_active())
                                        .text_color(DarkTechTheme::text_accent())
                                        .text_size(px(11.0))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.view_logs(
                                                id_for_logs.clone(),
                                                name_for_logs.clone(),
                                                cx,
                                            );
                                        }))
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_1()
                                                .child(
                                                    Icon::file()
                                                        .with_size(px(10.0))
                                                        .with_color(DarkTechTheme::accent_cyan()),
                                                )
                                                .child(crate::t!("docker.logs")),
                                        ),
                                )
                                // RM button
                                .child(
                                    div()
                                        .id(ElementId::Name(format!("btn_rm_{}", id_for_rm).into()))
                                        .px_2()
                                        .py_0p5()
                                        .rounded_md()
                                        .bg(DarkTechTheme::bg_input())
                                        .border_1()
                                        .border_color(DarkTechTheme::border_default())
                                        .text_color(DarkTechTheme::status_crit())
                                        .text_size(px(11.0))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.rm_container(id_for_rm.clone(), cx);
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
            .bg(DarkTechTheme::bg_root())
            .flex()
            .flex_col()
            .overflow_hidden()
            .p_4()
            .gap_3()
            // Top Toolbar: Search, Filters, Refresh
            .child(
                div()
                    .flex_shrink_0()
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    // Search and filter pills
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            // Search Box Container
                            .child(
                                div()
                                    .id("docker_search_container")
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
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(DarkTechTheme::text_muted())
                                            .child(
                                                Icon::search()
                                                    .with_size(px(12.0))
                                                    .with_color(DarkTechTheme::text_muted()),
                                            ),
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
                                                crate::t!("docker.search").to_string()
                                            } else {
                                                self.filter_query.clone()
                                            }),
                                    ),
                            )
                            // Filter Pills
                            .child(
                                div()
                                    .id("docker_filter_all")
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
                                    .id("docker_filter_running")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(if self.filter_query == "running" {
                                        DarkTechTheme::bg_panel_hover()
                                    } else {
                                        DarkTechTheme::bg_panel()
                                    })
                                    .border_1()
                                    .border_color(if self.filter_query == "running" {
                                        DarkTechTheme::border_active()
                                    } else {
                                        DarkTechTheme::border_default()
                                    })
                                    .text_color(if self.filter_query == "running" {
                                        DarkTechTheme::text_accent()
                                    } else {
                                        DarkTechTheme::text_secondary()
                                    })
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.filter_query = "running".to_string();
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .size(px(6.0))
                                            .rounded_full()
                                            .bg(DarkTechTheme::status_online()),
                                    )
                                    .child(crate::t!("common.running")),
                            )
                            .child(
                                div()
                                    .id("docker_filter_exited")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(if self.filter_query == "exited" {
                                        DarkTechTheme::bg_panel_hover()
                                    } else {
                                        DarkTechTheme::bg_panel()
                                    })
                                    .border_1()
                                    .border_color(if self.filter_query == "exited" {
                                        DarkTechTheme::border_active()
                                    } else {
                                        DarkTechTheme::border_default()
                                    })
                                    .text_color(if self.filter_query == "exited" {
                                        DarkTechTheme::text_accent()
                                    } else {
                                        DarkTechTheme::text_secondary()
                                    })
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.filter_query = "exited".to_string();
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .size(px(6.0))
                                            .rounded_full()
                                            .bg(DarkTechTheme::status_crit()),
                                    )
                                    .child(crate::t!("common.stopped")),
                            )
                            .when(!self.filter_query.is_empty(), |d| {
                                d.child(
                                    div()
                                        .id("docker_filter_clear")
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
                    // Refresh Button & Container counter
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(DarkTechTheme::text_secondary())
                                    .child(format!("共 {} 个容器", total_count)),
                            )
                            .child(
                                div()
                                    .id("btn_refresh_docker")
                                    .px_3()
                                    .py_1p5()
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_panel())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_default())
                                    .text_color(DarkTechTheme::text_primary())
                                    .text_size(px(12.0))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.refresh(cx);
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
                                            .child(if self.is_loading {
                                                crate::t!("common.loading")
                                            } else {
                                                crate::t!("common.refresh")
                                            }),
                                    ),
                            ),
                    ),
            )
            // Status Message Banner
            .when_some(self.status_message.as_ref(), |d, (msg, is_success)| {
                let is_ok = *is_success;
                let msg_cloned = msg.clone();
                d.child(
                    div()
                        .id("docker_status_banner")
                        .flex_shrink_0()
                        .w_full()
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
                                .text_size(px(12.0))
                                .text_color(if is_ok {
                                    DarkTechTheme::status_online()
                                } else {
                                    DarkTechTheme::status_crit()
                                })
                                .child(msg_cloned),
                        )
                        .child(
                            div()
                                .id("btn_dismiss_docker_status")
                                .cursor_pointer()
                                .text_size(px(11.0))
                                .text_color(DarkTechTheme::text_muted())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.status_message = None;
                                    cx.notify();
                                }))
                                .child(
                                    Icon::close()
                                        .with_size(px(9.0))
                                        .with_color(DarkTechTheme::text_muted()),
                                ),
                        ),
                )
            })
            // Container Table / List Container
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
                                    .child(crate::t!("docker.col_status")),
                            )
                            .child(
                                div()
                                    .w(px(220.0))
                                    .flex_shrink_0()
                                    .child(crate::t!("docker.col_name")),
                            )
                            .child(
                                div()
                                    .w(px(200.0))
                                    .flex_shrink_0()
                                    .child(crate::t!("docker.col_image")),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(120.0))
                                    .child(crate::t!("docker.col_ports")),
                            )
                            .child(
                                div()
                                    .w(px(70.0))
                                    .flex_shrink_0()
                                    .text_align(TextAlign::Right)
                                    .child("CPU%"),
                            )
                            .child(
                                div()
                                    .w(px(140.0))
                                    .flex_shrink_0()
                                    .text_align(TextAlign::Right)
                                    .child(crate::t!("docker.col_mem")),
                            )
                            .child(
                                div()
                                    .w(px(190.0))
                                    .flex_shrink_0()
                                    .text_align(TextAlign::Right)
                                    .child(crate::t!("common.actions")),
                            ),
                    )
                    // Rows
                    .child(
                        div()
                            .id("docker_container_rows")
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
            // Floating Log Viewer Dialog
            .when_some(
                self.selected_log_container.clone(),
                |d, (log_id, log_name)| {
                    let log_text = self.log_content.clone().unwrap_or_default();
                    let short_id = if log_id.len() >= 12 {
                        log_id[..12].to_string()
                    } else {
                        log_id
                    };

                    d.child(
                        div()
                            .id("modal_docker_log_backdrop")
                            .absolute()
                            .inset_0()
                            .bg(rgba(0x000000bb))
                            .flex()
                            .items_center()
                            .justify_center()
                            .p_6()
                            .child(
                                div()
                                    .id("modal_docker_log_box")
                                    .w(px(760.0))
                                    .h(px(520.0))
                                    .bg(DarkTechTheme::bg_panel())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_active())
                                    .rounded_xl()
                                    .flex()
                                    .flex_col()
                                    .overflow_hidden()
                                    // Modal Header
                                    .child(
                                        div()
                                            .h(px(44.0))
                                            .flex_shrink_0()
                                            .w_full()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_b_1()
                                            .border_color(DarkTechTheme::border_default())
                                            .px_4()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(
                                                        Icon::file()
                                                            .with_size(px(14.0))
                                                            .with_color(
                                                                DarkTechTheme::accent_cyan(),
                                                            ),
                                                    )
                                                    .child(
                                                        div()
                                                            .font_weight(FontWeight::BOLD)
                                                            .text_size(px(13.0))
                                                            .text_color(
                                                                DarkTechTheme::text_primary(),
                                                            )
                                                            .child(format!(
                                                                "容器日志: {} ({})",
                                                                log_name, short_id
                                                            )),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .id("btn_close_docker_logs")
                                                    .px_2p5()
                                                    .py_1()
                                                    .rounded_md()
                                                    .bg(DarkTechTheme::bg_panel())
                                                    .border_1()
                                                    .border_color(DarkTechTheme::border_default())
                                                    .text_size(px(12.0))
                                                    .text_color(DarkTechTheme::text_secondary())
                                                    .cursor_pointer()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.close_logs(cx);
                                                    }))
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .flex_row()
                                                            .items_center()
                                                            .gap_1()
                                                            .child(
                                                                Icon::close()
                                                                    .with_size(px(10.0))
                                                                    .with_color(
                                                                    DarkTechTheme::text_secondary(),
                                                                ),
                                                            )
                                                            .child(crate::t!("common.close")),
                                                    ),
                                            ),
                                    )
                                    // Modal Body: Monospace Logs
                                    .child(
                                        div()
                                            .id("docker_log_content_area")
                                            .flex_1()
                                            .w_full()
                                            .min_h(px(0.0))
                                            .bg(DarkTechTheme::bg_root())
                                            .p_4()
                                            .overflow_y_scroll()
                                            .font_family("Menlo")
                                            .text_size(px(11.5))
                                            .text_color(DarkTechTheme::text_primary())
                                            .child(log_text),
                                    )
                                    // Modal Footer
                                    .child(
                                        div()
                                            .h(px(36.0))
                                            .flex_shrink_0()
                                            .w_full()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_t_1()
                                            .border_color(DarkTechTheme::border_default())
                                            .px_4()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .justify_between()
                                            .text_size(px(11.0))
                                            .text_color(DarkTechTheme::text_muted())
                                            .child("显示最近 200 行标准输出与标准错误")
                                            .child(
                                                div()
                                                    .id("btn_close_docker_logs_bottom")
                                                    .cursor_pointer()
                                                    .text_color(DarkTechTheme::text_accent())
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.close_logs(cx);
                                                    }))
                                                    .child("完成"),
                                            ),
                                    ),
                            ),
                    )
                },
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
            id: HostId("host-test-docker".into()),
            name: "Docker Test Node".into(),
            hostname: "127.0.0.1".into(),
            port: 22,
            user: "root".into(),
            auth: AuthMethod::Password {
                credential_id: "pwd-1".into(),
            },
            group: "Default".into(),
            target_os: TargetOs::Linux,
            tags: vec!["docker".into()],
            jump_host: None,
            proxy_jump_id: None,
            bandwidth_limit_gb: None,
            bandwidth_reset_day: Some(1),
        }
    }

    #[test]
    fn test_docker_panel_initial_state() {
        let host = make_test_host();
        let session_mgr = Arc::new(SessionManager::new());
        let panel = DockerPanel::new(host, session_mgr);

        assert_eq!(panel.host.name, "Docker Test Node");
        assert!(panel.containers.is_empty());
        assert!(panel.filter_query.is_empty());
        assert!(panel.selected_log_container.is_none());
        assert!(panel.log_content.is_none());
        assert!(!panel.is_loading);
        assert!(panel.status_message.is_none());
    }

    #[test]
    fn test_docker_panel_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1048576 * 12), "12.0 MB");
        assert_eq!(format_bytes(1073741824 * 2), "2.00 GB");
    }

    #[test]
    fn test_docker_panel_container_filtering() {
        let c1 = DockerContainerDetail {
            id: "c11122233344".into(),
            name: "my-nginx".into(),
            image: "nginx:alpine".into(),
            status: "Up 2 hours".into(),
            state: "running".into(),
            created: "2026-09-24".into(),
            ports: vec!["0.0.0.0:80->80/tcp".into()],
            cpu_percent: 1.2,
            mem_usage_bytes: 25 * 1024 * 1024,
            mem_limit_bytes: 1024 * 1024 * 1024,
        };
        let c2 = DockerContainerDetail {
            id: "c55566677788".into(),
            name: "db-postgres".into(),
            image: "postgres:16".into(),
            status: "Exited (0) 10 minutes ago".into(),
            state: "exited".into(),
            created: "2026-09-24".into(),
            ports: vec![],
            cpu_percent: 0.0,
            mem_usage_bytes: 0,
            mem_limit_bytes: 0,
        };

        let list = [c1, c2];
        let q = "nginx";
        let filtered: Vec<_> = list
            .iter()
            .filter(|c| c.name.contains(q) || c.image.contains(q))
            .collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "my-nginx");

        let q2 = "exited";
        let filtered2: Vec<_> = list.iter().filter(|c| c.state == q2).collect();
        assert_eq!(filtered2.len(), 1);
        assert_eq!(filtered2[0].name, "db-postgres");
    }
}
