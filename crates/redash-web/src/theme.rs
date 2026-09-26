pub use redash_ui_core::theme::*;

pub struct ThemeColors {
    pub bg_root: &'static str,
    pub bg_sidebar: &'static str,
    pub bg_card: &'static str,
    pub bg_card_hover: &'static str,
    pub bg_input: &'static str,
    pub border_default: &'static str,
    pub border_muted: &'static str,
    pub border_accent: &'static str,
    pub text_primary: &'static str,
    pub text_secondary: &'static str,
    pub text_muted: &'static str,
    pub accent_cyan: &'static str,
    pub accent_purple: &'static str,
    pub status_online: &'static str,
    pub status_warn: &'static str,
    pub status_crit: &'static str,
}

pub const DARK_TECH_THEME: ThemeColors = ThemeColors {
    bg_root: "#0b0f14",
    bg_sidebar: "#13171e",
    bg_card: "#161b22",
    bg_card_hover: "#1c2129",
    bg_input: "#0f1319",
    border_default: "#21262d",
    border_muted: "#30363d",
    border_accent: "#00ffcc",
    text_primary: "#f0f6fc",
    text_secondary: "#8b949e",
    text_muted: "#484f58",
    accent_cyan: "#00ffcc",
    accent_purple: "#bd93f9",
    status_online: "#3fb950",
    status_warn: "#d29922",
    status_crit: "#f85149",
};
