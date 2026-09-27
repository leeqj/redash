use gpui::*;
use std::sync::Arc;
use tokio::sync::mpsc;

use super::emulator::{SelectionRange, TerminalEmulator};
use super::search::TerminalSearch;
use crate::components::icon::Icon;
use crate::components::theme::DarkTechTheme;
use redash_core::probe::agent::{AgentDetector, AgentStatus, DetectedAgent};
use redash_core::session::PtyChannel;

pub const TERM_CHAR_WIDTH: f32 = 7.82666;
pub const TERM_ROW_HEIGHT: f32 = 18.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalKeyAction {
    SendBytes(Vec<u8>),
    Paste,
    Copy,
    Clear,
    Find,
    Scroll(i32),
    AgentApprove,
    AgentReject,
    AgentAlways,
    Ignore,
}

/// Extracts a URL or file path under the clicked cursor token.
pub fn extract_url_or_path(text: &str) -> Option<String> {
    let trimmed = text.trim();
    // 1. HTTP / HTTPS URLs
    if let Some(start) = trimmed.find("http://").or_else(|| trimmed.find("https://")) {
        let candidate = &trimmed[start..];
        let end = candidate
            .find(|c: char| {
                c.is_whitespace() || c == '"' || c == '\'' || c == ')' || c == ']' || c == '>'
            })
            .unwrap_or(candidate.len());
        let url = &candidate[..end];
        let clean = url.trim_end_matches(['.', ',', ';', ':']);
        if clean.starts_with("http://") || clean.starts_with("https://") {
            return Some(clean.to_string());
        }
    }
    // 2. Absolute / Home file paths
    if (trimmed.starts_with('/') || trimmed.starts_with("~/")) && trimmed.len() > 2 {
        let end = trimmed
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ':')
            .unwrap_or(trimmed.len());
        let path = &trimmed[..end];
        return Some(path.to_string());
    }
    None
}

/// Maps keyboard inputs and modifier states to terminal actions or ANSI escape byte sequences.
pub fn map_keystroke(key: &str, modifiers: &Modifiers) -> TerminalKeyAction {
    // 0. Native Clipboard Copy: Cmd+C (macOS) or Ctrl+Shift+C
    if (modifiers.platform && key.eq_ignore_ascii_case("c"))
        || (modifiers.control && modifiers.shift && key.eq_ignore_ascii_case("c"))
    {
        return TerminalKeyAction::Copy;
    }

    // 1. Native Clipboard Paste:
    // Intercept Cmd+V (macOS) or Ctrl+V (Linux/Windows) or Shift+Insert
    if ((modifiers.platform || modifiers.control) && key.eq_ignore_ascii_case("v"))
        || (modifiers.shift && key.eq_ignore_ascii_case("insert"))
    {
        return TerminalKeyAction::Paste;
    }

    // 2. Terminal Clear shortcut: Cmd+K (macOS convention)
    if modifiers.platform && key.eq_ignore_ascii_case("k") {
        return TerminalKeyAction::Clear;
    }

    // 3. In-Terminal Full-Text Search: Cmd+F or Ctrl+F
    if (modifiers.platform || modifiers.control) && key.eq_ignore_ascii_case("f") {
        return TerminalKeyAction::Find;
    }

    // 4. AI Agent Quick Action shortcuts: Cmd+Y, Cmd+N, Cmd+A
    if modifiers.platform && key.eq_ignore_ascii_case("y") {
        return TerminalKeyAction::AgentApprove;
    }
    if modifiers.platform && key.eq_ignore_ascii_case("n") {
        return TerminalKeyAction::AgentReject;
    }
    if modifiers.platform && key.eq_ignore_ascii_case("a") {
        return TerminalKeyAction::AgentAlways;
    }

    // 5. Shift+PageUp / Shift+PageDown: scroll terminal screen
    if modifiers.shift && key.eq_ignore_ascii_case("pageup") {
        return TerminalKeyAction::Scroll(20);
    }
    if modifiers.shift && key.eq_ignore_ascii_case("pagedown") {
        return TerminalKeyAction::Scroll(-20);
    }

    // 4. Translate keystrokes into ANSI escape sequences / control characters
    let bytes: Option<Vec<u8>> = match (key, modifiers.control, modifiers.alt) {
        // Basic control keys
        ("enter", false, false) => Some(b"\r".to_vec()),
        ("backspace", false, false) => Some(b"\x7f".to_vec()),
        ("tab", false, false) => {
            if modifiers.shift {
                Some(b"\x1b[Z".to_vec()) // Backtab
            } else {
                Some(b"\t".to_vec())
            }
        }
        ("escape", false, false) => Some(b"\x1b".to_vec()),
        ("space", false, false) => Some(b" ".to_vec()),
        ("delete", false, false) => Some(b"\x1b[3~".to_vec()),
        ("insert", false, false) => Some(b"\x1b[2~".to_vec()),

        // Navigation keys
        ("up", false, false) => {
            if modifiers.shift {
                Some(b"\x1b[1;2A".to_vec())
            } else {
                Some(b"\x1b[A".to_vec())
            }
        }
        ("down", false, false) => {
            if modifiers.shift {
                Some(b"\x1b[1;2B".to_vec())
            } else {
                Some(b"\x1b[B".to_vec())
            }
        }
        ("right", false, false) => {
            if modifiers.shift {
                Some(b"\x1b[1;2C".to_vec())
            } else {
                Some(b"\x1b[C".to_vec())
            }
        }
        ("left", false, false) => {
            if modifiers.shift {
                Some(b"\x1b[1;2D".to_vec())
            } else {
                Some(b"\x1b[D".to_vec())
            }
        }
        ("home", false, false) => Some(b"\x1b[H".to_vec()),
        ("end", false, false) => Some(b"\x1b[F".to_vec()),
        ("pageup", false, false) => Some(b"\x1b[5~".to_vec()),
        ("pagedown", false, false) => Some(b"\x1b[6~".to_vec()),

        // Ctrl + Navigation keys
        ("up", true, false) => Some(b"\x1b[1;5A".to_vec()),
        ("down", true, false) => Some(b"\x1b[1;5B".to_vec()),
        ("right", true, false) => Some(b"\x1b[1;5C".to_vec()),
        ("left", true, false) => Some(b"\x1b[1;5D".to_vec()),

        // Alt / Option word navigation and editing
        ("left", false, true) | ("b", false, true) => Some(b"\x1bb".to_vec()),
        ("right", false, true) | ("f", false, true) => Some(b"\x1bf".to_vec()),
        ("d", false, true) => Some(b"\x1bd".to_vec()),
        ("c", false, true) => Some(b"\x1bc".to_vec()),
        ("u", false, true) => Some(b"\x1bu".to_vec()),
        ("l", false, true) => Some(b"\x1bl".to_vec()),
        (".", false, true) => Some(b"\x1b.".to_vec()),
        ("backspace", false, true) => Some(b"\x17".to_vec()), // Word delete
        ("delete", false, true) => Some(b"\x1bd".to_vec()),

        // Special Control combinations
        ("backspace", true, _) => Some(b"\x08".to_vec()),
        ("[", true, _) => Some(b"\x1b".to_vec()),
        ("\\", true, _) => Some(b"\x1c".to_vec()),
        ("]", true, _) => Some(b"\x1d".to_vec()),
        ("^", true, _) => Some(b"\x1e".to_vec()),
        ("_", true, _) | ("/", true, _) => Some(b"\x1f".to_vec()),
        ("space", true, _) | ("@", true, _) => Some(b"\x00".to_vec()),

        // Function keys F1 - F12
        ("f1", false, false) => Some(b"\x1bOP".to_vec()),
        ("f2", false, false) => Some(b"\x1bOQ".to_vec()),
        ("f3", false, false) => Some(b"\x1bOR".to_vec()),
        ("f4", false, false) => Some(b"\x1bOS".to_vec()),
        ("f5", false, false) => Some(b"\x1b[15~".to_vec()),
        ("f6", false, false) => Some(b"\x1b[17~".to_vec()),
        ("f7", false, false) => Some(b"\x1b[18~".to_vec()),
        ("f8", false, false) => Some(b"\x1b[19~".to_vec()),
        ("f9", false, false) => Some(b"\x1b[20~".to_vec()),
        ("f10", false, false) => Some(b"\x1b[21~".to_vec()),
        ("f11", false, false) => Some(b"\x1b[23~".to_vec()),
        ("f12", false, false) => Some(b"\x1b[24~".to_vec()),

        // All A-Z Control combinations (Ctrl+A = 1, ..., Ctrl+Z = 26)
        _ if modifiers.control && key.len() == 1 => {
            let ch = key.chars().next().unwrap();
            if ch.is_ascii_alphabetic() {
                Some(vec![ch.to_ascii_lowercase() as u8 - b'a' + 1])
            } else {
                None
            }
        }

        // Alt + single character sends ESC + char
        _ if modifiers.alt && !modifiers.control && !modifiers.platform && key.len() == 1 => {
            let mut bytes = vec![0x1b];
            bytes.extend_from_slice(key.as_bytes());
            Some(bytes)
        }

        // Normal printable characters
        _ => {
            if !modifiers.control && !modifiers.platform && key.chars().count() == 1 {
                Some(key.as_bytes().to_vec())
            } else {
                None
            }
        }
    };

    match bytes {
        Some(b) => TerminalKeyAction::SendBytes(b),
        None => TerminalKeyAction::Ignore,
    }
}

