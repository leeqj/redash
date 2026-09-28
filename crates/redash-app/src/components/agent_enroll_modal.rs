use gpui::*;
use redash_ui_core::control_plane::ClientSigner;
use crate::theme::DarkTechTheme;

#[derive(Debug, Clone)]
pub enum AgentEnrollModalAction {
    Close,
    CopyCommand(String),
}

pub struct AgentEnrollModal {
    pub hub_url: String,
    pub node_id_input: String,
    pub auth_token_input: String,
    pub public_key_hex: Option<String>,
    pub active_tab: usize, // 0 = Bash, 1 = Docker
    pub copied: bool,
}

impl AgentEnrollModal {
    pub fn new(hub_url: String, public_key_hex: Option<String>) -> Self {
        Self {
            hub_url,
            node_id_input: String::new(),
            auth_token_input: "default-token".to_string(),
            public_key_hex,
            active_tab: 0,
            copied: false,
        }
    }

    pub fn current_command(&self) -> String {
        let node_id = if self.node_id_input.trim().is_empty() {
            None
        } else {
            Some(self.node_id_input.trim())
        };

        if self.active_tab == 0 {
            ClientSigner::format_onboarding_command(
                &self.hub_url,
                node_id,
                &self.auth_token_input,
                self.public_key_hex.as_deref(),
            )
        } else {
            ClientSigner::format_docker_command(
                &self.hub_url,
                node_id,
                &self.auth_token_input,
                self.public_key_hex.as_deref(),
            )
        }
    }
}

impl Render for AgentEnrollModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let cmd = self.current_command();
        let cmd_clone = cmd.clone();
        let active_tab = self.active_tab;
        let copied = self.copied;

        div()
            .absolute()
            .inset_0()
            .bg(rgba(0x000000bb))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(640.0))
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
                                            .child("添加受控节点 (Zero-Trust Agent Onboarding)"),
                                    ),
                            )
                            .child(
                                div()
                                    .id("btn_close_agent_enroll")
                                    .cursor_pointer()
                                    .text_color(DarkTechTheme::text_muted())
                                    .hover(|s| s.text_color(DarkTechTheme::text_primary()))
                                    .child("✕")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.copied = false;
                                        cx.emit(AgentEnrollModalAction::Close);
                                    })),
                            ),
                    )
                    .child(
                        // Description banner
                        div()
                            .p_3()
                            .rounded_md()
                            .bg(rgba(0x38bdf815))
                            .border_1()
                            .border_color(rgba(0x38bdf840))
                            .text_xs()
                            .text_color(DarkTechTheme::accent_cyan())
                            .child("探针采用主动向外出站长连接（443 WSS），无论是大内网 NAS 还是海外多云 VPS，均无需公网 IP 与端口映射即可秒级入网。"),
                    )
                    .child(
                        // Mode selection tabs
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .id("tab_shell_install")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .bg(if active_tab == 0 { DarkTechTheme::accent_cyan() } else { DarkTechTheme::bg_input() })
                                    .text_color(if active_tab == 0 { DarkTechTheme::bg_root() } else { DarkTechTheme::text_secondary() })
                                    .child("Shell 脚本一键安装 (推荐)")
                                    .on_click(cx.listener(|this, _, _, _| {
                                        this.active_tab = 0;
                                        this.copied = false;
                                    })),
                            )
                            .child(
                                div()
                                    .id("tab_docker_install")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .bg(if active_tab == 1 { DarkTechTheme::accent_cyan() } else { DarkTechTheme::bg_input() })
                                    .text_color(if active_tab == 1 { DarkTechTheme::bg_root() } else { DarkTechTheme::text_secondary() })
                                    .child("Docker 容器化启动")
                                    .on_click(cx.listener(|this, _, _, _| {
                                        this.active_tab = 1;
                                        this.copied = false;
                                    })),
                            ),
                    )
                    .child(
                        // Command display box
                        div()
                            .p_3()
                            .bg(DarkTechTheme::bg_root())
                            .border_1()
                            .border_color(DarkTechTheme::border_muted())
                            .rounded_md()
                            .child(
                                div()
                                    .text_xs()
                                    .font_family("monospace")
                                    .text_color(DarkTechTheme::accent_emerald())
                                    .child(cmd),
                            ),
                    )
                    .child(
                        // Footer with action buttons
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(div().w_2().h_2().rounded_full().bg(DarkTechTheme::status_warn()))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(DarkTechTheme::text_muted())
                                            .child("在目标服务器粘贴并执行命令后，即可在控制台看到节点上线。"),
                                    ),
                            )
                            .child(
                                div()
                                    .id("btn_copy_enroll_cmd")
                                    .px_4()
                                    .py_2()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .bg(if copied { DarkTechTheme::accent_emerald() } else { DarkTechTheme::accent_cyan() })
                                    .text_color(DarkTechTheme::bg_root())
                                    .child(if copied { "✓ 已复制到剪贴板" } else { "📋 复制一键命令" })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.copied = true;
                                        cx.write_to_clipboard(ClipboardItem::new_string(cmd_clone.clone()));
                                        cx.emit(AgentEnrollModalAction::CopyCommand(cmd_clone.clone()));
                                    })),
                            ),
                    ),
            )
    }
}

impl EventEmitter<AgentEnrollModalAction> for AgentEnrollModal {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_enroll_modal_commands() {
        let modal = AgentEnrollModal::new(
            "wss://hub.test.dev/v1/control/ws-agent".to_string(),
            Some("pk-test-123456".to_string()),
        );

        // Bash tab (0)
        let bash_cmd = modal.current_command();
        assert!(bash_cmd.contains("install_agent.sh"));
        assert!(bash_cmd.contains("wss://hub.test.dev/v1/control/ws-agent"));
        assert!(bash_cmd.contains("--key pk-test-123456"));

        // Docker tab (1)
        let mut docker_modal = modal;
        docker_modal.active_tab = 1;
        docker_modal.node_id_input = "node-alpha".to_string();
        let docker_cmd = docker_modal.current_command();
        assert!(docker_cmd.contains("docker run -d --name redash-agent"));
        assert!(docker_cmd.contains("-e REDASH_HUB_URL=wss://hub.test.dev/v1/control/ws-agent"));
        assert!(docker_cmd.contains("-e REDASH_NODE_ID=node-alpha"));
        assert!(docker_cmd.contains("-e REDASH_TRUSTED_KEY=pk-test-123456"));
    }
}
