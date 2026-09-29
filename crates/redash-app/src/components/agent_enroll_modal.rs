use crate::theme::DarkTechTheme;
use gpui::*;
use redash_ui_core::control_plane::ClientSigner;

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
    pub agent_identity: (String, String),
    pub active_tab: usize, // 0 = Bash, 1 = Docker
    pub copied: bool,
}

impl AgentEnrollModal {
    pub fn new(hub_url: String, public_key_hex: Option<String>) -> Self {
        Self {
            hub_url,
            node_id_input: format!("node-{}", &redash_ui_core::e2ee::random_hex()[..12]),
            auth_token_input: redash_ui_core::e2ee::random_hex(),
            agent_identity: ClientSigner::generate_keypair(),
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
            let command = ClientSigner::format_onboarding_command(
                &self.hub_url,
                node_id,
                &self.auth_token_input,
                self.public_key_hex.as_deref(),
            );
            format!("{} --identity-key {}", command, self.agent_identity.1)
        } else if self.active_tab == 1 {
            let command = ClientSigner::format_docker_command(
                &self.hub_url,
                node_id,
                &self.auth_token_input,
                self.public_key_hex.as_deref(),
            );
            command.replace(
                " ghcr.io/",
                &format!(" -e REDASH_IDENTITY_KEY={} ghcr.io/", self.agent_identity.1),
            )
        } else {
            serde_json::to_string_pretty(&serde_json::json!({ self.node_id_input.clone(): {
                "auth_token": self.auth_token_input,
                "trusted_public_key": self.public_key_hex.clone().unwrap_or_default()
            }}))
            .unwrap()
        }
    }
}

impl Render for AgentEnrollModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let cmd = self.current_command();
        let cmd_clone = cmd.clone();
        let qr_ascii: Option<String> = None;
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
                                            .child("受控节点入网与移动端配对 (Zero-Trust)"),
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
                            .child(if active_tab == 2 {
                                "使用移动端扫描二维码，或复制配对链接在手机 App 中导入，即可直接对齐公钥实现端到端加密与节点漫游。"
                            } else {
                                "探针采用主动向外出站长连接（443 WSS），无论是大内网 NAS 还是海外多云 VPS，均无需公网 IP 与端口映射即可秒级入网。"
                            }),
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
                                    .child("Shell 脚本安装")
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
                                    .child("Docker 启动")
                                    .on_click(cx.listener(|this, _, _, _| {
                                        this.active_tab = 1;
                                        this.copied = false;
                                    })),
                            )
                            .child(
                                div()
                                    .id("tab_mobile_pair")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .bg(if active_tab == 2 { DarkTechTheme::accent_cyan() } else { DarkTechTheme::bg_input() })
                                    .text_color(if active_tab == 2 { DarkTechTheme::bg_root() } else { DarkTechTheme::text_secondary() })
                                    .child("Hub 注册配置")
                                    .on_click(cx.listener(|this, _, _, _| {
                                        this.active_tab = 2;
                                        this.copied = false;
                                    })),
                            ),
                    )
                    .child(
                        // Display box (QR code or command)
                        div()
                            .p_3()
                            .bg(DarkTechTheme::bg_root())
                            .border_1()
                            .border_color(DarkTechTheme::border_muted())
                            .rounded_md()
                            .child(if let Some(qr) = qr_ascii {
                                div()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_family("monospace")
                                            .text_color(DarkTechTheme::accent_cyan())
                                            .child(qr),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_family("monospace")
                                            .text_color(DarkTechTheme::text_muted())
                                            .child(cmd),
                                    )
                            } else {
                                div()
                                    .text_xs()
                                    .font_family("monospace")
                                    .text_color(DarkTechTheme::accent_emerald())
                                    .child(cmd)
                            }),
                    )
                    .child(div().text_xs().text_color(DarkTechTheme::text_secondary()).child(
                        format!("先将“Hub 注册配置”合并到 Hub 的 agent_enrollments.json（权限 600）并重启 Hub，再执行安装命令。节点 {} 的 Agent 公钥会在复制安装命令时固定到本机；请勿公开安装命令中的凭据。", self.node_id_input)
                    ))
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
                                            .child(if active_tab == 2 {
                                                "扫码后移动端将对齐公钥，直连内网探针或经由 Hub 加密访问。"
                                            } else {
                                                "在目标服务器粘贴并执行命令后，即可在控制台看到节点上线。"
                                            }),
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
                                    .child(if copied {
                                        "✓ 已复制到剪贴板"
                                    } else if active_tab == 2 {
                                        "📋 复制配对链接"
                                    } else {
                                        "📋 复制一键命令"
                                    })
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
