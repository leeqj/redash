use gpui::prelude::FluentBuilder;
use gpui::*;
use std::sync::Arc;
use std::time::Duration;

use crate::components::icon::Icon;
use crate::components::theme::DarkTechTheme;
use redash_core::config::HostConfig;
use redash_core::probe::process::{ProcessItem, ProcessManager};
use redash_core::session::SessionManager;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessSortField {
    CpuDesc,
    MemDesc,
    PidAsc,
}

#[allow(dead_code)]
pub struct ProcessPanel {
    pub host: HostConfig,
    pub session_mgr: Arc<SessionManager>,
    pub processes: Vec<ProcessItem>,
    pub filter_query: String,
    pub sort_by: ProcessSortField,
    pub status_message: Option<(String, bool)>,
    pub scroll_handle: ScrollHandle,
}

#[allow(dead_code)]
impl ProcessPanel {
    pub fn new(host: HostConfig, session_mgr: Arc<SessionManager>) -> Self {
        Self {
            host,
            session_mgr,
            processes: Vec::new(),
            filter_query: String::new(),
            sort_by: ProcessSortField::CpuDesc,
            status_message: None,
            scroll_handle: ScrollHandle::new(),
        }
    }

    pub fn set_processes(&mut self, mut processes: Vec<ProcessItem>, cx: &mut Context<Self>) {
        Self::sort_process_list(&mut processes, self.sort_by);
        self.processes = processes;
        cx.notify();
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let sort_key = match self.sort_by {
            ProcessSortField::MemDesc => "mem",
            _ => "cpu",
        };
        let cmd = ProcessManager::list_cmd(sort_key, 100);
        let session_mgr = Arc::clone(&self.session_mgr);
        let host = self.host.clone();
        let current_sort = self.sort_by;

        cx.spawn(async move |this, cx| {
            let res = session_mgr.exec(&host, &cmd, Duration::from_secs(10)).await;
            if let Ok(r) = res {
                let mut procs = ProcessManager::parse_processes(&r.stdout);
                Self::sort_process_list(&mut procs, current_sort);
                let _ = this.update(cx, |view, cx| {
                    view.processes = procs;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub fn kill_process(&mut self, pid: u32, force: bool, cx: &mut Context<Self>) {
        let cmd = ProcessManager::kill_cmd(pid, force);
        let session_mgr = Arc::clone(&self.session_mgr);
        let host = self.host.clone();
        let sig_name = if force { "SIGKILL (9)" } else { "SIGTERM (15)" };

        cx.spawn(async move |this, cx| {
            let res = session_mgr.exec(&host, &cmd, Duration::from_secs(10)).await;
            let (msg, ok) = match res {
                Ok(r) if r.exit_code == 0 => {
                    (format!("向进程 {} 发送 {} 成功", pid, sig_name), true)
                }
                Ok(r) => (
                    format!("发送信号失败 (code {}): {}", r.exit_code, r.stderr.trim()),
                    false,
                ),
                Err(e) => (format!("命令执行失败: {}", e), false),
            };

            let _ = this.update(cx, |view, cx| {
                view.status_message = Some((msg, ok));
                view.refresh(cx);
            });
        })
        .detach();
    }

    pub fn set_sort(&mut self, sort: ProcessSortField, cx: &mut Context<Self>) {
        self.sort_by = sort;
        Self::sort_process_list(&mut self.processes, sort);
        cx.notify();
    }

    pub fn sort_process_list(procs: &mut [ProcessItem], sort: ProcessSortField) {
        match sort {
            ProcessSortField::CpuDesc => {
                procs.sort_by(|a, b| {
                    b.cpu_percent
                        .partial_cmp(&a.cpu_percent)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            ProcessSortField::MemDesc => {
                procs.sort_by(|a, b| {
                    b.mem_percent
                        .partial_cmp(&a.mem_percent)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            ProcessSortField::PidAsc => {
                procs.sort_by_key(|p| p.pid);
            }
        }
    }

    fn format_bytes(bytes: u64) -> String {
        if bytes == 0 {
            "-".to_string()
        } else {
            redash_types::formatters::format_bytes(bytes)
        }
    }
}

impl Render for ProcessPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let q = self.filter_query.to_lowercase().trim().to_string();
        let filtered_processes: Vec<ProcessItem> = self
            .processes
            .iter()
            .filter(|p| {
                if q.is_empty() {
                    true
                } else {
                    p.command.to_lowercase().contains(&q)
                        || p.user.to_lowercase().contains(&q)
                        || p.pid.to_string().contains(&q)
                        || p.status.to_lowercase().contains(&q)
                }
            })
            .cloned()
            .collect();

        let row_elements: Vec<_> = if filtered_processes.is_empty() {
            vec![
                div()
                    .id("proc_empty_placeholder")
                    .p_8()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .child(
                        Icon::cpu()
                            .with_size(px(24.0))
                            .with_color(DarkTechTheme::text_muted()),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(DarkTechTheme::text_muted())
                            .child(crate::t!("process.no_match")),
                    ),
            ]
        } else {
            filtered_processes
                .into_iter()
                .enumerate()
                .map(|(idx, p)| {
                    let pid = p.pid;
                    let user = p.user.clone();
                    let status = p.status.clone();
                    let command = p.command.clone();
                    let rss_str = Self::format_bytes(p.rss_bytes);

                    let cpu_color = if p.cpu_percent > 60.0 {
                        DarkTechTheme::status_crit()
                    } else if p.cpu_percent > 20.0 {
                        DarkTechTheme::status_warn()
                    } else {
                        DarkTechTheme::text_primary()
                    };

                    let mem_color = if p.mem_percent > 50.0 {
                        DarkTechTheme::status_warn()
                    } else {
                        DarkTechTheme::text_secondary()
                    };

                    div()
                        .id(ElementId::NamedInteger("proc_row".into(), idx as u64))
                        .h(px(38.0))
                        .w_full()
                        .px_3()
                        .border_b_1()
                        .border_color(DarkTechTheme::border_muted())
                        .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()))
                        .flex()
                        .flex_row()
                        .items_center()
                        .text_size(px(11.5))
                        // PID
                        .child(
                            div()
                                .w(px(70.0))
                                .flex_shrink_0()
                                .font_family("Menlo")
                                .text_color(DarkTechTheme::text_accent())
                                .child(pid.to_string()),
                        )
                        // USER
                        .child(
                            div()
                                .w(px(90.0))
                                .flex_shrink_0()
                                .truncate()
                                .text_color(DarkTechTheme::text_secondary())
                                .child(user),
                        )
                        // CPU%
                        .child(
                            div()
                                .w(px(75.0))
                                .flex_shrink_0()
                                .font_family("Menlo")
                                .text_color(cpu_color)
                                .child(format!("{:.1}%", p.cpu_percent)),
                        )
                        // MEM%
                        .child(
                            div()
                                .w(px(75.0))
                                .flex_shrink_0()
                                .font_family("Menlo")
                                .text_color(mem_color)
                                .child(format!("{:.1}%", p.mem_percent)),
                        )
                        // STAT
                        .child(
                            div()
                                .w(px(60.0))
                                .flex_shrink_0()
                                .font_family("Menlo")
                                .text_color(DarkTechTheme::text_muted())
                                .child(status),
                        )
                        // RSS
                        .child(
                            div()
                                .w(px(90.0))
                                .flex_shrink_0()
                                .font_family("Menlo")
                                .text_color(DarkTechTheme::text_secondary())
                                .child(rss_str),
                        )
                        // COMMAND
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(120.0))
                                .font_family("Menlo")
                                .text_color(DarkTechTheme::text_primary())
                                .truncate()
                                .child(command),
                        )
                        // Actions
                        .child(
                            div()
                                .w(px(140.0))
                                .flex_shrink_0()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_end()
                                .gap_1p5()
                                // Terminate (SIGTERM 15)
                                .child(
                                    div()
                                        .id(ElementId::Name(
                                            format!("btn_kill_term_{}", pid).into(),
                                        ))
                                        .px_2()
                                        .py_0p5()
                                        .rounded_md()
                                        .bg(DarkTechTheme::bg_input())
                                        .border_1()
                                        .border_color(DarkTechTheme::status_warn())
                                        .text_color(DarkTechTheme::status_warn())
                                        .text_size(px(10.5))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.kill_process(pid, false, cx);
                                        }))
                                        .child(crate::t!("process.kill_15")),
                                )
                                // Force Kill (SIGKILL 9)
                                .child(
                                    div()
                                        .id(ElementId::Name(
                                            format!("btn_kill_force_{}", pid).into(),
                                        ))
                                        .px_2()
                                        .py_0p5()
                                        .rounded_md()
                                        .bg(DarkTechTheme::bg_input())
                                        .border_1()
                                        .border_color(DarkTechTheme::status_crit())
                                        .text_color(DarkTechTheme::status_crit())
                                        .text_size(px(10.5))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.kill_process(pid, true, cx);
                                        }))
                                        .child(crate::t!("process.kill_9")),
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
            // Top Toolbar: Search, Sort Buttons, Refresh
            .child(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    // Search box and clear button
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id("proc_search_container")
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
                                                crate::t!("process.search").to_string()
                                            } else {
                                                self.filter_query.clone()
                                            }),
                                    ),
                            )
                            .when(!self.filter_query.is_empty(), |d| {
                                d.child(
                                    div()
                                        .id("proc_filter_clear")
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
                    // Sort Buttons & Refresh
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            // Sort CPU
                            .child(
                                div()
                                    .id("btn_sort_cpu")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(if self.sort_by == ProcessSortField::CpuDesc {
                                        DarkTechTheme::bg_panel_hover()
                                    } else {
                                        DarkTechTheme::bg_panel()
                                    })
                                    .border_1()
                                    .border_color(if self.sort_by == ProcessSortField::CpuDesc {
                                        DarkTechTheme::border_active()
                                    } else {
                                        DarkTechTheme::border_default()
                                    })
                                    .text_color(if self.sort_by == ProcessSortField::CpuDesc {
                                        DarkTechTheme::text_accent()
                                    } else {
                                        DarkTechTheme::text_secondary()
                                    })
                                    .text_size(px(11.5))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.set_sort(ProcessSortField::CpuDesc, cx);
                                    }))
                                    .child("CPU% ▼"),
                            )
                            // Sort MEM
                            .child(
                                div()
                                    .id("btn_sort_mem")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(if self.sort_by == ProcessSortField::MemDesc {
                                        DarkTechTheme::bg_panel_hover()
                                    } else {
                                        DarkTechTheme::bg_panel()
                                    })
                                    .border_1()
                                    .border_color(if self.sort_by == ProcessSortField::MemDesc {
                                        DarkTechTheme::border_active()
                                    } else {
                                        DarkTechTheme::border_default()
                                    })
                                    .text_color(if self.sort_by == ProcessSortField::MemDesc {
                                        DarkTechTheme::text_accent()
                                    } else {
                                        DarkTechTheme::text_secondary()
                                    })
                                    .text_size(px(11.5))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.set_sort(ProcessSortField::MemDesc, cx);
                                    }))
                                    .child(crate::t!("process.sort_mem")),
                            )
                            // Sort PID
                            .child(
                                div()
                                    .id("btn_sort_pid")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(if self.sort_by == ProcessSortField::PidAsc {
                                        DarkTechTheme::bg_panel_hover()
                                    } else {
                                        DarkTechTheme::bg_panel()
                                    })
                                    .border_1()
                                    .border_color(if self.sort_by == ProcessSortField::PidAsc {
                                        DarkTechTheme::border_active()
                                    } else {
                                        DarkTechTheme::border_default()
                                    })
                                    .text_color(if self.sort_by == ProcessSortField::PidAsc {
                                        DarkTechTheme::text_accent()
                                    } else {
                                        DarkTechTheme::text_secondary()
                                    })
                                    .text_size(px(11.5))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.set_sort(ProcessSortField::PidAsc, cx);
                                    }))
                                    .child("PID ▲"),
                            )
                            // Refresh Button
                            .child(
                                div()
                                    .id("btn_refresh_processes")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_panel())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_default())
                                    .text_color(DarkTechTheme::text_primary())
                                    .text_size(px(11.5))
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
                                            .child(crate::t!("common.refresh")),
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
                        .id("proc_status_banner")
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
                                .child(if is_ok {
                                    Icon::check()
                                        .with_size(px(12.0))
                                        .with_color(DarkTechTheme::status_online())
                                } else {
                                    Icon::close()
                                        .with_size(px(12.0))
                                        .with_color(DarkTechTheme::status_crit())
                                })
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
                                .id("btn_dismiss_proc_status")
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
            // Process Table Container
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
                            .child(div().w(px(70.0)).flex_shrink_0().child("PID"))
                            .child(
                                div()
                                    .w(px(90.0))
                                    .flex_shrink_0()
                                    .child(crate::t!("process.col_user")),
                            )
                            .child(div().w(px(75.0)).flex_shrink_0().child("CPU%"))
                            .child(div().w(px(75.0)).flex_shrink_0().child("MEM%"))
                            .child(div().w(px(60.0)).flex_shrink_0().child("STAT"))
                            .child(
                                div()
                                    .w(px(90.0))
                                    .flex_shrink_0()
                                    .child(crate::t!("process.col_mem")),
                            )
                            .child(div().flex_1().min_w(px(120.0)).child("COMMAND"))
                            .child(
                                div()
                                    .w(px(140.0))
                                    .flex_shrink_0()
                                    .text_align(TextAlign::Right)
                                    .child(crate::t!("common.actions")),
                            ),
                    )
                    // Table Body / Rows
                    .child(
                        div()
                            .id("proc_table_rows")
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
            id: HostId("host-test-proc".into()),
            name: "Proc Test Node".into(),
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
    fn test_process_panel_initial_state() {
        let host = make_test_host();
        let session_mgr = Arc::new(SessionManager::new());
        let panel = ProcessPanel::new(host, session_mgr);

        assert_eq!(panel.host.name, "Proc Test Node");
        assert!(panel.processes.is_empty());
        assert!(panel.filter_query.is_empty());
        assert_eq!(panel.sort_by, ProcessSortField::CpuDesc);
        assert!(panel.status_message.is_none());
    }

    #[test]
    fn test_process_sort_modes() {
        let p1 = ProcessItem {
            pid: 100,
            user: "root".into(),
            cpu_percent: 5.0,
            mem_percent: 20.0,
            status: "Ss".into(),
            rss_bytes: 1024 * 1024,
            command: "/sbin/init".into(),
        };
        let p2 = ProcessItem {
            pid: 50,
            user: "www-data".into(),
            cpu_percent: 45.0,
            mem_percent: 10.0,
            status: "R".into(),
            rss_bytes: 2048 * 1024,
            command: "nginx".into(),
        };
        let p3 = ProcessItem {
            pid: 200,
            user: "postgres".into(),
            cpu_percent: 2.0,
            mem_percent: 50.0,
            status: "S".into(),
            rss_bytes: 4096 * 1024,
            command: "postgres".into(),
        };

        let mut list = vec![p1, p2, p3];

        // Sort CPU desc
        ProcessPanel::sort_process_list(&mut list, ProcessSortField::CpuDesc);
        assert_eq!(list[0].pid, 50); // 45%
        assert_eq!(list[1].pid, 100); // 5%
        assert_eq!(list[2].pid, 200); // 2%

        // Sort Mem desc
        ProcessPanel::sort_process_list(&mut list, ProcessSortField::MemDesc);
        assert_eq!(list[0].pid, 200); // 50%
        assert_eq!(list[1].pid, 100); // 20%
        assert_eq!(list[2].pid, 50); // 10%

        // Sort PID asc
        ProcessPanel::sort_process_list(&mut list, ProcessSortField::PidAsc);
        assert_eq!(list[0].pid, 50);
        assert_eq!(list[1].pid, 100);
        assert_eq!(list[2].pid, 200);
    }

    #[test]
    fn test_process_filter() {
        let p1 = ProcessItem {
            pid: 101,
            user: "root".into(),
            cpu_percent: 1.0,
            mem_percent: 2.0,
            status: "S".into(),
            rss_bytes: 1024,
            command: "/usr/sbin/sshd".into(),
        };
        let p2 = ProcessItem {
            pid: 202,
            user: "redis".into(),
            cpu_percent: 2.0,
            mem_percent: 5.0,
            status: "S".into(),
            rss_bytes: 4096,
            command: "redis-server 127.0.0.1:6379".into(),
        };

        let list = [p1, p2];
        let q = "redis";
        let filtered: Vec<_> = list
            .iter()
            .filter(|p| p.command.contains(q) || p.user.contains(q))
            .collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].pid, 202);
    }
}
