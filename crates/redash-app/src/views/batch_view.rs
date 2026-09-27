use gpui::prelude::FluentBuilder;
use gpui::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::components::icon::Icon;
use crate::components::theme::DarkTechTheme;
use redash_core::batch::{
    BatchJobResult, BatchProgressEvent, BatchRunner, HostTaskExecution, TaskState,
};
use redash_core::config::{HostConfig, HostId};
use redash_core::session::SessionManager;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchViewMode {
    SplitGrid,
    List,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultFilter {
    All,
    SuccessOnly,
    FailedOnly,
}

pub struct DevOpsPreset {
    pub label: &'static str,
    pub command: &'static str,
    #[allow(dead_code)]
    pub description: &'static str,
}

pub const DEVOPS_PRESETS: &[DevOpsPreset] = &[
    DevOpsPreset {
        label: "系统概要",
        command: "uname -a && uptime",
        description: "查看内核版本、主机架构与运行负载",
    },
    DevOpsPreset {
        label: "磁盘空间",
        command: "df -h / /var /home 2>/dev/null || df -h",
        description: "检查关键分区磁盘使用与剩余空间",
    },
    DevOpsPreset {
        label: "磁盘深度清理",
        command: "journalctl --vacuum-size=100M 2>/dev/null; rm -rf /tmp/*.log 2>/dev/null; df -h /",
        description: "清理 systemd 历史日志与临时文件并显示释放结果",
    },
    DevOpsPreset {
        label: "容器健康检查",
        command: r#"docker ps --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}" 2>/dev/null || echo "Docker not installed""#,
        description: "列出当前运行中的 Docker 容器与端口映射",
    },
    DevOpsPreset {
        label: "Top 资源消耗",
        command: "ps aux --sort=-%cpu | head -n 8",
        description: "获取占用 CPU 最多前 8 个系统进程",
    },
    DevOpsPreset {
        label: "监听端口",
        command: "ss -tulpn 2>/dev/null || netstat -tulpn 2>/dev/null || lsof -i -P -n",
        description: "扫描系统正在侦听的网络端口与对应进程",
    },
    DevOpsPreset {
        label: "内存诊断",
        command: "free -h 2>/dev/null || vm_stat",
        description: "显示系统物理内存、空闲内存与交换分区使用",
    },
];

pub fn format_duration_detailed(ms: u64, us: u64) -> String {
    if ms == 0 && us == 0 {
        "0ms".to_string()
    } else if us > 0 && us < 1000 {
        format!("{}µs", us)
    } else if ms < 1000 {
        if us > 0 {
            format!("{}ms ({}µs)", ms, us)
        } else {
            format!("{}ms", ms)
        }
    } else {
        let secs = ms as f64 / 1000.0;
        format!("{:.2}s ({}ms)", secs, ms)
    }
}

pub struct BatchView {
    run_generation: u64,
    pub targets: Vec<HostConfig>,
    pub command_input: String,
    pub cursor_pos: usize,
    pub command: String,
    pub session_mgr: Arc<SessionManager>,
    pub is_running: bool,
    pub last_result: Option<BatchJobResult>,
    pub active_executions: HashMap<HostId, HostTaskExecution>,
    pub focus_handle: Option<FocusHandle>,
    pub view_mode: BatchViewMode,
    pub filter: ResultFilter,
    pub toast_message: Option<String>,
    pub scroll_handle: ScrollHandle,
    pub cancel_handle: Option<tokio::task::AbortHandle>,
}

impl Drop for BatchView {
    fn drop(&mut self) {
        if let Some(handle) = self.cancel_handle.take() {
            handle.abort();
        }
    }
}

impl BatchView {
    pub fn new(targets: Vec<HostConfig>, session_mgr: Arc<SessionManager>) -> Self {
        let initial_cmd = "uname -a && uptime".to_string();
        let initial_len = initial_cmd.chars().count();
        Self {
            targets,
            command_input: initial_cmd.clone(),
            cursor_pos: initial_len,
            command: initial_cmd,
            session_mgr,
            is_running: false,
            last_result: None,
            active_executions: HashMap::new(),
            focus_handle: None,
            view_mode: BatchViewMode::SplitGrid,
            filter: ResultFilter::All,
            toast_message: None,
            scroll_handle: ScrollHandle::new(),
            cancel_handle: None,
            run_generation: 0,
        }
    }

    pub fn set_hosts(&mut self, targets: Vec<HostConfig>, cx: &mut Context<Self>) {
        self.targets = targets;
        cx.notify();
    }

    pub fn char_to_byte_index(&self, char_idx: usize) -> usize {
        self.command_input
            .char_indices()
            .nth(char_idx)
            .map(|(idx, _)| idx)
            .unwrap_or(self.command_input.len())
    }

    pub fn insert_str_at_cursor(&mut self, s: &str) {
        let byte_idx = self.char_to_byte_index(self.cursor_pos);
        self.command_input.insert_str(byte_idx, s);
        self.cursor_pos += s.chars().count();
        self.command = self.command_input.clone();
    }

    pub fn insert_char_at_cursor(&mut self, ch: char) {
        let byte_idx = self.char_to_byte_index(self.cursor_pos);
        self.command_input.insert(byte_idx, ch);
        self.cursor_pos += 1;
        self.command = self.command_input.clone();
    }

    pub fn backspace_at_cursor(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            let byte_idx = self.char_to_byte_index(self.cursor_pos);
            let next_byte_idx = self.char_to_byte_index(self.cursor_pos + 1);
            self.command_input
                .replace_range(byte_idx..next_byte_idx, "");
            self.command = self.command_input.clone();
        }
    }

    pub fn delete_at_cursor(&mut self) {
        let total_chars = self.command_input.chars().count();
        if self.cursor_pos < total_chars {
            let byte_idx = self.char_to_byte_index(self.cursor_pos);
            let next_byte_idx = self.char_to_byte_index(self.cursor_pos + 1);
            self.command_input
                .replace_range(byte_idx..next_byte_idx, "");
            self.command = self.command_input.clone();
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_pos < self.command_input.chars().count() {
            self.cursor_pos += 1;
        }
    }

    pub fn move_cursor_home(&mut self) {
        self.cursor_pos = 0;
    }

    pub fn move_cursor_end(&mut self) {
        self.cursor_pos = self.command_input.chars().count();
    }

    pub fn clear_to_start(&mut self) {
        if self.cursor_pos > 0 {
            let byte_idx = self.char_to_byte_index(self.cursor_pos);
            self.command_input.replace_range(0..byte_idx, "");
            self.cursor_pos = 0;
            self.command = self.command_input.clone();
        }
    }

    pub fn clear_to_end(&mut self) {
        let byte_idx = self.char_to_byte_index(self.cursor_pos);
        self.command_input.truncate(byte_idx);
        self.command = self.command_input.clone();
    }

    pub fn delete_word_before_cursor(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        let chars: Vec<char> = self.command_input.chars().collect();
        let mut idx = self.cursor_pos;
        while idx > 0 && chars[idx - 1].is_whitespace() {
            idx -= 1;
        }
        while idx > 0 && !chars[idx - 1].is_whitespace() {
            idx -= 1;
        }
        let start_byte = self.char_to_byte_index(idx);
        let end_byte = self.char_to_byte_index(self.cursor_pos);
        self.command_input.replace_range(start_byte..end_byte, "");
        self.cursor_pos = idx;
        self.command = self.command_input.clone();
    }

    pub fn apply_preset(&mut self, cmd: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.command_input = cmd.to_string();
        self.command = cmd.to_string();
        self.cursor_pos = self.command_input.chars().count();
        let fh = self
            .focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        window.focus(&fh);
        cx.notify();
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        self.run_generation += 1;
        if let Some(handle) = self.cancel_handle.take() {
            handle.abort();
        }
        self.is_running = false;
        for exec in self.active_executions.values_mut() {
            if exec.state == TaskState::Running || exec.state == TaskState::Pending {
                exec.state = TaskState::Failed;
                exec.error = Some("用户取消执行".to_string());
            }
        }
        self.toast_message =
            Some("已停止本地任务并请求关闭远端信道；脱离会话的进程可能仍在运行".to_string());
        cx.notify();
    }

    pub fn execute(&mut self, cx: &mut Context<Self>) {
        if self.is_running || self.targets.is_empty() || self.command_input.trim().is_empty() {
            return;
        }

        self.run_generation += 1;
        let generation = self.run_generation;
        self.is_running = true;
        self.last_result = None;
        self.active_executions.clear();
        self.toast_message = None;

        for host in &self.targets {
            self.active_executions.insert(
                host.id.clone(),
                HostTaskExecution {
                    host_id: host.id.0.clone(),
                    host_name: host.name.clone(),
                    state: TaskState::Pending,
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: None,
                    duration_ms: 0,
                    duration_us: 0,
                    error: None,
                },
            );
        }

        let hosts = self.targets.clone();
        let cmd = self.command_input.clone();
        let session_mgr = Arc::clone(&self.session_mgr);

        let (progress_tx, mut progress_rx) = mpsc::unbounded_channel::<BatchProgressEvent>();

        cx.spawn(async move |this, cx| {
            let runner_handle = tokio::spawn(async move {
                BatchRunner::run_batch_streaming(
                    hosts,
                    cmd,
                    session_mgr,
                    Duration::from_secs(30),
                    Some(progress_tx),
                )
                .await
            });

            let abort_handle = runner_handle.abort_handle();
            if this
                .update(cx, |view, _cx| {
                    if view.run_generation != generation {
                        abort_handle.abort();
                        return;
                    }
                    view.cancel_handle = Some(abort_handle);
                })
                .is_err()
            {
                runner_handle.abort();
                let _ = runner_handle.await;
                return;
            }

            while let Some(event) = progress_rx.recv().await {
                let _ = this.update(cx, |view, cx| {
                    if view.run_generation != generation {
                        return;
                    }
                    match event {
                        BatchProgressEvent::HostStarted { host_id, .. } => {
                            if let Some(exec) = view.active_executions.get_mut(&HostId(host_id)) {
                                exec.state = TaskState::Running;
                            }
                        }
                        BatchProgressEvent::HostCompleted(exec) => {
                            view.active_executions.insert(HostId(exec.host_id.clone()), exec);
                        }
                        BatchProgressEvent::AllCompleted(result) => {
                            view.is_running = false;
                            view.cancel_handle = None;
                            view.last_result = Some(result);
                        }
                    }
                    cx.notify();
                });
            }

            if let Ok(final_result) = runner_handle.await {
                let _ = this.update(cx, |view, cx| {
                    if view.run_generation != generation {
                        return;
                    }
                    view.is_running = false;
                    view.cancel_handle = None;
                    if view.last_result.is_none() {
                        view.last_result = Some(final_result);
                    }
                    cx.notify();
                });
            } else {
                let _ = this.update(cx, |view, cx| {
                    if view.run_generation != generation {
                        return;
                    }
                    view.is_running = false;
                    view.cancel_handle = None;
                    cx.notify();
                });
            }
        })
        .detach();

        cx.notify();
    }

    fn handle_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = &event.keystroke.modifiers;

        // Enter or Cmd+Enter triggers batch execution
        if key == "enter" {
            self.execute(cx);
            cx.stop_propagation();
            return;
        }

        // Cmd+V / Ctrl+V clipboard paste
        if (modifiers.platform || modifiers.control) && (key == "v" || key == "V") {
            if let Some(item) = cx.read_from_clipboard()
                && let Some(text) = item.text()
            {
                let cleaned = text
                    .replace("\r\n", " && ")
                    .replace('\n', " && ")
                    .replace('\r', " ");
                self.insert_str_at_cursor(&cleaned);
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }

        // Ctrl+A / Home
        if (modifiers.control && (key == "a" || key == "A")) || key == "home" {
            self.move_cursor_home();
            cx.notify();
            cx.stop_propagation();
            return;
        }

        // Ctrl+E / End
        if (modifiers.control && (key == "e" || key == "E")) || key == "end" {
            self.move_cursor_end();
            cx.notify();
            cx.stop_propagation();
            return;
        }

        // Ctrl+U
        if modifiers.control && (key == "u" || key == "U") {
            self.clear_to_start();
            cx.notify();
            cx.stop_propagation();
            return;
        }

        // Ctrl+K
        if modifiers.control && (key == "k" || key == "K") {
            self.clear_to_end();
            cx.notify();
            cx.stop_propagation();
            return;
        }

        // Ctrl+W
        if modifiers.control && (key == "w" || key == "W") {
            self.delete_word_before_cursor();
            cx.notify();
            cx.stop_propagation();
            return;
        }

        // Backspace
        if key == "backspace" {
            self.backspace_at_cursor();
            cx.notify();
            cx.stop_propagation();
            return;
        }

        // Delete
        if key == "delete" {
            self.delete_at_cursor();
            cx.notify();
            cx.stop_propagation();
            return;
        }

        // Left Arrow
        if key == "left" {
            self.move_cursor_left();
            cx.notify();
            cx.stop_propagation();
            return;
        }

        // Right Arrow
        if key == "right" {
            self.move_cursor_right();
            cx.notify();
            cx.stop_propagation();
            return;
        }

        // Space
        if key == "space" && !modifiers.control && !modifiers.platform {
            self.insert_char_at_cursor(' ');
            cx.notify();
            cx.stop_propagation();
            return;
        }

        // Normal printable characters
        if !modifiers.control && !modifiers.platform {
            if let Some(ref kc) = event.keystroke.key_char {
                if !kc.is_empty() && kc != "\n" && kc != "\r" && kc != "\t" {
                    self.insert_str_at_cursor(kc);
                    cx.notify();
                    cx.stop_propagation();
                }
            } else if key.chars().count() == 1
                && let Some(ch) = key.chars().next()
            {
                self.insert_char_at_cursor(ch);
                cx.notify();
                cx.stop_propagation();
            }
        }
    }
}

impl Render for BatchView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self
            .focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        let is_focused = focus_handle.is_focused(window);
        let is_running_val = self.is_running;
        let target_count = self.targets.len();

        let mut success_count = 0;
        let mut failed_count = 0;
        let mut running_count = 0;
        let mut pending_count = 0;

        for host in &self.targets {
            if let Some(exec) = self.active_executions.get(&host.id) {
                match exec.state {
                    TaskState::Pending => pending_count += 1,
                    TaskState::Running => running_count += 1,
                    TaskState::Success => success_count += 1,
                    TaskState::Failed => failed_count += 1,
                }
            } else if let Some(ref job) = self.last_result
                && let Some(res) = job.hosts_results.get(&host.id.0)
            {
                if res.state == TaskState::Success {
                    success_count += 1;
                } else {
                    failed_count += 1;
                }
            }
        }

        let current_filter = self.filter;
        let current_view_mode = self.view_mode;

        // Collect items to display in stable order
        let display_items: Vec<HostTaskExecution> = self
            .targets
            .iter()
            .filter_map(|host| {
                if let Some(exec) = self.active_executions.get(&host.id) {
                    Some(exec.clone())
                } else if let Some(ref job) = self.last_result {
                    job.hosts_results.get(&host.id.0).cloned()
                } else {
                    None
                }
            })
            .filter(|exec| match current_filter {
                ResultFilter::All => true,
                ResultFilter::SuccessOnly => exec.state == TaskState::Success,
                ResultFilter::FailedOnly => exec.state == TaskState::Failed,
            })
            .collect();

        // Render command input text and cursor
        let chars: Vec<char> = self.command_input.chars().collect();
        let total_chars = chars.len();
        let safe_cursor = self.cursor_pos.min(total_chars);
        let before_text: String = chars[..safe_cursor].iter().collect();
        let cursor_char = chars.get(safe_cursor).copied();
        let after_text: String = if safe_cursor < total_chars {
            chars[safe_cursor + 1..].iter().collect()
        } else {
            String::new()
        };

        div()
            .size_full()
            .bg(rgb(0x0a0b10))
            .flex()
            .flex_col()
            .overflow_hidden()
            .p_4()
            .gap_3()
            // Top Header & Telemetry Bar
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .items_center()
                    .pb_2()
                    .border_b_1()
                    .border_color(rgb(0x1a1c28))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(16.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(0xf8fafc))
                                            .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1p5()
                                            .child(Icon::zap().with_size(px(14.0)).with_color(rgb(0x38bdf8)))
                                            .child("BATCH COMMAND CENTER"),
                                    )
                                    )
                                    .child(
                                        div()
                                            .px_2()
                                            .py_0p5()
                                            .bg(rgb(0x181a26))
                                            .border_1()
                                            .border_color(rgb(0x282b3d))
                                            .rounded_md()
                                            .text_size(px(10.0))
                                            .text_color(rgb(0x38bdf8))
                                            .font_weight(FontWeight::BOLD)
                                            .child(format!("{} HOSTS TARGETED", target_count))
                                    )
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgb(0x64748b))
                                    .child(crate::t!("batch.subtitle"))
                            )
                    )
                    // Telemetry Status Capsules & Toolbar
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            // Execution counters
                            .when(running_count > 0, |d| {
                                d.child(
                                    div()
                                        .px_2().py_1().bg(rgb(0x0369a1)).rounded_md().text_size(px(10.5))
                                        .text_color(rgb(0xf0f9ff)).font_weight(FontWeight::BOLD)
                                        .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(Icon::zap().with_size(px(10.0)).with_color(rgb(0xf0f9ff)))
                                            .child(format!("正在执行: {}", running_count)),
                                    )
                                )
                            })
                            .when(pending_count > 0, |d| {
                                d.child(
                                    div()
                                        .px_2().py_1().bg(rgb(0x1e2030)).border_1().border_color(rgb(0x313244)).rounded_md().text_size(px(10.5))
                                        .text_color(rgb(0xa6adc8))
                                        .child(format!("等待中: {}", pending_count))
                                )
                            })
                            .when(success_count > 0, |d| {
                                d.child(
                                    div()
                                        .px_2().py_1().bg(rgb(0x064e3b)).rounded_md().text_size(px(10.5))
                                        .text_color(rgb(0x6ee7b7)).font_weight(FontWeight::BOLD)
                                        .child(format!("成功: {}", success_count))
                                )
                            })
                            .when(failed_count > 0, |d| {
                                d.child(
                                    div()
                                        .px_2().py_1().bg(rgb(0x7f1d1d)).rounded_md().text_size(px(10.5))
                                        .text_color(rgb(0xfca5a5)).font_weight(FontWeight::BOLD)
                                        .flex().flex_row().items_center().gap_1()
                                        .child(Icon::close().with_size(px(10.0)).with_color(rgb(0xfca5a5)))
                                        .child(format!("失败: {}", failed_count))
                                )
                            })
                            // Total duration badge
                            .when_some(self.last_result.as_ref(), |d, job| {
                                d.child(
                                    div()
                                        .px_2().py_1().bg(rgb(0x131520)).border_1().border_color(rgb(0x282b3d)).rounded_md().text_size(px(10.5))
                                        .text_color(rgb(0x38bdf8))
                                        .flex().flex_row().items_center().gap_1()
                                        .child(Icon::clock().with_size(px(11.0)).with_color(rgb(0x38bdf8)))
                                        .child(format!("总耗时: {}", format_duration_detailed(job.total_duration_ms, job.total_duration_us)))
                                )
                            })
                            // View Mode Toggle
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .bg(rgb(0x12141f))
                                    .border_1()
                                    .border_color(rgb(0x282b3d))
                                    .rounded_md()
                                    .p_0p5()
                                    .child(
                                        div()
                                            .id("btn_view_split")
                                            .px_2()
                                            .py_1()
                                            .rounded_sm()
                                            .cursor_pointer()
                                            .bg(if current_view_mode == BatchViewMode::SplitGrid { rgb(0x38bdf8) } else { rgb(0x12141f) })
                                            .text_color(if current_view_mode == BatchViewMode::SplitGrid { rgb(0x0a0b10) } else { rgb(0x94a3b8) })
                                            .font_weight(FontWeight::BOLD)
                                            .text_size(px(10.5))
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.view_mode = BatchViewMode::SplitGrid;
                                                cx.notify();
                                            }))
                                            .child(Icon::windows().with_size(px(10.0)).with_color(if current_view_mode == BatchViewMode::SplitGrid { rgb(0x0a0b10) } else { rgb(0x94a3b8) }))
                                            .child(crate::t!("batch.split_view"))
                                    )
                                    .child(
                                        div()
                                            .id("btn_view_list")
                                            .px_2()
                                            .py_1()
                                            .rounded_sm()
                                            .cursor_pointer()
                                            .bg(if current_view_mode == BatchViewMode::List { rgb(0x38bdf8) } else { rgb(0x12141f) })
                                            .text_color(if current_view_mode == BatchViewMode::List { rgb(0x0a0b10) } else { rgb(0x94a3b8) })
                                            .font_weight(FontWeight::BOLD)
                                            .text_size(px(10.5))
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.view_mode = BatchViewMode::List;
                                                cx.notify();
                                            }))
                                            .child(Icon::sort().with_size(px(10.0)).with_color(if current_view_mode == BatchViewMode::List { rgb(0x0a0b10) } else { rgb(0x94a3b8) }))
                                            .child(crate::t!("batch.list_view"))
                                    )
                            )
                    )
            )
            // Toast notification banner if copied
            .when_some(self.toast_message.as_ref(), |d, msg| {
                d.child(
                    div()
                        .flex_shrink_0()
                        .w_full()
                        .px_3()
                        .py_1p5()
                        .bg(rgb(0x064e3b))
                        .border_1()
                        .border_color(rgb(0x059669))
                        .rounded_md()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1p5()
                                .child(Icon::check().with_size(px(11.0)).with_color(rgb(0xa7f3d0)))
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(rgb(0xa7f3d0))
                                        .font_weight(FontWeight::BOLD)
                                        .child(msg.clone())
                                )
                        )
                        .child(
                            div()
                                .id("btn_close_toast")
                                .cursor_pointer()
                                .text_color(rgb(0xa7f3d0))
                                .text_size(px(11.0))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toast_message = None;
                                    cx.notify();
                                }))
                                .child(Icon::close().with_size(px(9.0)).with_color(rgb(0xa7f3d0)))
                        )
                )
            })
            // Interactive Shell Command Center Box
            .child(
                div()
                    .flex_shrink_0()
                    .w_full()
                    .bg(rgb(0x11121a))
                    .border_1()
                    .border_color(if is_focused { rgb(0x38bdf8) } else { rgb(0x232738) })
                    .rounded_lg()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_3()
                    // Top hint line
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(11.5))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(0x94a3b8))
                                            .child("SHELL COMMAND (自由交互键入)")
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(rgb(0x475569))
                                            .child("· 焦点已就绪，直接打字输入，支持 Enter 执行与 Cmd+V 粘贴")
                                    )
                            )
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(rgb(0x64748b))
                                    .child(format!("字符数: {} | 光标位置: {}", total_chars, safe_cursor))
                            )
                    )
                    // Interactive Command Line Input Box
                    .child(
                        div()
                            .id("batch_shell_input_container")
                            .track_focus(&focus_handle)
                            .w_full()
                            .min_h(px(40.0))
                            .bg(rgb(0x06070a))
                            .border_1()
                            .border_color(if is_focused { rgb(0x0284c7) } else { rgb(0x1e2030) })
                            .rounded_md()
                            .px_3()
                            .py_2()
                            .cursor_text()
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                                let fh = this.focus_handle.get_or_insert_with(|| cx.focus_handle()).clone();
                                window.focus(&fh);
                                cx.notify();
                            }))
                            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                                this.handle_key_down(event, cx);
                            }))
                            .flex()
                            .flex_row()
                            .items_center()
                            .font_family("Menlo")
                            .text_size(px(13.0))
                            .child(
                                div()
                                    .text_color(rgb(0x38bdf8))
                                    .font_weight(FontWeight::BOLD)
                                    .mr_2()
                                    .child("$ ")
                            )
                            .child(
                                if chars.is_empty() {
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .when(is_focused, |d| {
                                            d.child(
                                                div()
                                                    .w(px(8.0))
                                                    .h(px(16.0))
                                                    .bg(rgb(0x38bdf8))
                                                    .rounded_xs()
                                            )
                                        })
                                        .child(
                                            div()
                                                .text_color(rgb(0x475569))
                                                .ml_1()
                                                .child(crate::t!("batch.input_placeholder"))
                                        )
                                } else {
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .child(
                                            div()
                                                .text_color(rgb(0xf1f5f9))
                                                .child(before_text)
                                        )
                                        .child(
                                            if let Some(ch) = cursor_char {
                                                if is_focused {
                                                    div()
                                                        .bg(rgb(0x38bdf8))
                                                        .text_color(rgb(0x0a0b10))
                                                        .font_weight(FontWeight::BOLD)
                                                        .child(ch.to_string())
                                                } else {
                                                    div()
                                                        .border_b_2()
                                                        .border_color(rgb(0x64748b))
                                                        .text_color(rgb(0xf1f5f9))
                                                        .child(ch.to_string())
                                                }
                                            } else if is_focused {
                                                div()
                                                    .w(px(8.0))
                                                    .h(px(16.0))
                                                    .bg(rgb(0x38bdf8))
                                                    .rounded_xs()
                                            } else {
                                                div()
                                                    .w(px(2.0))
                                                    .h(px(16.0))
                                                    .bg(rgb(0x475569))
                                            }
                                        )
                                        .child(
                                            div()
                                                .text_color(rgb(0xf1f5f9))
                                                .child(after_text)
                                        )
                                }
                            )
                    )
                    // DevOps Presets Ribbon
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(0x64748b))
                                            .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1p5()
                                            .child(Icon::zap().with_size(px(11.0)).with_color(rgb(0x38bdf8)))
                                            .child(crate::t!("batch.presets")),
                                    )
                                    )
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .children(DEVOPS_PRESETS.iter().enumerate().map(|(idx, preset)| {
                                        let cmd_str = preset.command;
                                        div()
                                            .id(ElementId::NamedInteger("preset_pill".into(), idx as u64))
                                            .px_2p5()
                                            .py_1()
                                            .bg(rgb(0x161824))
                                            .border_1()
                                            .border_color(rgb(0x2d3148))
                                            .rounded_md()
                                            .text_size(px(11.0))
                                            .text_color(rgb(0xcdd6f4))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(0x23273c)).border_color(rgb(0x38bdf8)).text_color(rgb(0x38bdf8)))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.apply_preset(cmd_str, window, cx);
                                            }))
                                            .child(preset.label)
                                    }))
                            )
                    )
                    // Actions and Execution Trigger
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .items_center()
                            .pt_1()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(rgb(0x64748b))
                                            .child("超时设定: 30 秒 | 异步 PTY 高性能并发引擎 | 支持管道符与复合命令")
                                    )
                                    .when(self.last_result.is_some() || !self.active_executions.is_empty(), |d| {
                                        d.child(
                                            div()
                                                .id("btn_clear_results")
                                                .cursor_pointer()
                                                .text_size(px(11.0))
                                                .text_color(rgb(0x94a3b8))
                                                .hover(|s| s.text_color(rgb(0xf87171)))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.active_executions.clear();
                                                    this.last_result = None;
                                                    cx.notify();
                                                }))
                                                .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_1()
                                                    .child(Icon::trash().with_size(px(10.0)).with_color(rgb(0x94a3b8)))
                                                    .child(crate::t!("common.clear")),
                                            )
                                        )
                                    })
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id("btn_exec_batch")
                                            .px_4()
                                            .py_2()
                                            .bg(if is_running_val { rgb(0x334155) } else { rgb(0x38bdf8) })
                                            .text_color(if is_running_val { rgb(0x94a3b8) } else { rgb(0x0a0b10) })
                                            .font_weight(FontWeight::BOLD)
                                            .rounded_md()
                                            .text_size(px(12.0))
                                            .cursor_pointer()
                                            .hover(|s| if !is_running_val { s.bg(rgb(0x7dd3fc)) } else { s })
                                            .on_click(cx.listener(|this, _e: &ClickEvent, _window, cx| {
                                                this.execute(cx);
                                            }))
                                            .child(if is_running_val {
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_1p5()
                                                    .child(Icon::refresh().with_size(px(12.0)).with_color(rgb(0x94a3b8)))
                                                    .child(crate::t!("batch.running"))
                                            } else {
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_1p5()
                                                    .child(Icon::play().with_size(px(12.0)).with_color(rgb(0x0a0b10)))
                                                    .child(crate::t!("batch.exec_btn"))
                                            })
                                    )
                                    .children({
                                        if is_running_val {
                                            Some(
                                                div()
                                                    .id("btn_cancel_batch")
                                                    .px_3()
                                                    .py_2()
                                                    .bg(DarkTechTheme::status_crit().opacity(0.2))
                                                    .border_1()
                                                    .border_color(DarkTechTheme::status_crit().opacity(0.6))
                                                    .text_color(DarkTechTheme::status_crit())
                                                    .font_weight(FontWeight::BOLD)
                                                    .rounded_md()
                                                    .text_size(px(12.0))
                                                    .cursor_pointer()
                                                    .hover(|s| s.bg(DarkTechTheme::status_crit().opacity(0.3)))
                                                    .on_click(cx.listener(|this, _e: &ClickEvent, _window, cx| {
                                                        this.cancel(cx);
                                                    }))
                                                    .child(crate::t!("batch.cancel_btn"))
                                            )
                                        } else {
                                            None
                                        }
                                    })
                            )
                    )
            )
            // Filter Bar
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .items_center()
                    .px_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0x64748b))
                                    .child("结果筛选:")
                            )
                            .child(
                                div()
                                    .id("filter_all")
                                    .px_2().py_0p5().rounded_sm().cursor_pointer().text_size(px(10.5))
                                    .bg(if current_filter == ResultFilter::All { rgb(0x282b3d) } else { rgb(0x12141f) })
                                    .text_color(if current_filter == ResultFilter::All { rgb(0xf1f5f9) } else { rgb(0x64748b) })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.filter = ResultFilter::All;
                                        cx.notify();
                                    }))
                                    .child("全部显示")
                            )
                            .child(
                                div()
                                    .id("filter_success")
                                    .px_2().py_0p5().rounded_sm().cursor_pointer().text_size(px(10.5))
                                    .bg(if current_filter == ResultFilter::SuccessOnly { rgb(0x064e3b) } else { rgb(0x12141f) })
                                    .text_color(if current_filter == ResultFilter::SuccessOnly { rgb(0x6ee7b7) } else { rgb(0x64748b) })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.filter = ResultFilter::SuccessOnly;
                                        cx.notify();
                                    }))
                                    .child("仅成功")
                            )
                            .child(
                                div()
                                    .id("filter_failed")
                                    .px_2().py_0p5().rounded_sm().cursor_pointer().text_size(px(10.5))
                                    .bg(if current_filter == ResultFilter::FailedOnly { rgb(0x7f1d1d) } else { rgb(0x12141f) })
                                    .text_color(if current_filter == ResultFilter::FailedOnly { rgb(0xfca5a5) } else { rgb(0x64748b) })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.filter = ResultFilter::FailedOnly;
                                        cx.notify();
                                    }))
                                    .child("仅失败")
                            )
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(0x475569))
                            .child(format!("显示结果: {} 项", display_items.len()))
                    )
            )
            // Execution Results Container
            .child(
                div()
                    .id("batch_results_scroll")
                    .track_scroll(&self.scroll_handle)
                    .flex_1()
                    .w_full()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(
                        if display_items.is_empty() {
                            div()
                                .w_full()
                                .h(px(180.0))
                                .bg(rgb(0x0e0f17))
                                .border_1()
                                .border_color(rgb(0x1e2030))
                                .rounded_lg()
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .gap_2()
                                .child(
                                    Icon::zap()
                                        .with_size(px(24.0))
                                        .with_color(rgb(0x38bdf8)),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.5))
                                        .text_color(rgb(0x64748b))
                                        .child("就绪中 · 点击上方预设或输入 Shell 命令后按 Enter 即可启动并发任务")
                                )
                        } else if current_view_mode == BatchViewMode::SplitGrid {
                            // Split-screen comparison (2-Column Grid)
                            let chunks: Vec<Vec<HostTaskExecution>> = display_items.chunks(2).map(|c| c.to_vec()).collect();
                            div()
                                .w_full()
                                .flex()
                                .flex_col()
                                .gap_3()
                                .children(chunks.into_iter().map(|pair| {
                                    div()
                                        .w_full()
                                        .flex()
                                        .flex_row()
                                        .gap_3()
                                        .children(pair.into_iter().map(|item| {
                                            self.render_host_card(&item, true, cx)
                                        }))
                                }))
                        } else {
                            // Full-width List View
                            div()
                                .w_full()
                                .flex()
                                .flex_col()
                                .gap_3()
                                .children(display_items.into_iter().map(|item| {
                                    self.render_host_card(&item, false, cx)
                                }))
                        }
                    )
            )
    }
}

