use crate::theme::DarkTechTheme;
use gpui::*;

#[derive(Debug, Clone)]
pub enum PortRemediationModalAction {
    Close,
    Diagnose { node_id: String, port: u16 },
    KillConflict { node_id: String, port: u16 },
}

pub struct PortRemediationModal {
    pub node_id: String,
    pub port_input: String,
    pub error_msg: Option<String>,
    focus_handle: Option<FocusHandle>,
    cursor: usize,
    select_all: bool,
}

impl PortRemediationModal {
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            port_input: "8080".to_string(),
            error_msg: None,
            focus_handle: None,
            cursor: 4,
            select_all: true,
        }
    }

    fn insert_text(&mut self, text: &str) {
        if self.select_all {
            self.port_input.clear();
            self.cursor = 0;
            self.select_all = false;
        }
        let text: String = text.chars().filter(char::is_ascii_digit).collect();
        let remaining = 5usize.saturating_sub(self.port_input.len());
        let text = &text[..text.len().min(remaining)];
        self.cursor = self.cursor.min(self.port_input.len());
        self.port_input.insert_str(self.cursor, text);
        self.cursor += text.len();
        self.error_msg = None;
    }

    fn handle_key(&mut self, event: &KeyDownEvent, cx: &App) {
        let key = event.keystroke.key.as_str();
        let modifier = event.keystroke.modifiers.platform || event.keystroke.modifiers.control;
        if modifier && key.eq_ignore_ascii_case("a") {
            self.select_all = true;
            return;
        }
        if modifier && key.eq_ignore_ascii_case("v") {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                self.insert_text(&text);
            }
            return;
        }
        match key {
            "backspace" | "delete" if self.select_all => {
                self.port_input.clear();
                self.cursor = 0;
                self.select_all = false;
            }
            "backspace" if self.cursor > 0 => {
                self.cursor -= 1;
                self.port_input.remove(self.cursor);
            }
            "delete" if self.cursor < self.port_input.len() => {
                self.port_input.remove(self.cursor);
            }
            "left" => {
                self.cursor = self.cursor.saturating_sub(1);
                self.select_all = false;
            }
            "right" => {
                self.cursor = (self.cursor + 1).min(self.port_input.len());
                self.select_all = false;
            }
            "home" => {
                self.cursor = 0;
                self.select_all = false;
            }
            "end" => {
                self.cursor = self.port_input.len();
                self.select_all = false;
            }
            _ if !modifier => self.insert_text(event.keystroke.key_char.as_deref().unwrap_or(key)),
            _ => {}
        }
        self.error_msg = None;
    }

    pub fn parse_port(&self) -> Result<u16, String> {
        let trimmed = self.port_input.trim();
        if trimmed.is_empty() {
            return Err("请输入端口号".to_string());
        }
        let port = trimmed
            .parse::<u16>()
            .map_err(|_| "端口号必须是 1~65535 之间的有效整数".to_string())?;
        if port == 0 {
            return Err("端口号不能为 0".to_string());
        }
        if port == 22 {
            return Err("端口 22 是 SSH 远程管理端口，已被安全自保机制保护锁定".to_string());
        }
        Ok(port)
    }
}

