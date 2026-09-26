use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RgbaColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32,
}

impl RgbaColor {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn from_hex(hex: u32) -> Self {
        let r = ((hex >> 16) & 0xFF) as u8;
        let g = ((hex >> 8) & 0xFF) as u8;
        let b = (hex & 0xFF) as u8;
        Self { r, g, b, a: 1.0 }
    }

    pub const fn from_hex_alpha(hex: u32, a: f32) -> Self {
        let r = ((hex >> 16) & 0xFF) as u8;
        let g = ((hex >> 8) & 0xFF) as u8;
        let b = (hex & 0xFF) as u8;
        Self { r, g, b, a }
    }

    pub fn to_css_rgba(&self) -> String {
        if (self.a - 1.0).abs() < f32::EPSILON {
            format!("rgb({}, {}, {})", self.r, self.g, self.b)
        } else {
            format!("rgba({}, {}, {}, {:.2})", self.r, self.g, self.b, self.a)
        }
    }

    pub fn to_canvas_style(&self) -> String {
        self.to_css_rgba()
    }

    pub fn to_hex_str(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    pub fn to_u32_rgb(&self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemePalette {
    pub name: &'static str,
    pub bg_root: RgbaColor,
    pub bg_sidebar: RgbaColor,
    pub bg_card: RgbaColor,
    pub bg_card_hover: RgbaColor,
    pub bg_input: RgbaColor,
    pub border_default: RgbaColor,
    pub border_muted: RgbaColor,
    pub border_accent: RgbaColor,
    pub text_primary: RgbaColor,
    pub text_secondary: RgbaColor,
    pub text_muted: RgbaColor,
    pub accent_cyan: RgbaColor,
    pub accent_purple: RgbaColor,
    pub accent_blue: RgbaColor,
    pub status_online: RgbaColor,
    pub status_warn: RgbaColor,
    pub status_crit: RgbaColor,
}

pub static DARK_TECH_PALETTE: ThemePalette = ThemePalette {
    name: "DarkTech",
    bg_root: RgbaColor::from_hex(0x0b0f14),
    bg_sidebar: RgbaColor::from_hex(0x13171e),
    bg_card: RgbaColor::from_hex(0x161b22),
    bg_card_hover: RgbaColor::from_hex(0x1c2129),
    bg_input: RgbaColor::from_hex(0x0f1319),
    border_default: RgbaColor::from_hex(0x21262d),
    border_muted: RgbaColor::from_hex(0x30363d),
    border_accent: RgbaColor::from_hex(0x00ffcc),
    text_primary: RgbaColor::from_hex(0xf0f6fc),
    text_secondary: RgbaColor::from_hex(0x8b949e),
    text_muted: RgbaColor::from_hex(0x484f58),
    accent_cyan: RgbaColor::from_hex(0x00ffcc),
    accent_purple: RgbaColor::from_hex(0xbd93f9),
    accent_blue: RgbaColor::from_hex(0x38bdf8),
    status_online: RgbaColor::from_hex(0x3fb950),
    status_warn: RgbaColor::from_hex(0xd29922),
    status_crit: RgbaColor::from_hex(0xf85149),
};

pub static CYBERPUNK_PALETTE: ThemePalette = ThemePalette {
    name: "CyberpunkNeon",
    bg_root: RgbaColor::from_hex(0x080614),
    bg_sidebar: RgbaColor::from_hex(0x110c26),
    bg_card: RgbaColor::from_hex(0x181236),
    bg_card_hover: RgbaColor::from_hex(0x221a4c),
    bg_input: RgbaColor::from_hex(0x120d2b),
    border_default: RgbaColor::from_hex(0x2f1e60),
    border_muted: RgbaColor::from_hex(0x432b85),
    border_accent: RgbaColor::from_hex(0xff007f),
    text_primary: RgbaColor::from_hex(0xfff5fa),
    text_secondary: RgbaColor::from_hex(0xb2a1d9),
    text_muted: RgbaColor::from_hex(0x715f9e),
    accent_cyan: RgbaColor::from_hex(0x00f0ff),
    accent_purple: RgbaColor::from_hex(0xff007f),
    accent_blue: RgbaColor::from_hex(0x7000ff),
    status_online: RgbaColor::from_hex(0x00ff9f),
    status_warn: RgbaColor::from_hex(0xffb800),
    status_crit: RgbaColor::from_hex(0xff0055),
};

pub static SOLARIZED_DARK_PALETTE: ThemePalette = ThemePalette {
    name: "SolarizedDark",
    bg_root: RgbaColor::from_hex(0x002b36),
    bg_sidebar: RgbaColor::from_hex(0x073642),
    bg_card: RgbaColor::from_hex(0x09414f),
    bg_card_hover: RgbaColor::from_hex(0x0d4e5f),
    bg_input: RgbaColor::from_hex(0x06313c),
    border_default: RgbaColor::from_hex(0x0e5a6d),
    border_muted: RgbaColor::from_hex(0x146d84),
    border_accent: RgbaColor::from_hex(0x2aa198),
    text_primary: RgbaColor::from_hex(0xfdf6e3),
    text_secondary: RgbaColor::from_hex(0x93a1a1),
    text_muted: RgbaColor::from_hex(0x657b83),
    accent_cyan: RgbaColor::from_hex(0x2aa198),
    accent_purple: RgbaColor::from_hex(0x6c71c4),
    accent_blue: RgbaColor::from_hex(0x268bd2),
    status_online: RgbaColor::from_hex(0x859900),
    status_warn: RgbaColor::from_hex(0xb58900),
    status_crit: RgbaColor::from_hex(0xdc322f),
};

pub fn get_palette(name: &str) -> &'static ThemePalette {
    match name {
        "CyberpunkNeon" | "Cyberpunk" => &CYBERPUNK_PALETTE,
        "SolarizedDark" | "Solarized" => &SOLARIZED_DARK_PALETTE,
        _ => &DARK_TECH_PALETTE,
    }
}