pub struct TerminalView {
    pub connection_error: Option<String>,
    input_error: std::sync::Mutex<Option<String>>,
    pub cursor_style: String,
    pub copy_on_select: bool,
    notify_agent: bool,
    pub pty_channel: Option<Arc<PtyChannel>>,
    pub emulator: TerminalEmulator,
    focus_handle: FocusHandle,
    pub host_name: String,
    pub font_family: String,
    pub font_size: f32,
    pub detected_agent: Option<DetectedAgent>,
    pub active_window_title: Option<String>,
    pub recent_output_buf: String,
    pub selection: Option<SelectionRange>,
    pub search: TerminalSearch,
}

#[allow(dead_code)]
impl TerminalView {
    pub fn new(
        cols: usize,
        rows: usize,
        pty_channel: Option<Arc<PtyChannel>>,
        output_rx: mpsc::Receiver<Vec<u8>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let emulator = TerminalEmulator::new(cols, rows);
        let focus_handle = cx.focus_handle();

        cx.spawn(async move |this, cx| {
            let mut rx = output_rx;
            while let Some(bytes) = rx.recv().await {
                let update_res = this.update(cx, |view, cx| {
                    // 1. Advance terminal emulation
                    view.emulator.process_input(&bytes);
                    view.flush_responses();

                    // If search is currently active, re-index matches against new output
                    if view.search.is_active && !view.search.query.is_empty() {
                        let lines = view.emulator.screen_lines_as_strings();
                        view.search.set_query(view.search.query.clone(), &lines);
                    }

                    // 2. Extract OSC titles (e.g. \x1b]0;Claude Code\x07)
                    let osc_titles = AgentDetector::extract_osc_titles(&bytes);
                    if let Some(latest) = osc_titles.last() {
                        view.active_window_title = Some(latest.clone());
                    }

                    // 3. Maintain recent output buffer for AI Agent prompt scanning
                    let text = String::from_utf8_lossy(&bytes);
                    view.recent_output_buf.push_str(&text);
                    if view.recent_output_buf.len() > 4096 {
                        let excess = view.recent_output_buf.len() - 4096;
                        let safe_boundary = (excess..=view.recent_output_buf.len())
                            .find(|&idx| view.recent_output_buf.is_char_boundary(idx))
                            .unwrap_or(view.recent_output_buf.len());
                        view.recent_output_buf.drain(..safe_boundary);
                    }

                    // 4. Multi-tier Agent Detection
                    if let Some(agent) = AgentDetector::detect(
                        None,
                        view.active_window_title.as_deref(),
                        &view.recent_output_buf,
                    ) {
                        let prev_status = view.detected_agent.as_ref().map(|a| a.status);
                        let new_status = agent.status;
                        view.detected_agent = Some(agent.clone());

                        // Dispatch desktop alert when transition to NeedsInput or Done occurs
                        if view.notify_agent && prev_status != Some(new_status) {
                            if new_status == AgentStatus::NeedsInput {
                                let host_label = if view.host_name.is_empty() { "远程服务器".to_string() } else { view.host_name.clone() };
                                let title = format!("{} 正在等待授权", agent.name);
                                let msg = format!("主机 [{}] 上的 {} 正在等待您的确认或输入", host_label, agent.name);
                                tokio::spawn(async move {
                                    let _ = redash_core::config::alert::AlertDispatcher::send_macos_notification(&title, &msg).await;
                                });
                            } else if new_status == AgentStatus::Done {
                                let host_label = if view.host_name.is_empty() { "远程服务器".to_string() } else { view.host_name.clone() };
                                let title = format!("{} 任务完成", agent.name);
                                let msg = format!("主机 [{}] 上的 {} 本轮任务执行完毕", host_label, agent.name);
                                tokio::spawn(async move {
                                    let _ = redash_core::config::alert::AlertDispatcher::send_macos_notification(&title, &msg).await;
                                });
                            }
                        }
                    }

                    cx.notify();
                });
                if update_res.is_err() {
                    break;
                }
            }
            let _ = this.update(cx, |view, cx| {
                view.detected_agent = None;
                view.pty_channel = None;
                view.connection_error.get_or_insert("终端已断开，请使用工作台的重新连接按钮".into());
                cx.notify();
            });
        }).detach();

        Self {
            pty_channel,
            connection_error: None,
            input_error: std::sync::Mutex::new(None),
            cursor_style: "Block".into(),
            copy_on_select: false,
            notify_agent: true,
            emulator,
            focus_handle,
            host_name: String::new(),
            font_family: "Menlo".to_string(),
            font_size: 13.0,
            detected_agent: None,
            active_window_title: None,
            recent_output_buf: String::with_capacity(4096),
            selection: None,
            search: TerminalSearch::new(),
        }
    }

    pub fn with_host_name(mut self, host_name: String) -> Self {
        self.host_name = host_name;
        self
    }

    pub fn attach_pty(&mut self, pty: Arc<PtyChannel>, cx: &mut Context<Self>) {
        if pty.is_closed() {
            self.connection_error = Some("终端已关闭，请重新连接".into());
        } else {
            self.pty_channel = Some(pty);
            self.connection_error = None;
            self.flush_responses();
        }
        cx.notify();
    }
    fn flush_responses(&self) {
        if self.pty_channel.is_some() {
            for response in self.emulator.drain_responses() {
                self.send_input(&response);
            }
        }
    }
    pub fn apply_settings(
        &mut self,
        settings: &redash_core::config::AppSettings,
        cx: &mut Context<Self>,
    ) {
        self.cursor_style = settings.terminal_cursor_style.clone();
        self.copy_on_select = settings.terminal_copy_on_select;
        self.notify_agent = settings.alert_macos_notification;
        self.emulator
            .set_scrollback(settings.terminal_scrollback_lines);
        self.update_font(
            settings.terminal_font_family.clone(),
            settings.terminal_font_size,
            cx,
        );
    }

