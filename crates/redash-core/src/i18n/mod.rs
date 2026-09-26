pub mod dict;
pub mod locale;

pub use locale::Locale;
use std::sync::atomic::{AtomicU8, Ordering};

/// Global wait-free atomic storage for the active application locale.
/// Default: 0 (`Locale::ZhCn`).
static CURRENT_LOCALE: AtomicU8 = AtomicU8::new(0);

/// Global test mutex for serializing tests that alter the global locale.
#[doc(hidden)]
pub static TEST_LOCALE_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Central internationalization (i18n) runtime service.
pub struct I18n;

impl I18n {
    /// Returns the currently active `Locale` (wait-free, zero allocation).
    #[inline]
    pub fn locale() -> Locale {
        Locale::from_u8(CURRENT_LOCALE.load(Ordering::Relaxed))
    }

    /// Sets the application active `Locale` immediately across all threads.
    pub fn set_locale(locale: Locale) {
        CURRENT_LOCALE.store(locale.to_u8(), Ordering::Relaxed);
    }

    /// Sets the active locale by string code (e.g. "zh-CN", "en-US", "zh-TW", "ja-JP").
    /// Falls back to `Locale::ZhCn` if code is unrecognized.
    pub fn set_locale_by_code(code: &str) -> Locale {
        let loc = Locale::from_code(code).unwrap_or(Locale::ZhCn);
        Self::set_locale(loc);
        loc
    }

    /// Translates a static key with zero-allocation, cascading fallback.
    ///
    /// Fallback order:
    /// 1. Active Locale translation
    /// 2. `Locale::ZhCn` (Base fallback)
    /// 3. `Locale::EnUs` (Secondary fallback)
    /// 4. Raw `key` literal itself (fail-safe)
    #[inline]
    pub fn t(key: &'static str) -> &'static str {
        let current = Self::locale();

        // 1. Primary lookup
        if let Some(val) = dict::lookup_in_locale(current, key) {
            return val;
        }

        // 2. Base fallback: zh-CN
        if current != Locale::ZhCn
            && let Some(val) = dict::lookup_in_locale(Locale::ZhCn, key)
        {
            return val;
        }

        // 3. Secondary fallback: en-US
        if current != Locale::EnUs
            && let Some(val) = dict::lookup_in_locale(Locale::EnUs, key)
        {
            return val;
        }

        // 4. Fail-safe: return literal key
        key
    }

    /// Translates a template key and replaces named `{param}` placeholders.
    pub fn t_fmt(key: &'static str, params: &[(&str, &str)]) -> String {
        let template = Self::t(key);
        let mut result = template.to_string();
        for (name, val) in params {
            let placeholder = format!("{{{}}}", name);
            result = result.replace(&placeholder, val);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_i18n_default_and_switching() {
        let _guard = TEST_LOCALE_MUTEX.lock().unwrap();
        I18n::set_locale(Locale::ZhCn);
        assert_eq!(I18n::locale(), Locale::ZhCn);
        assert_eq!(I18n::t("nav.fleet"), "全局脉搏大盘");
        assert_eq!(I18n::t("common.save"), "保存修改");

        // Switch to English
        I18n::set_locale(Locale::EnUs);
        assert_eq!(I18n::locale(), Locale::EnUs);
        assert_eq!(I18n::t("nav.fleet"), "Fleet Pulse Matrix");
        assert_eq!(I18n::t("common.save"), "Save Changes");

        // Switch to Traditional Chinese
        I18n::set_locale(Locale::ZhTw);
        assert_eq!(I18n::locale(), Locale::ZhTw);
        assert_eq!(I18n::t("nav.fleet"), "全局脈搏大盤");
        assert_eq!(I18n::t("common.save"), "儲存修改");

        // Switch to Japanese
        I18n::set_locale(Locale::JaJp);
        assert_eq!(I18n::locale(), Locale::JaJp);
        assert_eq!(I18n::t("nav.fleet"), "フリート・パルス");
        assert_eq!(I18n::t("common.save"), "変更を保存");

        // Restore to default
        I18n::set_locale(Locale::ZhCn);
    }

    #[test]
    fn test_i18n_fallback_for_unknown_key() {
        let _guard = TEST_LOCALE_MUTEX.lock().unwrap();
        I18n::set_locale(Locale::EnUs);
        // Non-existent key should return the literal key itself, never panic
        assert_eq!(I18n::t("unknown.system.key"), "unknown.system.key");
    }

    #[test]
    fn test_i18n_parametric_interpolation() {
        let _guard = TEST_LOCALE_MUTEX.lock().unwrap();
        I18n::set_locale(Locale::ZhCn);
        let s_cn = I18n::t_fmt("batch.summary", &[("success", "3"), ("total", "5")]);
        assert_eq!(s_cn, "已完成 3/5 台主机并发执行");

        I18n::set_locale(Locale::EnUs);
        let s_en = I18n::t_fmt("batch.summary", &[("success", "3"), ("total", "5")]);
        assert_eq!(s_en, "Completed 3/5 hosts concurrently");

        I18n::set_locale(Locale::ZhTw);
        let s_tw = I18n::t_fmt("batch.summary", &[("success", "3"), ("total", "5")]);
        assert_eq!(s_tw, "已完成 3/5 台主機並發執行");

        I18n::set_locale(Locale::ZhCn);
    }

    #[test]
    fn test_set_locale_by_code() {
        let _guard = TEST_LOCALE_MUTEX.lock().unwrap();
        assert_eq!(I18n::set_locale_by_code("en-US"), Locale::EnUs);
        assert_eq!(I18n::locale(), Locale::EnUs);

        assert_eq!(I18n::set_locale_by_code("zh-CN"), Locale::ZhCn);
        assert_eq!(I18n::locale(), Locale::ZhCn);

        assert_eq!(I18n::set_locale_by_code("invalid_code"), Locale::ZhCn);
    }
}
