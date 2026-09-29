//! Native text controls provide selection, clipboard and IME semantics over the canvas.
use crate::app::{ActiveView, AppState, WorkbenchTab};
use crate::gateway::GatewayClient;
use crate::input::handle_key_down;
use crate::render::{self, LAYOUT};
use crate::theme::ThemeColors;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{
    CompositionEvent, Event, HtmlCanvasElement, HtmlTextAreaElement, InputEvent, KeyboardEvent,
};

#[derive(Clone, Copy, PartialEq)]
enum Target {
    Host(usize),
    Filter,
    Batch,
    Editor,
    Search,
    Terminal,
}
struct Field {
    target: Target,
    element: HtmlTextAreaElement,
}
pub struct TextInputs {
    fields: Vec<Field>,
}
impl TextInputs {
    pub fn new(
        state: Rc<RefCell<AppState>>,
        gateway: Rc<RefCell<GatewayClient>>,
        canvas: HtmlCanvasElement,
    ) -> Result<Self, JsValue> {
        let document = web_sys::window().unwrap().document().unwrap();
        let mut fields = Vec::new();
        for (target, label, id) in [
            (Target::Host(0), "主机名称", "host-name"),
            (Target::Host(1), "主机地址", "host-address"),
            (Target::Host(2), "SSH 端口", "host-port"),
            (Target::Host(3), "SSH 用户名", "host-user"),
            (Target::Filter, "搜索节点名称、IP 或标签", "fleet-search"),
            (Target::Batch, "批量执行命令", "batch-command"),
            (Target::Editor, "文件内容", "file-content"),
            (Target::Search, "搜索终端输出", "terminal-search"),
            (Target::Terminal, "终端输入", "terminal-input"),
        ] {
            let element: HtmlTextAreaElement = document.create_element("textarea")?.dyn_into()?;
            element.set_id(id);
            element.set_attribute("aria-label", label)?;
            element.set_attribute("spellcheck", "false")?;
            element.set_attribute("autocomplete", "off")?;
            element.set_attribute("autocapitalize", "off")?;
            element.set_attribute("rows", "1")?;
            if target == Target::Host(2) {
                element.set_attribute("inputmode", "numeric")?;
            }
            if target == Target::Filter {
                element.set_placeholder(label);
            }
            element.style().set_property("display", "none")?;
            document.body().unwrap().append_child(&element)?;

            let composing = Rc::new(Cell::new(false));
            let input_state = state.clone();
            let input_gateway = gateway.clone();
            let input_el = element.clone();
            let is_composing = composing.clone();
            let on_input = Closure::<dyn FnMut(_)>::new(move |event: InputEvent| {
                if target == Target::Terminal && (event.is_composing() || is_composing.get()) {
                    return;
                }
                commit_value(target, &input_el, &input_state, &input_gateway);
            });
            element.set_oninput(Some(on_input.as_ref().unchecked_ref()));
            on_input.forget();
            let composing_start = composing.clone();
            let start =
                Closure::<dyn FnMut(_)>::new(move |_: CompositionEvent| composing_start.set(true));
            element.add_event_listener_with_callback(
                "compositionstart",
                start.as_ref().unchecked_ref(),
            )?;
            start.forget();
            let end_state = state.clone();
            let end_gateway = gateway.clone();
            let end_el = element.clone();
            let end = Closure::<dyn FnMut(_)>::new(move |_: CompositionEvent| {
                composing.set(false);
                commit_value(target, &end_el, &end_state, &end_gateway);
            });
            element
                .add_event_listener_with_callback("compositionend", end.as_ref().unchecked_ref())?;
            end.forget();
            let focus_state = state.clone();
            let focus = Closure::<dyn FnMut(_)>::new(move |_: Event| {
                let mut sm = focus_state.borrow_mut();
                if let Target::Host(index) = target {
                    sm.modal_field_idx = index;
                }
                sm.is_filter_focused = target == Target::Filter;
            });
            element.set_onfocus(Some(focus.as_ref().unchecked_ref()));
            focus.forget();
            let key_state = state.clone();
            let key_gateway = gateway.clone();
            let focus_canvas = canvas.clone();
            let keydown = Closure::<dyn FnMut(_)>::new(move |event: KeyboardEvent| {
                if event.is_composing() || event.key_code() == 229 {
                    return;
                }
                let key = event.key();
                let ctrl = event.ctrl_key() || event.meta_key();
                // Clipboard shortcuts belong to the native editor. Terminal Ctrl-C
                // remains SIGINT; Command-C and Ctrl/Command-V remain clipboard operations.
                if (ctrl && key.eq_ignore_ascii_case("v"))
                    || (event.meta_key()
                        && (key.eq_ignore_ascii_case("c") || key.eq_ignore_ascii_case("x")))
                {
                    return;
                }
                let handled = match target {
                    Target::Host(_) => matches!(key.as_str(), "Escape" | "Enter"),
                    Target::Filter => matches!(key.as_str(), "Escape" | "Enter"),
                    Target::Editor => key == "Escape" || (ctrl && key.eq_ignore_ascii_case("s")),
                    Target::Batch => ctrl && key == "Enter",
                    Target::Search => matches!(key.as_str(), "Escape" | "Enter"),
                    Target::Terminal => key.chars().count() != 1 || ctrl,
                };
                if !handled {
                    return;
                }
                event.prevent_default();
                let action = handle_key_down(&mut key_state.borrow_mut(), &key, ctrl);
                if let Some(action) = action {
                    crate::execute_ui_action(action, &key_state, &key_gateway);
                }
                if matches!(target, Target::Filter | Target::Search)
                    && matches!(key.as_str(), "Escape" | "Enter")
                {
                    let _ = focus_canvas.focus();
                }
            });
            element.set_onkeydown(Some(keydown.as_ref().unchecked_ref()));
            keydown.forget();
            fields.push(Field { target, element });
        }
        Ok(Self { fields })
    }