    pub fn update_font(&mut self, font_family: String, font_size: f32, cx: &mut Context<Self>) {
        self.font_family = font_family;
        self.font_size = font_size;
        cx.notify();
    }

    /// Feeds raw terminal bytes directly (useful for testing or local replay)
    pub fn process_input_bytes(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        self.emulator.process_input(bytes);
        let osc_titles = AgentDetector::extract_osc_titles(bytes);
        if let Some(latest) = osc_titles.last() {
            self.active_window_title = Some(latest.clone());
        }
        let text = String::from_utf8_lossy(bytes);
        self.recent_output_buf.push_str(&text);
        if self.recent_output_buf.len() > 4096 {
            let excess = self.recent_output_buf.len() - 4096;
            self.recent_output_buf.drain(..excess);
        }
        if let Some(agent) = AgentDetector::detect(
            None,
            self.active_window_title.as_deref(),
            &self.recent_output_buf,
        ) {
            self.detected_agent = Some(agent);
        }
        cx.notify();
    }

    /// Inspects running host processes to proactively detect AI agents running in this session.
    /// Expose focus activation on the window
    pub fn focus(&self, window: &mut Window) {
        window.focus(&self.focus_handle);
    }

    /// Check if this view currently has input focus
    pub fn is_focused(&self, window: &Window) -> bool {
        self.focus_handle.is_focused(window)
    }

