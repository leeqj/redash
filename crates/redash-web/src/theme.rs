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

pub const CYBERPUNK_THEME: ThemeColors = ThemeColors {
    bg_root: "#080614",
    bg_sidebar: "#110c26",
    bg_card: "#181236",
    bg_card_hover: "#221a4c",
    bg_input: "#120d2b",
    border_default: "#2f1e60",
    border_muted: "#432b85",
    border_accent: "#ff007f",
    text_primary: "#fff5fa",
    text_secondary: "#b2a1d9",
    text_muted: "#715f9e",
    accent_cyan: "#00f0ff",
    accent_purple: "#ff007f",
    status_online: "#00ff9f",
    status_warn: "#ffb800",
    status_crit: "#ff0055",
};

pub const SOLARIZED_DARK_THEME: ThemeColors = ThemeColors {
    bg_root: "#002b36",
    bg_sidebar: "#073642",
    bg_card: "#09414f",
    bg_card_hover: "#0d4e5f",
    bg_input: "#06313c",
    border_default: "#0e5a6d",
    border_muted: "#146d84",
    border_accent: "#2aa198",
    text_primary: "#fdf6e3",
    text_secondary: "#93a1a1",
    text_muted: "#657b83",
    accent_cyan: "#2aa198",
    accent_purple: "#6c71c4",
    status_online: "#859900",
    status_warn: "#b58900",
    status_crit: "#dc322f",
};

pub const HIGH_CONTRAST_THEME: ThemeColors = ThemeColors {
    bg_root: "#000000",
    bg_sidebar: "#0a0a0a",
    bg_card: "#121212",
    bg_card_hover: "#222222",
    bg_input: "#050505",
    border_default: "#555555",
    border_muted: "#777777",
    border_accent: "#00ffff",
    text_primary: "#ffffff",
    text_secondary: "#d0d0d0",
    text_muted: "#999999",
    accent_cyan: "#00ffff",
    accent_purple: "#ff00ff",
    status_online: "#00ff00",
    status_warn: "#ffff00",
    status_crit: "#ff0000",
};

impl ThemeColors {
    pub fn from_palette(palette: &ThemePalette) -> Self {
        match palette.name {
            "CyberpunkNeon" | "Cyberpunk" => CYBERPUNK_THEME,
            "SolarizedDark" | "Solarized" => SOLARIZED_DARK_THEME,
            "HighContrast" | "High Contrast" => HIGH_CONTRAST_THEME,
            _ => DARK_TECH_THEME,
        }
    }
}
