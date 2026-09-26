use gpui::prelude::FluentBuilder;
use gpui::*;
use std::path::PathBuf;

use crate::components::icon::Icon;
use crate::components::theme::DarkTechTheme;
use redash_core::config::{AuthMethod, HostConfig, HostId, TargetOs};

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostModalMode {
    Create,
    Edit(Box<HostConfig>),
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthTypeSelection {
    Password,
    PrivateKey,
    Agent,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostModalField {
    Name,
    Group,
    Hostname,
    Port,
    User,
    Password,
    KeyPath,
    Passphrase,
    Tags,
    BandwidthLimit,
    BandwidthResetDay,
}

#[allow(dead_code, clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum HostModalAction {
    Save {
        host: Box<HostConfig>,
        password: Option<String>,
        passphrase: Option<String>,
    },
    Cancel,
    Delete(HostId),
    Clone(HostConfig),
}

pub type HostModalCallback =
    Box<dyn Fn(HostModalAction, &mut Window, &mut Context<HostModal>) + 'static>;

#[allow(dead_code)]
pub struct HostModal {
    pub mode: HostModalMode,
    pub name: String,
    pub group: String,
    pub hostname: String,
    pub port: String,
    pub user: String,
    pub auth_type: AuthTypeSelection,
    pub password: String,
    pub show_password: bool,
    pub key_path: String,
    pub passphrase: String,
    pub show_passphrase: bool,
    pub target_os: TargetOs,
    pub tags_input: String,
    pub bandwidth_limit_gb: String,
    pub bandwidth_reset_day: String,
    pub active_field: HostModalField,
    pub cursor_pos: usize,
    pub error_msg: Option<String>,
    pub focus_handle: Option<FocusHandle>,
    pub on_action: Option<HostModalCallback>,
}

#[allow(dead_code)]
impl HostModal {
    pub fn new_create() -> Self {
        Self {
            mode: HostModalMode::Create,
            name: String::new(),
            group: "Default".to_string(),
            hostname: String::new(),
            port: "22".to_string(),
            user: "root".to_string(),
            auth_type: AuthTypeSelection::Password,
            password: String::new(),
            show_password: false,
            key_path: "~/.ssh/id_ed25519".to_string(),
            passphrase: String::new(),
            show_passphrase: false,
            target_os: TargetOs::Linux,
            tags_input: String::new(),
            bandwidth_limit_gb: String::new(),
            bandwidth_reset_day: "1".to_string(),
            active_field: HostModalField::Name,
            cursor_pos: 0,
            error_msg: None,
            focus_handle: None,
            on_action: None,
        }
    }

    pub fn new_edit(host: HostConfig) -> Self {
        // Blank secret fields preserve existing references without reading the keychain.
        let (auth_type, password, key_path, passphrase) = match &host.auth {
            AuthMethod::Password { .. } => (
                AuthTypeSelection::Password,
                String::new(),
                "~/.ssh/id_ed25519".to_string(),
                String::new(),
            ),
            AuthMethod::PrivateKey { key_path, .. } => (
                AuthTypeSelection::PrivateKey,
                String::new(),
                key_path.to_string_lossy().to_string(),
                String::new(),
            ),
            AuthMethod::Agent => (
                AuthTypeSelection::Agent,
                String::new(),
                "~/.ssh/id_ed25519".to_string(),
                String::new(),
            ),
        };

        let tags_str = host.tags.join(", ");
        let name_len = host.name.chars().count();
        let bw_limit_str = host
            .bandwidth_limit_gb
            .map(|v| v.to_string())
            .unwrap_or_default();
        let bw_reset_str = host
            .bandwidth_reset_day
            .map(|v| v.to_string())
            .unwrap_or_else(|| "1".to_string());

        Self {
            mode: HostModalMode::Edit(Box::new(host.clone())),
            name: host.name,
            group: host.group,
            hostname: host.hostname,
            port: host.port.to_string(),
            user: host.user,
            auth_type,
            password,
            show_password: false,
            key_path,
            passphrase,
            show_passphrase: false,
            target_os: host.target_os,
            tags_input: tags_str,
            bandwidth_limit_gb: bw_limit_str,
            bandwidth_reset_day: bw_reset_str,
            active_field: HostModalField::Name,
            cursor_pos: name_len,
            error_msg: None,
            focus_handle: None,
            on_action: None,
        }
    }

    pub fn set_on_action<F>(&mut self, callback: F)
    where
        F: Fn(HostModalAction, &mut Window, &mut Context<Self>) + 'static,
    {
        self.on_action = Some(Box::new(callback));
    }

    pub fn with_focus_handle(mut self, fh: FocusHandle) -> Self {
        self.focus_handle = Some(fh);
        self
    }

    pub fn available_fields(&self) -> Vec<HostModalField> {
        let mut fields = vec![
            HostModalField::Name,
            HostModalField::Group,
            HostModalField::Hostname,
            HostModalField::Port,
            HostModalField::User,
        ];
        match self.auth_type {
            AuthTypeSelection::Password => {
                fields.push(HostModalField::Password);
            }
            AuthTypeSelection::PrivateKey => {
                fields.push(HostModalField::KeyPath);
                fields.push(HostModalField::Passphrase);
            }
            AuthTypeSelection::Agent => {}
        }
        fields.push(HostModalField::Tags);
        fields.push(HostModalField::BandwidthLimit);
        fields.push(HostModalField::BandwidthResetDay);
        fields
    }

    pub fn cycle_field(&mut self, forward: bool) {
        let fields = self.available_fields();
        if let Some(pos) = fields.iter().position(|f| *f == self.active_field) {
            let next_pos = if forward {
                (pos + 1) % fields.len()
            } else if pos == 0 {
                fields.len().saturating_sub(1)
            } else {
                pos - 1
            };
            self.set_active_field(fields[next_pos]);
        } else if let Some(&first) = fields.first() {
            self.set_active_field(first);
        }
    }

    pub fn set_active_field(&mut self, field: HostModalField) {
        self.active_field = field;
        self.cursor_pos = self.get_field_text(field).chars().count();
    }

    pub fn get_field_text(&self, field: HostModalField) -> &str {
        match field {
            HostModalField::Name => &self.name,
            HostModalField::Group => &self.group,
            HostModalField::Hostname => &self.hostname,
            HostModalField::Port => &self.port,
            HostModalField::User => &self.user,
            HostModalField::Password => &self.password,
            HostModalField::KeyPath => &self.key_path,
            HostModalField::Passphrase => &self.passphrase,
            HostModalField::Tags => &self.tags_input,
            HostModalField::BandwidthLimit => &self.bandwidth_limit_gb,
            HostModalField::BandwidthResetDay => &self.bandwidth_reset_day,
        }
    }

    pub fn get_field_text_mut(&mut self, field: HostModalField) -> &mut String {
        match field {
            HostModalField::Name => &mut self.name,
            HostModalField::Group => &mut self.group,
            HostModalField::Hostname => &mut self.hostname,
            HostModalField::Port => &mut self.port,
            HostModalField::User => &mut self.user,
            HostModalField::Password => &mut self.password,
            HostModalField::KeyPath => &mut self.key_path,
            HostModalField::Passphrase => &mut self.passphrase,
            HostModalField::Tags => &mut self.tags_input,
            HostModalField::BandwidthLimit => &mut self.bandwidth_limit_gb,
            HostModalField::BandwidthResetDay => &mut self.bandwidth_reset_day,
        }
    }

    fn char_to_byte_index(text: &str, char_idx: usize) -> usize {
        text.char_indices()
            .nth(char_idx)
            .map(|(idx, _)| idx)
            .unwrap_or(text.len())
    }

    pub fn insert_char(&mut self, ch: char) {
        let field = self.active_field;
        let pos = self.cursor_pos;
        let text = self.get_field_text_mut(field);
        let byte_idx = Self::char_to_byte_index(text, pos);
        text.insert(byte_idx, ch);
        self.cursor_pos += 1;
        self.error_msg = None;
    }

    pub fn insert_str(&mut self, s: &str) {
        let field = self.active_field;
        let pos = self.cursor_pos;
        let text = self.get_field_text_mut(field);
        let byte_idx = Self::char_to_byte_index(text, pos);
        text.insert_str(byte_idx, s);
        self.cursor_pos += s.chars().count();
        self.error_msg = None;
    }

    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 {
            let field = self.active_field;
            self.cursor_pos -= 1;
            let pos = self.cursor_pos;
            let text = self.get_field_text_mut(field);
            let start = Self::char_to_byte_index(text, pos);
            let end = Self::char_to_byte_index(text, pos + 1);
            text.replace_range(start..end, "");
            self.error_msg = None;
        }
    }

    pub fn delete(&mut self) {
        let field = self.active_field;
        let pos = self.cursor_pos;
        let text = self.get_field_text_mut(field);
        let total = text.chars().count();
        if pos < total {
            let start = Self::char_to_byte_index(text, pos);
            let end = Self::char_to_byte_index(text, pos + 1);
            text.replace_range(start..end, "");
            self.error_msg = None;
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    pub fn move_right(&mut self) {
        let field = self.active_field;
        let total = self.get_field_text(field).chars().count();
        if self.cursor_pos < total {
            self.cursor_pos += 1;
        }
    }

    pub fn move_home(&mut self) {
        self.cursor_pos = 0;
    }

    pub fn move_end(&mut self) {
        let field = self.active_field;
        self.cursor_pos = self.get_field_text(field).chars().count();
    }

    pub fn validate_and_build(
        &self,
    ) -> Result<(HostConfig, Option<String>, Option<String>), String> {
        let trimmed_name = self.name.trim();
        if trimmed_name.is_empty() {
            return Err(crate::t!("host.val_name_empty").to_string());
        }
        if trimmed_name.chars().count() > 64 {
            return Err("服务器名称长度不能超过 64 个字符".to_string());
        }

        let trimmed_hostname = self.hostname.trim();
        if trimmed_hostname.is_empty() {
            return Err(crate::t!("host.val_addr_empty").to_string());
        }

        let port_num = self
            .port
            .trim()
            .parse::<u16>()
            .map_err(|_| crate::t!("host.val_port_invalid").to_string())?;
        if port_num == 0 {
            return Err("端口不能为 0".to_string());
        }

        let trimmed_user = self.user.trim();
        if trimmed_user.is_empty() {
            return Err(crate::t!("host.val_user_empty").to_string());
        }

        let host_id = match &self.mode {
            HostModalMode::Edit(existing) => existing.id.clone(),
            HostModalMode::Create => HostId::new(),
        };

        let mut out_password = None;
        let mut out_passphrase = None;

        let auth = match self.auth_type {
            AuthTypeSelection::Password => {
                if self.password.is_empty()
                    && !matches!(&self.mode, HostModalMode::Edit(host) if matches!(host.auth, AuthMethod::Password { .. }))
                {
                    return Err(crate::t!("host.val_pwd_empty").to_string());
                }
                let cred_id = match &self.mode {
                    HostModalMode::Edit(existing) => match &existing.auth {
                        AuthMethod::Password { credential_id } => credential_id.clone(),
                        _ => format!("redash_host_{}", host_id.0),
                    },
                    HostModalMode::Create => format!("redash_host_{}", host_id.0),
                };
                if !self.password.is_empty() {
                    out_password = Some(self.password.clone());
                }
                AuthMethod::Password {
                    credential_id: cred_id,
                }
            }
            AuthTypeSelection::PrivateKey => {
                let trimmed_key = self.key_path.trim();
                if trimmed_key.is_empty() {
                    return Err(crate::t!("host.val_key_empty").to_string());
                }
                let passphrase_id = if !self.passphrase.is_empty() {
                    let pid = match &self.mode {
                        HostModalMode::Edit(existing) => match &existing.auth {
                            AuthMethod::PrivateKey {
                                passphrase_id: Some(id),
                                ..
                            } => id.clone(),
                            _ => format!("redash_pass_{}", host_id.0),
                        },
                        HostModalMode::Create => format!("redash_pass_{}", host_id.0),
                    };
                    out_passphrase = Some(self.passphrase.clone());
                    Some(pid)
                } else {
                    match &self.mode {
                        HostModalMode::Edit(host) => match &host.auth {
                            AuthMethod::PrivateKey {
                                key_path,
                                passphrase_id,
                            } if key_path == &PathBuf::from(trimmed_key) => passphrase_id.clone(),
                            _ => None,
                        },
                        _ => None,
                    }
                };
                AuthMethod::PrivateKey {
                    key_path: PathBuf::from(trimmed_key),
                    passphrase_id,
                }
            }
            AuthTypeSelection::Agent => AuthMethod::Agent,
        };

        let tags: Vec<String> = self
            .tags_input
            .split([',', ' ', ';'])
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();

        let group = if self.group.trim().is_empty() {
            "Default".to_string()
        } else {
            self.group.trim().to_string()
        };

        let bandwidth_limit = if self.bandwidth_limit_gb.trim().is_empty() {
            None
        } else {
            let val = self
                .bandwidth_limit_gb
                .trim()
                .parse::<u64>()
                .map_err(|_| crate::t!("host.val_bandwidth_invalid").to_string())?;
            Some(val)
        };

        let bandwidth_reset = if self.bandwidth_reset_day.trim().is_empty() {
            None
        } else {
            let val = self
                .bandwidth_reset_day
                .trim()
                .parse::<u8>()
                .map_err(|_| crate::t!("host.val_reset_day_invalid").to_string())?;
            if !(1..=31).contains(&val) {
                return Err("流量重置日必须在 1~31 之间".to_string());
            }
            Some(val)
        };

        let host = HostConfig {
            id: host_id,
            name: trimmed_name.to_string(),
            hostname: trimmed_hostname.to_string(),
            port: port_num,
            user: trimmed_user.to_string(),
            auth,
            group,
            tags,
            target_os: self.target_os.clone(),
            jump_host: match &self.mode {
                HostModalMode::Edit(existing) => existing.jump_host.clone(),
                _ => None,
            },
            proxy_jump_id: match &self.mode {
                HostModalMode::Edit(existing) => existing.proxy_jump_id.clone(),
                _ => None,
            },
            bandwidth_limit_gb: bandwidth_limit,
            bandwidth_reset_day: bandwidth_reset,
        };

        host.validate().map_err(|e| format!("{}", e))?;

        Ok((host, out_password, out_passphrase))
    }

    pub fn submit(&mut self) -> Option<HostModalAction> {
        match self.validate_and_build() {
            Ok((host, password, passphrase)) => {
                self.error_msg = None;
                Some(HostModalAction::Save {
                    host: Box::new(host),
                    password,
                    passphrase,
                })
            }
            Err(e) => {
                self.error_msg = Some(e);
                None
            }
        }
    }

    pub fn handle_key_down(&mut self, event: &KeyDownEvent, cx: &App) -> Option<HostModalAction> {
        let key = event.keystroke.key.as_str();
        let modifiers = &event.keystroke.modifiers;

        if key == "escape" {
            return Some(HostModalAction::Cancel);
        }

        if key == "tab" {
            let forward = !modifiers.shift;
            self.cycle_field(forward);
            return None;
        }

        if key == "enter" {
            return self.submit();
        }

        if (modifiers.platform || modifiers.control) && (key == "v" || key == "V") {
            if let Some(item) = cx.read_from_clipboard()
                && let Some(text) = item.text()
            {
                let clean = text.replace("\r\n", "").replace(['\n', '\r'], "");
                self.insert_str(&clean);
            }
            return None;
        }

        if key == "backspace" {
            self.backspace();
            return None;
        }

        if key == "delete" {
            self.delete();
            return None;
        }

        if key == "left" {
            self.move_left();
            return None;
        }

        if key == "right" {
            self.move_right();
            return None;
        }

        if key == "home"
            || ((modifiers.control || modifiers.platform) && (key == "a" || key == "A"))
        {
            self.move_home();
            return None;
        }

        if key == "end" || ((modifiers.control || modifiers.platform) && (key == "e" || key == "E"))
        {
            self.move_end();
            return None;
        }

        // Space
        if key == "space" && !modifiers.control && !modifiers.platform {
            self.insert_char(' ');
            return None;
        }

        // Printable characters - prioritize key_char (Shift+key, uppercase, symbols, IME)
        if !modifiers.control && !modifiers.platform {
            if let Some(ref kc) = event.keystroke.key_char {
                if !kc.is_empty() && kc != "\n" && kc != "\r" && kc != "\t" {
                    self.insert_str(kc);
                    return None;
                }
            } else if key.chars().count() == 1
                && let Some(ch) = key.chars().next()
            {
                self.insert_char(ch);
                return None;
            }
        }

        None
    }

    fn render_input_box(
        &self,
        field: HostModalField,
        value: &str,
        is_masked: bool,
        placeholder: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_active = self.active_field == field;
        let pos = self.cursor_pos;

        let display_val = if is_masked {
            "•".repeat(value.chars().count())
        } else {
            value.to_string()
        };

        let chars: Vec<char> = display_val.chars().collect();
        let clamped = pos.min(chars.len());
        let before: String = chars[..clamped].iter().collect();
        let (cursor_char, after): (Option<char>, String) = if clamped < chars.len() {
            (Some(chars[clamped]), chars[clamped + 1..].iter().collect())
        } else {
            (None, String::new())
        };

        div()
            .h(px(32.0))
            .w_full()
            .bg(DarkTechTheme::bg_input())
            .border_1()
            .border_color(if is_active {
                DarkTechTheme::border_active()
            } else {
                DarkTechTheme::border_default()
            })
            .rounded_md()
            .px_2p5()
            .flex()
            .flex_row()
            .items_center()
            .cursor_text()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.set_active_field(field);
                    if let Some(ref fh) = this.focus_handle {
                        window.focus(fh);
                    }
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .child(if display_val.is_empty() && !is_active {
                div()
                    .text_size(px(12.0))
                    .text_color(DarkTechTheme::text_muted())
                    .font_family("Menlo")
                    .child(placeholder.to_string())
            } else if is_active {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .text_size(px(12.0))
                    .text_color(DarkTechTheme::text_primary())
                    .font_family("Menlo")
                    .child(before)
                    .child(
                        div()
                            .bg(DarkTechTheme::border_active())
                            .text_color(DarkTechTheme::bg_root())
                            .rounded_xs()
                            .child(match cursor_char {
                                Some(c) => c.to_string(),
                                None => " ".to_string(),
                            }),
                    )
                    .child(after)
            } else {
                div()
                    .text_size(px(12.0))
                    .text_color(DarkTechTheme::text_primary())
                    .font_family("Menlo")
                    .child(display_val)
            })
    }
}

impl Render for HostModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self
            .focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone();

        let is_edit = matches!(self.mode, HostModalMode::Edit(_));

        div()
            .id("host_modal_backdrop")
            .absolute()
            .inset_0()
            .bg(rgba(0x000000bf))
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                if let Some(cb) = &this.on_action {
                    cb(HostModalAction::Cancel, window, cx);
                }
            }))
            .child(
                div()
                    .id("host_modal_dialog")
                    .track_focus(&focus_handle)
                    .w(px(580.0))
                    .max_h(px(640.0))
                    .bg(DarkTechTheme::bg_panel())
                    .border_1()
                    .border_color(DarkTechTheme::border_default())
                    .rounded_xl()
                    .p_5()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .gap_3()
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                        if let Some(ref fh) = this.focus_handle {
                            window.focus(fh);
                        }
                        cx.notify();
                        cx.stop_propagation();
                    }))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        if let Some(action) = this.handle_key_down(event, cx)
                            && let Some(cb) = &this.on_action {
                                cb(action, window, cx);
                            }
                        cx.notify();
                        cx.stop_propagation();
                    }))
                    // 1. Dialog Header
                    .child(
                        div()
                            .w_full()
                            .flex_shrink_0()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
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
                                                if is_edit {
                                                    Icon::edit().with_size(px(14.0)).with_color(DarkTechTheme::accent_indigo())
                                                } else {
                                                    Icon::plus().with_size(px(14.0)).with_color(DarkTechTheme::accent_cyan())
                                                }
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(14.0))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(if is_edit {
                                                        DarkTechTheme::accent_indigo()
                                                    } else {
                                                        DarkTechTheme::accent_cyan()
                                                    })
                                                    .child(if is_edit {
                                                        crate::t!("host.edit_title")
                                                    } else {
                                                        crate::t!("host.add_title")
                                                    }),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(DarkTechTheme::text_muted())
                                            .child(crate::t!("host.addr_placeholder")),
                                    ),
                            )
                            .child(
                                div()
                                    .id("btn_modal_close")
                                    .size(px(24.0))
                                    .rounded_md()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(DarkTechTheme::text_muted())
                                    .hover(|s| {
                                        s.bg(DarkTechTheme::bg_panel_hover())
                                            .text_color(DarkTechTheme::status_crit())
                                    })
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        if let Some(cb) = &this.on_action {
                                            cb(HostModalAction::Cancel, window, cx);
                                        }
                                    }))
                                    .child(Icon::close().with_size(px(11.0))),
                            ),
                    )
                    // Form Content Area with Scroll
                    .child(
                        div()
                            .id("host_modal_form_scroll")
                            .flex_1()
                            .w_full()
                            .min_h(px(0.0))
                            .overflow_y_scroll()
                            .pr_1()
                            .flex()
                            .flex_col()
                            .gap_3()
                            // 2. Error Banner if validation failed
                            .children(self.error_msg.as_ref().map(|err| {
                        div()
                            .w_full()
                            .bg(rgba(0xef444422))
                            .border_1()
                            .border_color(DarkTechTheme::status_crit())
                            .rounded_md()
                            .px_3()
                            .py_2()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                Icon::close().with_size(px(12.0)).with_color(DarkTechTheme::status_crit()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(DarkTechTheme::status_crit())
                                    .child(err.clone()),
                            )
                    }))
                    // 3. Row 1: Host Name & Group
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_row()
                            .gap_3()
                            // Host Name
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(DarkTechTheme::text_secondary())
                                            .font_weight(FontWeight::BOLD)
                                            .child(crate::t!("host.name_label")),
                                    )
                                    .child(self.render_input_box(
                                        HostModalField::Name,
                                        &self.name,
                                        false,
                                        crate::t!("host.name_placeholder"),
                                        cx,
                                    )),
                            )
                            // Group
                            .child(
                                div()
                                    .w(px(200.0))
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(DarkTechTheme::text_secondary())
                                            .font_weight(FontWeight::BOLD)
                                            .child(crate::t!("host.group_label")),
                                    )
                                    .child(self.render_input_box(
                                        HostModalField::Group,
                                        &self.group,
                                        false,
                                        "Default",
                                        cx,
                                    )),
                            ),
                    )
                    // Group presets pills
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(DarkTechTheme::text_muted())
                                    .child("快速分组:"),
                            )
                            .children(
                                ["Production", "Staging", "Database", "Default"]
                                    .iter()
                                    .enumerate()
                                    .map(|(idx, preset)| {
                                        let p = preset.to_string();
                                        let p_val = p.clone();
                                        let is_current = self.group == p;
                                        div()
                                            .id(ElementId::NamedInteger("preset_grp".into(), idx as u64))
                                            .px_2()
                                            .py_0p5()
                                            .rounded_xs()
                                            .bg(if is_current {
                                                DarkTechTheme::bg_panel_hover()
                                            } else {
                                                DarkTechTheme::bg_input()
                                            })
                                            .border_1()
                                            .border_color(if is_current {
                                                DarkTechTheme::border_active()
                                            } else {
                                                DarkTechTheme::border_muted()
                                            })
                                            .text_size(px(10.0))
                                            .text_color(if is_current {
                                                DarkTechTheme::text_accent()
                                            } else {
                                                DarkTechTheme::text_secondary()
                                            })
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.group = p_val.clone();
                                                cx.notify();
                                            }))
                                            .child(p)

                                    }),
                            ),

                    )
                    // 4. Row 2: Hostname & Port
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_row()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(DarkTechTheme::text_secondary())
                                            .font_weight(FontWeight::BOLD)
                                            .child(crate::t!("host.addr_label")),
                                    )
                                    .child(self.render_input_box(
                                        HostModalField::Hostname,
                                        &self.hostname,
                                        false,
                                        crate::t!("host.addr_placeholder"),
                                        cx,
                                    )),
                            )
                            .child(
                                div()
                                    .w(px(90.0))
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(DarkTechTheme::text_secondary())
                                            .font_weight(FontWeight::BOLD)
                                            .child(crate::t!("host.port_label")),
                                    )
                                    .child(self.render_input_box(
                                        HostModalField::Port,
                                        &self.port,
                                        false,
                                        "22",
                                        cx,
                                    )),
                            ),
                    )
                    // 5. Row 3: User & Target OS
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_row()
                            .gap_3()
                            .child(
                                div()
                                    .w(px(200.0))
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(DarkTechTheme::text_secondary())
                                            .font_weight(FontWeight::BOLD)
                                            .child(crate::t!("host.user_label")),
                                    )
                                    .child(self.render_input_box(
                                        HostModalField::User,
                                        &self.user,
                                        false,
                                        "root",
                                        cx,
                                    )),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(DarkTechTheme::text_secondary())
                                            .font_weight(FontWeight::BOLD)
                                            .child(crate::t!("host.os_label")),
                                    )
                                    .child(
                                        div()
                                            .h(px(32.0))
                                            .w_full()
                                            .flex()
                                            .flex_row()
                                            .gap_2()
                                            .child({
                                                let is_sel = self.target_os == TargetOs::Linux;
                                                div()
                                                    .id("os_linux")
                                                    .flex_1()
                                                    .h_full()
                                                    .rounded_md()
                                                    .bg(if is_sel {
                                                        DarkTechTheme::bg_panel_hover()
                                                    } else {
                                                        DarkTechTheme::bg_input()
                                                    })
                                                    .border_1()
                                                    .border_color(if is_sel {
                                                        DarkTechTheme::border_active()
                                                    } else {
                                                        DarkTechTheme::border_default()
                                                    })
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .text_size(px(11.0))
                                                    .text_color(if is_sel {
                                                        DarkTechTheme::text_accent()
                                                    } else {
                                                        DarkTechTheme::text_secondary()
                                                    })
                                                    .cursor_pointer()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_1p5()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.target_os = TargetOs::Linux;
                                                        cx.notify();
                                                    }))
                                                    .child(Icon::linux().with_size(px(13.0)).with_color(if is_sel { DarkTechTheme::text_accent() } else { DarkTechTheme::text_secondary() }))
                                                    .child("Linux")
                                            })
                                            .child({
                                                let is_sel = self.target_os == TargetOs::Darwin;
                                                div()
                                                    .id("os_darwin")
                                                    .flex_1()
                                                    .h_full()
                                                    .rounded_md()
                                                    .bg(if is_sel {
                                                        DarkTechTheme::bg_panel_hover()
                                                    } else {
                                                        DarkTechTheme::bg_input()
                                                    })
                                                    .border_1()
                                                    .border_color(if is_sel {
                                                        DarkTechTheme::border_active()
                                                    } else {
                                                        DarkTechTheme::border_default()
                                                    })
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .text_size(px(11.0))
                                                    .text_color(if is_sel {
                                                        DarkTechTheme::text_accent()
                                                    } else {
                                                        DarkTechTheme::text_secondary()
                                                    })
                                                    .cursor_pointer()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_1p5()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.target_os = TargetOs::Darwin;
                                                        cx.notify();
                                                    }))
                                                    .child(Icon::apple().with_size(px(13.0)).with_color(if is_sel { DarkTechTheme::text_accent() } else { DarkTechTheme::text_secondary() }))
                                                    .child("macOS")
                                            })
                                            .child({
                                                let is_sel = self.target_os == TargetOs::Windows;
                                                div()
                                                    .id("os_windows")
                                                    .flex_1()
                                                    .h_full()
                                                    .rounded_md()
                                                    .bg(if is_sel {
                                                        DarkTechTheme::bg_panel_hover()
                                                    } else {
                                                        DarkTechTheme::bg_input()
                                                    })
                                                    .border_1()
                                                    .border_color(if is_sel {
                                                        DarkTechTheme::border_active()
                                                    } else {
                                                        DarkTechTheme::border_default()
                                                    })
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .text_size(px(11.0))
                                                    .text_color(if is_sel {
                                                        DarkTechTheme::text_accent()
                                                    } else {
                                                        DarkTechTheme::text_secondary()
                                                    })
                                                    .cursor_pointer()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_1p5()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.target_os = TargetOs::Windows;
                                                        cx.notify();
                                                    }))
                                                    .child(Icon::windows().with_size(px(13.0)).with_color(if is_sel { DarkTechTheme::text_accent() } else { DarkTechTheme::text_secondary() }))
                                                    .child("Windows")
                                            }),
                                    ),
                            ),
                    )
                    // 6. Row 4: Auth Method Tabs
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(DarkTechTheme::text_secondary())
                                    .font_weight(FontWeight::BOLD)
                                    .child(crate::t!("host.auth_type")),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .h(px(32.0))
                                    .bg(DarkTechTheme::bg_input())
                                    .rounded_md()
                                    .p_0p5()
                                    .flex()
                                    .flex_row()
                                    .gap_1()
                                    .child({
                                        let is_sel = self.auth_type == AuthTypeSelection::Password;
                                        div()
                                            .id("auth_tab_password")
                                            .flex_1()
                                            .h_full()
                                            .rounded_sm()
                                            .bg(if is_sel {
                                                DarkTechTheme::border_active()
                                            } else {
                                                DarkTechTheme::bg_input()
                                            })
                                            .text_color(if is_sel {
                                                DarkTechTheme::bg_root()
                                            } else {
                                                DarkTechTheme::text_secondary()
                                            })
                                            .font_weight(if is_sel {
                                                FontWeight::BOLD
                                            } else {
                                                FontWeight::NORMAL
                                            })
                                            .text_size(px(11.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .cursor_pointer()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .justify_center()
                                            .gap_1p5()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.auth_type = AuthTypeSelection::Password;
                                                this.set_active_field(HostModalField::Password);
                                                cx.notify();
                                            }))
                                            .child(Icon::shield().with_size(px(12.0)).with_color(if is_sel { DarkTechTheme::bg_root() } else { DarkTechTheme::text_secondary() }))
                                            .child(crate::t!("host.auth_password"))
                                    })
                                    .child({
                                        let is_sel =
                                            self.auth_type == AuthTypeSelection::PrivateKey;
                                        div()
                                            .id("auth_tab_key")
                                            .flex_1()
                                            .h_full()
                                            .rounded_sm()
                                            .bg(if is_sel {
                                                DarkTechTheme::border_active()
                                            } else {
                                                DarkTechTheme::bg_input()
                                            })
                                            .text_color(if is_sel {
                                                DarkTechTheme::bg_root()
                                            } else {
                                                DarkTechTheme::text_secondary()
                                            })
                                            .font_weight(if is_sel {
                                                FontWeight::BOLD
                                            } else {
                                                FontWeight::NORMAL
                                            })
                                            .text_size(px(11.0))
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .justify_center()
                                            .gap_1p5()
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.auth_type = AuthTypeSelection::PrivateKey;
                                                this.set_active_field(HostModalField::KeyPath);
                                                cx.notify();
                                            }))
                                            .child(Icon::file().with_size(px(12.0)).with_color(if is_sel { DarkTechTheme::bg_root() } else { DarkTechTheme::text_secondary() }))
                                            .child(crate::t!("host.auth_key"))
                                    })
                                    .child({
                                        let is_sel = self.auth_type == AuthTypeSelection::Agent;
                                        div()
                                            .id("auth_tab_agent")
                                            .flex_1()
                                            .h_full()
                                            .rounded_sm()
                                            .bg(if is_sel {
                                                DarkTechTheme::border_active()
                                            } else {
                                                DarkTechTheme::bg_input()
                                            })
                                            .text_color(if is_sel {
                                                DarkTechTheme::bg_root()
                                            } else {
                                                DarkTechTheme::text_secondary()
                                            })
                                            .font_weight(if is_sel {
                                                FontWeight::BOLD
                                            } else {
                                                FontWeight::NORMAL
                                            })
                                            .text_size(px(11.0))
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .justify_center()
                                            .gap_1p5()
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.auth_type = AuthTypeSelection::Agent;
                                                this.set_active_field(HostModalField::Tags);
                                                cx.notify();
                                            }))
                                            .child(Icon::terminal().with_size(px(12.0)).with_color(if is_sel { DarkTechTheme::bg_root() } else { DarkTechTheme::text_secondary() }))
                                            .child("SSH Agent")
                                    }),
                            ),
                    )
                    // 7. Conditional Auth Fields
                    .child(match self.auth_type {
                        AuthTypeSelection::Password => {
                            let show = self.show_password;
                            div()
                                .w_full()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .w_full()
                                        .flex()
                                        .flex_row()
                                        .justify_between()
                                        .items_center()
                                        .child(
                                            div()
                                                .text_size(px(11.0))
                                                .text_color(DarkTechTheme::text_secondary())
                                                .font_weight(FontWeight::BOLD)
                                                .child(crate::t!("host.pwd_label")),
                                        )
                                        .child(
                                            div()
                                                .id("btn_toggle_pwd")
                                                .text_size(px(10.0))
                                                .text_color(DarkTechTheme::text_muted())
                                                .hover(|s| s.text_color(DarkTechTheme::text_accent()))
                                                .cursor_pointer()
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.show_password = !this.show_password;
                                                    cx.notify();
                                                }))
                                                .child(if show { "隐藏密码" } else { "显示密码" }),
                                        ),
                                )
                                .child(self.render_input_box(
                                    HostModalField::Password,
                                    &self.password,
                                    !show,
                                    crate::t!("host.pwd_placeholder"),
                                    cx,
                                ))
                        }
                        AuthTypeSelection::PrivateKey => {
                            let show_pass = self.show_passphrase;
                            div()
                                .w_full()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(
                                    div()
                                        .w_full()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(
                                            div()
                                                .w_full()
                                                .flex()
                                                .flex_row()
                                                .justify_between()
                                                .items_center()
                                                .child(
                                                    div()
                                                        .text_size(px(11.0))
                                                        .text_color(DarkTechTheme::text_secondary())
                                                        .font_weight(FontWeight::BOLD)
                                                        .child(crate::t!("host.key_path_label")),
                                                )
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_row()
                                                        .gap_2()
                                                        .child(
                                                            div()
                                                                .id("btn_key_ed25519")
                                                                .text_size(px(10.0))
                                                                .text_color(DarkTechTheme::text_accent())
                                                                .cursor_pointer()
                                                                .on_click(cx.listener(|this, _, _, cx| {
                                                                    this.key_path = "~/.ssh/id_ed25519".to_string();
                                                                    cx.notify();
                                                                }))
                                                                .child("id_ed25519"),
                                                        )
                                                        .child(
                                                            div()
                                                                .id("btn_key_rsa")
                                                                .text_size(px(10.0))
                                                                .text_color(DarkTechTheme::text_accent())
                                                                .cursor_pointer()
                                                                .on_click(cx.listener(|this, _, _, cx| {
                                                                    this.key_path = "~/.ssh/id_rsa".to_string();
                                                                    cx.notify();
                                                                }))
                                                                .child("id_rsa"),
                                                        ),
                                                ),
                                        )
                                        .child(self.render_input_box(
                                            HostModalField::KeyPath,
                                            &self.key_path,
                                            false,
                                            "~/.ssh/id_ed25519",
                                            cx,
                                        )),
                                )
                                .child(
                                    div()
                                        .w_full()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(
                                            div()
                                                .w_full()
                                                .flex()
                                                .flex_row()
                                                .justify_between()
                                                .items_center()
                                                .child(
                                                    div()
                                                        .text_size(px(11.0))
                                                        .text_color(DarkTechTheme::text_secondary())
                                                        .font_weight(FontWeight::BOLD)
                                                        .child(crate::t!("host.passphrase_label")),
                                                )
                                                .child(
                                                    div()
                                                        .id("btn_toggle_pass")
                                                        .text_size(px(10.0))
                                                        .text_color(DarkTechTheme::text_muted())
                                                        .hover(|s| s.text_color(DarkTechTheme::text_accent()))
                                                        .cursor_pointer()
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.show_passphrase = !this.show_passphrase;
                                                            cx.notify();
                                                        }))
                                                        .child(if show_pass { "隐藏口令" } else { "显示口令" }),
                                                ),
                                        )
                                        .child(self.render_input_box(
                                            HostModalField::Passphrase,
                                            &self.passphrase,
                                            !show_pass,
                                            "私钥无口令请留空",
                                            cx,
                                        )),
                                )
                        }

                        AuthTypeSelection::Agent => {
                            div()
                                .w_full()
                                .bg(DarkTechTheme::bg_input())
                                .border_1()
                                .border_color(DarkTechTheme::border_muted())
                                .rounded_md()
                                .p_2p5()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .child(Icon::zap().with_size(px(13.0)).with_color(DarkTechTheme::accent_emerald()))
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(DarkTechTheme::accent_emerald())
                                        .child("将自动使用本地系统 SSH Agent ($SSH_AUTH_SOCK) 凭据连接"),
                                )
                        }
                    })
                    // 8. Row 5: Tags
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(DarkTechTheme::text_secondary())
                                    .font_weight(FontWeight::BOLD)
                                    .child("分类标签 (Tags)"),
                            )
                            .child(self.render_input_box(
                                HostModalField::Tags,
                                &self.tags_input,
                                false,
                                "例如: web, prod, nginx (以逗号分隔)",
                                cx,
                            )),
                    )
                    // 9. Row 6: VPS 月度流量额度监控
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_row()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(DarkTechTheme::text_secondary())
                                            .font_weight(FontWeight::BOLD)
                                            .child(crate::t!("host.bandwidth_limit")),
                                    )
                                    .child(self.render_input_box(
                                        HostModalField::BandwidthLimit,
                                        &self.bandwidth_limit_gb,
                                        false,
                                        "例如: 1000 (代表 1TB)",
                                        cx,
                                    )),
                            )
                            .child(
                                div()
                                    .w(px(160.0))
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(DarkTechTheme::text_secondary())
                                            .font_weight(FontWeight::BOLD)
                                            .child(crate::t!("host.bandwidth_reset_day")),
                                    )
                                    .child(self.render_input_box(
                                        HostModalField::BandwidthResetDay,
                                        &self.bandwidth_reset_day,
                                        false,
                                        "1",
                                        cx,
                                    )),
                            ),
                    ),
                    )
                    // 10. Footer actions
                    .child(
                        div()
                            .w_full()
                            .flex_shrink_0()
                            .pt_2()
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
                                    .when(is_edit, |d| {
                                        let edit_host = match &self.mode {
                                            HostModalMode::Edit(h) => Some((**h).clone()),
                                            _ => None,
                                        };
                                        if let Some(host) = edit_host {
                                            let host_clone = host.clone();
                                            let host_id = host.id.clone();
                                            d.child(
                                                div()
                                                    .id("btn_modal_delete_host")
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_1()
                                                    .px_3()
                                                    .py_1p5()
                                                    .rounded_md()
                                                    .bg(DarkTechTheme::bg_input())
                                                    .border_1()
                                                    .border_color(DarkTechTheme::status_crit())
                                                    .text_size(px(11.0))
                                                    .text_color(DarkTechTheme::status_crit())
                                                    .hover(|s| s.bg(DarkTechTheme::status_crit()).text_color(DarkTechTheme::bg_root()))
                                                    .cursor_pointer()
                                                    .on_click(cx.listener(move |this, _, window, cx| {
                                                        if let Some(cb) = &this.on_action {
                                                            cb(HostModalAction::Delete(host_id.clone()), window, cx);
                                                        }
                                                    }))
                                                    .child(Icon::trash().with_size(px(11.0)).with_color(DarkTechTheme::status_crit()))
                                                    .child(crate::t!("common.delete")),
                                            )
                                            .child(
                                                div()
                                                    .id("btn_modal_clone_host")
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_1()
                                                    .px_3()
                                                    .py_1p5()
                                                    .rounded_md()
                                                    .bg(DarkTechTheme::bg_input())
                                                    .border_1()
                                                    .border_color(DarkTechTheme::border_default())
                                                    .text_size(px(11.0))
                                                    .text_color(DarkTechTheme::text_secondary())
                                                    .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()).text_color(DarkTechTheme::accent_cyan()))
                                                    .cursor_pointer()
                                                    .on_click(cx.listener(move |this, _, window, cx| {
                                                        if let Some(cb) = &this.on_action {
                                                            cb(HostModalAction::Clone(host_clone.clone()), window, cx);
                                                        }
                                                    }))
                                                    .child(Icon::copy().with_size(px(11.0)).with_color(DarkTechTheme::text_secondary()))
                                                    .child(crate::t!("common.copy")),
                                            )
                                        } else {
                                            d
                                        }
                                    })
                                    .when(!is_edit, |d| {
                                        d.child(
                                            div()
                                                .text_size(px(10.0))
                                                .text_color(DarkTechTheme::text_muted())
                                                .child("Tab 循环切输入框 | Enter 保存 | Esc 取消"),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id("btn_modal_cancel")
                                            .px_4()
                                            .py_1p5()
                                            .rounded_md()
                                            .bg(DarkTechTheme::bg_panel_hover())
                                            .border_1()
                                            .border_color(DarkTechTheme::border_default())
                                            .text_size(px(12.0))
                                            .text_color(DarkTechTheme::text_secondary())
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                if let Some(cb) = &this.on_action {
                                                    cb(HostModalAction::Cancel, window, cx);
                                                }
                                            }))
                                            .child(crate::t!("common.cancel")),
                                    )
                                    .child(
                                        div()
                                            .id("btn_modal_save")
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_1p5()
                                            .px_4()
                                            .py_1p5()
                                            .rounded_md()
                                            .bg(DarkTechTheme::border_active())
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(DarkTechTheme::bg_root())
                                            .hover(|s| s.bg(DarkTechTheme::accent_cyan()))
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                if let Some(action) = this.submit()
                                                    && let Some(cb) = &this.on_action {
                                                        cb(action, window, cx);
                                                    }
                                                cx.notify();
                                            }))
                                            .child(if is_edit {
                                                Icon::save().with_size(px(12.0)).with_color(DarkTechTheme::bg_root())
                                            } else {
                                                Icon::plus().with_size(px(12.0)).with_color(DarkTechTheme::bg_root())
                                            })
                                            .child(if is_edit {
                                                crate::t!("common.save")
                                            } else {
                                                crate::t!("fleet.add_host")
                                            }),
                                    ),
                            ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn editing_preserves_proxy_and_unread_credentials() {
        let mut host = HostConfig::new("old", "example.test", "test");
        host.proxy_jump_id = Some(HostId::new());
        host.jump_host = host.proxy_jump_id.clone();
        host.auth = AuthMethod::PrivateKey {
            key_path: "/tmp/key".into(),
            passphrase_id: Some("retained".into()),
        };
        let mut modal = HostModal::new_edit(host.clone());
        modal.name = "new".into();
        let (edited, password, passphrase) = modal.validate_and_build().unwrap();
        assert_eq!(edited.auth, host.auth);
        assert_eq!(edited.proxy_jump_id, host.proxy_jump_id);
        assert_eq!(edited.jump_host, host.jump_host);
        assert!(password.is_none() && passphrase.is_none());
    }

    #[core::prelude::v1::test]
    fn test_host_modal_create_default_state() {
        let modal = HostModal::new_create();
        assert_eq!(modal.mode, HostModalMode::Create);
        assert_eq!(modal.name, "");
        assert_eq!(modal.group, "Default");
        assert_eq!(modal.hostname, "");
        assert_eq!(modal.port, "22");
        assert_eq!(modal.user, "root");
        assert_eq!(modal.auth_type, AuthTypeSelection::Password);
        assert_eq!(modal.target_os, TargetOs::Linux);
        assert_eq!(modal.active_field, HostModalField::Name);
        assert_eq!(modal.cursor_pos, 0);
        assert!(modal.error_msg.is_none());
    }

    #[core::prelude::v1::test]
    fn test_host_modal_edit_state() {
        let mut host = HostConfig::new("Production-DB", "10.0.0.50", "admin");
        host.port = 2222;
        host.group = "Database".to_string();
        host.tags = vec!["prod".to_string(), "db".to_string()];
        host.target_os = TargetOs::Darwin;
        let host_id = host.id.clone();

        let modal = HostModal::new_edit(host);
        assert!(matches!(modal.mode, HostModalMode::Edit(_)));
        assert_eq!(modal.name, "Production-DB");
        assert_eq!(modal.hostname, "10.0.0.50");
        assert_eq!(modal.port, "2222");
        assert_eq!(modal.user, "admin");
        assert_eq!(modal.group, "Database");
        assert_eq!(modal.tags_input, "prod, db");
        assert_eq!(modal.target_os, TargetOs::Darwin);
        assert_eq!(modal.active_field, HostModalField::Name);
        assert_eq!(modal.cursor_pos, "Production-DB".chars().count());

        // Validate building in edit mode retains existing ID
        let (built, _, _) = modal.validate_and_build().expect("Edit should validate");
        assert_eq!(built.id, host_id);
    }

    #[core::prelude::v1::test]
    fn test_host_modal_field_cycling() {
        let mut modal = HostModal::new_create();
        assert_eq!(modal.active_field, HostModalField::Name);

        // Forward tab
        modal.cycle_field(true);
        assert_eq!(modal.active_field, HostModalField::Group);

        modal.cycle_field(true);
        assert_eq!(modal.active_field, HostModalField::Hostname);

        modal.cycle_field(true);
        assert_eq!(modal.active_field, HostModalField::Port);

        modal.cycle_field(true);
        assert_eq!(modal.active_field, HostModalField::User);

        modal.cycle_field(true);
        assert_eq!(modal.active_field, HostModalField::Password);

        modal.cycle_field(true);
        assert_eq!(modal.active_field, HostModalField::Tags);

        modal.cycle_field(true);
        assert_eq!(modal.active_field, HostModalField::BandwidthLimit);

        modal.cycle_field(true);
        assert_eq!(modal.active_field, HostModalField::BandwidthResetDay);

        // Wrap around to Name
        modal.cycle_field(true);
        assert_eq!(modal.active_field, HostModalField::Name);

        // Backward Shift-Tab
        modal.cycle_field(false);
        assert_eq!(modal.active_field, HostModalField::BandwidthResetDay);
    }

    #[core::prelude::v1::test]
    fn test_host_modal_text_editing() {
        let mut modal = HostModal::new_create();
        modal.set_active_field(HostModalField::Name);

        modal.insert_str("Server");
        assert_eq!(modal.name, "Server");
        assert_eq!(modal.cursor_pos, 6);

        modal.insert_char('-');
        modal.insert_str("01");
        assert_eq!(modal.name, "Server-01");
        assert_eq!(modal.cursor_pos, 9);

        modal.backspace();
        assert_eq!(modal.name, "Server-0");
        assert_eq!(modal.cursor_pos, 8);

        modal.move_left();
        assert_eq!(modal.cursor_pos, 7);

        modal.insert_char('X');
        assert_eq!(modal.name, "Server-X0");
        assert_eq!(modal.cursor_pos, 8);

        modal.delete();
        assert_eq!(modal.name, "Server-X");

        modal.move_home();
        assert_eq!(modal.cursor_pos, 0);

        modal.move_end();
        assert_eq!(modal.cursor_pos, 8);
    }

    #[core::prelude::v1::test]
    fn test_host_modal_validation_failure() {
        let mut modal = HostModal::new_create();
        // Empty fields should fail
        assert!(modal.validate_and_build().is_err());

        modal.name = "MyHost".to_string();
        assert!(modal.validate_and_build().is_err());

        modal.hostname = "invalid host name with spaces".to_string();
        modal.password = "secret".to_string();
        let err = modal.validate_and_build().unwrap_err();
        assert!(err.contains("whitespace") || err.contains("Hostname"));

        // Invalid port
        modal.hostname = "192.168.1.1".to_string();
        modal.port = "0".to_string();
        let err_port = modal.validate_and_build().unwrap_err();
        assert!(err_port.contains("0"));
    }

    #[core::prelude::v1::test]
    fn test_host_modal_validation_success() {
        let mut modal = HostModal::new_create();
        modal.name = "web-prod-01".to_string();
        modal.group = "Production".to_string();
        modal.hostname = "10.0.0.1".to_string();
        modal.port = "22".to_string();
        modal.user = "root".to_string();
        modal.auth_type = AuthTypeSelection::Password;
        modal.password = "secure_pass_123".to_string();
        modal.tags_input = "prod, web, nginx".to_string();
        modal.target_os = TargetOs::Linux;

        let res = modal.validate_and_build();
        assert!(res.is_ok());
        let (host, pass, _) = res.unwrap();
        assert_eq!(host.name, "web-prod-01");
        assert_eq!(host.group, "Production");
        assert_eq!(host.hostname, "10.0.0.1");
        assert_eq!(host.port, 22);
        assert_eq!(host.user, "root");
        assert_eq!(host.tags, vec!["prod", "web", "nginx"]);
        assert_eq!(host.target_os, TargetOs::Linux);
        assert_eq!(pass, Some("secure_pass_123".to_string()));
    }

    #[core::prelude::v1::test]
    fn test_host_modal_bandwidth_configuration() {
        let mut modal = HostModal::new_create();
        modal.name = "vps-tokyo".to_string();
        modal.hostname = "192.0.2.1".to_string();
        modal.password = "secret".to_string();
        modal.bandwidth_limit_gb = "1000".to_string();
        modal.bandwidth_reset_day = "5".to_string();

        let (host, _, _) = modal.validate_and_build().expect("Should validate");
        assert_eq!(host.bandwidth_limit_gb, Some(1000));
        assert_eq!(host.bandwidth_reset_day, Some(5));

        // Test invalid reset day > 31
        modal.bandwidth_reset_day = "32".to_string();
        assert!(modal.validate_and_build().is_err());
    }
}