    /// No state borrow is held while focusing: a focus event can synchronously
    /// invoke another callback that updates the selected form field.
    pub fn sync(&self, state: &Rc<RefCell<AppState>>, width: f64, height: f64) {
        let sm = state.borrow();
        let theme = ThemeColors::from_palette(sm.current_palette());
        let mut focus = None;
        let document = web_sys::window().unwrap().document().unwrap();
        let active = document
            .active_element()
            .map(|e| e.id())
            .unwrap_or_default();
        for field in &self.fields {
            let t = field.target;
            let overlay = sm.show_add_modal
                || sm.sftp_editor.is_some()
                || sm.docker_log_modal.is_some()
                || sm.batch_selected_log_host.is_some();
            let visible = match t {
                Target::Host(_) => sm.show_add_modal,
                Target::Editor => sm.sftp_editor.is_some(),
                Target::Filter => !overlay && sm.active_view == ActiveView::Fleet,
                Target::Batch => !overlay && sm.active_view == ActiveView::Batch,
                Target::Search => {
                    !overlay && sm.active_view == ActiveView::Terminal && sm.terminal_search_active
                }
                Target::Terminal => {
                    !overlay
                        && sm.active_view == ActiveView::Terminal
                        && sm.active_workbench_tab == WorkbenchTab::Terminal
                        && !sm.terminal_search_active
                }
            };
            if !visible {
                let _ = field.element.style().set_property("display", "none");
                continue;
            }
            let (rect, value, wants_focus) = match t {
                Target::Host(index) => {
                    let value = match index {
                        0 => &sm.modal_name,
                        1 => &sm.modal_hostname,
                        2 => &sm.modal_port,
                        _ => &sm.modal_user,
                    };
                    (
                        (
                            (width - 420.0) / 2.0 + 20.0,
                            (height - 360.0) / 2.0 + 80.0 + index as f64 * 56.0,
                            380.0,
                            28.0,
                        ),
                        value.as_str(),
                        sm.modal_field_idx == index,
                    )
                }
                Target::Filter => (
                    render::get_fleet_search_bar_rect(
                        LAYOUT.sidebar_width,
                        LAYOUT.topbar_height,
                        width - LAYOUT.sidebar_width,
                    ),
                    sm.filter_query.as_str(),
                    sm.is_filter_focused,
                ),
                Target::Batch => (
                    (
                        LAYOUT.sidebar_width + 312.0,
                        LAYOUT.topbar_height + 84.0,
                        (width - LAYOUT.sidebar_width - 348.0).max(20.0),
                        52.0,
                    ),
                    sm.batch_command.as_str(),
                    false,
                ),
                Target::Editor => {
                    let (x, y, w, h) = render::get_sftp_editor_modal_rect(width, height);
                    (
                        (x + 12.0, y + 52.0, w - 24.0, h - 82.0),
                        sm.sftp_editor.as_ref().unwrap().1.as_str(),
                        true,
                    )
                }
                Target::Search => {
                    let (x, y, w, h) = render::get_terminal_search_bar_rect(
                        LAYOUT.sidebar_width,
                        LAYOUT.topbar_height
                            + render::WORKBENCH_TAB_BAR_HEIGHT
                            + if sm.agent.is_some() { 32.0 } else { 0.0 },
                        width - LAYOUT.sidebar_width,
                    );
                    (
                        (x + 3.0, y + 3.0, w - 80.0, h - 6.0),
                        sm.terminal_search_query.as_str(),
                        true,
                    )
                }
                Target::Terminal => (
                    (LAYOUT.sidebar_width + 16.0, height - 24.0, 2.0, 18.0),
                    "",
                    true,
                ),
            };
            if t != Target::Terminal && field.element.value() != value {
                field.element.set_value(value);
            }
            let (x, y, w, h) = rect;
            let opacity = if t == Target::Terminal { "0.01" } else { "1" };
            field.element.style().set_css_text(&format!("position:fixed;display:block;box-sizing:border-box;resize:none;z-index:10;left:{x}px;top:{y}px;width:{w}px;height:{h}px;background:{};color:{};border:1px solid {};outline-color:{};font:13px monospace;padding:3px 6px;opacity:{opacity};",theme.bg_input,theme.text_primary,theme.border_default,theme.accent_cyan));
            if wants_focus && active != field.element.id() {
                focus = Some(field.element.clone());
            }
        }
        drop(sm);
        if let Some(element) = focus {
            let _ = element.focus();
        }
    }
}
fn commit_value(
    target: Target,
    element: &HtmlTextAreaElement,
    state: &Rc<RefCell<AppState>>,
    gateway: &Rc<RefCell<GatewayClient>>,
) {
    let value = element.value();
    if target == Target::Terminal {
        if !value.is_empty() {
            gateway.borrow().send_terminal_input(&value);
            element.set_value("");
        }
        return;
    }
    let mut sm = state.borrow_mut();
    match target {
        Target::Host(index) => {
            sm.modal_test_status = None;
            match index {
                0 => sm.modal_name = value,
                1 => sm.modal_hostname = value,
                2 => sm.modal_port = value,
                _ => sm.modal_user = value,
            }
        }
        Target::Filter => {
            sm.filter_query = value;
            sm.fleet_scroll = 0.0;
        }
        Target::Batch => {
            sm.set_batch_command(value);
        }
        Target::Editor => {
            if let Some((_, content)) = &mut sm.sftp_editor {
                *content = value;
                sm.sftp_editor_modified = true;
            }
        }
        Target::Search => {
            sm.set_terminal_search_query(value);
        }
        Target::Terminal => unreachable!(),
    }
}
