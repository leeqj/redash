use gpui::*;
use std::sync::RwLock;

/// Definitive palette of color tokens used across the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemePalette {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,

    // Backgrounds
    pub bg_root: u32,
    pub bg_panel: u32,
    pub bg_panel_hover: u32,
    pub bg_input: u32,
    pub bg_popup: u32,

    // Borders & Precision Lines
    pub border_default: u32,
    pub border_muted: u32,
    pub border_active: u32,
    pub border_accent: u32,

    // Status Telemetry
    pub status_online: u32,
    pub status_warn: u32,
    pub status_crit: u32,
    pub status_offline: u32,

    // Accent Colors
    pub accent_cyan: u32,
    pub accent_indigo: u32,
    pub accent_emerald: u32,

    // Typography
    pub text_primary: u32,
    pub text_secondary: u32,
    pub text_muted: u32,
    pub text_accent: u32,
}

impl ThemePalette {
    /// 1. Minimalist Dark Tech (深邃碳黑 #0a0b10 + 1px 科技蓝微光)
    pub const fn minimalist_dark_tech() -> Self {
        Self {
            name: "Minimalist Dark Tech",
            title: "Minimalist Dark Tech",
            description: "深邃碳黑 #0a0b10 + 1px 科技蓝微光",
            bg_root: 0x0a0b10,
            bg_panel: 0x12131a,
            bg_panel_hover: 0x171922,
            bg_input: 0x0e0f16,
            bg_popup: 0x141620,

            border_default: 0x232738,
            border_muted: 0x191c2b,
            border_active: 0x38bdf8,
            border_accent: 0x818cf8,

            status_online: 0x10b981,
            status_warn: 0xf59e0b,
            status_crit: 0xef4444,
            status_offline: 0x64748b,

            accent_cyan: 0x38bdf8,
            accent_indigo: 0x818cf8,
            accent_emerald: 0x34d399,

            text_primary: 0xf1f5f9,
            text_secondary: 0x94a3b8,
            text_muted: 0x64748b,
            text_accent: 0x38bdf8,
        }
    }

    /// 2. Cyberpunk Neon (赛博霓虹: 炫彩深紫黑 + 荧光青绿边框)
    pub const fn cyberpunk_neon() -> Self {
        Self {
            name: "Cyberpunk Neon",
            title: "Cyberpunk Neon",
            description: "赛博霓虹: 炫彩深紫黑 + 荧光青绿边框",
            bg_root: 0x0d0718,
            bg_panel: 0x170f2a,
            bg_panel_hover: 0x23173d,
            bg_input: 0x110a20,
            bg_popup: 0x1e1338,

            border_default: 0x3b2466,
            border_muted: 0x2a174a,
            border_active: 0x00f0ff,
            border_accent: 0xff007f,

            status_online: 0x00ff9f,
            status_warn: 0xffb800,
            status_crit: 0xff0055,
            status_offline: 0x796894,

            accent_cyan: 0x00f0ff,
            accent_indigo: 0xd946ef,
            accent_emerald: 0x00ff9f,

            text_primary: 0xfdf8ff,
            text_secondary: 0xc4b2e8,
            text_muted: 0x7f69a5,
            text_accent: 0x00f0ff,
        }
    }

    /// 3. Monokai Pro (经典极客代码黑)
    pub const fn monokai_pro() -> Self {
        Self {
            name: "Monokai Pro",
            title: "Monokai Pro",
            description: "经典极客代码黑: 暖色碳灰 + 金黄色高亮",
            bg_root: 0x19181a,
            bg_panel: 0x221f22,
            bg_panel_hover: 0x2d2a2e,
            bg_input: 0x1b191c,
            bg_popup: 0x252226,

            border_default: 0x3a363b,
            border_muted: 0x2a272c,
            border_active: 0xffd866,
            border_accent: 0x78dce8,

            status_online: 0xa9dc76,
            status_warn: 0xffd866,
            status_crit: 0xff6188,
            status_offline: 0x727072,

            accent_cyan: 0x78dce8,
            accent_indigo: 0xab9df2,
            accent_emerald: 0xa9dc76,

            text_primary: 0xfcfcfa,
            text_secondary: 0xc1c0c0,
            text_muted: 0x727072,
            text_accent: 0xffd866,
        }
    }

