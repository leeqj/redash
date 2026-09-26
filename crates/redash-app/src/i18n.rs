pub use redash_core::i18n::{I18n, Locale};

/// Ergonomic translation macro for static strings.
/// Returns `&'static str` with zero heap allocation.
#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::i18n::I18n::t($key)
    };
}

/// Ergonomic translation macro with named parametric interpolation.
///
/// Example:
/// ```ignore
/// let msg = t_fmt!("batch.summary", success = 3, total = 5);
/// ```
#[macro_export]
macro_rules! t_fmt {
    ($key:expr, $($name:ident = $val:expr),* $(,)?) => {
        $crate::i18n::I18n::t_fmt($key, &[
            $((stringify!($name), &$val.to_string())),*
        ])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_i18n_macros() {
        let _guard = redash_core::i18n::TEST_LOCALE_MUTEX.lock().unwrap();
        I18n::set_locale(Locale::ZhCn);
        assert_eq!(t!("nav.fleet"), "全局脉搏大盘");
        assert_eq!(t!("common.save"), "保存修改");

        let msg = t_fmt!("batch.summary", success = 4, total = 8);
        assert_eq!(msg, "已完成 4/8 台主机并发执行");

        I18n::set_locale(Locale::EnUs);
        assert_eq!(t!("nav.fleet"), "Fleet Pulse Matrix");
        assert_eq!(t!("common.save"), "Save Changes");

        let msg_en = t_fmt!("batch.summary", success = 4, total = 8);
        assert_eq!(msg_en, "Completed 4/8 hosts concurrently");

        // Traditional Chinese
        I18n::set_locale(Locale::ZhTw);
        assert_eq!(t!("nav.fleet"), "全局脈搏大盤");
        assert_eq!(t!("common.save"), "儲存修改");
        assert_eq!(t!("settings.cat_probe"), "探針與監控");

        // Japanese
        I18n::set_locale(Locale::JaJp);
        assert_eq!(t!("nav.fleet"), "フリート・パルス");
        assert_eq!(t!("common.save"), "変更を保存");
        assert_eq!(t!("settings.cat_probe"), "プローブと監視");

        I18n::set_locale(Locale::ZhCn);
    }

    #[test]
    fn test_app_i18n_hot_language_switch_consistency() {
        let _guard = redash_core::i18n::TEST_LOCALE_MUTEX.lock().unwrap();
        for loc in Locale::all() {
            I18n::set_locale(*loc);
            assert_eq!(I18n::locale(), *loc);

            // Test key operations across all views
            assert!(!t!("common.save").is_empty());
            assert!(!t!("common.cancel").is_empty());
            assert!(!t!("common.refresh").is_empty());
            assert!(!t!("fleet.title").is_empty());
            assert!(!t!("host.add_title").is_empty());
            assert!(!t!("batch.title").is_empty());
            assert!(!t!("workbench.tab_terminal").is_empty());
            assert!(!t!("sftp.title").is_empty());
            assert!(!t!("settings.title").is_empty());
            assert!(!t!("menu.quit").is_empty());
        }
        I18n::set_locale(Locale::ZhCn);
    }
}