impl BatchView {
    fn render_host_card(
        &self,
        res: &HostTaskExecution,
        is_grid: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (status_bg, status_border, status_color, status_text) = match res.state {
            TaskState::Pending => (
                rgb(0x1e2030),
                rgb(0x313244),
                rgb(0xa6adc8),
                "等待中 (Pending)",
            ),
            TaskState::Running => (
                rgb(0x0c4a6e),
                rgb(0x38bdf8),
                rgb(0x38bdf8),
                "执行中 (Running)",
            ),
            TaskState::Success => (
                rgb(0x064e3b),
                rgb(0x10b981),
                rgb(0x34d399),
                "成功 (Success)",
            ),
            TaskState::Failed => (rgb(0x7f1d1d), rgb(0xef4444), rgb(0xf87171), "失败 (Failed)"),
        };

        let duration_display = format_duration_detailed(res.duration_ms, res.duration_us);

        let output_content = if !res.stdout.is_empty() {
            res.stdout.clone()
        } else if !res.stderr.is_empty() {
            res.stderr.clone()
        } else if let Some(err) = &res.error {
            err.clone()
        } else if res.state == TaskState::Running {
            "正在向目标节点发送指令并流式接收输出...".to_string()
        } else if res.state == TaskState::Pending {
            "排队等待网络连接与执行会话分配...".to_string()
        } else {
            "(命令执行完毕，标准输出与标准错误为空)".to_string()
        };

        let host_name_str = res.host_name.clone();
        let copy_text = output_content.clone();

        div()
            .when(is_grid, |d| d.flex_1())
            .when(!is_grid, |d| d.w_full())
            .bg(rgb(0x10121b))
            .border_1()
            .border_color(match res.state {
                TaskState::Running => rgb(0x0284c7),
                TaskState::Success => rgb(0x164e63),
                TaskState::Failed => rgb(0x881337),
                TaskState::Pending => rgb(0x232738),
            })
            .rounded_lg()
            .p_3()
            .flex()
            .flex_col()
            .gap_2p5()
            // Host Card Header
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .items_center()
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
                                    .bg(status_bg)
                                    .border_1()
                                    .border_color(status_border)
                                    .text_color(status_color)
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(10.0))
                                    .child(status_text),
                            )
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(13.0))
                                    .text_color(rgb(0xf1f5f9))
                                    .child(res.host_name.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .when(
                                res.state == TaskState::Success || res.state == TaskState::Failed,
                                |d| {
                                    d.child(
                                        div()
                                            .px_1p5()
                                            .py_0p5()
                                            .bg(rgb(0x181a26))
                                            .rounded_sm()
                                            .text_size(px(10.0))
                                            .text_color(if res.exit_code == Some(0) {
                                                rgb(0x34d399)
                                            } else {
                                                rgb(0xf87171)
                                            })
                                            .child(format!(
                                                "Exit: {:?}",
                                                res.exit_code.unwrap_or(1)
                                            )),
                                    )
                                },
                            )
                            .child(
                                div()
                                    .px_2()
                                    .py_0p5()
                                    .bg(rgb(0x181a26))
                                    .border_1()
                                    .border_color(rgb(0x282b3d))
                                    .rounded_md()
                                    .text_size(px(10.0))
                                    .text_color(rgb(0x38bdf8))
                                    .font_family("Menlo")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        Icon::clock().with_size(px(10.0)).with_color(rgb(0x38bdf8)),
                                    )
                                    .child(duration_display),
                            ),
                    ),
            )
            // Output Window Frame
            .child(
                div()
                    .w_full()
                    .bg(rgb(0x06070a))
                    .border_1()
                    .border_color(rgb(0x181a26))
                    .rounded_md()
                    .flex()
                    .flex_col()
                    // Terminal Mini Titlebar
                    .child(
                        div()
                            .px_2p5()
                            .py_1()
                            .bg(rgb(0x0c0e14))
                            .border_b_1()
                            .border_color(rgb(0x181a26))
                            .flex()
                            .flex_row()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .font_family("Menlo")
                                    .text_color(rgb(0x64748b))
                                    .child("TERMINAL STDOUT / STDERR"),
                            )
                            .child(
                                div()
                                    .id(ElementId::NamedInteger(
                                        "btn_copy_host".into(),
                                        res.duration_us,
                                    ))
                                    .cursor_pointer()
                                    .text_size(px(10.0))
                                    .text_color(rgb(0x64748b))
                                    .hover(|s| s.text_color(rgb(0x38bdf8)))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            copy_text.clone(),
                                        ));
                                        this.toast_message = Some(format!(
                                            "已复制 {} 的控制台输出至剪贴板",
                                            host_name_str
                                        ));
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                Icon::copy()
                                                    .with_size(px(10.0))
                                                    .with_color(rgb(0x64748b)),
                                            )
                                            .child(crate::t!("common.copy")),
                                    ),
                            ),
                    )
                    // Terminal Body Text
                    .child(
                        div()
                            .id(ElementId::Name(
                                format!("terminal_body_{}", res.host_id).into(),
                            ))
                            .w_full()
                            .when(is_grid, |d| d.h(px(180.0)))
                            .when(!is_grid, |d| d.max_h(px(260.0)))
                            .overflow_y_scroll()
                            .p_2p5()
                            .font_family("Menlo")
                            .text_size(px(11.0))
                            .text_color(if res.state == TaskState::Failed {
                                rgb(0xfca5a5)
                            } else if res.state == TaskState::Running {
                                rgb(0x38bdf8)
                            } else if res.state == TaskState::Pending {
                                rgb(0x64748b)
                            } else {
                                rgb(0x94a3b8)
                            })
                            .child(output_content),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{BatchView, BatchViewMode, DEVOPS_PRESETS, ResultFilter, format_duration_detailed};
    use redash_core::session::SessionManager;
    use std::sync::Arc;

    #[test]
    fn test_command_input_char_insert_and_cursor() {
        let mut view = BatchView::new(Vec::new(), Arc::new(SessionManager::new()));
        view.command_input.clear();
        view.cursor_pos = 0;

        view.insert_str_at_cursor("ls -la");
        assert_eq!(view.command_input, "ls -la");
        assert_eq!(view.cursor_pos, 6);

        view.cursor_pos = 3;
        view.insert_char_at_cursor('/');
        assert_eq!(view.command_input, "ls /-la");
        assert_eq!(view.cursor_pos, 4);

        // Unicode multi-byte support
        view.command_input.clear();
        view.cursor_pos = 0;
        view.insert_str_at_cursor("echo 运维测试");
        assert_eq!(view.command_input, "echo 运维测试");
        assert_eq!(view.cursor_pos, 9);
    }

    #[test]
    fn test_command_input_backspace_and_delete() {
        let mut view = BatchView::new(Vec::new(), Arc::new(SessionManager::new()));
        view.command_input = "docker ps".to_string();
        view.cursor_pos = 9;

        view.backspace_at_cursor();
        assert_eq!(view.command_input, "docker p");
        assert_eq!(view.cursor_pos, 8);

        view.cursor_pos = 6;
        view.delete_at_cursor();
        assert_eq!(view.command_input, "dockerp");
        assert_eq!(view.cursor_pos, 6);

        // Edge case: empty string
        view.command_input.clear();
        view.cursor_pos = 0;
        view.backspace_at_cursor();
        view.delete_at_cursor();
        assert_eq!(view.command_input, "");
        assert_eq!(view.cursor_pos, 0);
    }

    #[test]
    fn test_command_input_navigation_and_shortcuts() {
        let mut view = BatchView::new(Vec::new(), Arc::new(SessionManager::new()));
        view.command_input = "systemctl restart nginx".to_string();
        view.cursor_pos = view.command_input.chars().count();

        // Left / Right navigation
        view.move_cursor_left();
        assert_eq!(view.cursor_pos, 22);
        view.move_cursor_right();
        assert_eq!(view.cursor_pos, 23);

        // Home / End
        view.move_cursor_home();
        assert_eq!(view.cursor_pos, 0);
        view.move_cursor_end();
        assert_eq!(view.cursor_pos, 23);

        // Word deletion (Ctrl+W)
        view.delete_word_before_cursor();
        assert_eq!(view.command_input, "systemctl restart ");
        assert_eq!(view.cursor_pos, 18);

        // Clear to start (Ctrl+U)
        view.cursor_pos = 9;
        view.clear_to_start();
        assert_eq!(view.command_input, " restart ");
        assert_eq!(view.cursor_pos, 0);

        // Clear to end (Ctrl+K)
        view.cursor_pos = 4;
        view.clear_to_end();
        assert_eq!(view.command_input, " res");
    }

    #[test]
    fn test_devops_presets_definitions() {
        assert_eq!(DEVOPS_PRESETS.len(), 7);
        for preset in DEVOPS_PRESETS {
            assert!(!preset.label.is_empty(), "Preset label must not be empty");
            assert!(
                !preset.command.is_empty(),
                "Preset command must not be empty"
            );
            assert!(
                !preset.description.is_empty(),
                "Preset description must not be empty"
            );
        }

        let sys_preset = DEVOPS_PRESETS
            .iter()
            .find(|p| p.label.contains("系统概要"))
            .unwrap();
        assert_eq!(sys_preset.command, "uname -a && uptime");

        let disk_preset = DEVOPS_PRESETS
            .iter()
            .find(|p| p.label.contains("磁盘空间"))
            .unwrap();
        assert_eq!(
            disk_preset.command,
            "df -h / /var /home 2>/dev/null || df -h"
        );

        let clean_preset = DEVOPS_PRESETS
            .iter()
            .find(|p| p.label.contains("磁盘深度清理"))
            .unwrap();
        assert_eq!(
            clean_preset.command,
            "journalctl --vacuum-size=100M 2>/dev/null; rm -rf /tmp/*.log 2>/dev/null; df -h /"
        );

        let docker_preset = DEVOPS_PRESETS
            .iter()
            .find(|p| p.label.contains("容器健康检查"))
            .unwrap();
        assert!(docker_preset.command.contains("docker ps"));

        let top_preset = DEVOPS_PRESETS
            .iter()
            .find(|p| p.label.contains("Top 资源消耗"))
            .unwrap();
        assert_eq!(top_preset.command, "ps aux --sort=-%cpu | head -n 8");

        let port_preset = DEVOPS_PRESETS
            .iter()
            .find(|p| p.label.contains("监听端口"))
            .unwrap();
        assert!(port_preset.command.contains("ss -tulpn"));

        let mem_preset = DEVOPS_PRESETS
            .iter()
            .find(|p| p.label.contains("内存诊断"))
            .unwrap();
        assert_eq!(mem_preset.command, "free -h 2>/dev/null || vm_stat");
    }

    #[test]
    fn test_batch_view_mode_toggle_and_filter() {
        let mut view = BatchView::new(Vec::new(), Arc::new(SessionManager::new()));
        assert_eq!(view.view_mode, BatchViewMode::SplitGrid);
        assert_eq!(view.filter, ResultFilter::All);

        view.view_mode = BatchViewMode::List;
        assert_eq!(view.view_mode, BatchViewMode::List);

        view.filter = ResultFilter::FailedOnly;
        assert_eq!(view.filter, ResultFilter::FailedOnly);

        view.filter = ResultFilter::SuccessOnly;
        assert_eq!(view.filter, ResultFilter::SuccessOnly);
    }

    #[test]
    fn test_format_duration_detailed() {
        assert_eq!(format_duration_detailed(0, 0), "0ms");
        assert_eq!(format_duration_detailed(0, 450), "450µs");
        assert_eq!(format_duration_detailed(142, 142350), "142ms (142350µs)");
        assert_eq!(format_duration_detailed(1500, 1500000), "1.50s (1500ms)");
    }
}
