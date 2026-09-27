use gpui::prelude::FluentBuilder;
use gpui::*;
use std::sync::Arc;
use std::time::Duration;

use crate::components::icon::Icon;
use crate::components::theme::DarkTechTheme;
use redash_core::config::HostConfig;
use redash_core::config::snippet::{CommandSnippet, SnippetLibrary};
use redash_core::session::SessionManager;

pub type SnippetInjectCallback =
    Box<dyn Fn(String, &mut Window, &mut Context<SnippetsPanel>) + 'static>;

pub struct SnippetsPanel {
    pub snippets: SnippetLibrary,
    pub selected_category: String,
    pub search_query: String,
    pub execution_output: Option<(String, String)>,
    pub on_inject_terminal: Option<SnippetInjectCallback>,
    pub host: Option<HostConfig>,
    pub session_mgr: Option<Arc<SessionManager>>,
    pub scroll_handle: ScrollHandle,
}

impl Default for SnippetsPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl SnippetsPanel {
    pub fn new() -> Self {
        Self {
            snippets: SnippetLibrary::new(),
            selected_category: "全部".to_string(),
            search_query: String::new(),
            execution_output: None,
            on_inject_terminal: None,
            host: None,
            session_mgr: None,
            scroll_handle: ScrollHandle::new(),
        }
    }

    pub fn with_session(mut self, host: HostConfig, session_mgr: Arc<SessionManager>) -> Self {
        self.host = Some(host);
        self.session_mgr = Some(session_mgr);
        self
    }

    pub fn set_on_inject_terminal(
        &mut self,
        callback: impl Fn(String, &mut Window, &mut Context<Self>) + 'static,
    ) {
        self.on_inject_terminal = Some(Box::new(callback));
    }

    pub fn inject_to_terminal(&self, command: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(cb) = &self.on_inject_terminal {
            cb(command, window, cx);
        }
    }

    pub fn exec_silent(
        &mut self,
        session_mgr: Arc<SessionManager>,
        host: HostConfig,
        title: String,
        command: String,
        cx: &mut Context<Self>,
    ) {
        self.execution_output = Some((title.clone(), "正在执行命令，请稍候...".to_string()));
        cx.notify();

        cx.spawn(async move |this, cx| {
            let timeout = Duration::from_secs(30);
            let res = session_mgr.exec(&host, &command, timeout).await;
            let out_text = match res {
                Ok(r) => {
                    if r.exit_code == 0 {
                        if r.stdout.is_empty() {
                            "（命令执行成功，无标准输出）".to_string()
                        } else {
                            r.stdout
                        }
                    } else {
                        format!(
                            "命令退出码: {}\n\n--- 错误输出 (stderr) ---\n{}\n\n--- 标准输出 (stdout) ---\n{}",
                            r.exit_code,
                            if r.stderr.is_empty() { "（无）" } else { &r.stderr },
                            if r.stdout.is_empty() { "（无）" } else { &r.stdout }
                        )
                    }
                }
                Err(e) => format!("执行失败: {}", e),
            };

            let _ = this.update(cx, |view, cx| {
                view.execution_output = Some((title, out_text));
                cx.notify();
            });
        }).detach();
    }

    pub fn close_output(&mut self, cx: &mut Context<Self>) {
        self.execution_output = None;
        cx.notify();
    }
}