    /// 4. GitHub Dark (板岩沉浸暗色)
    pub const fn github_dark() -> Self {
        Self {
            name: "GitHub Dark",
            title: "GitHub Dark",
            description: "板岩沉浸暗色: 经典 GitHub 暗夜蓝灰",
            bg_root: 0x0d1117,
            bg_panel: 0x161b22,
            bg_panel_hover: 0x1f242c,
            bg_input: 0x090d13,
            bg_popup: 0x1c2128,

            border_default: 0x30363d,
            border_muted: 0x21262d,
            border_active: 0x58a6ff,
            border_accent: 0xbc8cff,

            status_online: 0x3fb950,
            status_warn: 0xd29922,
            status_crit: 0xf85149,
            status_offline: 0x8b949e,

            accent_cyan: 0x58a6ff,
            accent_indigo: 0xbc8cff,
            accent_emerald: 0x3fb950,

            text_primary: 0xc9d1d9,
            text_secondary: 0x8b949e,
            text_muted: 0x6e7681,
            text_accent: 0x58a6ff,
        }
    }

    pub fn all_presets() -> [ThemePalette; 4] {
        [
            Self::minimalist_dark_tech(),
            Self::cyberpunk_neon(),
            Self::monokai_pro(),
            Self::github_dark(),
        ]
    }

    pub fn by_name(name: &str) -> Self {
        match name {
            "Cyberpunk Neon" => Self::cyberpunk_neon(),
            "Monokai Pro" => Self::monokai_pro(),
            "GitHub Dark" => Self::github_dark(),
            _ => Self::minimalist_dark_tech(),
        }
    }
}

/// Global active theme state, allowing instant runtime switching and real-time previews.
static ACTIVE_THEME: RwLock<ThemePalette> = RwLock::new(ThemePalette::minimalist_dark_tech());

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct DarkTechTheme;

#[allow(dead_code)]
impl DarkTechTheme {
    // Legacy constants kept for compatibility and unit tests
    pub const BG_ROOT: u32 = 0x0a0b10;
    pub const BG_PANEL: u32 = 0x12131a;
    pub const BG_PANEL_HOVER: u32 = 0x171922;
    pub const BG_INPUT: u32 = 0x0e0f16;
    pub const BG_POPUP: u32 = 0x141620;

    pub const BORDER_DEFAULT: u32 = 0x232738;
    pub const BORDER_MUTED: u32 = 0x191c2b;
    pub const BORDER_ACTIVE: u32 = 0x38bdf8;
    pub const BORDER_ACCENT: u32 = 0x818cf8;

    pub const STATUS_ONLINE: u32 = 0x10b981;
    pub const STATUS_WARN: u32 = 0xf59e0b;
    pub const STATUS_CRIT: u32 = 0xef4444;
    pub const STATUS_OFFLINE: u32 = 0x64748b;

    pub const ACCENT_CYAN: u32 = 0x38bdf8;
    pub const ACCENT_INDIGO: u32 = 0x818cf8;
    pub const ACCENT_EMERALD: u32 = 0x34d399;

    pub const TEXT_PRIMARY: u32 = 0xf1f5f9;
    pub const TEXT_SECONDARY: u32 = 0x94a3b8;
    pub const TEXT_MUTED: u32 = 0x64748b;
    pub const TEXT_ACCENT: u32 = 0x38bdf8;

    /// Gets a copy of the current active theme palette.
    #[inline]
    pub fn current_palette() -> ThemePalette {
        ACTIVE_THEME
            .read()
            .map(|t| *t)
            .unwrap_or(ThemePalette::minimalist_dark_tech())
    }

    /// Sets the active theme by preset name immediately.
    pub fn set_active_theme(name: &str) -> ThemePalette {
        let palette = ThemePalette::by_name(name);
        if let Ok(mut lock) = ACTIVE_THEME.write() {
            *lock = palette;
        }
        palette
    }

