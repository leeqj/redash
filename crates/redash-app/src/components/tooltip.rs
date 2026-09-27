use crate::components::theme::DarkTechTheme;
use gpui::*;

pub struct TooltipView {
    text: SharedString,
}

impl Render for TooltipView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .bg(DarkTechTheme::bg_input())
            .border_1()
            .border_color(DarkTechTheme::border_active())
            .rounded_sm()
            .px_2()
            .py_1()
            .text_size(px(11.0))
            .text_color(DarkTechTheme::text_primary())
            .shadow_md()
            .child(self.text.clone())
    }
}

pub fn tooltip(
    label: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let label: SharedString = label.into();
    move |_window, cx| {
        let label = label.clone();
        cx.new(|_cx| TooltipView { text: label }).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn test_tooltip_view_struct() {
        let text: SharedString = "SSH 终端".into();
        let view = TooltipView { text: text.clone() };
        assert_eq!(view.text, text);
    }
}