impl Render for PortRemediationModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let node_id = self.node_id.clone();
        let node_id_for_diagnose = self.node_id.clone();
        let node_id_for_kill = self.node_id.clone();
        if self.focus_handle.is_none() {
            let handle = cx.focus_handle();
            window.focus(&handle);
            self.focus_handle = Some(handle);
        }
        let focus = self.focus_handle.as_ref().unwrap().clone();
        let mut port_text = self.port_input.clone();
        if focus.is_focused(window) && !self.select_all {
            port_text.insert(self.cursor.min(port_text.len()), '|');
        }

        let error_msg = self.error_msg.clone();

        div()
            .id("port-remediation-modal")
            .track_focus(&focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" { cx.emit(PortRemediationModalAction::Close); }
                else { this.handle_key(event, cx); }
                cx.notify();
                cx.stop_propagation();
            }))
            .absolute()
            .inset_0()
            .bg(rgba(0x000000bb))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(460.0))
                    .bg(DarkTechTheme::bg_popup())
                    .border_1()
                    .border_color(DarkTechTheme::border_default())
                    .rounded_lg()
                    .shadow_lg()
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(
                        // Header
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(div().text_xl().child("⚡"))
                                    .child(
                                        div()
                                            .text_base()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(DarkTechTheme::text_primary())
                                            .child(format!("端口治理与冲突释放 · {}", node_id)),
                                    ),
                            )
                            .child(
                                div()
                                    .id("btn_close_port_modal_x")
                                    .cursor_pointer()
                                    .text_color(DarkTechTheme::text_muted())
                                    .hover(|s| s.text_color(DarkTechTheme::text_primary()))
                                    .child("✕")
                                    .on_click(cx.listener(|_this, _, _window, cx| {
                                        cx.emit(PortRemediationModalAction::Close);
                                    })),
                            ),
                    )
                    .child(
                        // Body
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(DarkTechTheme::text_secondary())
                                    .child("输入需诊断或强制释放的目标 TCP 端口。系统内置防误杀保护（已锁定 SSH 22 端口与 Agent 自身 PID）。"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(DarkTechTheme::text_primary())
                                            .child("目标端口 (1 - 65535)"),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .id("port-number-input")
                                                    .cursor_text()
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        if let Some(fh) = &this.focus_handle { window.focus(fh); }
                                                        this.select_all = true;
                                                        cx.notify();
                                                    }))
                                                    .flex_1()
                                                    .h(px(34.0))
                                                    .px_3()
                                                    .bg(DarkTechTheme::bg_input())
                                                    .border_1()
                                                    .border_color(DarkTechTheme::border_default())
                                                    .rounded_md()
                                                    .flex()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_family("Menlo")
                                                            .text_color(DarkTechTheme::accent_cyan())
                                                            .child(port_text),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        // Quick Port Pills
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1p5()
                                            .pt_1()
                                            .child(div().text_size(px(10.0)).text_color(DarkTechTheme::text_muted()).child("快捷选择:"))
                                            .children(["8080", "3000", "8000", "80", "443", "5432", "3306"].map(|p| {
                                                let p_str = p.to_string();
                                                div()
                                                    .id(ElementId::Name(format!("pill_port_{}", p).into()))
                                                    .cursor_pointer()
                                                    .px_2()
                                                    .py_0p5()
                                                    .rounded_xs()
                                                    .bg(DarkTechTheme::bg_input())
                                                    .border_1()
                                                    .border_color(DarkTechTheme::border_muted())
                                                    .text_size(px(10.0))
                                                    .text_color(DarkTechTheme::text_secondary())
                                                    .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()).border_color(DarkTechTheme::accent_cyan()))
                                                    .child(p)
                                                    .on_click(cx.listener(move |this, _, _window, cx| {
                                                        this.port_input = p_str.clone();
                                                        this.cursor = this.port_input.len();
                                                        this.select_all = true;
                                                        this.error_msg = None;
                                                        cx.notify();
                                                    }))
                                            })),
                                    ),
                            )
                            .children(error_msg.map(|err| {
                                div()
                                    .p_2()
                                    .rounded_md()
                                    .bg(rgba(0xef44441a))
                                    .border_1()
                                    .border_color(rgba(0xef444450))
                                    .text_xs()
                                    .text_color(DarkTechTheme::status_crit())
                                    .child(format!("⚠️ {}", err))
                            })),
                    )
                    .child(
                        // Footer Actions
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .pt_2()
                            .child(
                                div()
                                    .id("btn_cancel_port_modal")
                                    .cursor_pointer()
                                    .px_3()
                                    .h(px(32.0))
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_input())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_default())
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_xs()
                                    .text_color(DarkTechTheme::text_secondary())
                                    .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()))
                                    .child("取消")
                                    .on_click(cx.listener(|_this, _, _window, cx| {
                                        cx.emit(PortRemediationModalAction::Close);
                                    })),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id("btn_modal_diagnose_port")
                                            .cursor_pointer()
                                            .px_3()
                                            .h(px(32.0))
                                            .rounded_md()
                                            .bg(DarkTechTheme::accent_cyan().opacity(0.15))
                                            .border_1()
                                            .border_color(DarkTechTheme::accent_cyan().opacity(0.4))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_xs()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(DarkTechTheme::accent_cyan())
                                            .hover(|s| s.bg(DarkTechTheme::accent_cyan().opacity(0.25)))
                                            .child("🔍 诊断占用")
                                            .on_click(cx.listener(move |this, _, _window, cx| {
                                                match this.parse_port() {
                                                    Ok(port) => {
                                                        this.error_msg = None;
                                                        cx.emit(PortRemediationModalAction::Diagnose {
                                                            node_id: node_id_for_diagnose.clone(),
                                                            port,
                                                        });
                                                    }
                                                    Err(e) => {
                                                        this.error_msg = Some(e);
                                                        cx.notify();
                                                    }
                                                }
                                            })),
                                    )
                                    .child(
                                        div()
                                            .id("btn_modal_kill_port")
                                            .cursor_pointer()
                                            .px_3()
                                            .h(px(32.0))
                                            .rounded_md()
                                            .bg(DarkTechTheme::status_crit().opacity(0.2))
                                            .border_1()
                                            .border_color(DarkTechTheme::status_crit().opacity(0.5))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_xs()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(DarkTechTheme::status_crit())
                                            .hover(|s| s.bg(DarkTechTheme::status_crit().opacity(0.35)))
                                            .child("⚡ 强制释放")
                                            .on_click(cx.listener(move |this, _, _window, cx| {
                                                match this.parse_port() {
                                                    Ok(port) => {
                                                        this.error_msg = None;
                                                        cx.emit(PortRemediationModalAction::KillConflict {
                                                            node_id: node_id_for_kill.clone(),
                                                            port,
                                                        });
                                                    }
                                                    Err(e) => {
                                                        this.error_msg = Some(e);
                                                        cx.notify();
                                                    }
                                                }
                                            })),
                                    ),
                            ),
                    ),
            )
    }
}

impl EventEmitter<PortRemediationModalAction> for PortRemediationModal {}
