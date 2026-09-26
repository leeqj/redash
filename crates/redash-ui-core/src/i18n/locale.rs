use serde::{Deserialize, Serialize};

/// Supported application display languages / locales.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum Locale {
    /// 简体中文 (Simplified Chinese) - Default Base Locale
    #[default]
    ZhCn = 0,
    /// English (US)
    EnUs = 1,
    /// 繁體中文 (Traditional Chinese)
    ZhTw = 2,
    /// 日本語 (Japanese)
    JaJp = 3,
}

impl Locale {
    /// Converts a numeric index (from atomic storage) back to `Locale`.
    pub fn from_u8(val: u8) -> Self {
        match val {
            0 => Locale::ZhCn,
            1 => Locale::EnUs,
            2 => Locale::ZhTw,
            3 => Locale::JaJp,
            _ => Locale::ZhCn,
        }
    }

    /// Converts the `Locale` to its numeric atomic index.
    pub const fn to_u8(self) -> u8 {
        self as u8
    }

    /// Standard BCP-47 / IETF language code.
    pub const fn code(&self) -> &'static str {
        match self {
            Locale::ZhCn => "zh-CN",
            Locale::EnUs => "en-US",
            Locale::ZhTw => "zh-TW",
            Locale::JaJp => "ja-JP",
        }
    }

    /// Native localized name displayed in user interfaces.
    pub const fn display_name(&self) -> &'static str {
        match self {
            Locale::ZhCn => "简体中文",
            Locale::EnUs => "English",
            Locale::ZhTw => "繁體中文",
            Locale::JaJp => "日本語",
        }
    }

    /// English description / country qualifier.
    pub const fn english_name(&self) -> &'static str {
        match self {
            Locale::ZhCn => "Simplified Chinese",
            Locale::EnUs => "English (US)",
            Locale::ZhTw => "Traditional Chinese",
            Locale::JaJp => "Japanese",
        }
    }

    /// Complete list of supported locales in application order.
    pub const fn all() -> &'static [Locale] {
        &[Locale::ZhCn, Locale::EnUs, Locale::ZhTw, Locale::JaJp]
    }

    /// Resolves a locale from standard language code strings (case-insensitive).
    pub fn from_code(code: &str) -> Option<Self> {
        let normalized = code.trim().replace('_', "-").to_lowercase();
        match normalized.as_str() {
            "zh-cn" | "zh" | "zh-hans" | "zh-sg" => Some(Locale::ZhCn),
            "en-us" | "en" | "en-gb" | "en-ca" | "en-au" => Some(Locale::EnUs),
            "zh-tw" | "zh-hk" | "zh-mo" | "zh-hant" => Some(Locale::ZhTw),
            "ja-jp" | "ja" => Some(Locale::JaJp),
            _ => None,
        }
    }

    /// Detects user's operating system language preference via environment variables.
    pub fn detect_system() -> Self {
        let env_candidates = ["LC_ALL", "LC_MESSAGES", "LANG"];
        for var_name in env_candidates {
            if let Ok(val) = std::env::var(var_name) {
                let trimmed = val.trim();
                if !trimmed.is_empty() {
                    let lang_part = trimmed.split('.').next().unwrap_or(trimmed);
                    if let Some(loc) = Self::from_code(lang_part) {
                        return loc;
                    }
                }
            }
        }
        Locale::ZhCn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locale_u8_roundtrip() {
        for loc in Locale::all() {
            let u = loc.to_u8();
            assert_eq!(Locale::from_u8(u), *loc);
        }
        assert_eq!(Locale::from_u8(99), Locale::ZhCn);
    }

    #[test]
    fn test_locale_from_code() {
        assert_eq!(Locale::from_code("zh-CN"), Some(Locale::ZhCn));
        assert_eq!(Locale::from_code("zh_CN"), Some(Locale::ZhCn));
        assert_eq!(Locale::from_code("en-US"), Some(Locale::EnUs));
        assert_eq!(Locale::from_code("en"), Some(Locale::EnUs));
        assert_eq!(Locale::from_code("zh-TW"), Some(Locale::ZhTw));
        assert_eq!(Locale::from_code("zh_HK"), Some(Locale::ZhTw));
        assert_eq!(Locale::from_code("ja-JP"), Some(Locale::JaJp));
        assert_eq!(Locale::from_code("ja"), Some(Locale::JaJp));
        assert_eq!(Locale::from_code("fr-FR"), None);
    }

    #[test]
    fn test_locale_display_properties() {
        assert_eq!(Locale::ZhCn.display_name(), "简体中文");
        assert_eq!(Locale::EnUs.display_name(), "English");
        assert_eq!(Locale::ZhTw.display_name(), "繁體中文");
        assert_eq!(Locale::JaJp.display_name(), "日本語");
    }

    #[test]
    fn test_locale_detect_system_fallback() {
        let loc = Locale::detect_system();
        assert!(Locale::all().contains(&loc));
    }
}
