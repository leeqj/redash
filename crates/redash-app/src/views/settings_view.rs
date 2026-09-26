use crate::components::icon::Icon;
use crate::components::theme::{DarkTechTheme, ThemePalette};
use crate::i18n::{I18n, Locale};
use gpui::prelude::FluentBuilder;
use gpui::*;
use redash_core::config::{AlertDispatcher, AlertEvent, AppSettings, AppSettingsExt, HostStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsCategory {
    Probe,
    Terminal,
    Appearance,
    Alerts,
    Storage,
    About,
}

pub enum SettingsAction {
    Save(AppSettings),
    ResetDefaults,
    ResetDemoHosts,
    ExportHosts,
    ImportHosts,
}

pub type SettingsActionCallback =
    Box<dyn Fn(SettingsAction, &mut Window, &mut Context<SettingsView>) + 'static>;

pub type ThemePreviewCallback = Box<dyn Fn(&str, &mut Context<SettingsView>) + 'static>;
pub type LocalePreviewCallback = Box<dyn Fn(&str, &mut Context<SettingsView>) + 'static>;

pub struct SettingsView {
    category: SettingsCategory,
    draft: AppSettings,
    saved: AppSettings,
    status_message: Option<(String, bool)>, // (message, is_success)
    on_action: Option<SettingsActionCallback>,
    pub on_theme_preview: Option<ThemePreviewCallback>,
    pub on_locale_preview: Option<LocalePreviewCallback>,
    pub scroll_handle: ScrollHandle,
}

impl SettingsView {
    pub fn new(settings: AppSettings, _cx: &mut Context<Self>) -> Self {
        Self {
            category: SettingsCategory::Probe,
            draft: settings.clone(),
            saved: settings,
            status_message: None,
            on_action: None,
            on_theme_preview: None,
            on_locale_preview: None,
            scroll_handle: ScrollHandle::new(),
        }
    }

    pub fn on_action(
        &mut self,
        callback: impl Fn(SettingsAction, &mut Window, &mut Context<Self>) + 'static,
    ) {
        self.on_action = Some(Box::new(callback));
    }

    pub fn set_on_theme_preview(&mut self, callback: impl Fn(&str, &mut Context<Self>) + 'static) {
        self.on_theme_preview = Some(Box::new(callback));
    }

    pub fn set_on_locale_preview(&mut self, callback: impl Fn(&str, &mut Context<Self>) + 'static) {
        self.on_locale_preview = Some(Box::new(callback));
    }

    /// Previews a theme in real time across the entire application without restarting.
    pub fn preview_theme(&mut self, theme_name: String, cx: &mut Context<Self>) {
        self.draft.theme_name = theme_name.clone();
        DarkTechTheme::set_active_theme(&theme_name);
        if let Some(cb) = &self.on_theme_preview {
            cb(&theme_name, cx);
        }
        cx.notify();
    }

    /// Previews a display language in real time across the entire application without restarting.
    pub fn preview_language(&mut self, language_code: String, cx: &mut Context<Self>) {
        self.draft.language = language_code.clone();
        I18n::set_locale_by_code(&language_code);
        if let Some(cb) = &self.on_locale_preview {
            cb(&language_code, cx);
        }
        cx.notify();
    }

    /// Reverts all pending changes back to the saved settings, restoring active theme & language.
    pub fn revert_changes(&mut self, cx: &mut Context<Self>) {
        let saved_theme = self.saved.theme_name.clone();
        let saved_lang = self.saved.language.clone();
        self.draft = self.saved.clone();
        self.status_message = Some((crate::t!("settings.reverted_msg").to_string(), true));
        DarkTechTheme::set_active_theme(&saved_theme);
        I18n::set_locale_by_code(&saved_lang);
        if let Some(cb) = &self.on_theme_preview {
            cb(&saved_theme, cx);
        }
        if let Some(cb) = &self.on_locale_preview {
            cb(&saved_lang, cx);
        }
        cx.notify();
    }

    pub fn set_settings(&mut self, settings: AppSettings, cx: &mut Context<Self>) {
        self.draft = settings.clone();
        self.saved = settings;
        cx.notify();
    }

    pub fn set_status_message(&mut self, msg: String, is_success: bool, cx: &mut Context<Self>) {
        self.status_message = Some((msg, is_success));
        cx.notify();
    }

    pub fn is_dirty(&self) -> bool {
        self.draft != self.saved
    }

    fn render_sidebar_item(
        &self,
        category: SettingsCategory,
        label: &'static str,
        icon: Icon,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_selected = self.category == category;
        let icon_color = if is_selected {
            DarkTechTheme::text_accent()
        } else {
            DarkTechTheme::text_secondary()
        };

        div()
            .id(ElementId::NamedInteger(
                "settings_cat".into(),
                category as u64,
            ))
            .h(px(38.0))
            .w_full()
            .px_3()
            .rounded_md()
            .flex()
            .flex_row()
            .items_center()
            .gap_2p5()
            .cursor_pointer()
            .bg(if is_selected {
                DarkTechTheme::bg_panel_hover()
            } else {
                gpui::hsla(0.0, 0.0, 0.0, 0.0)
            })
            .border_1()
            .border_color(if is_selected {
                DarkTechTheme::border_active()
            } else {
                gpui::hsla(0.0, 0.0, 0.0, 0.0)
            })
            .hover(|s| {
                if is_selected {
                    s
                } else {
                    s.bg(DarkTechTheme::bg_panel_hover())
                        .border_color(DarkTechTheme::border_muted())
                }
            })
            .on_click(cx.listener(move |this, _event: &ClickEvent, _window, cx| {
                this.category = category;
                this.status_message = None;
                cx.notify();
            }))
            .child(icon.with_size(px(14.0)).with_color(icon_color))
            .child(
                div()
                    .text_size(px(12.5))
                    .font_weight(if is_selected {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(icon_color)
                    .child(label),
            )
    }

    fn render_option_pill<T: PartialEq + Clone + 'static>(
        id_prefix: &'static str,
        label: &'static str,
        value: T,
        current_value: &T,
        on_select: impl Fn(T, &mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_active = value == *current_value;
        let val_clone = value.clone();

        div()
            .id(ElementId::Name(format!("{}_{}", id_prefix, label).into()))
            .h(px(28.0))
            .px_3()
            .rounded_md()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_size(px(11.5))
            .font_weight(if is_active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            })
            .bg(if is_active {
                DarkTechTheme::bg_panel_hover()
            } else {
                DarkTechTheme::bg_input()
            })
            .border_1()
            .border_color(if is_active {
                DarkTechTheme::border_active()
            } else {
                DarkTechTheme::border_default()
            })
            .text_color(if is_active {
                DarkTechTheme::text_accent()
            } else {
                DarkTechTheme::text_secondary()
            })
            .hover(|s| {
                if is_active {
                    s
                } else {
                    s.border_color(DarkTechTheme::border_muted())
                        .text_color(DarkTechTheme::text_primary())
                }
            })
            .on_click(cx.listener(move |this, _event: &ClickEvent, _window, cx| {
                on_select(val_clone.clone(), this, cx);
            }))
            .child(label)
    }

    fn render_toggle(
        enabled: bool,
        label: &'static str,
        desc: &'static str,
        on_toggle: impl Fn(bool, &mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let new_state = !enabled;

        div()
            .w_full()
            .p_3()
            .rounded_lg()
            .bg(DarkTechTheme::bg_input())
            .border_1()
            .border_color(DarkTechTheme::border_muted())
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(DarkTechTheme::text_primary())
                            .child(label),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(DarkTechTheme::text_muted())
                            .child(desc),
                    ),
            )
            .child(
                div()
                    .id(ElementId::Name(format!("toggle_{}", label).into()))
                    .w(px(42.0))
                    .h(px(22.0))
                    .rounded_full()
                    .p(px(2.0))
                    .cursor_pointer()
                    .bg(if enabled {
                        DarkTechTheme::border_active()
                    } else {
                        DarkTechTheme::border_default()
                    })
                    .on_click(cx.listener(move |this, _event: &ClickEvent, _window, cx| {
                        on_toggle(new_state, this, cx);
                    }))
                    .child(
                        div()
                            .size(px(18.0))
                            .rounded_full()
                            .bg(DarkTechTheme::bg_root())
                            .when(enabled, |d| d.ml(px(20.0))),
                    ),
            )
    }

    fn render_probe_settings(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let draft = &self.draft;

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(DarkTechTheme::text_primary())
                            .child(crate::t!("settings.probe_title")),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(DarkTechTheme::text_muted())
                            .child(crate::t!("settings.probe_desc")),
                    ),
            )
            // 1. 刷新周期
            .child(
                div()
                    .p_3p5()
                    .rounded_lg()
                    .bg(DarkTechTheme::bg_panel())
                    .border_1()
                    .border_color(DarkTechTheme::border_default())
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(DarkTechTheme::text_primary())
                                    .child(crate::t!("settings.interval_label")),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .font_family("Menlo")
                                    .text_color(DarkTechTheme::text_accent())
                                    .child(format!("{} 秒/次", draft.probe_interval_secs)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(Self::render_option_pill(
                                "probe_interval",
                                crate::t!("settings.opt_1s"),
                                1u64,
                                &draft.probe_interval_secs,
                                |v, this, cx| {
                                    this.draft.probe_interval_secs = v;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(Self::render_option_pill(
                                "probe_interval",
                                crate::t!("settings.opt_2s"),
                                2u64,
                                &draft.probe_interval_secs,
                                |v, this, cx| {
                                    this.draft.probe_interval_secs = v;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(Self::render_option_pill(
                                "probe_interval",
                                crate::t!("settings.opt_5s"),
                                5u64,
                                &draft.probe_interval_secs,
                                |v, this, cx| {
                                    this.draft.probe_interval_secs = v;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(Self::render_option_pill(
                                "probe_interval",
                                crate::t!("settings.opt_10s"),
                                10u64,
                                &draft.probe_interval_secs,
                                |v, this, cx| {
                                    this.draft.probe_interval_secs = v;
                                    cx.notify();
                                },
                                cx,
                            )),
                    ),
            )
            // 2. 超时时间
            .child(
                div()
                    .p_3p5()
                    .rounded_lg()
                    .bg(DarkTechTheme::bg_panel())
                    .border_1()
                    .border_color(DarkTechTheme::border_default())
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(DarkTechTheme::text_primary())
                                    .child(crate::t!("settings.timeout_label")),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .font_family("Menlo")
                                    .text_color(DarkTechTheme::text_accent())
                                    .child(format!("{} 秒", draft.probe_timeout_secs)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(Self::render_option_pill(
                                "probe_timeout",
                                "3s (严苛)",
                                3u64,
                                &draft.probe_timeout_secs,
                                |v, this, cx| {
                                    this.draft.probe_timeout_secs = v;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(Self::render_option_pill(
                                "probe_timeout",
                                "5s (默认)",
                                5u64,
                                &draft.probe_timeout_secs,
                                |v, this, cx| {
                                    this.draft.probe_timeout_secs = v;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(Self::render_option_pill(
                                "probe_timeout",
                                "10s (宽松)",
                                10u64,
                                &draft.probe_timeout_secs,
                                |v, this, cx| {
                                    this.draft.probe_timeout_secs = v;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(Self::render_option_pill(
                                "probe_timeout",
                                "15s (高延迟网络)",
                                15u64,
                                &draft.probe_timeout_secs,
                                |v, this, cx| {
                                    this.draft.probe_timeout_secs = v;
                                    cx.notify();
                                },
                                cx,
                            )),
                    ),
            )
            // 3. 历史脉搏环形缓冲容量
            .child(
                div()
                    .p_3p5()
                    .rounded_lg()
                    .bg(DarkTechTheme::bg_panel())
                    .border_1()
                    .border_color(DarkTechTheme::border_default())
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(DarkTechTheme::text_primary())
                                    .child("脉搏历史点数 (Sparkline Buffer Points)"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .font_family("Menlo")
                                    .text_color(DarkTechTheme::text_accent())
                                    .child(format!("{} 点", draft.history_points)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(Self::render_option_pill(
                                "history_points",
                                "20 点 (极简)",
                                20usize,
                                &draft.history_points,
                                |v, this, cx| {
                                    this.draft.history_points = v;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(Self::render_option_pill(
                                "history_points",
                                "30 点 (标准)",
                                30usize,
                                &draft.history_points,
                                |v, this, cx| {
                                    this.draft.history_points = v;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(Self::render_option_pill(
                                "history_points",
                                "60 点 (精细)",
                                60usize,
                                &draft.history_points,
                                |v, this, cx| {
                                    this.draft.history_points = v;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(Self::render_option_pill(
                                "history_points",
                                "120 点 (全景)",
                                120usize,
                                &draft.history_points,
                                |v, this, cx| {
                                    this.draft.history_points = v;
                                    cx.notify();
                                },
                                cx,
                            )),
                    ),
            )
            // 4. 自动刷新开关
            .child(Self::render_toggle(
                draft.auto_refresh,
                "后台自动执行实时探针探测",
                "开启后将在后台定期采集 CPU、内存、磁盘与网络吞吐指标，关闭后仅在手动刷新时探测。",
                |v, this, cx| {
                    this.draft.auto_refresh = v;
                    cx.notify();
                },
                cx,
            ))
    }

    fn render_terminal_settings(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let draft = &self.draft;

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(DarkTechTheme::text_primary())
                            .child(crate::t!("settings.terminal_title")),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(DarkTechTheme::text_muted())
                            .child("定制 SSH PTY 终端字体族、字号规格、光标形态与回滚缓冲限制。"),
                    ),
            )
            // 1. 字体选择
            .child(
                div()
                    .p_3p5()
                    .rounded_lg()
                    .bg(DarkTechTheme::bg_panel())
                    .border_1()
                    .border_color(DarkTechTheme::border_default())
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(DarkTechTheme::text_primary())
                            .child(crate::t!("settings.font_family")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap_2()
                            .child(Self::render_option_pill(
                                "term_font",
                                "Menlo",
                                "Menlo".to_string(),
                                &draft.terminal_font_family,
                                |v, this, cx| {
                                    this.draft.terminal_font_family = v;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(Self::render_option_pill(
                                "term_font",
                                "SF Mono",
                                "SF Mono".to_string(),
                                &draft.terminal_font_family,
                                |v, this, cx| {
                                    this.draft.terminal_font_family = v;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(Self::render_option_pill(
                                "term_font",
                                "JetBrains Mono",
                                "JetBrains Mono".to_string(),
                                &draft.terminal_font_family,
                                |v, this, cx| {
                                    this.draft.terminal_font_family = v;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(Self::render_option_pill(
                                "term_font",
                                "Fira Code",
                                "Fira Code".to_string(),
                                &draft.terminal_font_family,
                                |v, this, cx| {
                                    this.draft.terminal_font_family = v;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(Self::render_option_pill(
                                "term_font",
                                "Courier New",
                                "Courier New".to_string(),
                                &draft.terminal_font_family,
                                |v, this, cx| {
                                    this.draft.terminal_font_family = v;
                                    cx.notify();
                                },
                                cx,
                            )),
                    ),
            )
            // 2. 字号选择
            .child(
                div()
                    .p_3p5()
                    .rounded_lg()
                    .bg(DarkTechTheme::bg_panel())
                    .border_1()
                    .border_color(DarkTechTheme::border_default())
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(DarkTechTheme::text_primary())
                            .child(crate::t!("settings.font_size")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(Self::render_option_pill(
                                "term_size",
                                "11 px",
                                11.0f32,
                                &draft.terminal_font_size,
                                |v, this, cx| {
                                    this.draft.terminal_font_size = v;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(Self::render_option_pill(
                                "term_size",
                                "12 px (推荐)",
                                12.0f32,
                                &draft.terminal_font_size,
                                |v, this, cx| {
                                    this.draft.terminal_font_size = v;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(Self::render_option_pill(
                                "term_size",
                                "13 px",
                                13.0f32,
                                &draft.terminal_font_size,
                                |v, this, cx| {
                                    this.draft.terminal_font_size = v;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(Self::render_option_pill(
                                "term_size",
                                "14 px",
                                14.0f32,
                                &draft.terminal_font_size,
                                |v, this, cx| {
                                    this.draft.terminal_font_size = v;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(Self::render_option_pill(
                                "term_size",
                                "16 px",
                                16.0f32,
                                &draft.terminal_font_size,
                                |v, this, cx| {
                                    this.draft.terminal_font_size = v;
                                    cx.notify();
                                },
                                cx,
                            )),
                    ),
            )
            // 3. 光标样式
            .child(
                div()
                    .p_3p5()
                    .rounded_lg()
                    .bg(DarkTechTheme::bg_panel())
                    .border_1()
                    .border_color(DarkTechTheme::border_default())
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(DarkTechTheme::text_primary())
                            .child("光标样式 (Cursor Style)"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(Self::render_option_pill(
                                "term_cursor",
                                "█ 块状方块 (Block)",
                                "Block".to_string(),
                                &draft.terminal_cursor_style,
                                |v, this, cx| {
                                    this.draft.terminal_cursor_style = v;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(Self::render_option_pill(
                                "term_cursor",
                                "| 细竖线 (Line)",
                                "Line".to_string(),
                                &draft.terminal_cursor_style,
                                |v, this, cx| {
                                    this.draft.terminal_cursor_style = v;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(Self::render_option_pill(
                                "term_cursor",
                                "_ 下划线 (Underline)",
                                "Underline".to_string(),
                                &draft.terminal_cursor_style,
                                |v, this, cx| {
                                    this.draft.terminal_cursor_style = v;
                                    cx.notify();
                                },
                                cx,
                            )),
                    ),
            )
            // 4. 实时终端预览卡片
            .child(
                div()
                    .p_3()
                    .rounded_lg()
                    .bg(DarkTechTheme::bg_input())
                    .border_1()
                    .border_color(DarkTechTheme::border_active())
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(DarkTechTheme::text_accent())
                            .font_weight(FontWeight::BOLD)
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .child(
                                        Icon::play()
                                            .with_size(px(10.0))
                                            .with_color(DarkTechTheme::text_accent()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.5))
                                            .text_color(DarkTechTheme::text_accent())
                                            .font_weight(FontWeight::BOLD)
                                            .child("实时终端渲染预览 (Live Preview)"),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .h(px(64.0))
                            .w_full()
                            .p_2()
                            .rounded_md()
                            .bg(DarkTechTheme::bg_root())
                            .border_1()
                            .border_color(DarkTechTheme::border_muted())
                            .font_family(draft.terminal_font_family.clone())
                            .text_size(px(draft.terminal_font_size))
                            .flex()
                            .flex_col()
                            .justify_center()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_color(DarkTechTheme::status_online())
                                            .child("redash@mesh-gateway"),
                                    )
                                    .child(div().text_color(DarkTechTheme::text_muted()).child(":"))
                                    .child(
                                        div().text_color(DarkTechTheme::accent_indigo()).child("~"),
                                    )
                                    .child(
                                        div()
                                            .text_color(DarkTechTheme::text_primary())
                                            .child("$ uname -srm"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_color(DarkTechTheme::text_secondary())
                                            .child("Linux 6.8.0-45-generic x86_64"),
                                    )
                                    .child(div().text_color(DarkTechTheme::accent_cyan()).child(
                                        match draft.terminal_cursor_style.as_str() {
                                            "Line" => "|",
                                            "Underline" => "_",
                                            _ => "█",
                                        },
                                    )),
                            ),
                    ),
            )
            // 5. 选中文本自动复制开关
            .child(Self::render_toggle(
                draft.terminal_copy_on_select,
                "光标选中文本时自动复制到剪贴板 (Copy on Select)",
                "在终端中拖拽鼠标选择文字内容时，自动将其复制到操作系统系统剪贴板。",
                |v, this, cx| {
                    this.draft.terminal_copy_on_select = v;
                    cx.notify();
                },
                cx,
            ))
    }

    fn render_appearance_settings(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(DarkTechTheme::text_primary())
                            .child(crate::t!("settings.cat_appearance")),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(DarkTechTheme::text_muted())
                            .child("管理深色科技调色板、1px 微发光边框状态与大盘卡片密度。"),
                    ),
            )
            // 1. 主题方案 (支持即时换肤与实时预览)
            .child(
                div()
                    .p_3p5()
                    .rounded_lg()
                    .bg(DarkTechTheme::bg_panel())
                    .border_1()
                    .border_color(DarkTechTheme::border_default())
                    .flex()
                    .flex_col()
                    .gap_2p5()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(DarkTechTheme::text_primary())
                                    .child(crate::t!("settings.theme_title")),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(DarkTechTheme::text_accent())
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                Icon::zap()
                                                    .with_size(px(10.0))
                                                    .with_color(DarkTechTheme::text_accent()),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.0))
                                                    .text_color(DarkTechTheme::text_accent())
                                                    .child("无需重启，点击方案即刻全局实时预览"),
                                            ),
                                    ),
                            ),
                    )
                    .child(self.render_theme_cards(cx)),
            )
            // 2. 界面显示语言与地区 (Language & Region)
            .child(
                div()
                    .p_3p5()
                    .rounded_lg()
                    .bg(DarkTechTheme::bg_panel())
                    .border_1()
                    .border_color(DarkTechTheme::border_default())
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(DarkTechTheme::text_primary())
                                    .child(crate::t!("settings.language_title")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        Icon::zap()
                                            .with_size(px(10.0))
                                            .with_color(DarkTechTheme::text_accent()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(DarkTechTheme::text_accent())
                                            .child(crate::t!("settings.language_desc")),
                                    ),
                            ),
                    )
                    .child(self.render_language_cards(cx)),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(DarkTechTheme::text_muted())
                    .child("发光特效与紧凑布局尚未支持，暂不提供设置。"),
            )
    }

    fn render_language_cards(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let current_lang = self.draft.language.clone();
        let saved_lang = self.saved.language.clone();
        let locales = Locale::all();

        div()
            .flex()
            .flex_col()
            .gap_2p5()
            .children(locales.iter().enumerate().map(|(idx, &loc)| {
                let code = loc.code().to_string();
                let code_clone = code.clone();
                let is_selected = current_lang == code;
                let is_saved = saved_lang == code;

                div()
                    .id(ElementId::NamedInteger("language_card".into(), idx as u64))
                    .w_full()
                    .p_3()
                    .rounded_lg()
                    .bg(if is_selected {
                        DarkTechTheme::bg_panel_hover()
                    } else {
                        DarkTechTheme::bg_input()
                    })
                    .border_1()
                    .border_color(if is_selected {
                        DarkTechTheme::border_active()
                    } else {
                        DarkTechTheme::border_default()
                    })
                    .hover(|s| {
                        s.bg(DarkTechTheme::bg_panel_hover())
                            .border_color(DarkTechTheme::border_active())
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _e: &ClickEvent, _window, cx| {
                        this.preview_language(code_clone.clone(), cx);
                    }))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    // Left: Radio indicator + Title + English description
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            // Radio dot
                            .child(
                                div()
                                    .size(px(16.0))
                                    .rounded_full()
                                    .border_2()
                                    .border_color(if is_selected {
                                        DarkTechTheme::border_active()
                                    } else {
                                        DarkTechTheme::border_default()
                                    })
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .when(is_selected, |d| {
                                        d.child(
                                            div()
                                                .size(px(8.0))
                                                .rounded_full()
                                                .bg(DarkTechTheme::border_active()),
                                        )
                                    }),
                            )
                            // Title & Description
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(if is_selected {
                                                        DarkTechTheme::text_accent()
                                                    } else {
                                                        DarkTechTheme::text_primary()
                                                    })
                                                    .child(loc.display_name()),
                                            )
                                            .child(
                                                div()
                                                    .px_1p5()
                                                    .py_0p5()
                                                    .rounded_sm()
                                                    .bg(DarkTechTheme::bg_root())
                                                    .border_1()
                                                    .border_color(DarkTechTheme::border_default())
                                                    .text_size(px(10.0))
                                                    .font_family("Menlo")
                                                    .text_color(DarkTechTheme::text_muted())
                                                    .child(loc.code()),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(DarkTechTheme::text_muted())
                                            .child(loc.english_name()),
                                    ),
                            ),
                    )
                    // Right: status badge
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .when(is_selected && !is_saved, |d| {
                                d.child(
                                    div()
                                        .px_2()
                                        .py_0p5()
                                        .rounded_full()
                                        .bg(DarkTechTheme::status_warn())
                                        .text_size(px(10.0))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(DarkTechTheme::bg_root())
                                        .child("实时预览中"),
                                )
                            })
                            .when(is_saved, |d| {
                                d.child(
                                    div()
                                        .px_2()
                                        .py_0p5()
                                        .rounded_full()
                                        .bg(DarkTechTheme::status_online())
                                        .text_size(px(10.0))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(DarkTechTheme::bg_root())
                                        .child("当前配置"),
                                )
                            }),
                    )
            }))
    }

    fn render_theme_cards(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let current_theme = self.draft.theme_name.clone();
        let saved_theme = self.saved.theme_name.clone();
        let presets = ThemePalette::all_presets();

        div()
            .flex()
            .flex_col()
            .gap_2p5()
            .children(presets.into_iter().enumerate().map(|(idx, p)| {
                let p_name = p.name.to_string();
                let p_name_clone = p_name.clone();
                let is_selected = current_theme == p.name;
                let is_saved = saved_theme == p.name;

                div()
                    .id(ElementId::NamedInteger("theme_card".into(), idx as u64))
                    .w_full()
                    .p_3()
                    .rounded_lg()
                    .bg(if is_selected {
                        DarkTechTheme::bg_panel_hover()
                    } else {
                        DarkTechTheme::bg_input()
                    })
                    .border_1()
                    .border_color(if is_selected {
                        DarkTechTheme::border_active()
                    } else {
                        DarkTechTheme::border_default()
                    })
                    .hover(|s| {
                        s.bg(DarkTechTheme::bg_panel_hover())
                            .border_color(DarkTechTheme::border_active())
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _e: &ClickEvent, _window, cx| {
                        this.preview_theme(p_name_clone.clone(), cx);
                    }))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    // Left: Radio indicator + Title + Description
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            // Radio dot
                            .child(
                                div()
                                    .size(px(16.0))
                                    .rounded_full()
                                    .border_2()
                                    .border_color(if is_selected {
                                        DarkTechTheme::border_active()
                                    } else {
                                        DarkTechTheme::border_default()
                                    })
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .when(is_selected, |d| {
                                        d.child(
                                            div()
                                                .size(px(8.0))
                                                .rounded_full()
                                                .bg(DarkTechTheme::border_active()),
                                        )
                                    }),
                            )
                            // Title & Description
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(if is_selected {
                                                        DarkTechTheme::text_accent()
                                                    } else {
                                                        DarkTechTheme::text_primary()
                                                    })
                                                    .child(p.title),
                                            )
                                            .when(is_selected && is_saved, |d| {
                                                d.child(
                                                div()
                                                    .px_1p5()
                                                    .py_0p5()
                                                    .rounded_sm()
                                                    .bg(DarkTechTheme::status_online_halo())
                                                    .text_size(px(9.5))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(DarkTechTheme::status_online())
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .flex_row()
                                                            .items_center()
                                                            .gap_1()
                                                            .child(
                                                                Icon::check()
                                                                    .with_size(px(9.0))
                                                                    .with_color(
                                                                    DarkTechTheme::status_online(),
                                                                ),
                                                            )
                                                            .child("当前使用"),
                                                    ),
                                            )
                                            })
                                            .when(is_selected && !is_saved, |d| {
                                                d.child(
                                                    div()
                                                        .px_1p5()
                                                        .py_0p5()
                                                        .rounded_sm()
                                                        .bg(DarkTechTheme::status_warn_halo())
                                                        .text_size(px(9.5))
                                                        .font_weight(FontWeight::BOLD)
                                                        .text_color(DarkTechTheme::status_warn())
                                                        .child("● 实时预览中 (未保存)"),
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(DarkTechTheme::text_muted())
                                            .child(p.description),
                                    ),
                            ),
                    )
                    // Right: 4-color palette swatch preview
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1p5()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(DarkTechTheme::bg_root())
                            .border_1()
                            .border_color(DarkTechTheme::border_muted())
                            // Swatch 1: Root BG
                            .child(
                                div()
                                    .size(px(14.0))
                                    .rounded_xs()
                                    .bg(rgb(p.bg_root))
                                    .border_1()
                                    .border_color(rgb(p.border_default)),
                            )
                            // Swatch 2: Panel BG
                            .child(
                                div()
                                    .size(px(14.0))
                                    .rounded_xs()
                                    .bg(rgb(p.bg_panel))
                                    .border_1()
                                    .border_color(rgb(p.border_default)),
                            )
                            // Swatch 3: Active Border / Glow
                            .child(div().size(px(14.0)).rounded_xs().bg(rgb(p.border_active)))
                            // Swatch 4: Brand Accent
                            .child(div().size(px(14.0)).rounded_xs().bg(rgb(p.accent_cyan))),
                    )
            }))
    }

    fn render_alerts_settings(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let draft = &self.draft;

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(DarkTechTheme::text_primary())
                            .child(crate::t!("settings.alerts_title")),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(DarkTechTheme::text_muted())
                            .child("实时监控指标超标、节点离线故障自动化检测，支持 macOS 原生桌面通知与 Webhook 机器人消息分发。"),
                    ),
            )
            // 1. CPU 阈值
            .child(
                div()
                    .p_3p5()
                    .rounded_lg()
                    .bg(DarkTechTheme::bg_panel())
                    .border_1()
                    .border_color(DarkTechTheme::border_default())
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(DarkTechTheme::text_primary())
                                    .child(crate::t!("settings.cpu_threshold")),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .font_family("Menlo")
                                    .text_color(DarkTechTheme::text_accent())
                                    .child(format!("{:.0}%", draft.alert_cpu_threshold)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(Self::render_option_pill("alert_cpu", "80%", 80.0f32, &draft.alert_cpu_threshold, |v, this, cx| {
                                this.draft.alert_cpu_threshold = v;
                                cx.notify();
                            }, cx))
                            .child(Self::render_option_pill("alert_cpu", "85%", 85.0f32, &draft.alert_cpu_threshold, |v, this, cx| {
                                this.draft.alert_cpu_threshold = v;
                                cx.notify();
                            }, cx))
                            .child(Self::render_option_pill("alert_cpu", "90% (推荐)", 90.0f32, &draft.alert_cpu_threshold, |v, this, cx| {
                                this.draft.alert_cpu_threshold = v;
                                cx.notify();
                            }, cx))
                            .child(Self::render_option_pill("alert_cpu", "95% (严苛)", 95.0f32, &draft.alert_cpu_threshold, |v, this, cx| {
                                this.draft.alert_cpu_threshold = v;
                                cx.notify();
                            }, cx)),
                    ),
            )
            // 2. 内存阈值
            .child(
                div()
                    .p_3p5()
                    .rounded_lg()
                    .bg(DarkTechTheme::bg_panel())
                    .border_1()
                    .border_color(DarkTechTheme::border_default())
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(DarkTechTheme::text_primary())
                                    .child(crate::t!("settings.mem_threshold")),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .font_family("Menlo")
                                    .text_color(DarkTechTheme::text_accent())
                                    .child(format!("{:.0}%", draft.alert_mem_threshold)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(Self::render_option_pill("alert_mem", "85%", 85.0f32, &draft.alert_mem_threshold, |v, this, cx| {
                                this.draft.alert_mem_threshold = v;
                                cx.notify();
                            }, cx))
                            .child(Self::render_option_pill("alert_mem", "90%", 90.0f32, &draft.alert_mem_threshold, |v, this, cx| {
                                this.draft.alert_mem_threshold = v;
                                cx.notify();
                            }, cx))
                            .child(Self::render_option_pill("alert_mem", "95% (推荐)", 95.0f32, &draft.alert_mem_threshold, |v, this, cx| {
                                this.draft.alert_mem_threshold = v;
                                cx.notify();
                            }, cx))
                            .child(Self::render_option_pill("alert_mem", "98% (极高)", 98.0f32, &draft.alert_mem_threshold, |v, this, cx| {
                                this.draft.alert_mem_threshold = v;
                                cx.notify();
                            }, cx)),
                    ),
            )
            // 3. 磁盘阈值
            .child(
                div()
                    .p_3p5()
                    .rounded_lg()
                    .bg(DarkTechTheme::bg_panel())
                    .border_1()
                    .border_color(DarkTechTheme::border_default())
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(DarkTechTheme::text_primary())
                                    .child(crate::t!("settings.disk_threshold")),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .font_family("Menlo")
                                    .text_color(DarkTechTheme::text_accent())
                                    .child(format!("{:.0}%", draft.alert_disk_threshold)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(Self::render_option_pill("alert_disk", "80%", 80.0f32, &draft.alert_disk_threshold, |v, this, cx| {
                                this.draft.alert_disk_threshold = v;
                                cx.notify();
                            }, cx))
                            .child(Self::render_option_pill("alert_disk", "85%", 85.0f32, &draft.alert_disk_threshold, |v, this, cx| {
                                this.draft.alert_disk_threshold = v;
                                cx.notify();
                            }, cx))
                            .child(Self::render_option_pill("alert_disk", "90% (推荐)", 90.0f32, &draft.alert_disk_threshold, |v, this, cx| {
                                this.draft.alert_disk_threshold = v;
                                cx.notify();
                            }, cx))
                            .child(Self::render_option_pill("alert_disk", "95% (紧迫)", 95.0f32, &draft.alert_disk_threshold, |v, this, cx| {
                                this.draft.alert_disk_threshold = v;
                                cx.notify();
                            }, cx)),
                    ),
            )
            // 4. 离线告警开关
            .child(Self::render_toggle(
                draft.alert_notify_offline,
                crate::t!("settings.notify_offline"),
                "当纳管服务器出现网络不可达、SSH 认证失败或连续探针超时无响应时立即发出告警预警。",
                |v, this, cx| {
                    this.draft.alert_notify_offline = v;
                    cx.notify();
                },
                cx,
            ))
            // 5. macOS 原生通知开关
            .child(Self::render_toggle(
                draft.alert_macos_notification,
                crate::t!("settings.notify_macos"),
                "发生过载或故障时，调用 macOS 系统通知中心弹出横幅提醒与提示音。",
                |v, this, cx| {
                    this.draft.alert_macos_notification = v;
                    cx.notify();
                },
                cx,
            ))
            // 6. Webhook 与飞书机器人配置卡片
            .child(
                div()
                    .p_3p5()
                    .rounded_lg()
                    .bg(DarkTechTheme::bg_panel())
                    .border_1()
                    .border_color(DarkTechTheme::border_default())
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(DarkTechTheme::text_primary())
                                    .child(crate::t!("settings.notify_webhook")),
                            )
                            .child(
                                if let Some(ref url) = draft.alert_webhook_url {
                                    if AlertDispatcher::is_feishu_webhook(url) {
                                        div()
                                            .px_2()
                                            .py_0p5()
                                            .rounded_md()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_1()
                                            .border_color(DarkTechTheme::border_active())
                                            .text_size(px(10.5))
                                            .text_color(DarkTechTheme::text_accent())
                                            .child("🤖 飞书群自定义机器人 (已适配交互卡片)")
                                    } else {
                                        div()
                                            .px_2()
                                            .py_0p5()
                                            .rounded_md()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_1()
                                            .border_color(DarkTechTheme::border_muted())
                                            .text_size(px(10.5))
                                            .text_color(DarkTechTheme::text_secondary())
                                            .child("🌐 通用 Webhook (JSON)")
                                    }
                                } else {
                                    div()
                                        .px_2()
                                        .py_0p5()
                                        .rounded_md()
                                        .bg(DarkTechTheme::bg_input())
                                        .text_size(px(10.5))
                                        .text_color(DarkTechTheme::text_muted())
                                        .child("未启用")
                                },
                            ),
                    )
                    // URL 显示与状态
                    .child(
                        div()
                            .p_2p5()
                            .rounded_md()
                            .bg(DarkTechTheme::bg_input())
                            .border_1()
                            .border_color(if draft.alert_webhook_url.is_some() {
                                DarkTechTheme::border_active()
                            } else {
                                DarkTechTheme::border_default()
                            })
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .font_family("Menlo")
                                    .text_color(if draft.alert_webhook_url.is_some() {
                                        DarkTechTheme::text_primary()
                                    } else {
                                        DarkTechTheme::text_muted()
                                    })
                                    .child(if let Some(ref url) = draft.alert_webhook_url {
                                        url.clone()
                                    } else {
                                        "https://open.feishu.cn/open-apis/bot/v2/hook/... (支持直接从剪贴板粘贴)".to_string()
                                    }),
                            ),
                    )
                    // 操作按钮栏
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap_2()
                            // 从剪贴板粘贴
                            .child(
                                div()
                                    .id("btn_paste_webhook")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_input())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_default())
                                    .text_color(DarkTechTheme::text_primary())
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .hover(|s| s.border_color(DarkTechTheme::border_active()).text_color(DarkTechTheme::text_accent()))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(clip) = cx.read_from_clipboard()
                                            && let Some(text) = clip.text()
                                        {
                                            let trimmed = text.trim();
                                            if !trimmed.is_empty() {
                                                this.draft.alert_webhook_url = Some(trimmed.to_string());
                                                this.status_message = Some(("已从剪贴板粘贴 Webhook 地址".to_string(), true));
                                                cx.notify();
                                                return;
                                            }
                                        }
                                        this.status_message = Some(("剪贴板中未发现有效文本内容".to_string(), false));
                                        cx.notify();
                                    }))
                                    .child("📋 从剪贴板粘贴 Webhook"),
                            )
                            // 填入飞书机器人示例
                            .child(
                                div()
                                    .id("btn_preset_webhook_feishu")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_input())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_default())
                                    .text_color(DarkTechTheme::text_secondary())
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .hover(|s| s.border_color(DarkTechTheme::border_active()).text_color(DarkTechTheme::text_accent()))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.draft.alert_webhook_url = Some("https://open.feishu.cn/open-apis/bot/v2/hook/demo-test".to_string());
                                        this.status_message = Some(("已填入飞书自定义机器人 Webhook 示例".to_string(), true));
                                        cx.notify();
                                    }))
                                    .child("🤖 填入飞书示例"),
                            )
                            // 清空 Webhook
                            .child(
                                div()
                                    .id("btn_clear_webhook")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_input())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_default())
                                    .text_color(DarkTechTheme::text_muted())
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .hover(|s| s.text_color(DarkTechTheme::status_crit()))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.draft.alert_webhook_url = None;
                                        this.status_message = Some(("已清除 Webhook 地址".to_string(), true));
                                        cx.notify();
                                    }))
                                    .child("清空 Webhook"),
                            )
                            // 测试飞书/Webhook 消息发送
                            .child(
                                div()
                                    .id("btn_test_webhook")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .bg(DarkTechTheme::border_active())
                                    .text_color(DarkTechTheme::bg_root())
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(DarkTechTheme::accent_cyan()))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let Some(ref url) = this.draft.alert_webhook_url else {
                                            this.status_message = Some(("请先配置或粘贴 Webhook 地址再进行测试".to_string(), false));
                                            cx.notify();
                                            return;
                                        };
                                        let url = url.clone();
                                        let is_feishu = AlertDispatcher::is_feishu_webhook(&url);
                                        let event = AlertEvent {
                                            host_id: "test-node-01".to_string(),
                                            host_name: "ReDash-Prod-Server".to_string(),
                                            alert_type: "cpu".to_string(),
                                            message: "这是一条来自 ReDash 桌面运维工作台的告警测试通知，指标监控与机器人通道运转正常。".to_string(),
                                            timestamp: std::time::SystemTime::now()
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .unwrap_or_default()
                                                .as_secs(),
                                        };
                                        cx.spawn(async move |this, cx| {
                                            let res = AlertDispatcher::send_webhook(&url, &event).await;
                                            let _ = this.update(cx, |this, cx| {
                                                match res {
                                                    Ok(()) => {
                                                        let target = if is_feishu { "飞书群机器人" } else { "Webhook" };
                                                        this.status_message = Some((format!("{} 测试消息推送成功！", target), true));
                                                    }
                                                    Err(err) => {
                                                        this.status_message = Some((format!("Webhook 推送失败: {:#}", err), false));
                                                    }
                                                }
                                                cx.notify();
                                            });
                                        })
                                        .detach();
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1p5()
                                            .child(Icon::play().with_size(px(11.0)))
                                            .child("测试 Webhook/飞书推送"),
                                    ),
                            )
                            // 测试原生系统桌面通知
                            .child(
                                div()
                                    .id("btn_test_notification")
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_input())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_default())
                                    .text_color(DarkTechTheme::text_secondary())
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .hover(|s| s.border_color(DarkTechTheme::border_muted()).text_color(DarkTechTheme::text_primary()))
                                    .on_click(cx.listener(|_this, _, _, cx| {
                                        cx.spawn(async move |this, cx| {
                                            let res = AlertDispatcher::send_macos_notification(
                                                "ReDash 告警测试",
                                                "这是一个自动化告警测试通知，指标监控系统运转正常。",
                                            )
                                            .await;
                                            let _ = this.update(cx, |this, cx| {
                                                if res.is_ok() {
                                                    this.status_message = Some(("已发送系统桌面测试通知".to_string(), true));
                                                } else {
                                                    this.status_message = Some(("发送系统桌面测试通知失败".to_string(), false));
                                                }
                                                cx.notify();
                                            });
                                        })
                                        .detach();
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1p5()
                                            .child(Icon::bell().with_size(px(11.0)))
                                            .child(crate::t!("settings.btn_test_alert")),
                                    ),
                            ),
                    ),
            )
    }

    fn render_storage_settings(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let hosts_path = HostStore::default_path();
        let settings_path = AppSettings::default_path();

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(DarkTechTheme::text_primary())
                            .child(crate::t!("settings.backup_title")),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(DarkTechTheme::text_muted())
                            .child("查看本地配置文件路径、凭据安全隔离机制与数据备份恢复。"),
                    ),
            )
            // 1. 本地存储路径卡片
            .child(
                div()
                    .p_3p5()
                    .rounded_lg()
                    .bg(DarkTechTheme::bg_panel())
                    .border_1()
                    .border_color(DarkTechTheme::border_default())
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(DarkTechTheme::text_muted())
                                    .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .child(Icon::folder().with_size(px(11.0)).with_color(DarkTechTheme::text_muted()))
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(DarkTechTheme::text_muted())
                                            .child("服务器主机配置文件 (hosts.json)"),
                                    ),
                            ),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_input())
                                    .font_family("Menlo")
                                    .text_size(px(11.0))
                                    .text_color(DarkTechTheme::text_secondary())
                                    .child(hosts_path.to_string_lossy().to_string()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(DarkTechTheme::text_muted())
                                    .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .child(Icon::settings().with_size(px(11.0)).with_color(DarkTechTheme::text_muted()))
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(DarkTechTheme::text_muted())
                                            .child("系统首选项配置文件 (settings.json)"),
                                    ),
                            ),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_input())
                                    .font_family("Menlo")
                                    .text_size(px(11.0))
                                    .text_color(DarkTechTheme::text_secondary())
                                    .child(settings_path.to_string_lossy().to_string()),
                            ),
                    ),
            )
            // 2. 凭据安全机制
            .child(
                div()
                    .p_3p5()
                    .rounded_lg()
                    .bg(DarkTechTheme::bg_input())
                    .border_1()
                    .border_color(DarkTechTheme::border_muted())
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1p5()
                            .child(
                                Icon::shield().with_size(px(14.0)).with_color(DarkTechTheme::status_online()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(DarkTechTheme::status_online())
                                    .child("操作系统级密钥安全保管库 (Credential Vault)"),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(DarkTechTheme::text_secondary())
                            .child("服务器 SSH 密码及私钥口令默认托管于系统底层安全钥匙串（macOS Keychain / Linux Secret Service / Windows Credential Manager），杜绝在磁盘明文存储任何核心鉴权口令。"),
                    ),
            )
            // 3. 运维操作按钮群
            .child(
                div()
                    .p_3p5()
                    .rounded_lg()
                    .bg(DarkTechTheme::bg_panel())
                    .border_1()
                    .border_color(DarkTechTheme::border_default())
                    .flex()
                    .flex_col()
                    .gap_2p5()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(DarkTechTheme::text_primary())
                            .child("配置备份与数据维护 (Backup & Maintenance)"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(
                                div()
                                    .id("btn_export_hosts_json")
                                    .h(px(30.0))
                                    .px_3()
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_input())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_default())
                                    .text_color(DarkTechTheme::text_primary())
                                    .text_size(px(11.5))
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_center()
                                    .gap_1p5()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()).border_color(DarkTechTheme::border_active()))
                                    .on_click(cx.listener(|this, _e: &ClickEvent, window, cx| {
                                        if let Some(cb) = &this.on_action {
                                            cb(SettingsAction::ExportHosts, window, cx);
                                        }
                                    }))
                                    .child(Icon::download().with_size(px(12.0)))
                                    .child(crate::t!("settings.btn_export")),
                            )
                            .child(
                                div()
                                    .id("btn_import_hosts_json")
                                    .h(px(30.0))
                                    .px_3()
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_input())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_default())
                                    .text_color(DarkTechTheme::text_primary())
                                    .text_size(px(11.5))
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_center()
                                    .gap_1p5()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()).border_color(DarkTechTheme::border_active()))
                                    .on_click(cx.listener(|this, _e: &ClickEvent, window, cx| {
                                        if let Some(cb) = &this.on_action {
                                            cb(SettingsAction::ImportHosts, window, cx);
                                        }
                                    }))
                                    .child(Icon::upload().with_size(px(12.0)))
                                    .child(crate::t!("settings.btn_import")),
                            )
                            .child(
                                div()
                                    .id("btn_reset_demo_hosts_cfg")
                                    .h(px(30.0))
                                    .px_3()
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_input())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_default())
                                    .text_color(DarkTechTheme::accent_indigo())
                                    .text_size(px(11.5))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()).border_color(DarkTechTheme::accent_indigo()))
                                    .on_click(cx.listener(|this, _e: &ClickEvent, window, cx| {
                                        if let Some(cb) = &this.on_action {
                                            cb(SettingsAction::ResetDemoHosts, window, cx);
                                        }
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                Icon::refresh()
                                                    .with_size(px(11.0))
                                                    .with_color(DarkTechTheme::text_primary()),
                                            )
                                            .child(crate::t!("settings.btn_reset_hosts")),
                                    ),
                            ),
                    ),
            )
    }

    fn render_about_settings(&self) -> impl IntoElement {
        let arch = std::env::consts::ARCH;
        let os = std::env::consts::OS;

        div()
            .flex()
            .flex_col()
            .gap_4()
            // 1. Logo & Title Header
            .child(
                div()
                    .p_5()
                    .rounded_xl()
                    .bg(DarkTechTheme::bg_panel())
                    .border_1()
                    .border_color(DarkTechTheme::border_active())
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .size(px(54.0))
                            .rounded_xl()
                            .bg(DarkTechTheme::border_active())
                            .text_color(DarkTechTheme::bg_root())
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(22.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child("SB"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(18.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(DarkTechTheme::text_primary())
                                    .child(crate::t!("settings.about_title")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .px_2()
                                            .py_0p5()
                                            .rounded_full()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_1()
                                            .border_color(DarkTechTheme::border_muted())
                                            .text_size(px(10.5))
                                            .font_family("Menlo")
                                            .text_color(DarkTechTheme::text_accent())
                                            .child("v0.1.0-beta"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(DarkTechTheme::text_muted())
                                            .child("Minimalist Dark Tech Edition"),
                                    ),
                            ),
                    ),
            )
            // 2. 核心架构技术栈卡片
            .child(
                div()
                    .p_3p5()
                    .rounded_lg()
                    .bg(DarkTechTheme::bg_panel())
                    .border_1()
                    .border_color(DarkTechTheme::border_default())
                    .flex()
                    .flex_col()
                    .gap_2p5()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(DarkTechTheme::text_primary())
                            .child("底层架构与驱动引擎 (Architecture & Tech Stack)"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                div()
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_input())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_muted())
                                    .text_size(px(11.0))
                                    .text_color(DarkTechTheme::accent_cyan())
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                Icon::activity()
                                                    .with_size(px(11.0))
                                                    .with_color(DarkTechTheme::accent_cyan()),
                                            )
                                            .child("Rust 2021 Edition"),
                                    ),
                            )
                            .child(
                                div()
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_input())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_muted())
                                    .text_size(px(11.0))
                                    .text_color(DarkTechTheme::accent_indigo())
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                Icon::server()
                                                    .with_size(px(11.0))
                                                    .with_color(DarkTechTheme::accent_indigo()),
                                            )
                                            .child("GPUI (GPU 加速纯原生 UI)"),
                                    ),
                            )
                            .child(
                                div()
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_input())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_muted())
                                    .text_size(px(11.0))
                                    .text_color(DarkTechTheme::status_online())
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                Icon::zap()
                                                    .with_size(px(11.0))
                                                    .with_color(DarkTechTheme::status_online()),
                                            )
                                            .child("russh + russh-sftp (纯异步 SSH)"),
                                    ),
                            )
                            .child(
                                div()
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_input())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_muted())
                                    .text_size(px(11.0))
                                    .text_color(DarkTechTheme::text_secondary())
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                Icon::refresh()
                                                    .with_size(px(11.0))
                                                    .with_color(DarkTechTheme::text_secondary()),
                                            )
                                            .child("Tokio Multi-Thread Reactor"),
                                    ),
                            ),
                    ),
            )
            // 3. 运行环境诊断
            .child(
                div()
                    .p_3p5()
                    .rounded_lg()
                    .bg(DarkTechTheme::bg_panel())
                    .border_1()
                    .border_color(DarkTechTheme::border_default())
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(DarkTechTheme::text_primary())
                            .child("本地主机运行环境 (Host Environment)"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .justify_between()
                                    .text_size(px(11.0))
                                    .child(
                                        div()
                                            .text_color(DarkTechTheme::text_muted())
                                            .child("操作系统 / 平台:"),
                                    )
                                    .child(
                                        div()
                                            .font_family("Menlo")
                                            .text_color(DarkTechTheme::text_primary())
                                            .child(os.to_string()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .justify_between()
                                    .text_size(px(11.0))
                                    .child(
                                        div()
                                            .text_color(DarkTechTheme::text_muted())
                                            .child("CPU 架构 / 目标指令集:"),
                                    )
                                    .child(
                                        div()
                                            .font_family("Menlo")
                                            .text_color(DarkTechTheme::text_primary())
                                            .child(arch.to_string()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .justify_between()
                                    .text_size(px(11.0))
                                    .child(
                                        div()
                                            .text_color(DarkTechTheme::text_muted())
                                            .child("协议支持:"),
                                    )
                                    .child(
                                        div()
                                            .font_family("Menlo")
                                            .text_color(DarkTechTheme::text_primary())
                                            .child("SSH-2.0, SFTP v3, PTY (xterm-256color)"),
                                    ),
                            ),
                    ),
            )
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_dirty = self.is_dirty();
        let status_message = self.status_message.clone();

        div()
            .size_full()
            .bg(DarkTechTheme::bg_root())
            .flex()
            .flex_col()
            .overflow_hidden()
            // 1. Top Header Bar
            .child(
                div()
                    .h(px(48.0))
                    .flex_shrink_0()
                    .w_full()
                    .px_4()
                    .bg(DarkTechTheme::bg_panel())
                    .border_b_1()
                    .border_color(DarkTechTheme::border_default())
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2p5()
                            .child(
                                Icon::settings()
                                    .with_size(px(16.0))
                                    .with_color(DarkTechTheme::accent_cyan()),
                            )
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(DarkTechTheme::text_primary())
                                    .child(crate::t!("settings.title")),
                            )
                            .when(is_dirty, |d| {
                                d.child(
                                    div()
                                        .px_2()
                                        .py_0p5()
                                        .rounded_full()
                                        .bg(DarkTechTheme::bg_input())
                                        .border_1()
                                        .border_color(DarkTechTheme::status_warn())
                                        .text_size(px(10.0))
                                        .text_color(DarkTechTheme::status_warn())
                                        .child(crate::t!("settings.pending_changes")),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            // Status feedback message
                            .when_some(status_message, |d, (msg, is_ok)| {
                                d.child(
                                    div()
                                        .px_2p5()
                                        .py_1()
                                        .rounded_md()
                                        .bg(DarkTechTheme::bg_input())
                                        .border_1()
                                        .border_color(if is_ok {
                                            DarkTechTheme::status_online()
                                        } else {
                                            DarkTechTheme::status_crit()
                                        })
                                        .text_size(px(11.0))
                                        .text_color(if is_ok {
                                            DarkTechTheme::status_online()
                                        } else {
                                            DarkTechTheme::status_crit()
                                        })
                                        .child(msg),
                                )
                            })
                            // Revert changes button (visible when dirty)
                            .when(is_dirty, |d| {
                                d.child(
                                    div()
                                        .id("btn_settings_revert_changes")
                                        .h(px(28.0))
                                        .px_3()
                                        .rounded_md()
                                        .bg(DarkTechTheme::bg_input())
                                        .border_1()
                                        .border_color(DarkTechTheme::border_muted())
                                        .text_color(DarkTechTheme::text_secondary())
                                        .text_size(px(11.5))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()).text_color(DarkTechTheme::status_crit()))
                                        .on_click(cx.listener(|this, _e: &ClickEvent, _window, cx| {
                                            this.revert_changes(cx);
                                        }))
                                        .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(Icon::undo().with_size(px(11.0)).with_color(DarkTechTheme::text_secondary()))
                                            .child(crate::t!("common.revert")),
                                    ),
                                )
                            })
                            // Reset defaults button
                            .child(
                                div()
                                    .id("btn_settings_reset_defaults")
                                    .h(px(28.0))
                                    .px_3()
                                    .rounded_md()
                                    .bg(DarkTechTheme::bg_input())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_default())
                                    .text_color(DarkTechTheme::text_muted())
                                    .text_size(px(11.5))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()).text_color(DarkTechTheme::text_primary()))
                                    .on_click(cx.listener(|this, _e: &ClickEvent, window, cx| {
                                        this.draft = AppSettings::default();
                                        this.saved = AppSettings::default();
                                        DarkTechTheme::set_active_theme(&this.draft.theme_name);
                                        I18n::set_locale_by_code(&this.draft.language);
                                        this.status_message = Some((crate::t!("settings.reset_msg").to_string(), true));
                                        if let Some(cb) = &this.on_action {
                                            cb(SettingsAction::ResetDefaults, window, cx);
                                        }
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(Icon::refresh().with_size(px(11.0)).with_color(DarkTechTheme::text_muted()))
                                            .child(crate::t!("common.reset_defaults")),
                                    ),
                            )
                            // Save button
                            .child(
                                div()
                                    .id("btn_settings_save_changes")
                                    .h(px(28.0))
                                    .px_3p5()
                                    .rounded_md()
                                    .bg(if is_dirty {
                                        DarkTechTheme::border_active()
                                    } else {
                                        DarkTechTheme::bg_panel_hover()
                                    })
                                    .border_1()
                                    .border_color(if is_dirty {
                                        DarkTechTheme::border_active()
                                    } else {
                                        DarkTechTheme::border_default()
                                    })
                                    .text_color(if is_dirty {
                                        DarkTechTheme::bg_root()
                                    } else {
                                        DarkTechTheme::text_muted()
                                    })
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(11.5))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(|s| {
                                        if is_dirty {
                                            s.opacity(0.9)
                                        } else {
                                            s
                                        }
                                    })
                                    .on_click(cx.listener(|this, _e: &ClickEvent, window, cx| {
                                        this.saved = this.draft.clone();
                                        DarkTechTheme::set_active_theme(&this.saved.theme_name);
                                        this.status_message = Some((crate::t!("settings.saved_msg").to_string(), true));
                                        if let Some(cb) = &this.on_action {
                                            cb(SettingsAction::Save(this.saved.clone()), window, cx);
                                        }
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                Icon::save()
                                                    .with_size(px(11.0))
                                                    .with_color(if is_dirty { DarkTechTheme::bg_root() } else { DarkTechTheme::text_muted() }),
                                            )
                                            .child(crate::t!("common.save")),
                                    ),
                            ),
                    ),
            )
            // 2. Main Body: Split Sidebar & Content
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .flex()
                    .flex_row()
                    // Left Category Navigation Bar (200px)
                    .child(
                        div()
                            .id("settings_sidebar_scroll")
                            .w(px(200.0))
                            .flex_shrink_0()
                            .h_full()
                            .overflow_y_scroll()
                            .bg(DarkTechTheme::bg_panel())
                            .border_r_1()
                            .border_color(DarkTechTheme::border_default())
                            .p_2p5()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(self.render_sidebar_item(
                                SettingsCategory::Probe,
                                crate::t!("settings.cat_probe"),
                                Icon::activity(),
                                cx,
                            ))
                            .child(self.render_sidebar_item(
                                SettingsCategory::Terminal,
                                crate::t!("settings.cat_terminal"),
                                Icon::terminal(),
                                cx,
                            ))
                            .child(self.render_sidebar_item(
                                SettingsCategory::Appearance,
                                crate::t!("settings.cat_appearance"),
                                Icon::palette(),
                                cx,
                            ))
                            .child(self.render_sidebar_item(
                                SettingsCategory::Alerts,
                                crate::t!("settings.cat_alerts"),
                                Icon::bell(),
                                cx,
                            ))
                            .child(self.render_sidebar_item(
                                SettingsCategory::Storage,
                                crate::t!("settings.cat_storage"),
                                Icon::shield(),
                                cx,
                            ))
                            .child(self.render_sidebar_item(
                                SettingsCategory::About,
                                crate::t!("settings.cat_about"),
                                Icon::info(),
                                cx,
                            )),
                    )
                    // Right Content Area (Scrollable)
                    .child(
                        div()
                            .id("settings_content_scroll")
                            .track_scroll(&self.scroll_handle)
                            .flex_1()
                            .h_full()
                            .min_h(px(0.0))
                            .overflow_y_scroll()
                            .p_6()
                            .child(
                                div()
                                    .max_w(px(720.0))
                                    .flex()
                                    .flex_col()
                                    .gap_4()
                                    .child(match self.category {
                                        SettingsCategory::Probe => self.render_probe_settings(cx).into_any_element(),
                                        SettingsCategory::Terminal => self.render_terminal_settings(cx).into_any_element(),
                                        SettingsCategory::Appearance => self.render_appearance_settings(cx).into_any_element(),
                                        SettingsCategory::Alerts => self.render_alerts_settings(cx).into_any_element(),
                                        SettingsCategory::Storage => self.render_storage_settings(cx).into_any_element(),
                                        SettingsCategory::About => self.render_about_settings().into_any_element(),
                                    }),
                            ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn test_settings_view_dirty_detection() {
        let initial = AppSettings::default();
        let mut view = SettingsView {
            category: SettingsCategory::Probe,
            draft: initial.clone(),
            saved: initial,
            status_message: None,
            on_action: None,
            on_theme_preview: None,
            on_locale_preview: None,
            scroll_handle: ScrollHandle::new(),
        };

        assert!(!view.is_dirty());
        view.draft.probe_interval_secs = 10;
        assert!(view.is_dirty());
        view.saved.probe_interval_secs = 10;
        assert!(!view.is_dirty());
    }

    #[test]
    fn test_settings_view_theme_preview_and_revert() {
        let initial = AppSettings::default();
        let mut view = SettingsView {
            category: SettingsCategory::Appearance,
            draft: initial.clone(),
            saved: initial,
            status_message: None,
            on_action: None,
            on_theme_preview: None,
            on_locale_preview: None,
            scroll_handle: ScrollHandle::new(),
        };

        assert_eq!(view.draft.theme_name, "Minimalist Dark Tech");
        assert!(!view.is_dirty());

        // Simulate previewing a theme
        view.draft.theme_name = "Cyberpunk Neon".to_string();
        DarkTechTheme::set_active_theme("Cyberpunk Neon");
        assert!(view.is_dirty());
        assert_eq!(DarkTechTheme::active_theme_name(), "Cyberpunk Neon");

        // Revert changes
        let saved_theme = view.saved.theme_name.clone();
        view.draft = view.saved.clone();
        DarkTechTheme::set_active_theme(&saved_theme);
        assert!(!view.is_dirty());
        assert_eq!(DarkTechTheme::active_theme_name(), "Minimalist Dark Tech");
    }

    #[test]
    fn test_settings_view_language_preview_and_revert() {
        let _guard = redash_core::i18n::TEST_LOCALE_MUTEX.lock().unwrap();
        let initial = AppSettings::default();
        let mut view = SettingsView {
            category: SettingsCategory::Appearance,
            draft: initial.clone(),
            saved: initial,
            status_message: None,
            on_action: None,
            on_theme_preview: None,
            on_locale_preview: None,
            scroll_handle: ScrollHandle::new(),
        };

        assert_eq!(view.draft.language, "zh-CN");
        assert!(!view.is_dirty());

        // Simulate previewing a language
        view.draft.language = "en-US".to_string();
        I18n::set_locale_by_code("en-US");
        assert!(view.is_dirty());
        assert_eq!(I18n::locale(), Locale::EnUs);

        // Revert changes
        let saved_lang = view.saved.language.clone();
        view.draft = view.saved.clone();
        I18n::set_locale_by_code(&saved_lang);
        assert!(!view.is_dirty());
        assert_eq!(I18n::locale(), Locale::ZhCn);
    }
}