    /// Returns the name of the active theme.
    pub fn active_theme_name() -> &'static str {
        ACTIVE_THEME
            .read()
            .map(|t| t.name)
            .unwrap_or("Minimalist Dark Tech")
    }

    // Dynamic GPUI Color Constructors (returning Hsla from the active theme)
    #[inline]
    pub fn bg_root() -> Hsla {
        rgb(ACTIVE_THEME
            .read()
            .map(|t| t.bg_root)
            .unwrap_or(Self::BG_ROOT))
        .into()
    }

    #[inline]
    pub fn bg_panel() -> Hsla {
        rgb(ACTIVE_THEME
            .read()
            .map(|t| t.bg_panel)
            .unwrap_or(Self::BG_PANEL))
        .into()
    }

    #[inline]
    pub fn bg_panel_hover() -> Hsla {
        rgb(ACTIVE_THEME
            .read()
            .map(|t| t.bg_panel_hover)
            .unwrap_or(Self::BG_PANEL_HOVER))
        .into()
    }

    #[inline]
    pub fn bg_input() -> Hsla {
        rgb(ACTIVE_THEME
            .read()
            .map(|t| t.bg_input)
            .unwrap_or(Self::BG_INPUT))
        .into()
    }

    #[inline]
    pub fn bg_popup() -> Hsla {
        rgb(ACTIVE_THEME
            .read()
            .map(|t| t.bg_popup)
            .unwrap_or(Self::BG_POPUP))
        .into()
    }

    #[inline]
    pub fn border_default() -> Hsla {
        rgb(ACTIVE_THEME
            .read()
            .map(|t| t.border_default)
            .unwrap_or(Self::BORDER_DEFAULT))
        .into()
    }

    #[inline]
    pub fn border_muted() -> Hsla {
        rgb(ACTIVE_THEME
            .read()
            .map(|t| t.border_muted)
            .unwrap_or(Self::BORDER_MUTED))
        .into()
    }

    #[inline]
    pub fn border_active() -> Hsla {
        rgb(ACTIVE_THEME
            .read()
            .map(|t| t.border_active)
            .unwrap_or(Self::BORDER_ACTIVE))
        .into()
    }

    #[inline]
    pub fn border_accent() -> Hsla {
        rgb(ACTIVE_THEME
            .read()
            .map(|t| t.border_accent)
            .unwrap_or(Self::BORDER_ACCENT))
        .into()
    }

    #[inline]
    pub fn border_active_glow() -> Hsla {
        let active = ACTIVE_THEME
            .read()
            .map(|t| t.border_active)
            .unwrap_or(Self::BORDER_ACTIVE);
        rgba((active << 8) | 0x33).into()
    }

    #[inline]
    pub fn status_online() -> Hsla {
        rgb(ACTIVE_THEME
            .read()
            .map(|t| t.status_online)
            .unwrap_or(Self::STATUS_ONLINE))
        .into()
    }

    #[inline]
    pub fn status_warn() -> Hsla {
        rgb(ACTIVE_THEME
            .read()
            .map(|t| t.status_warn)
            .unwrap_or(Self::STATUS_WARN))
        .into()
    }

    #[inline]
    pub fn status_crit() -> Hsla {
        rgb(ACTIVE_THEME
            .read()
            .map(|t| t.status_crit)
            .unwrap_or(Self::STATUS_CRIT))
        .into()
    }

    #[inline]
    pub fn status_offline() -> Hsla {
        rgb(ACTIVE_THEME
            .read()
            .map(|t| t.status_offline)
            .unwrap_or(Self::STATUS_OFFLINE))
        .into()
    }

    #[inline]
    pub fn status_online_halo() -> Hsla {
        let col = ACTIVE_THEME
            .read()
            .map(|t| t.status_online)
            .unwrap_or(Self::STATUS_ONLINE);
        rgba((col << 8) | 0x44).into()
    }

    #[inline]
    pub fn status_warn_halo() -> Hsla {
        let col = ACTIVE_THEME
            .read()
            .map(|t| t.status_warn)
            .unwrap_or(Self::STATUS_WARN);
        rgba((col << 8) | 0x44).into()
    }

    #[inline]
    pub fn status_crit_halo() -> Hsla {
        let col = ACTIVE_THEME
            .read()
            .map(|t| t.status_crit)
            .unwrap_or(Self::STATUS_CRIT);
        rgba((col << 8) | 0x44).into()
    }

    #[inline]
    pub fn status_offline_halo() -> Hsla {
        let col = ACTIVE_THEME
            .read()
            .map(|t| t.status_offline)
            .unwrap_or(Self::STATUS_OFFLINE);
        rgba((col << 8) | 0x33).into()
    }

    #[inline]
    pub fn accent_cyan() -> Hsla {
        rgb(ACTIVE_THEME
            .read()
            .map(|t| t.accent_cyan)
            .unwrap_or(Self::ACCENT_CYAN))
        .into()
    }

    #[inline]
    pub fn accent_indigo() -> Hsla {
        rgb(ACTIVE_THEME
            .read()
            .map(|t| t.accent_indigo)
            .unwrap_or(Self::ACCENT_INDIGO))
        .into()
    }

    #[inline]
    pub fn accent_emerald() -> Hsla {
        rgb(ACTIVE_THEME
            .read()
            .map(|t| t.accent_emerald)
            .unwrap_or(Self::ACCENT_EMERALD))
        .into()
    }

    #[inline]
    pub fn text_primary() -> Hsla {
        rgb(ACTIVE_THEME
            .read()
            .map(|t| t.text_primary)
            .unwrap_or(Self::TEXT_PRIMARY))
        .into()
    }

    #[inline]
    pub fn text_secondary() -> Hsla {
        rgb(ACTIVE_THEME
            .read()
            .map(|t| t.text_secondary)
            .unwrap_or(Self::TEXT_SECONDARY))
        .into()
    }

    #[inline]
    pub fn text_muted() -> Hsla {
        rgb(ACTIVE_THEME
            .read()
            .map(|t| t.text_muted)
            .unwrap_or(Self::TEXT_MUTED))
        .into()
    }

    #[inline]
    pub fn text_accent() -> Hsla {
        rgb(ACTIVE_THEME
            .read()
            .map(|t| t.text_accent)
            .unwrap_or(Self::TEXT_ACCENT))
        .into()
    }

    /// Evaluates telemetry metrics to determine status color.
    pub fn status_for_metrics(is_online: bool, cpu_usage: f32, ram_usage: f32) -> Hsla {
        if !is_online {
            Self::status_offline()
        } else if cpu_usage > 85.0 || ram_usage > 90.0 {
            Self::status_crit()
        } else if cpu_usage > 70.0 || ram_usage > 75.0 {
            Self::status_warn()
        } else {
            Self::status_online()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn test_theme_color_constants() {
        assert_eq!(DarkTechTheme::BG_ROOT, 0x0a0b10);
        assert_eq!(DarkTechTheme::BG_PANEL, 0x12131a);
        assert_eq!(DarkTechTheme::BG_INPUT, 0x0e0f16);
        assert_eq!(DarkTechTheme::BORDER_DEFAULT, 0x232738);
        assert_eq!(DarkTechTheme::BORDER_ACTIVE, 0x38bdf8);
        assert_eq!(DarkTechTheme::BORDER_ACCENT, 0x818cf8);
        assert_eq!(DarkTechTheme::STATUS_ONLINE, 0x10b981);
        assert_eq!(DarkTechTheme::STATUS_WARN, 0xf59e0b);
        assert_eq!(DarkTechTheme::STATUS_CRIT, 0xef4444);
        assert_eq!(DarkTechTheme::STATUS_OFFLINE, 0x64748b);
    }

    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[core::prelude::v1::test]
    fn test_status_for_metrics() {
        let _guard = TEST_LOCK.lock().unwrap();
        DarkTechTheme::set_active_theme("Minimalist Dark Tech");
        assert_eq!(
            DarkTechTheme::status_for_metrics(false, 10.0, 10.0),
            DarkTechTheme::status_offline()
        );
        assert_eq!(
            DarkTechTheme::status_for_metrics(true, 90.0, 20.0),
            DarkTechTheme::status_crit()
        );
        assert_eq!(
            DarkTechTheme::status_for_metrics(true, 75.0, 50.0),
            DarkTechTheme::status_warn()
        );
        assert_eq!(
            DarkTechTheme::status_for_metrics(true, 30.0, 40.0),
            DarkTechTheme::status_online()
        );
    }

    #[core::prelude::v1::test]
    fn test_dynamic_theme_switching() {
        let _guard = TEST_LOCK.lock().unwrap();
        // Reset to default
        DarkTechTheme::set_active_theme("Minimalist Dark Tech");
        assert_eq!(DarkTechTheme::active_theme_name(), "Minimalist Dark Tech");
        assert_eq!(DarkTechTheme::bg_root(), rgb(0x0a0b10).into());

        // Switch to Cyberpunk Neon
        DarkTechTheme::set_active_theme("Cyberpunk Neon");
        assert_eq!(DarkTechTheme::active_theme_name(), "Cyberpunk Neon");
        assert_eq!(DarkTechTheme::bg_root(), rgb(0x0d0718).into());
        assert_eq!(DarkTechTheme::border_active(), rgb(0x00f0ff).into());

        // Switch to Monokai Pro
        DarkTechTheme::set_active_theme("Monokai Pro");
        assert_eq!(DarkTechTheme::active_theme_name(), "Monokai Pro");
        assert_eq!(DarkTechTheme::border_active(), rgb(0xffd866).into());

        // Switch to GitHub Dark
        DarkTechTheme::set_active_theme("GitHub Dark");
        assert_eq!(DarkTechTheme::active_theme_name(), "GitHub Dark");
        assert_eq!(DarkTechTheme::border_active(), rgb(0x58a6ff).into());

        // Switch back to Minimalist Dark Tech
        DarkTechTheme::set_active_theme("Minimalist Dark Tech");
        assert_eq!(DarkTechTheme::active_theme_name(), "Minimalist Dark Tech");
        assert_eq!(DarkTechTheme::bg_root(), rgb(0x0a0b10).into());
    }
}