    /// Access the underlying FocusHandle
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// Resize terminal emulation grid and propagate window change to remote SSH PTY
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.clamp(20, 500);
        let rows = rows.clamp(5, 200);
        if cols == self.emulator.cols && rows == self.emulator.rows {
            return;
        }
        self.emulator.resize(cols, rows);
        if let Some(pty) = &self.pty_channel {
            let pty = Arc::clone(pty);
            tokio::spawn(async move {
                let _ = pty.resize(cols as u32, rows as u32).await;
            });
        }
    }

    /// Send input bytes asynchronously to PTY actor
    pub fn send_input(&self, data: &[u8]) {
        let result = self
            .pty_channel
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("终端尚未连接"))
            .and_then(|pty| pty.try_send_data(data));
        *self.input_error.lock().unwrap() = result.err().map(|error| format!("{error:#}"));
    }

    /// Read text from system clipboard and send UTF-8 bytes to PTY channel
    pub fn paste_from_clipboard(&self, cx: &App) {
        if let Some(item) = cx.read_from_clipboard()
            && let Some(text) = item.text()
            && !text.is_empty()
        {
            self.send_input(&self.emulator.prepare_paste(&text));
        }
    }

    /// Clear screen (send Form Feed \x0c)
    pub fn clear_screen(&self) {
        self.send_input(b"\x0c");
    }

    /// Toggle terminal search overlay
    pub fn toggle_search(&mut self, cx: &mut Context<Self>) {
        self.search.is_active = !self.search.is_active;
        if self.search.is_active {
            let lines = self.emulator.screen_lines_as_strings();
            self.search.set_query(self.search.query.clone(), &lines);
        }
        cx.notify();
    }

    /// Scroll to the bottom of the terminal buffer
    pub fn scroll_to_bottom(&mut self, cx: &mut Context<Self>) {
        self.emulator.scroll(-100_000);
        cx.notify();
    }

    /// Scroll terminal buffer by delta lines
    pub fn scroll(&mut self, delta: i32, cx: &mut Context<Self>) {
        self.emulator.scroll(delta);
        cx.notify();
    }

    pub fn cols(&self) -> usize {
        self.emulator.cols
    }

    pub fn rows(&self) -> usize {
        self.emulator.rows
    }

    pub fn is_connected(&self) -> bool {
        self.pty_channel
            .as_ref()
            .is_some_and(|pty| !pty.is_closed())
    }

    pub fn is_agent_needing_input(&self) -> bool {
        self.detected_agent
            .as_ref()
            .is_some_and(|a| a.status == AgentStatus::NeedsInput)
            && self
                .emulator
                .screen_lines_as_strings()
                .iter()
                .rev()
                .find(|line| !line.trim().is_empty())
                .is_some_and(|line| {
                    let text = line.to_lowercase();
                    text.trim_end_matches([':', '?', ' ']).ends_with("[y/n]")
                        || text.trim_end_matches([':', '?', ' ']).ends_with("(y/n)")
                })
    }

    pub fn copy_selection_to_clipboard(&self, cx: &App) -> bool {
        if let Some(sel) = &self.selection
            && !sel.is_empty()
        {
            let text = self.emulator.get_selected_text(sel);
            if !text.is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                return true;
            }
        }
        false
    }

    fn handle_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = &event.keystroke.modifiers;

        // In-Terminal Search Intercept
        if self.search.is_active {
            if key == "escape" {
                self.search.close();
                cx.notify();
                cx.stop_propagation();
                return;
            }
            if key == "enter" || key == "down" {
                if modifiers.shift {
                    self.search.prev();
                } else {
                    self.search.next();
                }
                cx.notify();
                cx.stop_propagation();
                return;
            }
            if key == "up" {
                self.search.prev();
                cx.notify();
                cx.stop_propagation();
                return;
            }
            if key == "backspace" {
                self.search.query.pop();
                let lines = self.emulator.screen_lines_as_strings();
                self.search.set_query(self.search.query.clone(), &lines);
                cx.notify();
                cx.stop_propagation();
                return;
            }
            if (modifiers.platform || modifiers.control) && key.eq_ignore_ascii_case("f") {
                self.search.close();
                cx.notify();
                cx.stop_propagation();
                return;
            }
            if !modifiers.control
                && !modifiers.platform
                && !modifiers.alt
                && key.chars().count() == 1
            {
                self.search.query.push_str(key);
                let lines = self.emulator.screen_lines_as_strings();
                self.search.set_query(self.search.query.clone(), &lines);
                cx.notify();
                cx.stop_propagation();
                return;
            }
            // Prevent all other keystrokes from leaking into remote shell while search overlay is open
            cx.stop_propagation();
            return;
        }

        match map_keystroke(key, modifiers) {
            TerminalKeyAction::Copy => {
                let _ = self.copy_selection_to_clipboard(cx);
                cx.stop_propagation();
            }
            TerminalKeyAction::Paste => {
                self.paste_from_clipboard(cx);
                cx.stop_propagation();
            }
            TerminalKeyAction::Clear => {
                self.clear_screen();
                cx.stop_propagation();
            }
            TerminalKeyAction::Find => {
                self.search.is_active = !self.search.is_active;
                if self.search.is_active {
                    let lines = self.emulator.screen_lines_as_strings();
                    self.search.set_query(self.search.query.clone(), &lines);
                }
                cx.notify();
                cx.stop_propagation();
            }
            TerminalKeyAction::AgentApprove => {
                if self.is_agent_needing_input() {
                    self.send_input(b"y\n");
                    cx.stop_propagation();
                }
            }
            TerminalKeyAction::AgentReject => {
                if self.is_agent_needing_input() {
                    self.send_input(b"n\n");
                    cx.stop_propagation();
                }
            }
            TerminalKeyAction::AgentAlways => {}
            TerminalKeyAction::Scroll(delta) => {
                self.emulator.scroll(delta);
                cx.notify();
                cx.stop_propagation();
            }
            TerminalKeyAction::SendBytes(bytes) => {
                if self.selection.is_some() {
                    self.selection = None;
                    cx.notify();
                }
                let mut bytes = bytes;
                if self.emulator.application_cursor()
                    && bytes.len() == 3
                    && bytes.starts_with(b"\x1b[")
                    && b"ABCDHF".contains(&bytes[2])
                {
                    bytes[1] = b'O';
                }
                if !modifiers.control
                    && !modifiers.platform
                    && !modifiers.alt
                    && let Some(text) = &event.keystroke.key_char
                    && !text.is_empty()
                    && !text.chars().any(char::is_control)
                {
                    bytes = text.as_bytes().to_vec();
                }
                self.send_input(&bytes);
                cx.notify();
                cx.stop_propagation();
            }
            TerminalKeyAction::Ignore => {}
        }
    }
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let cursor_opt = self.emulator.cursor_position();
        let search_ref = if self.search.is_active {
            Some(&self.search)
        } else {
            None
        };
        let lines = self
            .emulator
            .renderable_runs(cursor_opt, self.selection.as_ref(), search_ref);
        let is_focused = self.focus_handle.is_focused(window);
        let is_connected = self.is_connected();
        let cursor_style = self.cursor_style.clone();
        let has_selection = self.selection.is_some();

        div()
            .id("terminal_root_container")
            .relative()
            .track_focus(&self.focus_handle)
            // Click-to-focus on left mouse click anywhere
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseDownEvent, window, _cx| {
                    window.focus(&this.focus_handle);
                }),
            )
            // Right-click paste support: focuses terminal and pastes clipboard
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle);
                    this.paste_from_clipboard(cx);
                }),
            )
            // Keyboard shortcuts & input listener
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                this.handle_key_down(event, cx);
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(DarkTechTheme::bg_root())
            // Visual Focus feedback: Glowing 1px border when focused, subtle border when unfocused
            .border_1()
            .border_color(if is_focused {
                DarkTechTheme::border_active()
            } else {
                DarkTechTheme::border_default()
            })
            .overflow_hidden()
            .children(self.connection_error.as_ref().map(|error| {
                div()
                    .p_2()
                    .text_xs()
                    .text_color(DarkTechTheme::status_warn())
                    .child(error.clone())
            }))
            .children(self.input_error.lock().unwrap().clone().map(|error| {
                div()
                    .p_2()
                    .text_xs()
                    .text_color(DarkTechTheme::status_warn())
                    .child(error)
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.copy_on_select {
                        this.copy_selection_to_clipboard(cx);
                    }
                }),
            )
            // 1. High-Performance Terminal Screen Viewport (TextRun batched spans)
            .child(
                div()
                    .id("terminal_screen_viewport")
                    .flex_1()
                    .w_full()
                    .p_2()
                    .flex()
                    .flex_col()
                    .bg(DarkTechTheme::bg_root())
                    .font_family(self.font_family.clone())
                    .text_size(px(self.font_size))
                    .line_height(px(TERM_ROW_HEIGHT))
                    .text_color(DarkTechTheme::text_primary())
                    .overflow_hidden()
                    .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _window, cx| {
                        let delta_y = match event.delta {
                            ScrollDelta::Lines(delta) => delta.y as i32,
                            ScrollDelta::Pixels(delta) => {
                                (f32::from(delta.y) / TERM_ROW_HEIGHT) as i32
                            }
                        };
                        if delta_y != 0 {
                            this.emulator.scroll(delta_y * 3);
                            cx.notify();
                        }
                    }))
                    .children(lines.into_iter().enumerate().map(|(line_idx, line)| {
                        let mut row_el = div()
                            .id(ElementId::Name(format!("term_row_{}", line_idx).into()))
                            .flex()
                            .flex_row()
                            .h(px(TERM_ROW_HEIGHT))
                            .min_h(px(TERM_ROW_HEIGHT))
                            .max_h(px(TERM_ROW_HEIGHT))
                            .w_full()
                            .flex_shrink_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                    window.focus(&this.focus_handle);
                                    if !event.modifiers.shift {
                                        this.selection = None;
                                        cx.notify();
                                    }
                                }),
                            );

                        if !line.runs.is_empty() {
                            let mut current_col = 0;
                            row_el = row_el.children(line.runs.into_iter().map(|run| {
                                let start_col = current_col;
                                let end_col = current_col + run.cols;
                                current_col += run.cols;

                                let width_px = (run.cols as f32) * TERM_CHAR_WIDTH;
                                let run_text = run.text.clone();
                                let line_num = line_idx;

                                if run.is_cursor {
                                    if is_focused {
                                        // Glowing bright cyan filled cursor block
                                        let cursor = div()
                                            .w(px(width_px))
                                            .min_w(px(width_px))
                                            .h(px(TERM_ROW_HEIGHT))
                                            .flex_shrink_0()
                                            .whitespace_nowrap()
                                            .overflow_hidden()
                                            .text_color(DarkTechTheme::text_primary())
                                            .child(run.text);
                                        match cursor_style.as_str() {
                                            "Line" => cursor
                                                .border_l_2()
                                                .border_color(DarkTechTheme::accent_cyan()),
                                            "Underline" => cursor
                                                .border_b_2()
                                                .border_color(DarkTechTheme::accent_cyan()),
                                            _ => cursor
                                                .bg(DarkTechTheme::accent_cyan())
                                                .text_color(DarkTechTheme::bg_root()),
                                        }
                                        .into_any_element()
                                    } else {
                                        // Hollow / dimmed cursor when unfocused
                                        div()
                                            .w(px(width_px))
                                            .min_w(px(width_px))
                                            .h(px(TERM_ROW_HEIGHT))
                                            .flex_shrink_0()
                                            .whitespace_nowrap()
                                            .overflow_hidden()
                                            .border_1()
                                            .border_color(DarkTechTheme::text_muted())
                                            .bg(DarkTechTheme::bg_panel_hover())
                                            .text_color(DarkTechTheme::text_primary())
                                            .child(run.text)
                                            .into_any_element()
                                    }
                                } else {
                                    let mut span = div()
                                        .id(ElementId::Name(
                                            format!("cell_{}_{}", line_num, start_col).into(),
                                        ))
                                        .min_w(px(width_px))
                                        .h(px(TERM_ROW_HEIGHT))
                                        .flex_shrink_0()
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .text_color(rgb(run.fg))
                                        .cursor_text()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(
                                                move |this, event: &MouseDownEvent, window, cx| {
                                                    window.focus(&this.focus_handle);
                                                    // 1. Smart URL & Path Detection on Cmd+Click
                                                    if event.modifiers.platform
                                                        && let Some(target) =
                                                            extract_url_or_path(&run_text)
                                                        && (target.starts_with("http://")
                                                            || target.starts_with("https://"))
                                                    {
                                                        let _ = std::process::Command::new("open")
                                                            .arg(&target)
                                                            .spawn();
                                                        return;
                                                    }

                                                    // 2. Selection & Shift-extend
                                                    if event.modifiers.shift {
                                                        if let Some(existing) = &mut this.selection
                                                        {
                                                            existing.end = (end_col, line_num);
                                                        } else {
                                                            this.selection =
                                                                Some(SelectionRange::new(
                                                                    start_col, line_num, end_col,
                                                                    line_num,
                                                                ));
                                                        }
                                                    } else {
                                                        this.selection = Some(SelectionRange::new(
                                                            start_col, line_num, end_col, line_num,
                                                        ));
                                                    }
                                                    cx.notify();
                                                },
                                            ),
                                        );

                                    if let Some(bg) = run.bg {
                                        span = span.bg(rgb(bg));
                                    }
                                    if run.bold {
                                        span = span.font_weight(FontWeight::BOLD);
                                    }
                                    if run.underline {
                                        span = span.underline();
                                    }

                                    span.child(run.text).into_any_element()
                                }
                            }));
                        } else {
                            row_el = row_el
                                .child(div().h(px(TERM_ROW_HEIGHT)).w(px(0.0)).flex_shrink_0());
                        }

                        row_el
                    })),
            )
            // 2. Low-Latency 24px Bottom Status Bar
            .child(
                div()
                    .id("terminal_bottom_status_bar")
                    .h(px(24.0))
                    .w_full()
                    .bg(DarkTechTheme::bg_input())
                    .border_t_1()
                    .border_color(DarkTechTheme::border_default())
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .font_family("Menlo")
                    .text_size(px(11.0))
                    .line_height(px(24.0))
                    // Left section: PTY status, AI Agent Badge, Token/Cost HUD
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            // Dot indicator
                            .child(
                                div()
                                    .text_size(px(8.0))
                                    .text_color(if is_connected {
                                        DarkTechTheme::status_online()
                                    } else {
                                        DarkTechTheme::status_crit()
                                    })
                                    .child(if is_connected { "●" } else { "○" }),
                            )
                            // PTY status text
                            .child(
                                div()
                                    .text_color(if is_connected {
                                        DarkTechTheme::text_secondary()
                                    } else {
                                        DarkTechTheme::text_muted()
                                    })
                                    .child(if let Some(pty) = &self.pty_channel {
                                        format!(
                                            "PTY: Connected (xterm-256color) [Ch #{}]",
                                            pty.channel_id
                                        )
                                    } else {
                                        "PTY: Disconnected".to_string()
                                    }),
                            )
                            // AI Agent Awareness Badge (if detected)
                            .children(self.detected_agent.as_ref().map(|agent| {
                                div()
                                    .id("pill_terminal_ai_agent")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .px_2()
                                    .py_0p5()
                                    .rounded_sm()
                                    .bg(match agent.status {
                                        AgentStatus::NeedsInput => rgb(0x451a03),
                                        AgentStatus::Thinking => rgb(0x083344),
                                        AgentStatus::Done => rgb(0x064e3b),
                                        AgentStatus::Idle => rgb(DarkTechTheme::BG_PANEL),
                                    })
                                    .border_1()
                                    .border_color(rgb(agent.status.color_rgb()))
                                    .child(
                                        div()
                                            .text_size(px(8.0))
                                            .text_color(rgb(agent.status.color_rgb()))
                                            .child("●"),
                                    )
                                    .child(
                                        div()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(agent.status.color_rgb()))
                                            .child(format!(
                                                "{} [{}]",
                                                agent.name,
                                                agent.status.label()
                                            )),
                                    )
                            }))
                            // Token & Cost HUD
                            .children(self.detected_agent.as_ref().and_then(|agent| {
                                if agent.cost_usd.is_some() || agent.tokens.is_some() {
                                    let mut meter = div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_2()
                                        .px_2()
                                        .py_0p5()
                                        .rounded_sm()
                                        .bg(DarkTechTheme::bg_root())
                                        .border_1()
                                        .border_color(DarkTechTheme::border_default())
                                        .text_size(px(10.0));

                                    if let Some(cost) = agent.cost_usd {
                                        meter = meter.child(
                                            div()
                                                .text_color(rgb(0x34d399))
                                                .child(format!("💰 识别值 ${:.3}", cost)),
                                        );
                                    }
                                    if let Some(toks) = agent.tokens {
                                        let label = if toks >= 1000 {
                                            format!("{:.1}k", toks as f64 / 1000.0)
                                        } else {
                                            format!("{}", toks)
                                        };
                                        meter = meter.child(
                                            div()
                                                .text_color(DarkTechTheme::accent_cyan())
                                                .child(format!("⚡ 识别值 {} tokens", label)),
                                        );
                                    }
                                    Some(meter)
                                } else {
                                    None
                                }
                            })),
                    )
                    // Center section: Cols x Rows & UTF-8
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .text_color(DarkTechTheme::text_muted())
                            .child(div().text_color(DarkTechTheme::text_secondary()).child(
                                format!(
                                    "Cols: {} × Rows: {}",
                                    self.emulator.cols, self.emulator.rows
                                ),
                            ))
                            .child(div().text_color(DarkTechTheme::border_muted()).child("•"))
                            .child(
                                div()
                                    .text_color(DarkTechTheme::accent_cyan())
                                    .child("Encoding: UTF-8"),
                            ),
                    )
                    // Right section: Quick action pills
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            // Pill 1: Clear screen
                            .child(
                                div()
                                    .id("btn_pill_clear_screen")
                                    .px_2()
                                    .py_0p5()
                                    .rounded_sm()
                                    .bg(DarkTechTheme::bg_input())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_default())
                                    .text_color(DarkTechTheme::text_secondary())
                                    .text_size(px(10.0))
                                    .cursor_pointer()
                                    .hover(|s| {
                                        s.bg(DarkTechTheme::bg_panel_hover())
                                            .border_color(DarkTechTheme::accent_cyan())
                                            .text_color(DarkTechTheme::accent_cyan())
                                    })
                                    .on_click(cx.listener(|this, _e: &ClickEvent, window, cx| {
                                        window.focus(&this.focus_handle);
                                        this.clear_screen();
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                Icon::clear()
                                                    .with_size(px(10.0))
                                                    .with_color(DarkTechTheme::text_secondary()),
                                            )
                                            .child("清屏"),
                                    ),
                            )
                            // Pill 2: Copy Selection (if available)
                            .child(
                                div()
                                    .id("btn_pill_copy")
                                    .px_2()
                                    .py_0p5()
                                    .rounded_sm()
                                    .bg(if has_selection {
                                        DarkTechTheme::accent_cyan().opacity(0.2)
                                    } else {
                                        DarkTechTheme::bg_input()
                                    })
                                    .border_1()
                                    .border_color(if has_selection {
                                        DarkTechTheme::accent_cyan()
                                    } else {
                                        DarkTechTheme::border_default()
                                    })
                                    .text_color(if has_selection {
                                        DarkTechTheme::accent_cyan()
                                    } else {
                                        DarkTechTheme::text_secondary()
                                    })
                                    .text_size(px(10.0))
                                    .cursor_pointer()
                                    .hover(|s| {
                                        s.bg(DarkTechTheme::bg_panel_hover())
                                            .border_color(DarkTechTheme::accent_cyan())
                                            .text_color(DarkTechTheme::accent_cyan())
                                    })
                                    .on_click(cx.listener(|this, _e: &ClickEvent, window, cx| {
                                        window.focus(&this.focus_handle);
                                        let _ = this.copy_selection_to_clipboard(cx);
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(Icon::copy().with_size(px(10.0)).with_color(
                                                if has_selection {
                                                    DarkTechTheme::accent_cyan()
                                                } else {
                                                    DarkTechTheme::text_secondary()
                                                },
                                            ))
                                            .child("复制"),
                                    ),
                            )
                            // Pill 3: Paste
                            .child(
                                div()
                                    .id("btn_pill_paste")
                                    .px_2()
                                    .py_0p5()
                                    .rounded_sm()
                                    .bg(DarkTechTheme::bg_input())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_default())
                                    .text_color(DarkTechTheme::text_secondary())
                                    .text_size(px(10.0))
                                    .cursor_pointer()
                                    .hover(|s| {
                                        s.bg(DarkTechTheme::bg_panel_hover())
                                            .border_color(DarkTechTheme::accent_cyan())
                                            .text_color(DarkTechTheme::accent_cyan())
                                    })
                                    .on_click(cx.listener(|this, _e: &ClickEvent, window, cx| {
                                        window.focus(&this.focus_handle);
                                        this.paste_from_clipboard(cx);
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
                                                    .with_color(DarkTechTheme::text_secondary()),
                                            )
                                            .child("粘贴"),
                                    ),
                            )
                            // Pill 4: In-Terminal Search Toggle
                            .child(
                                div()
                                    .id("btn_pill_search")
                                    .px_2()
                                    .py_0p5()
                                    .rounded_sm()
                                    .bg(if self.search.is_active {
                                        DarkTechTheme::bg_panel_hover()
                                    } else {
                                        DarkTechTheme::bg_input()
                                    })
                                    .border_1()
                                    .border_color(if self.search.is_active {
                                        DarkTechTheme::accent_cyan()
                                    } else {
                                        DarkTechTheme::border_default()
                                    })
                                    .text_color(if self.search.is_active {
                                        DarkTechTheme::accent_cyan()
                                    } else {
                                        DarkTechTheme::text_secondary()
                                    })
                                    .text_size(px(10.0))
                                    .cursor_pointer()
                                    .hover(|s| {
                                        s.bg(DarkTechTheme::bg_panel_hover())
                                            .border_color(DarkTechTheme::accent_cyan())
                                            .text_color(DarkTechTheme::accent_cyan())
                                    })
                                    .on_click(cx.listener(|this, _e: &ClickEvent, window, cx| {
                                        window.focus(&this.focus_handle);
                                        this.search.is_active = !this.search.is_active;
                                        if this.search.is_active {
                                            let lines = this.emulator.screen_lines_as_strings();
                                            this.search
                                                .set_query(this.search.query.clone(), &lines);
                                        }
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(Icon::search().with_size(px(10.0)).with_color(
                                                if self.search.is_active {
                                                    DarkTechTheme::accent_cyan()
                                                } else {
                                                    DarkTechTheme::text_secondary()
                                                },
                                            ))
                                            .child("搜索"),
                                    ),
                            )
                            // Pill 5: Scroll to bottom
                            .child(
                                div()
                                    .id("btn_pill_scroll_bottom")
                                    .px_2()
                                    .py_0p5()
                                    .rounded_sm()
                                    .bg(DarkTechTheme::bg_input())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_default())
                                    .text_color(DarkTechTheme::text_secondary())
                                    .text_size(px(10.0))
                                    .cursor_pointer()
                                    .hover(|s| {
                                        s.bg(DarkTechTheme::bg_panel_hover())
                                            .border_color(DarkTechTheme::accent_cyan())
                                            .text_color(DarkTechTheme::accent_cyan())
                                    })
                                    .on_click(cx.listener(|this, _e: &ClickEvent, window, cx| {
                                        window.focus(&this.focus_handle);
                                        this.scroll_to_bottom(cx);
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                Icon::arrow_down()
                                                    .with_size(px(10.0))
                                                    .with_color(DarkTechTheme::text_secondary()),
                                            )
                                            .child("到底部"),
                                    ),
                            ),
                    ),
            )
            // 3. Floating In-Terminal Search Overlay (Top-Right)
            .children(if self.search.is_active {
                Some(
                    div()
                        .id("floating_terminal_search_bar")
                        .absolute()
                        .top(px(10.0))
                        .right(px(16.0))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .py_1p5()
                        .rounded_md()
                        .bg(rgb(0x0e0f17))
                        .border_1()
                        .border_color(DarkTechTheme::accent_cyan())
                        .shadow_lg()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1p5()
                                .child(
                                    Icon::search()
                                        .with_size(px(12.0))
                                        .with_color(DarkTechTheme::accent_cyan()),
                                )
                                .child(
                                    div()
                                        .min_w(px(120.0))
                                        .max_w(px(240.0))
                                        .text_size(px(12.0))
                                        .font_family("Menlo")
                                        .text_color(if self.search.query.is_empty() {
                                            DarkTechTheme::text_muted()
                                        } else {
                                            DarkTechTheme::text_primary()
                                        })
                                        .child(if self.search.query.is_empty() {
                                            "输入关键词搜索...".to_string()
                                        } else {
                                            self.search.query.clone()
                                        }),
                                ),
                        )
                        // Match counter pill
                        .child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded_sm()
                                .bg(DarkTechTheme::bg_input())
                                .border_1()
                                .border_color(DarkTechTheme::border_default())
                                .text_size(px(10.0))
                                .text_color(if self.search.matches.is_empty() {
                                    DarkTechTheme::text_muted()
                                } else {
                                    DarkTechTheme::accent_cyan()
                                })
                                .child(if self.search.matches.is_empty() {
                                    "0/0".to_string()
                                } else {
                                    self.search.match_summary()
                                }),
                        )
                        // Prev match button
                        .child(
                            div()
                                .id("btn_search_prev")
                                .px_1p5()
                                .py_0p5()
                                .rounded_sm()
                                .cursor_pointer()
                                .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.search.prev();
                                    cx.notify();
                                }))
                                .child(
                                    Icon::arrow_up()
                                        .with_size(px(10.0))
                                        .with_color(DarkTechTheme::text_secondary()),
                                ),
                        )
                        // Next match button
                        .child(
                            div()
                                .id("btn_search_next")
                                .px_1p5()
                                .py_0p5()
                                .rounded_sm()
                                .cursor_pointer()
                                .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.search.next();
                                    cx.notify();
                                }))
                                .child(
                                    Icon::arrow_down()
                                        .with_size(px(10.0))
                                        .with_color(DarkTechTheme::text_secondary()),
                                ),
                        )
                        // Case sensitive toggle
                        .child(
                            div()
                                .id("btn_search_case")
                                .px_1p5()
                                .py_0p5()
                                .rounded_sm()
                                .cursor_pointer()
                                .bg(if self.search.case_sensitive {
                                    DarkTechTheme::bg_panel_hover()
                                } else {
                                    DarkTechTheme::bg_root()
                                })
                                .border_1()
                                .border_color(if self.search.case_sensitive {
                                    DarkTechTheme::accent_cyan()
                                } else {
                                    DarkTechTheme::border_default()
                                })
                                .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let lines = this.emulator.screen_lines_as_strings();
                                    this.search.toggle_case_sensitive(&lines);
                                    cx.notify();
                                }))
                                .child(
                                    div()
                                        .font_family("Menlo")
                                        .text_size(px(10.0))
                                        .text_color(if self.search.case_sensitive {
                                            DarkTechTheme::accent_cyan()
                                        } else {
                                            DarkTechTheme::text_muted()
                                        })
                                        .child("Aa"),
                                ),
                        )
                        // Close search button
                        .child(
                            div()
                                .id("btn_search_close")
                                .px_1p5()
                                .py_0p5()
                                .rounded_sm()
                                .cursor_pointer()
                                .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.search.close();
                                    cx.notify();
                                }))
                                .child(
                                    Icon::close()
                                        .with_size(px(10.0))
                                        .with_color(DarkTechTheme::text_secondary()),
                                ),
                        ),
                )
            } else {
                None
            })
            // 4. Floating AI Agent Quick Action Card (Bottom-Right, above status bar)
            .children(if let Some(agent) = &self.detected_agent {
                if self.is_agent_needing_input() {
                    Some(
                        div()
                            .id("floating_agent_action_bar")
                            .absolute()
                            .bottom(px(32.0))
                            .right(px(16.0))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2p5()
                            .px_3()
                            .py_2()
                            .rounded_md()
                            .bg(rgb(0x18140a))
                            .border_1()
                            .border_color(rgb(0xf59e0b))
                            .shadow_lg()
                            // Alert label
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(rgb(0xf59e0b))
                                            .child("⚡"),
                                    )
                                    .child(
                                        div()
                                            .font_weight(FontWeight::BOLD)
                                            .text_size(px(12.0))
                                            .text_color(rgb(0xfef3c7))
                                            .child(format!("{} 当前 Y/N 提示（识别）", agent.name)),
                                    ),
                            )
                            // Allow button (Y / Cmd+Y)
                            .child(
                                div()
                                    .id("btn_agent_allow")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .px_2p5()
                                    .py_1()
                                    .rounded_sm()
                                    .bg(rgb(0x064e3b))
                                    .border_1()
                                    .border_color(rgb(0x10b981))
                                    .text_color(rgb(0xffffff))
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::BOLD)
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(0x047857)))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        window.focus(&this.focus_handle);
                                        if this.is_agent_needing_input() {
                                            this.send_input(b"y\n");
                                        }
                                        cx.notify();
                                    }))
                                    .child("🟢 允许 (Y) - ⌘Y"),
                            )
                            // Reject button (N / Cmd+N)
                            .child(
                                div()
                                    .id("btn_agent_deny")
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .px_2p5()
                                    .py_1()
                                    .rounded_sm()
                                    .bg(rgb(0x450a0a))
                                    .border_1()
                                    .border_color(rgb(0xef4444))
                                    .text_color(rgb(0xffffff))
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(0x7f1d1d)))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        window.focus(&this.focus_handle);
                                        if this.is_agent_needing_input() {
                                            this.send_input(b"n\n");
                                        }
                                        cx.notify();
                                    }))
                                    .child("🔴 拒绝 (N) - ⌘N"),
                            ),
                    )
                } else {
                    None
                }
            } else {
                None
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn test_clipboard_paste_shortcuts() {
        // macOS Cmd+V
        let mod_cmd = Modifiers {
            platform: true,
            ..Default::default()
        };
        assert_eq!(map_keystroke("v", &mod_cmd), TerminalKeyAction::Paste);
        assert_eq!(map_keystroke("V", &mod_cmd), TerminalKeyAction::Paste);

        // Linux/Windows Ctrl+V
        let mod_ctrl = Modifiers {
            control: true,
            ..Default::default()
        };
        assert_eq!(map_keystroke("v", &mod_ctrl), TerminalKeyAction::Paste);
        assert_eq!(map_keystroke("V", &mod_ctrl), TerminalKeyAction::Paste);

        // Shift+Insert
        let mod_shift = Modifiers {
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            map_keystroke("insert", &mod_shift),
            TerminalKeyAction::Paste
        );
    }

    #[test]
    fn test_terminal_control_shortcuts() {
        let mod_ctrl = Modifiers {
            control: true,
            ..Default::default()
        };

        // Ctrl+C -> \x03 (SIGINT)
        assert_eq!(
            map_keystroke("c", &mod_ctrl),
            TerminalKeyAction::SendBytes(vec![0x03])
        );
        // Ctrl+D -> \x04 (EOF)
        assert_eq!(
            map_keystroke("d", &mod_ctrl),
            TerminalKeyAction::SendBytes(vec![0x04])
        );
        // Ctrl+Z -> \x1a (SIGTSTP)
        assert_eq!(
            map_keystroke("z", &mod_ctrl),
            TerminalKeyAction::SendBytes(vec![0x1a])
        );
        // Ctrl+L -> \x0c (Clear screen)
        assert_eq!(
            map_keystroke("l", &mod_ctrl),
            TerminalKeyAction::SendBytes(vec![0x0c])
        );
        // Ctrl+A -> \x01 (Start of line)
        assert_eq!(
            map_keystroke("a", &mod_ctrl),
            TerminalKeyAction::SendBytes(vec![0x01])
        );
        // Ctrl+E -> \x05 (End of line)
        assert_eq!(
            map_keystroke("e", &mod_ctrl),
            TerminalKeyAction::SendBytes(vec![0x05])
        );
        // Ctrl+U -> \x15 (Kill line)
        assert_eq!(
            map_keystroke("u", &mod_ctrl),
            TerminalKeyAction::SendBytes(vec![0x15])
        );
        // Ctrl+W -> \x17 (Kill word)
        assert_eq!(
            map_keystroke("w", &mod_ctrl),
            TerminalKeyAction::SendBytes(vec![0x17])
        );
        // Ctrl+R -> \x12 (Reverse history search)
        assert_eq!(
            map_keystroke("r", &mod_ctrl),
            TerminalKeyAction::SendBytes(vec![0x12])
        );
    }

    #[test]
    fn test_navigation_and_editing_keys() {
        let none = Modifiers::default();

        assert_eq!(
            map_keystroke("enter", &none),
            TerminalKeyAction::SendBytes(b"\r".to_vec())
        );
        assert_eq!(
            map_keystroke("backspace", &none),
            TerminalKeyAction::SendBytes(b"\x7f".to_vec())
        );
        assert_eq!(
            map_keystroke("tab", &none),
            TerminalKeyAction::SendBytes(b"\t".to_vec())
        );
        assert_eq!(
            map_keystroke("escape", &none),
            TerminalKeyAction::SendBytes(b"\x1b".to_vec())
        );
        assert_eq!(
            map_keystroke("up", &none),
            TerminalKeyAction::SendBytes(b"\x1b[A".to_vec())
        );
        assert_eq!(
            map_keystroke("down", &none),
            TerminalKeyAction::SendBytes(b"\x1b[B".to_vec())
        );
        assert_eq!(
            map_keystroke("right", &none),
            TerminalKeyAction::SendBytes(b"\x1b[C".to_vec())
        );
        assert_eq!(
            map_keystroke("left", &none),
            TerminalKeyAction::SendBytes(b"\x1b[D".to_vec())
        );
        assert_eq!(
            map_keystroke("home", &none),
            TerminalKeyAction::SendBytes(b"\x1b[H".to_vec())
        );
        assert_eq!(
            map_keystroke("end", &none),
            TerminalKeyAction::SendBytes(b"\x1b[F".to_vec())
        );
    }

    #[test]
    fn test_shift_navigation_and_scroll() {
        let mod_shift = Modifiers {
            shift: true,
            ..Default::default()
        };

        // Shift+Tab -> Backtab \x1b[Z
        assert_eq!(
            map_keystroke("tab", &mod_shift),
            TerminalKeyAction::SendBytes(b"\x1b[Z".to_vec())
        );

        // Shift+PageUp / PageDown -> Scroll
        assert_eq!(
            map_keystroke("pageup", &mod_shift),
            TerminalKeyAction::Scroll(20)
        );
        assert_eq!(
            map_keystroke("pagedown", &mod_shift),
            TerminalKeyAction::Scroll(-20)
        );
    }

    #[test]
    fn test_cmd_k_clear() {
        let mod_cmd = Modifiers {
            platform: true,
            ..Default::default()
        };
        assert_eq!(map_keystroke("k", &mod_cmd), TerminalKeyAction::Clear);
    }

    #[test]
    fn test_alt_shortcuts() {
        let mod_alt = Modifiers {
            alt: true,
            ..Default::default()
        };

        assert_eq!(
            map_keystroke("b", &mod_alt),
            TerminalKeyAction::SendBytes(b"\x1bb".to_vec())
        );
        assert_eq!(
            map_keystroke("f", &mod_alt),
            TerminalKeyAction::SendBytes(b"\x1bf".to_vec())
        );
        assert_eq!(
            map_keystroke("d", &mod_alt),
            TerminalKeyAction::SendBytes(b"\x1bd".to_vec())
        );
        assert_eq!(
            map_keystroke(".", &mod_alt),
            TerminalKeyAction::SendBytes(b"\x1b.".to_vec())
        );
    }

    #[test]
    fn test_printable_characters() {
        let none = Modifiers::default();
        assert_eq!(
            map_keystroke("a", &none),
            TerminalKeyAction::SendBytes(b"a".to_vec())
        );
        assert_eq!(
            map_keystroke("Z", &none),
            TerminalKeyAction::SendBytes(b"Z".to_vec())
        );
        assert_eq!(
            map_keystroke("1", &none),
            TerminalKeyAction::SendBytes(b"1".to_vec())
        );
        assert_eq!(
            map_keystroke(" ", &none),
            TerminalKeyAction::SendBytes(b" ".to_vec())
        );
    }

    #[test]
    fn test_special_ctrl_shortcuts() {
        let mod_ctrl = Modifiers {
            control: true,
            ..Default::default()
        };

        assert_eq!(
            map_keystroke("space", &mod_ctrl),
            TerminalKeyAction::SendBytes(vec![0x00])
        );
        assert_eq!(
            map_keystroke("@", &mod_ctrl),
            TerminalKeyAction::SendBytes(vec![0x00])
        );
        assert_eq!(
            map_keystroke("[", &mod_ctrl),
            TerminalKeyAction::SendBytes(vec![0x1b])
        );
        assert_eq!(
            map_keystroke("\\", &mod_ctrl),
            TerminalKeyAction::SendBytes(vec![0x1c])
        );
        assert_eq!(
            map_keystroke("]", &mod_ctrl),
            TerminalKeyAction::SendBytes(vec![0x1d])
        );
        assert_eq!(
            map_keystroke("^", &mod_ctrl),
            TerminalKeyAction::SendBytes(vec![0x1e])
        );
        assert_eq!(
            map_keystroke("_", &mod_ctrl),
            TerminalKeyAction::SendBytes(vec![0x1f])
        );
        assert_eq!(
            map_keystroke("/", &mod_ctrl),
            TerminalKeyAction::SendBytes(vec![0x1f])
        );
    }

    #[test]
    fn test_function_keys_f1_to_f12() {
        let none = Modifiers::default();

        assert_eq!(
            map_keystroke("f1", &none),
            TerminalKeyAction::SendBytes(b"\x1bOP".to_vec())
        );
        assert_eq!(
            map_keystroke("f2", &none),
            TerminalKeyAction::SendBytes(b"\x1bOQ".to_vec())
        );
        assert_eq!(
            map_keystroke("f3", &none),
            TerminalKeyAction::SendBytes(b"\x1bOR".to_vec())
        );
        assert_eq!(
            map_keystroke("f4", &none),
            TerminalKeyAction::SendBytes(b"\x1bOS".to_vec())
        );
        assert_eq!(
            map_keystroke("f5", &none),
            TerminalKeyAction::SendBytes(b"\x1b[15~".to_vec())
        );
        assert_eq!(
            map_keystroke("f6", &none),
            TerminalKeyAction::SendBytes(b"\x1b[17~".to_vec())
        );
        assert_eq!(
            map_keystroke("f7", &none),
            TerminalKeyAction::SendBytes(b"\x1b[18~".to_vec())
        );
        assert_eq!(
            map_keystroke("f8", &none),
            TerminalKeyAction::SendBytes(b"\x1b[19~".to_vec())
        );
        assert_eq!(
            map_keystroke("f9", &none),
            TerminalKeyAction::SendBytes(b"\x1b[20~".to_vec())
        );
        assert_eq!(
            map_keystroke("f10", &none),
            TerminalKeyAction::SendBytes(b"\x1b[21~".to_vec())
        );
        assert_eq!(
            map_keystroke("f11", &none),
            TerminalKeyAction::SendBytes(b"\x1b[23~".to_vec())
        );
        assert_eq!(
            map_keystroke("f12", &none),
            TerminalKeyAction::SendBytes(b"\x1b[24~".to_vec())
        );
    }

    #[test]
    fn test_modified_arrows() {
        let mod_ctrl = Modifiers {
            control: true,
            ..Default::default()
        };
        assert_eq!(
            map_keystroke("up", &mod_ctrl),
            TerminalKeyAction::SendBytes(b"\x1b[1;5A".to_vec())
        );
        assert_eq!(
            map_keystroke("down", &mod_ctrl),
            TerminalKeyAction::SendBytes(b"\x1b[1;5B".to_vec())
        );
        assert_eq!(
            map_keystroke("right", &mod_ctrl),
            TerminalKeyAction::SendBytes(b"\x1b[1;5C".to_vec())
        );
        assert_eq!(
            map_keystroke("left", &mod_ctrl),
            TerminalKeyAction::SendBytes(b"\x1b[1;5D".to_vec())
        );

        let mod_shift = Modifiers {
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            map_keystroke("up", &mod_shift),
            TerminalKeyAction::SendBytes(b"\x1b[1;2A".to_vec())
        );
        assert_eq!(
            map_keystroke("down", &mod_shift),
            TerminalKeyAction::SendBytes(b"\x1b[1;2B".to_vec())
        );
        assert_eq!(
            map_keystroke("right", &mod_shift),
            TerminalKeyAction::SendBytes(b"\x1b[1;2C".to_vec())
        );
        assert_eq!(
            map_keystroke("left", &mod_shift),
            TerminalKeyAction::SendBytes(b"\x1b[1;2D".to_vec())
        );
    }

    #[test]
    fn test_alt_word_navigation() {
        let mod_alt = Modifiers {
            alt: true,
            ..Default::default()
        };

        assert_eq!(
            map_keystroke("backspace", &mod_alt),
            TerminalKeyAction::SendBytes(b"\x17".to_vec())
        );
        assert_eq!(
            map_keystroke("delete", &mod_alt),
            TerminalKeyAction::SendBytes(b"\x1bd".to_vec())
        );
        assert_eq!(
            map_keystroke("c", &mod_alt),
            TerminalKeyAction::SendBytes(b"\x1bc".to_vec())
        );
        assert_eq!(
            map_keystroke("u", &mod_alt),
            TerminalKeyAction::SendBytes(b"\x1bu".to_vec())
        );
        assert_eq!(
            map_keystroke("l", &mod_alt),
            TerminalKeyAction::SendBytes(b"\x1bl".to_vec())
        );
    }

    #[test]
    fn test_terminal_agent_status_labels_and_colors() {
        assert_eq!(AgentStatus::NeedsInput.label(), "等待授权");
        assert_eq!(AgentStatus::Thinking.label(), "思考中");
        assert_eq!(AgentStatus::Done.label(), "完成");
        assert_eq!(AgentStatus::NeedsInput.color_rgb(), 0xf59e0b);
    }

    #[test]
    fn test_terminal_agent_detection_pipeline() {
        let osc_chunk = b"\x1b]0;Claude Code\x07";
        let titles = AgentDetector::extract_osc_titles(osc_chunk);
        assert_eq!(titles, vec!["Claude Code"]);

        let prompt = "Allow tool call? Run bash command [y/N]: ";
        let agent = AgentDetector::detect(None, titles.last().map(|s| s.as_str()), prompt);
        assert!(agent.is_some());
        let a = agent.unwrap();
        assert_eq!(a.name, "Claude Code");
        assert_eq!(a.status, AgentStatus::NeedsInput);
    }

    #[test]
    fn test_terminal_agent_detection_from_process() {
        let agent = AgentDetector::detect(Some("aider"), None, "");
        assert!(agent.is_some());
        let a = agent.unwrap();
        assert_eq!(a.name, "Aider");
        assert_eq!(a.status, AgentStatus::Idle);
    }
}