impl Render for SnippetsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let q = self.search_query.to_lowercase().trim().to_string();
        let sel_cat = self.selected_category.clone();

        let filtered_snippets: Vec<CommandSnippet> = self
            .snippets
            .list()
            .iter()
            .filter(|s| {
                let matches_cat = sel_cat == "全部" || s.category.eq_ignore_ascii_case(&sel_cat);
                let matches_q = q.is_empty()
                    || s.name.to_lowercase().contains(&q)
                    || s.command.to_lowercase().contains(&q)
                    || s.description.to_lowercase().contains(&q);
                matches_cat && matches_q
            })
            .cloned()
            .collect();

        let categories = vec!["全部", "System", "Docker", "Network", "Maintenance"];

        let card_elements: Vec<_> = if filtered_snippets.is_empty() {
            vec![
                div()
                    .id("snippet_empty_placeholder")
                    .p_8()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .child(
                        Icon::snippet()
                            .with_size(px(32.0))
                            .with_color(DarkTechTheme::text_muted()),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(DarkTechTheme::text_muted())
                            .child("未找到匹配的运维常用脚本指令"),
                    ),
            ]
        } else {
            filtered_snippets
                .into_iter()
                .enumerate()
                .map(|(idx, s)| {
                    let s_id = s.id.clone();
                    let s_cmd = s.command.clone();
                    let s_cmd_inject = s.command.clone();
                    let s_cmd_silent = s.command.clone();
                    let s_title_silent = s.name.clone();

                    let (cat_bg, cat_border, cat_text) = match s.category.as_str() {
                        "System" => (rgba(0x818cf822).into(), DarkTechTheme::accent_indigo(), DarkTechTheme::accent_indigo()),
                        "Docker" => (rgba(0x38bdf822).into(), DarkTechTheme::accent_cyan(), DarkTechTheme::accent_cyan()),
                        "Network" => (rgba(0x34d39922).into(), DarkTechTheme::accent_emerald(), DarkTechTheme::accent_emerald()),
                        "Maintenance" => (rgba(0xf59e0b22).into(), DarkTechTheme::status_warn(), DarkTechTheme::status_warn()),
                        _ => (DarkTechTheme::bg_input(), DarkTechTheme::border_default(), DarkTechTheme::text_secondary()),
                    };

                    let host_opt = self.host.clone();
                    let session_mgr_opt = self.session_mgr.clone();

                    div()
                        .id(ElementId::NamedInteger("snippet_card".into(), idx as u64))
                        .w_full()
                        .bg(DarkTechTheme::bg_panel())
                        .border_1()
                        .border_color(DarkTechTheme::border_default())
                        .rounded_lg()
                        .p_3p5()
                        .flex()
                        .flex_col()
                        .gap_2()
                        // Card Header: Title, Category Badge, Actions
                        .child(
                            div()
                                .w_full()
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
                                            div()
                                                .font_weight(FontWeight::BOLD)
                                                .text_size(px(13.0))
                                                .text_color(DarkTechTheme::text_primary())
                                                .child(s.name.clone()),
                                        )
                                        .child(
                                            div()
                                                .px_2()
                                                .py_0p5()
                                                .rounded_sm()
                                                .bg(cat_bg)
                                                .border_1()
                                                .border_color(cat_border)
                                                .text_size(px(10.5))
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(cat_text)
                                                .child(s.category.clone()),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_2()
                                        // Inject to terminal button
                                        .child(
                                            div()
                                                .id(ElementId::Name(format!("btn_inject_{}", s_id).into()))
                                                .px_2p5()
                                                .py_1()
                                                .rounded_md()
                                                .bg(DarkTechTheme::bg_input())
                                                .border_1()
                                                .border_color(DarkTechTheme::border_default())
                                                .hover(|h| h.border_color(DarkTechTheme::border_active()))
                                                .text_color(DarkTechTheme::text_accent())
                                                .text_size(px(11.5))
                                                .cursor_pointer()
                                                .on_click(cx.listener(move |this, _, window, cx| {
                                                    this.inject_to_terminal(s_cmd_inject.clone(), window, cx);
                                                }))
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_row()
                                                        .items_center()
                                                        .gap_1()
                                                        .child(Icon::terminal().with_size(px(10.0)).with_color(DarkTechTheme::text_primary()))
                                                        .child(crate::t!("snippets.btn_inject")),
                                                ),
                                        )
                                        // Silent Execute button
                                        .child(
                                            div()
                                                .id(ElementId::Name(format!("btn_exec_silent_{}", s_id).into()))
                                                .px_2p5()
                                                .py_1()
                                                .rounded_md()
                                                .bg(DarkTechTheme::bg_input())
                                                .border_1()
                                                .border_color(DarkTechTheme::accent_emerald())
                                                .text_color(DarkTechTheme::accent_emerald())
                                                .text_size(px(11.5))
                                                .cursor_pointer()
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    if let (Some(mgr), Some(h)) = (session_mgr_opt.clone(), host_opt.clone()) {
                                                        this.exec_silent(mgr, h, s_title_silent.clone(), s_cmd_silent.clone(), cx);
                                                    } else {
                                                        this.execution_output = Some((
                                                            s_title_silent.clone(),
                                                            "未绑定当前主机的 SSH 会话，请先连接主机。".to_string(),
                                                        ));
                                                        cx.notify();
                                                    }
                                                }))
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_row()
                                                        .items_center()
                                                        .gap_1()
                                                        .child(Icon::zap().with_size(px(10.0)).with_color(DarkTechTheme::accent_cyan()))
                                                        .child(crate::t!("snippets.btn_exec")),
                                                ),
                                        ),
                                ),
                        )
                        // Description
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(DarkTechTheme::text_muted())
                                .child(s.description.clone()),
                        )
                        // Command Block
                        .child(
                            div()
                                .w_full()
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .bg(DarkTechTheme::bg_input())
                                .border_1()
                                .border_color(DarkTechTheme::border_muted())
                                .font_family("Menlo")
                                .text_size(px(11.5))
                                .text_color(DarkTechTheme::accent_cyan())
                                .child(s_cmd),
                        )
                })
                .collect()
        };

        div()
            .size_full()
            .overflow_hidden()
            .bg(DarkTechTheme::bg_root())
            .flex()
            .flex_col()
            .p_4()
            .gap_3()
            // Top Toolbar: Category Selector and Search
            .child(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    // Category Tabs
                    .child(div().flex().flex_row().items_center().gap_1p5().children(
                        categories.into_iter().enumerate().map(|(idx, cat)| {
                            let is_selected = self.selected_category == cat;
                            let cat_string = cat.to_string();

                            div()
                                .id(ElementId::NamedInteger("snippet_tab".into(), idx as u64))
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .bg(if is_selected {
                                    DarkTechTheme::bg_panel_hover()
                                } else {
                                    DarkTechTheme::bg_panel()
                                })
                                .border_1()
                                .border_color(if is_selected {
                                    DarkTechTheme::border_active()
                                } else {
                                    DarkTechTheme::border_default()
                                })
                                .text_color(if is_selected {
                                    DarkTechTheme::text_accent()
                                } else {
                                    DarkTechTheme::text_secondary()
                                })
                                .text_size(px(11.5))
                                .font_weight(if is_selected {
                                    FontWeight::BOLD
                                } else {
                                    FontWeight::NORMAL
                                })
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.selected_category = cat_string.clone();
                                    cx.notify();
                                }))
                                .child(cat)
                        }),
                    ))
                    // Search Box
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id("snippet_search_container")
                                    .px_3()
                                    .py_1p5()
                                    .bg(DarkTechTheme::bg_input())
                                    .border_1()
                                    .border_color(DarkTechTheme::border_default())
                                    .rounded_md()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        Icon::search()
                                            .with_size(px(12.0))
                                            .with_color(DarkTechTheme::text_muted()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(if self.search_query.is_empty() {
                                                DarkTechTheme::text_muted()
                                            } else {
                                                DarkTechTheme::text_primary()
                                            })
                                            .child(if self.search_query.is_empty() {
                                                crate::t!("snippets.filter").to_string()
                                            } else {
                                                self.search_query.clone()
                                            }),
                                    ),
                            )
                            .when(!self.search_query.is_empty(), |d| {
                                d.child(
                                    div()
                                        .id("btn_clear_snippet_search")
                                        .px_2()
                                        .py_1()
                                        .rounded_md()
                                        .bg(DarkTechTheme::bg_input())
                                        .border_1()
                                        .border_color(DarkTechTheme::border_default())
                                        .text_color(DarkTechTheme::text_muted())
                                        .text_size(px(11.0))
                                        .cursor_pointer()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_1()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.search_query.clear();
                                            cx.notify();
                                        }))
                                        .child(
                                            Icon::close()
                                                .with_size(px(10.0))
                                                .with_color(DarkTechTheme::text_muted()),
                                        )
                                        .child(crate::t!("common.clear")),
                                )
                            }),
                    ),
            )
            // Snippets Cards Grid / Scroll Container
            .child(
                div()
                    .id("snippets_grid_container")
                    .track_scroll(&self.scroll_handle)
                    .flex_1()
                    .w_full()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_2p5()
                    .children(card_elements),
            )
            // Execution Output Drawer / Sheet Modal
            .when_some(self.execution_output.clone(), |d, (title, output)| {
                let out_content = output;
                let title_str = title;

                d.child(
                    div()
                        .id("modal_exec_output_backdrop")
                        .absolute()
                        .inset_0()
                        .bg(rgba(0x000000aa))
                        .flex()
                        .items_center()
                        .justify_center()
                        .p_6()
                        .child(
                            div()
                                .id("modal_exec_output_box")
                                .w(px(740.0))
                                .h(px(460.0))
                                .bg(DarkTechTheme::bg_panel())
                                .border_1()
                                .border_color(DarkTechTheme::accent_emerald())
                                .rounded_xl()
                                .flex()
                                .flex_col()
                                .overflow_hidden()
                                // Drawer Header
                                .child(
                                    div()
                                        .h(px(44.0))
                                        .flex_shrink_0()
                                        .w_full()
                                        .bg(DarkTechTheme::bg_input())
                                        .border_b_1()
                                        .border_color(DarkTechTheme::border_default())
                                        .px_4()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .justify_between()
                                        .child(
                                            div()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_2()
                                                .child(
                                                    Icon::zap()
                                                        .with_size(px(14.0))
                                                        .with_color(DarkTechTheme::accent_cyan()),
                                                )
                                                .child(
                                                    div()
                                                        .font_weight(FontWeight::BOLD)
                                                        .text_size(px(13.0))
                                                        .text_color(DarkTechTheme::text_primary())
                                                        .child(format!(
                                                            "静默执行结果: {}",
                                                            title_str
                                                        )),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .id("btn_close_snippet_output")
                                                .px_2p5()
                                                .py_1()
                                                .rounded_md()
                                                .bg(DarkTechTheme::bg_panel())
                                                .border_1()
                                                .border_color(DarkTechTheme::border_default())
                                                .text_size(px(12.0))
                                                .text_color(DarkTechTheme::text_secondary())
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.close_output(cx);
                                                }))
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_row()
                                                        .items_center()
                                                        .gap_1()
                                                        .child(
                                                            Icon::close()
                                                                .with_size(px(10.0))
                                                                .with_color(
                                                                    DarkTechTheme::text_secondary(),
                                                                ),
                                                        )
                                                        .child(crate::t!("common.close")),
                                                ),
                                        ),
                                )
                                // Drawer Output Body
                                .child(
                                    div()
                                        .id("snippet_output_body")
                                        .flex_1()
                                        .w_full()
                                        .min_h(px(0.0))
                                        .bg(DarkTechTheme::bg_root())
                                        .p_4()
                                        .overflow_y_scroll()
                                        .font_family("Menlo")
                                        .text_size(px(11.5))
                                        .text_color(DarkTechTheme::text_primary())
                                        .child(out_content),
                                )
                                // Drawer Footer
                                .child(
                                    div()
                                        .h(px(36.0))
                                        .flex_shrink_0()
                                        .w_full()
                                        .bg(DarkTechTheme::bg_input())
                                        .border_t_1()
                                        .border_color(DarkTechTheme::border_default())
                                        .px_4()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .justify_end()
                                        .child(
                                            div()
                                                .id("btn_close_snippet_output_bottom")
                                                .cursor_pointer()
                                                .text_size(px(11.5))
                                                .text_color(DarkTechTheme::text_accent())
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.close_output(cx);
                                                }))
                                                .child("完成"),
                                        ),
                                ),
                        ),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn test_snippets_panel_initial_state() {
        let panel = SnippetsPanel::new();
        assert_eq!(panel.selected_category, "全部");
        assert!(panel.search_query.is_empty());
        assert!(panel.execution_output.is_none());
        assert!(panel.snippets.list().len() >= 10);
    }

    #[test]
    fn test_snippets_category_filtering() {
        let panel = SnippetsPanel::new();
        let docker_snippets = panel.snippets.list_by_category("Docker");
        assert_eq!(docker_snippets.len(), 3);
        assert!(docker_snippets.iter().any(|s| s.name == "查看容器资源占用"));
    }

    #[test]
    fn test_snippets_close_output() {
        let mut panel = SnippetsPanel::new();
        panel.execution_output = Some(("测试".into(), "输出".into()));
        assert!(panel.execution_output.is_some());

        panel.execution_output = None;
        assert!(panel.execution_output.is_none());
    }
}
