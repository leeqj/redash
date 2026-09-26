use crate::components::icon::Icon;
use crate::components::theme::DarkTechTheme;
use gpui::*;
use redash_core::config::HostConfig;
use redash_core::session::SessionManager;
use redash_core::sftp::{RemoteFileItem, SftpManager};
use russh_sftp::client::SftpSession;
use std::sync::Arc;

#[derive(Clone, PartialEq, Eq)]
pub enum ActiveModal {
    None,
    ConfirmDelete {
        file: RemoteFileItem,
    },
    Preview {
        file_name: String,
        content: String,
    },
    Editor {
        editor_id: u64,
        line_ending: String,
        file_name: String,
        file_path: String,
        lines: Vec<String>,
        cursor_row: usize,
        cursor_col: usize,
        is_dirty: bool,
        is_saving: bool,
        status_msg: String,
    },
    ContextMenu {
        file: RemoteFileItem,
        x: Pixels,
        y: Pixels,
    },
    RenameDialog {
        file_path: String,
        old_name: String,
        new_name: String,
        cursor_pos: usize,
    },
    ChmodDialog {
        file_path: String,
        current_mode: u32,
        input_mode: String,
        cursor_pos: usize,
    },
    NewItemDialog {
        is_dir: bool,
        name: String,
        cursor_pos: usize,
    },
}

pub struct SftpView {
    directory_generation: u64,
    editor_generation: u64,
    pub host: HostConfig,
    pub session_mgr: Arc<SessionManager>,
    pub sftp: Option<Arc<SftpSession>>,
    pub current_path: String,
    pub items: Vec<RemoteFileItem>,
    pub is_loading: bool,
    pub loading_msg: String,
    pub error_msg: Option<String>,
    pub modal: ActiveModal,
    pub scroll_handle: ScrollHandle,
    pub current_page: usize,
    pub page_size: usize,
    pub filter_query: String,
    pub show_hidden: bool,
    pub confirm_delete: bool,
    focus_handle: FocusHandle,
}

impl Focusable for SftpView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl SftpView {
    pub fn default_path_for_host(host: &HostConfig) -> String {
        if host.user.is_empty() {
            "/".to_string()
        } else if host.user == "root" {
            "/root".to_string()
        } else if host.target_os == redash_core::config::TargetOs::Darwin {
            format!("/Users/{}", host.user)
        } else {
            format!("/home/{}", host.user)
        }
    }

    pub fn new(host: HostConfig, session_mgr: Arc<SessionManager>, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let default_path = Self::default_path_for_host(&host);
        Self {
            directory_generation: 0,
            editor_generation: 0,
            host,
            session_mgr,
            sftp: None,
            current_path: default_path,
            items: Vec::new(),
            is_loading: true,
            loading_msg: "正在连接远程 SFTP 会话...".to_string(),
            error_msg: None,
            modal: ActiveModal::None,
            scroll_handle: ScrollHandle::new(),
            current_page: 1,
            page_size: 100,
            filter_query: String::new(),
            show_hidden: false,
            confirm_delete: true,
            focus_handle,
        }
    }

    #[allow(dead_code)]
    pub fn focus(&self, window: &mut Window) {
        window.focus(&self.focus_handle);
    }

    pub fn init_connection_and_load(&mut self, path: String, cx: &mut Context<Self>) {
        self.load_directory_request(path, true, cx);
    }
    pub fn go_home_dir(&mut self, cx: &mut Context<Self>) {
        self.load_directory_request(Self::default_path_for_host(&self.host), true, cx);
    }
    pub fn load_directory(&mut self, path: String, cx: &mut Context<Self>) {
        self.load_directory_request(path, false, cx);
    }
    fn load_directory_request(&mut self, path: String, resolve_home: bool, cx: &mut Context<Self>) {
        self.directory_generation += 1;
        let generation = self.directory_generation;
        self.is_loading = true;
        self.error_msg = None;
        self.loading_msg = format!("正在读取目录: {path}...");
        let existing = self.sftp.clone();
        let manager = Arc::clone(&self.session_mgr);
        let host = self.host.clone();
        cx.spawn(async move |this, cx| {
            let result: anyhow::Result<_> = async {
                let mut sftp = match existing {
                    Some(sftp) => sftp,
                    None => Arc::new(manager.open_sftp(&host).await?),
                };
                let path = if resolve_home {
                    SftpManager::resolve_initial_dir(&sftp, &path).await
                } else {
                    SftpManager::normalize_dir_path(&path)
                };
                let mut items = SftpManager::list_dir(&sftp, &path).await;
                if items.as_ref().is_err_and(SftpManager::is_connection_error) {
                    sftp = Arc::new(manager.open_sftp(&host).await?);
                    items = SftpManager::list_dir(&sftp, &path).await;
                }
                Ok((sftp, path, items?))
            }
            .await;
            let _ = this.update(cx, |view, cx| {
                if view.directory_generation != generation {
                    return;
                }
                view.is_loading = false;
                match result {
                    Ok((sftp, path, items)) => {
                        view.sftp = Some(sftp);
                        view.current_path = path;
                        view.items = items;
                        view.current_page = 1;
                    }
                    Err(error) => view.error_msg = Some(format!("读取目录失败: {error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub fn filtered_items(&self) -> Vec<RemoteFileItem> {
        let q = self.filter_query.trim().to_lowercase();
        self.items
            .iter()
            .filter(|i| {
                if !self.show_hidden && i.name.starts_with('.') {
                    return false;
                }
                if q.is_empty() {
                    true
                } else {
                    i.name.to_lowercase().contains(&q)
                }
            })
            .cloned()
            .collect()
    }

    pub fn total_pages(&self) -> usize {
        let total = self.filtered_items().len();
        if total == 0 {
            1
        } else {
            total.div_ceil(self.page_size)
        }
    }

    pub fn paged_items(&self) -> Vec<RemoteFileItem> {
        let all = self.filtered_items();
        let total = all.len();
        let tp = self.total_pages();
        let page = self.current_page.max(1).min(tp);
        let start = (page - 1) * self.page_size;
        if start >= total {
            Vec::new()
        } else {
            let end = (start + self.page_size).min(total);
            all[start..end].to_vec()
        }
    }

    pub fn prev_page(&mut self, cx: &mut Context<Self>) {
        if self.current_page > 1 {
            self.current_page -= 1;
            cx.notify();
        }
    }

    pub fn next_page(&mut self, cx: &mut Context<Self>) {
        if self.current_page < self.total_pages() {
            self.current_page += 1;
            cx.notify();
        }
    }

    pub fn set_page_size(&mut self, size: usize, cx: &mut Context<Self>) {
        self.page_size = size;
        self.current_page = 1;
        cx.notify();
    }

    pub fn open_preview(&mut self, file: RemoteFileItem, cx: &mut Context<Self>) {
        if file.size > 5 * 1024 * 1024 {
            self.error_msg = Some(format!(
                "文件 '{}' 体积过大 ({})，快速预览仅支持小于 5MB 的文本文件",
                file.name,
                Self::format_size(file.size)
            ));
            cx.notify();
            return;
        }
        if let Some(sftp) = &self.sftp {
            let sftp = Arc::clone(sftp);
            let path = file.path.clone();
            let name = file.name.clone();

            cx.spawn(async move |this, cx| {
                let res = SftpManager::read_file_chunk(&sftp, &path, 0, 128 * 1024).await;
                let _ = this.update(cx, |view, cx| {
                    match res {
                        Ok(bytes) => {
                            let text = String::from_utf8_lossy(&bytes).to_string();
                            view.modal = ActiveModal::Preview {
                                file_name: name,
                                content: text,
                            };
                        }
                        Err(e) => {
                            view.error_msg = Some(format!("预览失败: {}", e));
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }

    pub fn open_editor(&mut self, file: RemoteFileItem, cx: &mut Context<Self>) {
        if file.size > 2 * 1024 * 1024 {
            self.error_msg = Some(format!(
                "文件 '{}' 体积过大 ({})，内联编辑器仅支持小于 2MB 的文本文件编辑",
                file.name,
                Self::format_size(file.size)
            ));
            cx.notify();
            return;
        }
        if let Some(sftp) = &self.sftp {
            let sftp = Arc::clone(sftp);
            let path = file.path.clone();
            let name = file.name.clone();
            self.editor_generation += 1;
            let editor_id = self.editor_generation;
            self.modal = ActiveModal::Preview {
                file_name: name.clone(),
                content: "正在完整读取文件...".into(),
            };

            cx.spawn(async move |this, cx| {
                let res = SftpManager::read_full_file(&sftp, &path, 2 * 1024 * 1024)
                    .await
                    .and_then(|bytes| String::from_utf8(bytes).map_err(anyhow::Error::from));
                let _ = this.update(cx, |view, cx| {
                    if view.editor_generation != editor_id || view.modal == ActiveModal::None {
                        return;
                    }
                    match res {
                        Ok(text) => {
                            let line_ending = if text.contains("\r\n")
                                && text.matches('\n').count() == text.matches("\r\n").count()
                            {
                                "\r\n"
                            } else {
                                "\n"
                            }
                            .to_string();
                            let lines = text.split(&line_ending).map(str::to_string).collect();
                            let status_msg = "完整 UTF-8 文件 · Cmd+S 保存".to_string();
                            view.modal = ActiveModal::Editor {
                                editor_id,
                                line_ending,
                                file_name: name,
                                file_path: path,
                                lines,
                                cursor_row: 0,
                                cursor_col: 0,
                                is_dirty: false,
                                is_saving: false,
                                status_msg,
                            };
                        }
                        Err(e) => {
                            view.error_msg = Some(format!("读取文件失败: {}", e));
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }

    pub fn save_editor(&mut self, cx: &mut Context<Self>) {
        if let ActiveModal::Editor {
            editor_id,
            line_ending,
            file_path,
            lines,
            is_saving,
            status_msg,
            ..
        } = &mut self.modal
        {
            if *is_saving {
                return;
            }
            if let Some(sftp) = &self.sftp {
                *is_saving = true;
                *status_msg = "正在原子写入远程临时文件并原子替换...".to_string();
                let sftp = Arc::clone(sftp);
                let path = file_path.clone();
                let data = lines.join(line_ending).into_bytes();
                let saved_lines = lines.clone();
                let saved_id = *editor_id;
                let manager = Arc::clone(&self.session_mgr);
                let host = self.host.clone();

                cx.spawn(async move |this, cx| {
                    let res = async {
                        let raw = manager.open_sftp_atomic(&host).await?;
                        let result = SftpManager::write_file_atomic_with_extension(
                            &sftp, &raw, &path, &data,
                        )
                        .await;
                        let _ = raw.close_session();
                        result
                    }
                    .await;
                    let _ = this.update(cx, |view, cx| {
                        if let ActiveModal::Editor {
                            editor_id,
                            lines,
                            is_saving,
                            is_dirty,
                            status_msg,
                            ..
                        } = &mut view.modal
                        {
                            if *editor_id != saved_id {
                                return;
                            }
                            *is_saving = false;
                            match res {
                                Ok(_) => {
                                    *is_dirty = *lines != saved_lines;
                                    *status_msg = "文件原子保存成功".to_string();
                                }
                                Err(e) => {
                                    *status_msg = format!("保存失败: {e:#}");
                                }
                            }
                        }
                        cx.notify();
                    });
                })
                .detach();
            }
        }
        cx.notify();
    }

    pub fn editor_insert_char(&mut self, ch: char) {
        if let ActiveModal::Editor {
            lines,
            cursor_row,
            cursor_col,
            is_dirty,
            ..
        } = &mut self.modal
        {
            if lines.is_empty() {
                lines.push(String::new());
            }
            if *cursor_row >= lines.len() {
                *cursor_row = lines.len() - 1;
            }
            let line = &mut lines[*cursor_row];
            let mut chars: Vec<char> = line.chars().collect();
            let col = (*cursor_col).min(chars.len());
            chars.insert(col, ch);
            *line = chars.into_iter().collect();
            *cursor_col = col + 1;
            *is_dirty = true;
        }
    }

    pub fn editor_insert_text(&mut self, text: &str) {
        if let ActiveModal::Editor {
            lines,
            cursor_row,
            cursor_col,
            is_dirty,
            ..
        } = &mut self.modal
        {
            if lines.is_empty() {
                lines.push(String::new());
            }
            if *cursor_row >= lines.len() {
                *cursor_row = lines.len() - 1;
            }

            let clean_text = text.replace("\r\n", "\n").replace('\r', "\n");
            let paste_lines: Vec<&str> = clean_text.split('\n').collect();
            if paste_lines.is_empty() {
                return;
            }

            if paste_lines.len() == 1 {
                let current_line = &mut lines[*cursor_row];
                let mut chars: Vec<char> = current_line.chars().collect();
                let col = (*cursor_col).min(chars.len());
                let paste_chars: Vec<char> = paste_lines[0].chars().collect();
                let added_len = paste_chars.len();
                for (i, c) in paste_chars.into_iter().enumerate() {
                    chars.insert(col + i, c);
                }
                *current_line = chars.into_iter().collect();
                *cursor_col = col + added_len;
            } else {
                let current_line = lines[*cursor_row].clone();
                let chars: Vec<char> = current_line.chars().collect();
                let col = (*cursor_col).min(chars.len());
                let prefix: String = chars[..col].iter().collect();
                let suffix: String = chars[col..].iter().collect();

                lines[*cursor_row] = format!("{}{}", prefix, paste_lines[0]);

                let mut insert_idx = *cursor_row + 1;
                for mid_line in &paste_lines[1..paste_lines.len() - 1] {
                    lines.insert(insert_idx, mid_line.to_string());
                    insert_idx += 1;
                }

                let last_p = paste_lines.last().unwrap();
                let last_line_content = format!("{}{}", last_p, suffix);
                lines.insert(insert_idx, last_line_content);

                *cursor_row = insert_idx;
                *cursor_col = last_p.chars().count();
            }

            *is_dirty = true;
        }
    }

    pub fn editor_newline(&mut self) {
        if let ActiveModal::Editor {
            lines,
            cursor_row,
            cursor_col,
            is_dirty,
            ..
        } = &mut self.modal
        {
            if lines.is_empty() {
                lines.push(String::new());
            }
            if *cursor_row >= lines.len() {
                *cursor_row = lines.len() - 1;
            }

            let current_line = lines[*cursor_row].clone();
            let chars: Vec<char> = current_line.chars().collect();
            let col = (*cursor_col).min(chars.len());
            let prefix: String = chars[..col].iter().collect();
            let suffix: String = chars[col..].iter().collect();

            lines[*cursor_row] = prefix;
            lines.insert(*cursor_row + 1, suffix);
            *cursor_row += 1;
            *cursor_col = 0;
            *is_dirty = true;
        }
    }

    pub fn editor_backspace(&mut self) {
        if let ActiveModal::Editor {
            lines,
            cursor_row,
            cursor_col,
            is_dirty,
            ..
        } = &mut self.modal
        {
            if lines.is_empty() {
                return;
            }
            if *cursor_row >= lines.len() {
                *cursor_row = lines.len() - 1;
            }

            if *cursor_col > 0 {
                let line = &mut lines[*cursor_row];
                let mut chars: Vec<char> = line.chars().collect();
                if *cursor_col <= chars.len() {
                    chars.remove(*cursor_col - 1);
                    *line = chars.into_iter().collect();
                    *cursor_col -= 1;
                    *is_dirty = true;
                }
            } else if *cursor_row > 0 {
                let current_line = lines.remove(*cursor_row);
                *cursor_row -= 1;
                let prev_line = &mut lines[*cursor_row];
                let prev_len = prev_line.chars().count();
                prev_line.push_str(&current_line);
                *cursor_col = prev_len;
                *is_dirty = true;
            }
        }
    }

    pub fn editor_delete(&mut self) {
        if let ActiveModal::Editor {
            lines,
            cursor_row,
            cursor_col,
            is_dirty,
            ..
        } = &mut self.modal
        {
            if lines.is_empty() || *cursor_row >= lines.len() {
                return;
            }
            let line = &lines[*cursor_row];
            let char_count = line.chars().count();
            if *cursor_col < char_count {
                let mut chars: Vec<char> = line.chars().collect();
                chars.remove(*cursor_col);
                lines[*cursor_row] = chars.into_iter().collect();
                *is_dirty = true;
            } else if *cursor_row + 1 < lines.len() {
                let next_line = lines.remove(*cursor_row + 1);
                lines[*cursor_row].push_str(&next_line);
                *is_dirty = true;
            }
        }
    }

    pub fn editor_move_cursor(&mut self, direction: &str) {
        if let ActiveModal::Editor {
            lines,
            cursor_row,
            cursor_col,
            ..
        } = &mut self.modal
        {
            if lines.is_empty() {
                return;
            }
            if *cursor_row >= lines.len() {
                *cursor_row = lines.len() - 1;
            }
            match direction {
                "up" => {
                    if *cursor_row > 0 {
                        *cursor_row -= 1;
                        let line_len = lines[*cursor_row].chars().count();
                        *cursor_col = (*cursor_col).min(line_len);
                    }
                }
                "down" => {
                    if *cursor_row + 1 < lines.len() {
                        *cursor_row += 1;
                        let line_len = lines[*cursor_row].chars().count();
                        *cursor_col = (*cursor_col).min(line_len);
                    }
                }
                "left" => {
                    if *cursor_col > 0 {
                        *cursor_col -= 1;
                    } else if *cursor_row > 0 {
                        *cursor_row -= 1;
                        *cursor_col = lines[*cursor_row].chars().count();
                    }
                }
                "right" => {
                    let line_len = lines[*cursor_row].chars().count();
                    if *cursor_col < line_len {
                        *cursor_col += 1;
                    } else if *cursor_row + 1 < lines.len() {
                        *cursor_row += 1;
                        *cursor_col = 0;
                    }
                }
                "home" => {
                    *cursor_col = 0;
                }
                "end" => {
                    *cursor_col = lines[*cursor_row].chars().count();
                }
                "pageup" => {
                    *cursor_row = cursor_row.saturating_sub(15);
                    *cursor_col = (*cursor_col).min(lines[*cursor_row].chars().count());
                }
                "pagedown" => {
                    *cursor_row = (*cursor_row + 15).min(lines.len() - 1);
                    *cursor_col = (*cursor_col).min(lines[*cursor_row].chars().count());
                }
                _ => {}
            }
        }
    }

    pub fn rename_input_char(&mut self, ch: char) {
        if let ActiveModal::RenameDialog {
            new_name,
            cursor_pos,
            ..
        } = &mut self.modal
        {
            let mut chars: Vec<char> = new_name.chars().collect();
            let pos = (*cursor_pos).min(chars.len());
            chars.insert(pos, ch);
            *new_name = chars.into_iter().collect();
            *cursor_pos = pos + 1;
        }
    }

    pub fn rename_backspace(&mut self) {
        if let ActiveModal::RenameDialog {
            new_name,
            cursor_pos,
            ..
        } = &mut self.modal
            && *cursor_pos > 0
        {
            let mut chars: Vec<char> = new_name.chars().collect();
            if *cursor_pos <= chars.len() {
                chars.remove(*cursor_pos - 1);
                *new_name = chars.into_iter().collect();
                *cursor_pos -= 1;
            }
        }
    }

    pub fn rename_delete(&mut self) {
        if let ActiveModal::RenameDialog {
            new_name,
            cursor_pos,
            ..
        } = &mut self.modal
        {
            let mut chars: Vec<char> = new_name.chars().collect();
            if *cursor_pos < chars.len() {
                chars.remove(*cursor_pos);
                *new_name = chars.into_iter().collect();
            }
        }
    }

    pub fn new_item_input_char(&mut self, ch: char) {
        if let ActiveModal::NewItemDialog {
            name, cursor_pos, ..
        } = &mut self.modal
        {
            let mut chars: Vec<char> = name.chars().collect();
            let pos = (*cursor_pos).min(chars.len());
            chars.insert(pos, ch);
            *name = chars.into_iter().collect();
            *cursor_pos = pos + 1;
        }
    }

    pub fn new_item_backspace(&mut self) {
        if let ActiveModal::NewItemDialog {
            name, cursor_pos, ..
        } = &mut self.modal
            && *cursor_pos > 0
        {
            let mut chars: Vec<char> = name.chars().collect();
            if *cursor_pos <= chars.len() {
                chars.remove(*cursor_pos - 1);
                *name = chars.into_iter().collect();
                *cursor_pos -= 1;
            }
        }
    }

    pub fn new_item_delete(&mut self) {
        if let ActiveModal::NewItemDialog {
            name, cursor_pos, ..
        } = &mut self.modal
        {
            let mut chars: Vec<char> = name.chars().collect();
            if *cursor_pos < chars.len() {
                chars.remove(*cursor_pos);
                *name = chars.into_iter().collect();
            }
        }
    }

    pub fn chmod_input_char(&mut self, ch: char) {
        if let ActiveModal::ChmodDialog {
            input_mode,
            cursor_pos,
            ..
        } = &mut self.modal
            && ('0'..='7').contains(&ch)
        {
            let mut chars: Vec<char> = input_mode.chars().collect();
            let pos = (*cursor_pos).min(chars.len());
            if chars.len() < 4 {
                chars.insert(pos, ch);
                *input_mode = chars.into_iter().collect();
                *cursor_pos = pos + 1;
            }
        }
    }

    pub fn chmod_backspace(&mut self) {
        if let ActiveModal::ChmodDialog {
            input_mode,
            cursor_pos,
            ..
        } = &mut self.modal
            && *cursor_pos > 0
        {
            let mut chars: Vec<char> = input_mode.chars().collect();
            if *cursor_pos <= chars.len() {
                chars.remove(*cursor_pos - 1);
                *input_mode = chars.into_iter().collect();
                *cursor_pos -= 1;
            }
        }
    }

    pub fn apply_rename(&mut self, cx: &mut Context<Self>) {
        if let ActiveModal::RenameDialog {
            file_path,
            new_name,
            ..
        } = &self.modal
        {
            let new_name = new_name.trim().to_string();
            if new_name.is_empty() {
                self.error_msg = Some("文件名不能为空".to_string());
                return;
            }
            if new_name.contains('/')
                || new_name.contains('\\')
                || new_name == "."
                || new_name == ".."
                || new_name.contains("..")
            {
                self.error_msg = Some("文件名不能包含路径分隔符(/, \\)或遍历符号(..)".to_string());
                return;
            }
            if let Some(sftp) = &self.sftp {
                let sftp = Arc::clone(sftp);
                let old_path = file_path.clone();
                let parent_dir = file_path
                    .rsplit_once('/')
                    .map(|(parent, _)| parent)
                    .unwrap_or("")
                    .to_string();
                let new_path = if parent_dir.is_empty() {
                    format!("/{}", new_name)
                } else {
                    format!("{}/{}", parent_dir, new_name)
                };
                let curr = self.current_path.clone();

                cx.spawn(async move |this, cx| {
                    let res = SftpManager::rename(&sftp, &old_path, &new_path).await;
                    let _ = this.update(cx, |view, cx| {
                        view.modal = ActiveModal::None;
                        match res {
                            Ok(_) => {
                                view.load_directory(curr, cx);
                            }
                            Err(e) => {
                                view.error_msg = Some(format!("重命名失败: {}", e));
                            }
                        }
                        cx.notify();
                    });
                })
                .detach();
            }
        }
    }

    pub fn create_new_item(&mut self, is_dir: bool, name: &str, cx: &mut Context<Self>) {
        let name = name.trim();
        if name.is_empty() {
            self.error_msg = Some("名称不能为空".to_string());
            return;
        }
        if name.contains('/')
            || name.contains('\\')
            || name == "."
            || name == ".."
            || name.contains("..")
        {
            self.error_msg = Some("名称不能包含路径分隔符(/, \\)或遍历符号(..)".to_string());
            return;
        }
        if let Some(sftp) = &self.sftp {
            let sftp = Arc::clone(sftp);
            let target_path = format!("{}/{}", self.current_path.trim_end_matches('/'), name);
            let curr = self.current_path.clone();

            cx.spawn(async move |this, cx| {
                let res = if is_dir {
                    SftpManager::create_dir(&sftp, &target_path).await
                } else {
                    SftpManager::create_file(&sftp, &target_path).await
                };
                let _ = this.update(cx, |view, cx| {
                    view.modal = ActiveModal::None;
                    match res {
                        Ok(_) => {
                            view.load_directory(curr, cx);
                        }
                        Err(e) => {
                            view.error_msg = Some(format!("创建失败: {}", e));
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }

    pub fn delete_file(&mut self, file: RemoteFileItem, cx: &mut Context<Self>) {
        if self.confirm_delete {
            self.modal = ActiveModal::ConfirmDelete { file };
            cx.notify();
            return;
        }
        self.perform_delete(file, cx);
    }

    fn perform_delete(&mut self, file: RemoteFileItem, cx: &mut Context<Self>) {
        if let Some(sftp) = &self.sftp {
            let sftp = Arc::clone(sftp);
            let path = file.path.clone();
            let is_dir = file.is_dir;
            let current = self.current_path.clone();

            cx.spawn(async move |this, cx| {
                let res = SftpManager::remove_entry(&sftp, &path, is_dir).await;
                let _ = this.update(cx, |view, cx| {
                    view.modal = ActiveModal::None;
                    if let Err(e) = res {
                        view.error_msg = Some(format!("删除失败: {}", e));
                    } else {
                        view.load_directory(current, cx);
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }

    pub fn apply_chmod(&mut self, path: String, mode: u32, cx: &mut Context<Self>) {
        if let Some(sftp) = &self.sftp {
            let sftp = Arc::clone(sftp);
            let current = self.current_path.clone();

            cx.spawn(async move |this, cx| {
                let res = SftpManager::chmod(&sftp, &path, mode).await;
                let _ = this.update(cx, |view, cx| {
                    view.modal = ActiveModal::None;
                    if let Err(e) = res {
                        view.error_msg = Some(format!("修改权限失败: {}", e));
                    } else {
                        view.load_directory(current, cx);
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }

    pub fn format_size(bytes: u64) -> String {
        if bytes < 1024 {
            format!("{} B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else if bytes < 1024 * 1024 * 1024 {
            format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    }

    pub fn format_mode(mode: u32) -> String {
        format!("{:04o}", mode & 0o777)
    }

    pub fn format_time(ts: Option<u64>) -> String {
        let Some(secs) = ts else {
            return "-".to_string();
        };
        let sec = (secs % 60) as u32;
        let min = ((secs / 60) % 60) as u32;
        let hour = ((secs / 3600) % 24) as u32;
        let mut days = (secs / 86400) as i64;

        days += 719468;
        let era = if days >= 0 { days } else { days - 146096 } / 146097;
        let doe = (days - era * 146097) as u32;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe as i64 + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };

        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            y, m, d, hour, min, sec
        )
    }

    pub fn file_icon_and_color(item: &RemoteFileItem) -> (Icon, Rgba) {
        match item.category() {
            redash_core::sftp::FileCategory::Directory => (Icon::folder(), rgb(0x89b4fa)),
            redash_core::sftp::FileCategory::Symlink => (Icon::link(), rgb(0xb4befe)),
            redash_core::sftp::FileCategory::Log => (Icon::file(), rgb(0xf9e2af)),
            redash_core::sftp::FileCategory::Config => (Icon::settings(), rgb(0x94e2d5)),
            redash_core::sftp::FileCategory::ScriptOrBinary => (Icon::zap(), rgb(0xa6e3a1)),
            redash_core::sftp::FileCategory::Archive => (Icon::archive(), rgb(0xcba6f7)),
            redash_core::sftp::FileCategory::Code => (Icon::terminal(), rgb(0x89dceb)),
            redash_core::sftp::FileCategory::Document => (Icon::file(), rgb(0xcdd6f4)),
        }
    }
}

impl Render for SftpView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_path = self.current_path.clone();
        let total_items_count = self.filtered_items().len();
        let total_p = self.total_pages();
        let cur_page = self.current_page.max(1).min(total_p);
        let page_size = self.page_size;
        let filter_query = self.filter_query.clone();
        let paged_count = self.paged_items().len();

        div()
            .track_focus(&self.focus_handle)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, _cx| {
                window.focus(&this.focus_handle);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                let key = event.keystroke.key.as_str();
                let modifiers = &event.keystroke.modifiers;

                if key == "escape"
                    && this.modal != ActiveModal::None {
                        this.modal = ActiveModal::None;
                        cx.notify();
                        return;
                    }

                match &mut this.modal {
                    ActiveModal::Editor { .. } => {
                        if (modifiers.platform || modifiers.control) && key == "s" {
                            this.save_editor(cx);
                        } else if (modifiers.platform || modifiers.control) && key == "a" {
                            if let ActiveModal::Editor { lines, cursor_row, cursor_col, .. } = &mut this.modal {
                                if *cursor_row == 0 && *cursor_col == 0 {
                                    *cursor_row = lines.len().saturating_sub(1);
                                    *cursor_col = lines.get(*cursor_row).map(|l| l.chars().count()).unwrap_or(0);
                                } else {
                                    *cursor_row = 0;
                                    *cursor_col = 0;
                                }
                            }
                            cx.notify();
                        } else if (modifiers.platform || modifiers.control) && key == "v" {
                            if let Some(item) = cx.read_from_clipboard()
                                && let Some(text) = item.text() {
                                    this.editor_insert_text(&text);
                                    cx.notify();
                                }
                        } else if key == "enter" {
                            this.editor_newline();
                            cx.notify();
                        } else if key == "backspace" {
                            this.editor_backspace();
                            cx.notify();
                        } else if key == "delete" {
                            this.editor_delete();
                            cx.notify();
                        } else if key == "tab" {
                            this.editor_insert_text("    ");
                            cx.notify();
                        } else if matches!(key, "up" | "down" | "left" | "right" | "home" | "end" | "pageup" | "pagedown") {
                            this.editor_move_cursor(key);
                            cx.notify();
                        } else if !modifiers.control && !modifiers.platform {
                            if let Some(ref kc) = event.keystroke.key_char {
                                if !kc.is_empty() && kc != "\n" && kc != "\r" && kc != "\t" {
                                    this.editor_insert_text(kc);
                                    cx.notify();
                                }
                            } else if key == "space" {
                                this.editor_insert_char(' ');
                                cx.notify();
                            } else if key.chars().count() == 1 {
                                let ch = key.chars().next().unwrap();
                                this.editor_insert_char(ch);
                                cx.notify();
                            }
                        }
                    }
                    ActiveModal::RenameDialog { .. } => {
                        if key == "enter" {
                            this.apply_rename(cx);
                        } else if key == "backspace" {
                            this.rename_backspace();
                            cx.notify();
                        } else if key == "delete" {
                            this.rename_delete();
                            cx.notify();
                        } else if (modifiers.platform || modifiers.control) && key == "v" {
                            if let Some(item) = cx.read_from_clipboard()
                                && let Some(text) = item.text() {
                                    for ch in text.chars().filter(|c| *c != '\n' && *c != '\r') {
                                        this.rename_input_char(ch);
                                    }
                                    cx.notify();
                                }
                        } else if key == "left" {
                            if let ActiveModal::RenameDialog { cursor_pos, .. } = &mut this.modal
                                && *cursor_pos > 0 { *cursor_pos -= 1; }
                            cx.notify();
                        } else if key == "right" {
                            if let ActiveModal::RenameDialog { new_name, cursor_pos, .. } = &mut this.modal
                                && *cursor_pos < new_name.chars().count() { *cursor_pos += 1; }
                            cx.notify();
                        } else if !modifiers.control && !modifiers.platform {
                            if let Some(ref kc) = event.keystroke.key_char {
                                if !kc.is_empty() && kc != "\n" && kc != "\r" && kc != "\t" {
                                    for ch in kc.chars() {
                                        this.rename_input_char(ch);
                                    }
                                    cx.notify();
                                }
                            } else if key == "space" {
                                this.rename_input_char(' ');
                                cx.notify();
                            } else if key.chars().count() == 1 {
                                let ch = key.chars().next().unwrap();
                                this.rename_input_char(ch);
                                cx.notify();
                            }
                        }
                    }
                    ActiveModal::NewItemDialog { is_dir, name, .. } => {
                        let is_dir_val = *is_dir;
                        let name_val = name.clone();
                        if key == "enter" {
                            this.create_new_item(is_dir_val, &name_val, cx);
                        } else if key == "backspace" {
                            this.new_item_backspace();
                            cx.notify();
                        } else if key == "delete" {
                            this.new_item_delete();
                            cx.notify();
                        } else if (modifiers.platform || modifiers.control) && key == "v" {
                            if let Some(item) = cx.read_from_clipboard()
                                && let Some(text) = item.text() {
                                    for ch in text.chars().filter(|c| *c != '\n' && *c != '\r') {
                                        this.new_item_input_char(ch);
                                    }
                                    cx.notify();
                                }
                        } else if key == "left" {
                            if let ActiveModal::NewItemDialog { cursor_pos, .. } = &mut this.modal
                                && *cursor_pos > 0 { *cursor_pos -= 1; }
                            cx.notify();
                        } else if key == "right" {
                            if let ActiveModal::NewItemDialog { name, cursor_pos, .. } = &mut this.modal
                                && *cursor_pos < name.chars().count() { *cursor_pos += 1; }
                            cx.notify();
                        } else if !modifiers.control && !modifiers.platform {
                            if let Some(ref kc) = event.keystroke.key_char {
                                if !kc.is_empty() && kc != "\n" && kc != "\r" && kc != "\t" {
                                    for ch in kc.chars() {
                                        this.new_item_input_char(ch);
                                    }
                                    cx.notify();
                                }
                            } else if key == "space" {
                                this.new_item_input_char(' ');
                                cx.notify();
                            } else if key.chars().count() == 1 {
                                let ch = key.chars().next().unwrap();
                                this.new_item_input_char(ch);
                                cx.notify();
                            }
                        }
                    }
                    ActiveModal::ChmodDialog { file_path, input_mode, .. } => {
                        let path_val = file_path.clone();
                        let mode_val = input_mode.clone();
                        if key == "enter" {
                            if let Ok(mode) = u32::from_str_radix(&mode_val, 8) {
                                this.apply_chmod(path_val, mode, cx);
                            }
                        } else if key == "backspace" {
                            this.chmod_backspace();
                            cx.notify();
                        } else if !modifiers.control && !modifiers.platform && key.len() == 1 {
                            let ch = key.chars().next().unwrap();
                            this.chmod_input_char(ch);
                            cx.notify();
                        }
                    }
                    _ => {}
                }
            }))
            .size_full()
            .bg(rgb(0x0a0b10))
            .flex()
            .flex_col()
            .relative()
            .overflow_hidden()
            // 1. Navigation and Toolbar Header
            .child(
                div()
                    .h(px(40.0))
                    .flex_shrink_0()
                    .w_full()
                    .bg(rgb(0x12131a))
                    .border_b_1()
                    .border_color(rgb(0x232738))
                    .px_3()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    // Breadcrumb Trail
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .id("crumb_root")
                                    .px_2()
                                    .py_0p5()
                                    .bg(rgb(0x181926))
                                    .rounded_sm()
                                    .text_color(rgb(0x38bdf8))
                                    .text_size(px(12.0))
                                    .font_family("Menlo")
                                    .cursor_pointer()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.load_directory("/".into(), cx);
                                    }))
                                    .child(Icon::folder().with_size(px(12.0)).with_color(rgb(0x38bdf8)))
                                    .child(" /")
                            )
                            .children({
                                let parts: Vec<&str> = current_path.split('/').filter(|s| !s.is_empty()).collect();
                                parts.into_iter().enumerate().map(|(idx, seg)| {
                                    let mut full_path = String::new();
                                    let parts_sub: Vec<&str> = current_path.split('/').filter(|s| !s.is_empty()).take(idx + 1).collect();
                                    full_path.push('/');
                                    full_path.push_str(&parts_sub.join("/"));

                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_1()
                                        .child(div().text_color(rgb(0x45475a)).child("/"))
                                        .child(
                                            div()
                                                .id(ElementId::NamedInteger("crumb_part".into(), idx as u64))
                                                .px_1p5()
                                                .py_0p5()
                                                .bg(rgb(0x181926))
                                                .rounded_sm()
                                                .text_color(rgb(0x89b4fa))
                                                .text_size(px(12.0))
                                                .font_family("Menlo")
                                                .cursor_pointer()
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.load_directory(full_path.clone(), cx);
                                                }))
                                                .child(seg.to_string())
                                        )
                                }).collect::<Vec<_>>()
                            })
                    )
                    // Toolbar Buttons
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(
                                div()
                                    .id("btn_new_file")
                                    .px_2p5()
                                    .py_1()
                                    .bg(rgb(0x1e1e2e))
                                    .border_1()
                                    .border_color(rgb(0x313244))
                                    .hover(|s| s.bg(rgb(0x313244)).border_color(rgb(0x38bdf8)))
                                    .rounded_md()
                                    .text_size(px(11.0))
                                    .text_color(rgb(0xcdd6f4))
                                    .cursor_pointer()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.modal = ActiveModal::NewItemDialog {
                                            is_dir: false,
                                            name: "new_file.txt".into(),
                                            cursor_pos: 12,
                                        };
                                        cx.notify();
                                    }))
                                    .child(Icon::file().with_size(px(11.0)).with_color(rgb(0x38bdf8)))
                                    .child(crate::t!("sftp.new_file"))
                            )
                            .child(
                                div()
                                    .id("btn_new_dir")
                                    .px_2p5()
                                    .py_1()
                                    .bg(rgb(0x1e1e2e))
                                    .border_1()
                                    .border_color(rgb(0x313244))
                                    .hover(|s| s.bg(rgb(0x313244)).border_color(rgb(0x38bdf8)))
                                    .rounded_md()
                                    .text_size(px(11.0))
                                    .text_color(rgb(0xcdd6f4))
                                    .cursor_pointer()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.modal = ActiveModal::NewItemDialog {
                                            is_dir: true,
                                            name: "new_folder".into(),
                                            cursor_pos: 10,
                                        };
                                        cx.notify();
                                    }))
                                    .child(Icon::folder().with_size(px(11.0)).with_color(rgb(0x38bdf8)))
                                    .child(crate::t!("sftp.new_folder"))
                            )
                            .child(
                                div()
                                    .id("btn_parent_dir")
                                    .px_2p5()
                                    .py_1()
                                    .bg(rgb(0x1e1e2e))
                                    .border_1()
                                    .border_color(rgb(0x313244))
                                    .hover(|s| s.bg(rgb(0x313244)))
                                    .rounded_md()
                                    .text_size(px(11.0))
                                    .text_color(rgb(0xcdd6f4))
                                    .cursor_pointer()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .on_click(cx.listener(move |this, _event: &ClickEvent, _window, cx| {
                                        let mut parts: Vec<&str> = current_path.split('/').filter(|s| !s.is_empty()).collect();
                                        if !parts.is_empty() {
                                            parts.pop();
                                            let parent = if parts.is_empty() { "/".to_string() } else { format!("/{}", parts.join("/")) };
                                            this.load_directory(parent, cx);
                                        }
                                    }))
                                    .child(Icon::arrow_up().with_size(px(11.0)).with_color(rgb(0xcdd6f4)))
                                    .child(crate::t!("sftp.parent_dir"))
                            )
                            .child(
                                div()
                                    .id("btn_home_dir")
                                    .px_2p5()
                                    .py_1()
                                    .bg(rgb(0x1e1e2e))
                                    .border_1()
                                    .border_color(rgb(0x313244))
                                    .hover(|s| s.bg(rgb(0x313244)).border_color(DarkTechTheme::accent_cyan()))
                                    .rounded_md()
                                    .text_size(px(11.0))
                                    .text_color(rgb(0xcdd6f4))
                                    .cursor_pointer()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                                        this.go_home_dir(cx);
                                    }))
                                    .child(Icon::server().with_size(px(11.0)).with_color(DarkTechTheme::accent_cyan()))
                                    .child(crate::t!("sftp.home_dir"))
                            )
                            .child(
                                div()
                                    .id("btn_toggle_hidden")
                                    .px_2p5()
                                    .py_1()
                                    .bg(if self.show_hidden { rgb(0x232738) } else { rgb(0x1e1e2e) })
                                    .border_1()
                                    .border_color(if self.show_hidden { rgb(0x38bdf8) } else { rgb(0x313244) })
                                    .hover(|s| s.bg(rgb(0x313244)))
                                    .rounded_md()
                                    .text_size(px(11.0))
                                    .text_color(if self.show_hidden { rgb(0x38bdf8) } else { rgb(0xa6adc8) })
                                    .cursor_pointer()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                                        this.show_hidden = !this.show_hidden;
                                        this.current_page = 1;
                                        cx.notify();
                                    }))
                                    .child(Icon::terminal().with_size(px(11.0)).with_color(if self.show_hidden { rgb(0x38bdf8) } else { rgb(0xa6adc8) }))
                                    .child(if self.show_hidden { "隐藏文件: 开" } else { "隐藏文件: 关" })
                            )
                            .child(
                                div()
                                    .id("btn_refresh")
                                    .px_2p5()
                                    .py_1()
                                    .bg(rgb(0x1e1e2e))
                                    .border_1()
                                    .border_color(rgb(0x313244))
                                    .hover(|s| s.bg(rgb(0x313244)))
                                    .rounded_md()
                                    .text_size(px(11.0))
                                    .text_color(rgb(0xcdd6f4))
                                    .cursor_pointer()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                                        let path = this.current_path.clone();
                                        this.load_directory(path, cx);
                                    }))
                                    .child(Icon::refresh().with_size(px(11.0)).with_color(rgb(0xcdd6f4)))
                                    .child(crate::t!("common.refresh"))
                            )
                    )
            )
            // Error banner if any
            .children(if let Some(err) = &self.error_msg {
                vec![
                    div()
                        .w_full()
                        .px_3()
                        .py_2()
                        .bg(rgba(0xf38ba822))
                        .border_b_1()
                        .border_color(rgb(0xf38ba8))
                        .text_color(rgb(0xf38ba8))
                        .text_size(px(12.0))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(Icon::close().with_size(px(12.0)).with_color(rgb(0xf38ba8)))
                        .child(format!("错误: {}", err))
                ]
            } else {
                vec![]
            })
            // File List Column Header
            .child(
                div()
                    .h(px(26.0))
                    .flex_shrink_0()
                    .w_full()
                    .bg(rgb(0x12131a))
                    .border_b_1()
                    .border_color(rgb(0x232738))
                    .px_3()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(0x6c7086))
                            .font_weight(FontWeight::BOLD)
                            .child(crate::t!("sftp.col_name"))
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_4()
                            .child(
                                div()
                                    .w(px(50.0))
                                    .text_size(px(11.0))
                                    .text_color(rgb(0x6c7086))
                                    .font_weight(FontWeight::BOLD)
                                    .child(crate::t!("sftp.col_perm"))
                            )
                            .child(
                                div()
                                    .w(px(130.0))
                                    .text_size(px(11.0))
                                    .text_color(rgb(0x6c7086))
                                    .font_weight(FontWeight::BOLD)
                                    .child(crate::t!("sftp.col_modified"))
                            )
                            .child(
                                div()
                                    .w(px(70.0))
                                    .text_size(px(11.0))
                                    .text_color(rgb(0x6c7086))
                                    .font_weight(FontWeight::BOLD)
                                    .child(crate::t!("sftp.col_size"))
                            )
                            .child(
                                div()
                                    .w(px(50.0))
                                    .text_size(px(11.0))
                                    .text_color(rgb(0x6c7086))
                                    .font_weight(FontWeight::BOLD)
                                    .child(crate::t!("sftp.col_actions"))
                            )
                    )
            )
            // 2. Virtualized File List View (UniformList) or Loading/Empty/Error state
            .child(
                div()
                    .id("sftp_file_list_container")
                    .flex_1()
                    .w_full()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .child(if self.is_loading {
                        div()
                            .size_full()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap_3()
                            .child(
                                Icon::refresh()
                                    .with_size(px(28.0))
                                    .with_color(DarkTechTheme::accent_cyan()),
                            )
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(DarkTechTheme::text_primary())
                                    .child(self.loading_msg.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_family("Menlo")
                                    .text_color(DarkTechTheme::text_muted())
                                    .child(self.current_path.clone()),
                            )
                            .into_any_element()
                    } else if let Some(ref err) = self.error_msg {
                        div()
                            .size_full()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap_3()
                            .px_6()
                            .child(
                                Icon::close()
                                    .with_size(px(32.0))
                                    .with_color(DarkTechTheme::status_offline()),
                            )
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(DarkTechTheme::status_offline())
                                    .child("远程目录访问失败"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_family("Menlo")
                                    .text_color(DarkTechTheme::text_secondary())
                                    .child(err.clone()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_3()
                                    .mt_2()
                                    .child(
                                        div()
                                            .id("btn_err_retry")
                                            .px_3()
                                            .py_1p5()
                                            .rounded_md()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_1()
                                            .border_color(DarkTechTheme::border_active())
                                            .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()))
                                            .text_size(px(12.0))
                                            .text_color(DarkTechTheme::text_accent())
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                let path = this.current_path.clone();
                                                this.load_directory(path, cx);
                                            }))
                                            .child("🔄 重新尝试读取"),
                                    )
                                    .child(
                                        div()
                                            .id("btn_err_home")
                                            .px_3()
                                            .py_1p5()
                                            .rounded_md()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_1()
                                            .border_color(DarkTechTheme::border_default())
                                            .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()))
                                            .text_size(px(12.0))
                                            .text_color(DarkTechTheme::text_primary())
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.go_home_dir(cx);
                                            }))
                                            .child("🏠 返回用户主目录"),
                                    )
                                    .child(
                                        div()
                                            .id("btn_err_root")
                                            .px_3()
                                            .py_1p5()
                                            .rounded_md()
                                            .bg(DarkTechTheme::bg_input())
                                            .border_1()
                                            .border_color(DarkTechTheme::border_default())
                                            .hover(|s| s.bg(DarkTechTheme::bg_panel_hover()))
                                            .text_size(px(12.0))
                                            .text_color(DarkTechTheme::text_primary())
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.load_directory("/".to_string(), cx);
                                            }))
                                            .child("📁 前往根目录 (/)"),
                                    ),
                            )
                            .into_any_element()
                    } else if paged_count == 0 {
                        div()
                            .size_full()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .child(
                                Icon::folder()
                                    .with_size(px(32.0))
                                    .with_color(DarkTechTheme::text_muted()),
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(DarkTechTheme::text_secondary())
                                    .child(crate::t!("sftp.empty_dir")),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .font_family("Menlo")
                                    .text_color(DarkTechTheme::text_muted())
                                    .child(self.current_path.clone()),
                            )
                            .into_any_element()
                    } else {
                        let paged = self.paged_items();
                        div()
                            .id("sftp_file_rows")
                            .track_scroll(&self.scroll_handle)
                            .flex_1()
                            .w_full()
                            .min_h(px(0.0))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .children(
                                paged.into_iter().enumerate().map(|(idx, item)| {
                                    let item_for_click = item.clone();
                                    let item_for_context = item.clone();
                                    let is_dir = item.is_dir;
                                    let (icon, icon_color) = Self::file_icon_and_color(&item);

                                    div()
                                        .id(ElementId::NamedInteger("sftp_row".into(), idx as u64))
                                        .h(px(32.0))
                                        .w_full()
                                        .px_3()
                                        .border_b_1()
                                        .border_color(rgb(0x181926))
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .justify_between()
                                        .hover(|s| s.bg(rgb(0x181926)))
                                        .child(
                                            div()
                                                .id(ElementId::NamedInteger("sftp_row_name".into(), idx as u64))
                                                .flex_1()
                                                .min_w(px(0.0))
                                                .overflow_hidden()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_2()
                                                .cursor_pointer()
                                                .on_click(cx.listener(move |this, _event: &ClickEvent, _window, cx| {
                                                    if is_dir {
                                                        this.load_directory(item_for_click.path.clone(), cx);
                                                    } else {
                                                        this.open_editor(item_for_click.clone(), cx);
                                                    }
                                                }))
                                                .child(
                                                    icon.with_size(px(14.0)).with_color(icon_color)
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(12.0))
                                                        .font_family("Menlo")
                                                        .truncate()
                                                        .text_color(if is_dir { rgb(0x89b4fa) } else { rgb(0xcdd6f4) })
                                                        .child(item.name.clone())
                                                )
                                        )
                                        .child(
                                            div()
                                                .flex_shrink_0()
                                                .flex()
                                                .flex_row()
                                                .items_center()
                                                .gap_4()
                                                .child(
                                                    div()
                                                        .w(px(50.0))
                                                        .text_size(px(11.0))
                                                        .font_family("Menlo")
                                                        .text_color(rgb(0x6c7086))
                                                        .child(Self::format_mode(item.permissions))
                                                )
                                                .child(
                                                    div()
                                                        .w(px(130.0))
                                                        .text_size(px(11.0))
                                                        .font_family("Menlo")
                                                        .text_color(rgb(0x6c7086))
                                                        .child(Self::format_time(item.modified))
                                                )
                                                .child(
                                                    div()
                                                        .w(px(70.0))
                                                        .text_size(px(11.0))
                                                        .font_family("Menlo")
                                                        .text_color(rgb(0xa6adc8))
                                                        .child(if is_dir { "-".to_string() } else { Self::format_size(item.size) })
                                                )
                                                .child(
                                                    div()
                                                        .id(ElementId::NamedInteger("sftp_btn_ops".into(), idx as u64))
                                                        .w(px(50.0))
                                                        .py_0p5()
                                                        .bg(rgb(0x232738))
                                                        .hover(|s| s.bg(rgb(0x313244)))
                                                        .rounded_md()
                                                        .text_size(px(10.0))
                                                        .text_color(rgb(0xcdd6f4))
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .cursor_pointer()
                                                        .on_click(cx.listener(move |this, _event: &ClickEvent, _window, cx| {
                                                            this.modal = ActiveModal::ContextMenu {
                                                                file: item_for_context.clone(),
                                                                x: px(100.0),
                                                                y: px(100.0),
                                                            };
                                                            cx.notify();
                                                        }))
                                                        .child(crate::t!("sftp.col_actions"))
                                                )
                                        )
                                }).collect::<Vec<_>>()
                            )
                            .into_any_element()
                    })
            )
            // 3. Pagination & Status Footer
            .child(
                div()
                    .h(px(32.0))
                    .flex_shrink_0()
                    .w_full()
                    .bg(rgb(0x12131a))
                    .border_t_1()
                    .border_color(rgb(0x232738))
                    .px_3()
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
                            .text_size(px(11.0))
                            .font_family("Menlo")
                            .text_color(rgb(0xa6adc8))
                            .child(format!("共 {} 项", total_items_count))
                            .children(if !filter_query.is_empty() {
                                vec![
                                    div()
                                        .px_1p5()
                                        .py_0p5()
                                        .bg(rgb(0x1e1e2e))
                                        .rounded_sm()
                                        .text_color(rgb(0xf9e2af))
                                        .child(format!("过滤: \"{}\"", filter_query))
                                ]
                            } else {
                                vec![]
                            })
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .id("btn_sftp_prev_page")
                                    .px_2()
                                    .py_0p5()
                                    .bg(if cur_page > 1 { rgb(0x1e1e2e) } else { rgb(0x11111b) })
                                    .border_1()
                                    .border_color(if cur_page > 1 { rgb(0x313244) } else { rgb(0x181926) })
                                    .hover(|s| if cur_page > 1 { s.bg(rgb(0x313244)) } else { s })
                                    .rounded_sm()
                                    .text_size(px(11.0))
                                    .text_color(if cur_page > 1 { rgb(0xcdd6f4) } else { rgb(0x45475a) })
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.prev_page(cx);
                                    }))
                                    .child("‹ 上一页")
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .font_family("Menlo")
                                    .text_color(rgb(0xcdd6f4))
                                    .child(format!("第 {} / {} 页", cur_page, total_p))
                            )
                            .child(
                                div()
                                    .id("btn_sftp_next_page")
                                    .px_2()
                                    .py_0p5()
                                    .bg(if cur_page < total_p { rgb(0x1e1e2e) } else { rgb(0x11111b) })
                                    .border_1()
                                    .border_color(if cur_page < total_p { rgb(0x313244) } else { rgb(0x181926) })
                                    .hover(|s| if cur_page < total_p { s.bg(rgb(0x313244)) } else { s })
                                    .rounded_sm()
                                    .text_size(px(11.0))
                                    .text_color(if cur_page < total_p { rgb(0xcdd6f4) } else { rgb(0x45475a) })
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.next_page(cx);
                                    }))
                                    .child("下一页 ›")
                            )
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(rgb(0x6c7086))
                                    .child("每页:")
                            )
                            .children(vec![50, 100, 200, 500].into_iter().map(|ps| {
                                let is_selected = page_size == ps;
                                div()
                                    .id(ElementId::NamedInteger("btn_page_size".into(), ps as u64))
                                    .px_1p5()
                                    .py_0p5()
                                    .bg(if is_selected { rgb(0x38bdf8) } else { rgb(0x181926) })
                                    .rounded_sm()
                                    .text_size(px(10.0))
                                    .font_family("Menlo")
                                    .text_color(if is_selected { rgb(0x0a0b10) } else { rgb(0xa6adc8) })
                                    .font_weight(if is_selected { FontWeight::BOLD } else { FontWeight::NORMAL })
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.set_page_size(ps, cx);
                                    }))
                                    .child(ps.to_string())
                            }))
                    )
            )
            // 3. Modals: Wrapped in Backdrop Scrim for Click-Outside Dismissal
            .children(match &self.modal {
                ActiveModal::None => vec![],
                ActiveModal::ConfirmDelete { file } => {
                    let file = file.clone();
                    vec![div().id("confirm_delete_backdrop").absolute().inset_0().bg(rgba(0x000000aa)).flex().items_center().justify_center()
                        .child(div().p_4().rounded_lg().bg(DarkTechTheme::bg_panel()).flex().flex_col().gap_3()
                            .child(format!("确认删除 {}？", file.path))
                            .child(div().id("confirm_sftp_delete").cursor_pointer().text_color(DarkTechTheme::status_warn()).child("删除")
                                .on_click(cx.listener(move |this, _, _, cx| this.perform_delete(file.clone(), cx))))
                            .child(div().id("cancel_sftp_delete").cursor_pointer().child("取消")
                                .on_click(cx.listener(|this, _, _, cx| { this.modal = ActiveModal::None; cx.notify(); }))))
                        ]
                },
                ActiveModal::ContextMenu { file, .. } => {
                    let file_preview = file.clone();
                    let file_edit = file.clone();
                    let file_rename = file.clone();
                    let file_delete = file.clone();
                    let file_chmod = file.clone();

                    vec![
                        div()
                            .id("modal_backdrop_ctx")
                            .absolute()
                            .inset_0()
                            .bg(rgba(0x00000066))
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                this.modal = ActiveModal::None;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .id("ctx_menu_box")
                                    .absolute()
                                    .top(px(50.0))
                                    .right(px(30.0))
                                    .w(px(230.0))
                                    .bg(rgb(0x12131a))
                                    .border_1()
                                    .border_color(rgb(0x38bdf8))
                                    .rounded_lg()
                                    .p_2()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .font_family("Menlo")
                                            .text_color(rgb(0x89b4fa))
                                            .pb_1()
                                            .border_b_1()
                                            .border_color(rgb(0x232738))
                                            .child(format!("目标: {}", file.name))
                                    )
                                    .child(
                                        div()
                                            .id("ctx_btn_preview")
                                            .px_2()
                                            .py_1()
                                            .rounded_md()
                                            .hover(|s| s.bg(rgb(0x1e1e2e)))
                                            .cursor_pointer()
                                            .text_size(px(12.0))
                                            .text_color(rgb(0xcdd6f4))
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .on_click(cx.listener(move |this, _e: &ClickEvent, _window, cx| {
                                                this.open_preview(file_preview.clone(), cx);
                                            }))
                                            .child(Icon::search().with_size(px(12.0)).with_color(rgb(0xcdd6f4)))
                                            .child(crate::t!("sftp.preview_title"))
                                    )
                                    .child(
                                        div()
                                            .id("ctx_btn_edit")
                                            .px_2()
                                            .py_1()
                                            .rounded_md()
                                            .hover(|s| s.bg(rgb(0x1e1e2e)))
                                            .cursor_pointer()
                                            .text_size(px(12.0))
                                            .text_color(rgb(0xa6e3a1))
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .on_click(cx.listener(move |this, _e: &ClickEvent, _window, cx| {
                                                this.open_editor(file_edit.clone(), cx);
                                            }))
                                            .child(Icon::edit().with_size(px(12.0)).with_color(rgb(0xa6e3a1)))
                                            .child("极客代码编辑器 (Cmd+S)")
                                    )
                                    .child(
                                        div()
                                            .id("ctx_btn_rename")
                                            .px_2()
                                            .py_1()
                                            .rounded_md()
                                            .hover(|s| s.bg(rgb(0x1e1e2e)))
                                            .cursor_pointer()
                                            .text_size(px(12.0))
                                            .text_color(rgb(0x38bdf8))
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .on_click(cx.listener(move |this, _e: &ClickEvent, _window, cx| {
                                                let name = file_rename.name.clone();
                                                let pos = name.chars().count();
                                                this.modal = ActiveModal::RenameDialog {
                                                    file_path: file_rename.path.clone(),
                                                    old_name: name.clone(),
                                                    new_name: name,
                                                    cursor_pos: pos,
                                                };
                                                cx.notify();
                                            }))
                                            .child(Icon::edit().with_size(px(12.0)).with_color(rgb(0x38bdf8)))
                                            .child(crate::t!("sftp.rename"))
                                    )
                                    .child(
                                        div()
                                            .id("ctx_btn_chmod")
                                            .px_2()
                                            .py_1()
                                            .rounded_md()
                                            .hover(|s| s.bg(rgb(0x1e1e2e)))
                                            .cursor_pointer()
                                            .text_size(px(12.0))
                                            .text_color(rgb(0xcba6f7))
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .on_click(cx.listener(move |this, _e: &ClickEvent, _window, cx| {
                                                let mode_oct = format!("{:04o}", file_chmod.permissions & 0o777);
                                                let pos = mode_oct.len();
                                                this.modal = ActiveModal::ChmodDialog {
                                                    file_path: file_chmod.path.clone(),
                                                    current_mode: file_chmod.permissions,
                                                    input_mode: mode_oct,
                                                    cursor_pos: pos,
                                                };
                                                cx.notify();
                                            }))
                                            .child(Icon::shield().with_size(px(12.0)).with_color(rgb(0xcba6f7)))
                                            .child(crate::t!("sftp.chmod"))
                                    )
                                    .child(
                                        div()
                                            .id("ctx_btn_delete")
                                            .px_2()
                                            .py_1()
                                            .rounded_md()
                                            .hover(|s| s.bg(rgba(0xf38ba822)))
                                            .text_color(rgb(0xf38ba8))
                                            .cursor_pointer()
                                            .text_size(px(12.0))
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .on_click(cx.listener(move |this, _e: &ClickEvent, _window, cx| {
                                                this.delete_file(file_delete.clone(), cx);
                                            }))
                                            .child(Icon::trash().with_size(px(12.0)).with_color(rgb(0xf38ba8)))
                                            .child(crate::t!("common.delete"))
                                    )
                                    .child(
                                        div()
                                            .id("ctx_btn_close")
                                            .px_2()
                                            .py_1()
                                            .rounded_md()
                                            .hover(|s| s.bg(rgb(0x1e1e2e)))
                                            .text_color(rgb(0x6c7086))
                                            .cursor_pointer()
                                            .text_size(px(11.0))
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .on_click(cx.listener(|this, _e: &ClickEvent, _window, cx| {
                                                this.modal = ActiveModal::None;
                                                cx.notify();
                                            }))
                                            .child(Icon::close().with_size(px(10.0)).with_color(rgb(0x6c7086)))
                                            .child("关闭菜单 (Esc)")
                                    )
                            )
                    ]
                }
                ActiveModal::Preview { file_name, content } => {
                    let preview_name = file_name.clone();
                    let preview_content = content.clone();

                    vec![
                        div()
                            .id("modal_backdrop_preview")
                            .absolute()
                            .inset_0()
                            .bg(rgba(0x00000088))
                            .flex()
                            .items_center()
                            .justify_center()
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                this.modal = ActiveModal::None;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .id("preview_box")
                                    .w(px(720.0))
                                    .h(px(520.0))
                                    .bg(rgb(0x12131a))
                                    .border_1()
                                    .border_color(rgb(0x38bdf8))
                                    .rounded_lg()
                                    .flex()
                                    .flex_col()
                                    .p_4()
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .child(
                                        div()
                                            .w_full()
                                            .flex()
                                            .flex_row()
                                            .justify_between()
                                            .items_center()
                                            .pb_3()
                                            .border_b_1()
                                            .border_color(rgb(0x232738))
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(Icon::search().with_size(px(14.0)).with_color(rgb(0x38bdf8)))
                                                    .child(
                                                        div()
                                                            .text_size(px(14.0))
                                                            .font_weight(FontWeight::BOLD)
                                                            .text_color(rgb(0x38bdf8))
                                                            .child(format!("只读安全分块预览: {}", preview_name))
                                                    )
                                            )
                                            .child(
                                                div()
                                                    .id("btn_close_preview")
                                                    .px_2()
                                                    .py_1()
                                                    .bg(rgb(0x232738))
                                                    .hover(|s| s.bg(rgb(0x313244)))
                                                    .rounded_md()
                                                    .cursor_pointer()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_1p5()
                                                    .on_click(cx.listener(|this, _e: &ClickEvent, _window, cx| {
                                                        this.modal = ActiveModal::None;
                                                        cx.notify();
                                                    }))
                                                    .child(Icon::close().with_size(px(11.0)).with_color(rgb(0xcdd6f4)))
                                                    .child(crate::t!("common.close"))
                                            )
                                    )
                                    .child(
                                        div()
                                            .id("preview_content_area")
                                            .flex_1()
                                            .w_full()
                                            .min_h(px(0.0))
                                            .overflow_y_scroll()
                                            .mt_3()
                                            .bg(rgb(0x0a0b10))
                                            .border_1()
                                            .border_color(rgb(0x232738))
                                            .p_3()
                                            .rounded_md()
                                            .font_family("Menlo")
                                            .text_size(px(11.0))
                                            .text_color(rgb(0xcdd6f4))
                                            .child(preview_content)
                                    )
                            )
                    ]
                }
                ActiveModal::Editor { file_name, file_path, lines, cursor_row, cursor_col, is_dirty, is_saving, status_msg, .. } => {
                    let name = file_name.clone();
                    let path = file_path.clone();
                    let status = status_msg.clone();
                    let saving = *is_saving;
                    let dirty = *is_dirty;
                    let cur_row = *cursor_row;
                    let cur_col = *cursor_col;
                    let total_lines = lines.len();

                    vec![
                        div()
                            .id("modal_backdrop_editor")
                            .absolute()
                            .inset_0()
                            .bg(rgba(0x000000aa))
                            .flex()
                            .items_center()
                            .justify_center()
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                this.modal = ActiveModal::None;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .id("editor_box")
                                    .w(px(880.0))
                                    .h(px(600.0))
                                    .bg(rgb(0x12131a))
                                    .border_1()
                                    .border_color(rgb(0x232738))
                                    .rounded_lg()
                                    .flex()
                                    .flex_col()
                                    .p_3()
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    // Header
                                    .child(
                                        div()
                                            .w_full()
                                            .flex()
                                            .flex_row()
                                            .justify_between()
                                            .items_center()
                                            .pb_2p5()
                                            .border_b_1()
                                            .border_color(rgb(0x232738))
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(Icon::terminal().with_size(px(14.0)).with_color(rgb(0xa6e3a1)))
                                                    .child(
                                                        div()
                                                            .text_size(px(14.0))
                                                            .font_weight(FontWeight::BOLD)
                                                            .text_color(rgb(0xa6e3a1))
                                                            .child("极客代码编辑器:")
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(13.0))
                                                            .font_family("Menlo")
                                                            .font_weight(FontWeight::BOLD)
                                                            .text_color(rgb(0xffffff))
                                                            .child(name)
                                                    )
                                                    .child(
                                                        if dirty {
                                                            div()
                                                                .px_1p5()
                                                                .py_0p5()
                                                                .bg(rgba(0xf9e2af22))
                                                                .rounded_sm()
                                                                .text_size(px(10.0))
                                                                .font_family("Menlo")
                                                                .text_color(rgb(0xf9e2af))
                                                                .child("● 未保存 (Cmd+S)")
                                                        } else {
                                                            div()
                                                                .px_1p5()
                                                                .py_0p5()
                                                                .bg(rgba(0xa6e3a122))
                                                                .rounded_sm()
                                                                .text_size(px(10.0))
                                                                .font_family("Menlo")
                                                                .text_color(rgb(0xa6e3a1))
                                                                .child("● 已同步")
                                                        }
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(11.0))
                                                            .font_family("Menlo")
                                                            .text_color(rgb(0x6c7086))
                                                            .child(format!("({})", path))
                                                    )
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .gap_2()
                                                    .child(
                                                        div()
                                                            .id("btn_save_editor")
                                                            .px_3()
                                                            .py_1()
                                                            .bg(if dirty { rgb(0xa6e3a1) } else { rgb(0x232738) })
                                                            .text_color(if dirty { rgb(0x0a0b10) } else { rgb(0xcdd6f4) })
                                                            .hover(|s| s.bg(rgb(0xa6e3a1)).text_color(rgb(0x0a0b10)))
                                                            .font_weight(FontWeight::BOLD)
                                                            .rounded_md()
                                                            .text_size(px(12.0))
                                                            .cursor_pointer()
                                                            .flex()
                                                            .flex_row()
                                                            .items_center()
                                                            .gap_1p5()
                                                            .on_click(cx.listener(|this, _e: &ClickEvent, _window, cx| {
                                                                this.save_editor(cx);
                                                            }))
                                                            .child(
                                                                if saving {
                                                                    Icon::refresh().with_size(px(12.0)).with_color(if dirty { rgb(0x0a0b10) } else { rgb(0xcdd6f4) })
                                                                } else {
                                                                    Icon::save().with_size(px(12.0)).with_color(if dirty { rgb(0x0a0b10) } else { rgb(0xcdd6f4) })
                                                                }
                                                            )
                                                            .child(if saving { "写入中..." } else { "原子保存 (Cmd+S)" })
                                                    )
                                                    .child(
                                                        div()
                                                            .id("btn_close_editor")
                                                            .px_2()
                                                            .py_1()
                                                            .bg(rgb(0x232738))
                                                            .hover(|s| s.bg(rgb(0x313244)))
                                                            .rounded_md()
                                                            .text_size(px(12.0))
                                                            .text_color(rgb(0x6c7086))
                                                            .cursor_pointer()
                                                            .flex()
                                                            .flex_row()
                                                            .items_center()
                                                            .gap_1p5()
                                                            .on_click(cx.listener(|this, _e: &ClickEvent, _window, cx| {
                                                                this.modal = ActiveModal::None;
                                                                cx.notify();
                                                            }))
                                                            .child(Icon::close().with_size(px(11.0)).with_color(rgb(0x6c7086)))
                                                            .child("关闭 (Esc)")
                                                    )
                                            )
                                    )
                                    // Main Code Area with Line Numbers Gutter
                                    .child(
                                        div()
                                            .id("editor_textarea_area")
                                            .flex_1()
                                            .w_full()
                                            .min_h(px(0.0))
                                            .mt_2()
                                            .bg(rgb(0x0a0b10))
                                            .border_1()
                                            .border_color(rgb(0x232738))
                                            .rounded_md()
                                            .overflow_hidden()
                                            .flex()
                                            .flex_col()
                                            .child(
                                                div()
                                                    .id("editor_scroll_container")
                                                    .flex_1()
                                                    .w_full()
                                                    .min_h(px(0.0))
                                                    .overflow_y_scroll()
                                                    .py_2()
                                                    .children(lines.iter().enumerate().map(|(idx, line)| {
                                                        let is_current = idx == cur_row;
                                                        let line_content = if is_current {
                                                            let chars: Vec<char> = line.chars().collect();
                                                            let col = cur_col.min(chars.len());
                                                            let before: String = chars[..col].iter().collect();

                                                            let (cursor_char, after): (Option<char>, String) = if col < chars.len() {
                                                                (Some(chars[col]), chars[col + 1..].iter().collect())
                                                            } else {
                                                                (None, String::new())
                                                            };

                                                            div()
                                                                .flex()
                                                                .flex_row()
                                                                .items_center()
                                                                .children(if !before.is_empty() {
                                                                    vec![div().child(before)]
                                                                } else {
                                                                    vec![]
                                                                })
                                                                .child(
                                                                    if let Some(ch) = cursor_char {
                                                                        div()
                                                                            .bg(rgb(0x38bdf8))
                                                                            .text_color(rgb(0x0a0b10))
                                                                            .font_weight(FontWeight::BOLD)
                                                                            .child(ch.to_string())
                                                                    } else {
                                                                        div()
                                                                            .w(px(7.0))
                                                                            .h(px(15.0))
                                                                            .bg(rgb(0x38bdf8))
                                                                    }
                                                                )
                                                                .children(if !after.is_empty() {
                                                                    vec![div().child(after)]
                                                                } else {
                                                                    vec![]
                                                                })
                                                        } else {
                                                            div().child(if line.is_empty() { " ".to_string() } else { line.clone() })
                                                        };

                                                        div()
                                                            .id(ElementId::NamedInteger("editor_row".into(), idx as u64))
                                                            .w_full()
                                                            .min_h(px(18.0))
                                                            .px_2()
                                                            .flex()
                                                            .flex_row()
                                                            .items_center()
                                                            .bg(if is_current { rgb(0x181926) } else { rgb(0x0a0b10) })
                                                            .hover(|s| s.bg(rgb(0x181926)))
                                                            .cursor_pointer()
                                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                                if let ActiveModal::Editor { cursor_row, cursor_col, lines, .. } = &mut this.modal {
                                                                    *cursor_row = idx;
                                                                    let len = lines.get(idx).map(|l| l.chars().count()).unwrap_or(0);
                                                                    *cursor_col = (*cursor_col).min(len);
                                                                    cx.notify();
                                                                }
                                                            }))
                                                            .child(
                                                                div()
                                                                    .w(px(40.0))
                                                                    .flex()
                                                                    .justify_end()
                                                                    .pr_2()
                                                                    .text_size(px(11.0))
                                                                    .font_family("Menlo")
                                                                    .text_color(if is_current { rgb(0x38bdf8) } else { rgb(0x45475a) })
                                                                    .child(format!("{}", idx + 1))
                                                            )
                                                            .child(
                                                                div()
                                                                    .w(px(1.0))
                                                                    .h(px(16.0))
                                                                    .bg(rgb(0x232738))
                                                                    .mr_2()
                                                            )
                                                            .child(
                                                                div()
                                                                    .flex_1()
                                                                    .font_family("Menlo")
                                                                    .text_size(px(12.0))
                                                                    .text_color(if is_current { rgb(0xffffff) } else { rgb(0xcdd6f4) })
                                                                    .child(line_content)
                                                            )
                                                    }))
                                            )
                                    )
                                    // Bottom Status Bar
                                    .child(
                                        div()
                                            .h(px(26.0))
                                            .w_full()
                                            .mt_2()
                                            .bg(rgb(0x12131a))
                                            .border_1()
                                            .border_color(rgb(0x232738))
                                            .rounded_md()
                                            .px_3()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .gap_3()
                                                    .child(
                                                        div()
                                                            .text_size(px(11.0))
                                                            .font_family("Menlo")
                                                            .text_color(rgb(0x38bdf8))
                                                            .child(format!("Ln {}, Col {}", cur_row + 1, cur_col + 1))
                                                    )
                                                    .child(div().text_color(rgb(0x45475a)).child("|"))
                                                    .child(
                                                        div()
                                                            .text_size(px(11.0))
                                                            .font_family("Menlo")
                                                            .text_color(rgb(0x6c7086))
                                                            .child(format!("{} 行", total_lines))
                                                    )
                                                    .child(div().text_color(rgb(0x45475a)).child("|"))
                                                    .child(
                                                        div()
                                                            .text_size(px(11.0))
                                                            .font_family("Menlo")
                                                            .text_color(rgb(0x6c7086))
                                                            .child("UTF-8")
                                                    )
                                                    .child(div().text_color(rgb(0x45475a)).child("|"))
                                                    .child(
                                                        div()
                                                            .text_size(px(11.0))
                                                            .font_family("Menlo")
                                                            .text_color(if dirty { rgb(0xf9e2af) } else { rgb(0xa6e3a1) })
                                                            .child(if dirty { "* 未保存" } else { "已同步" })
                                                    )
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.0))
                                                    .font_family("Menlo")
                                                    .text_color(rgb(0x89b4fa))
                                                    .child(status)
                                            )
                                    )
                            )
                    ]
                }
                ActiveModal::RenameDialog { old_name, new_name, cursor_pos, .. } => {
                    let old = old_name.clone();
                    let current_input = new_name.clone();
                    let pos = *cursor_pos;

                    let chars: Vec<char> = current_input.chars().collect();
                    let clamped = pos.min(chars.len());
                    let before: String = chars[..clamped].iter().collect();
                    let (cursor_char, after): (Option<char>, String) = if clamped < chars.len() {
                        (Some(chars[clamped]), chars[clamped + 1..].iter().collect())
                    } else {
                        (None, String::new())
                    };

                    vec![
                        div()
                            .id("modal_backdrop_rename")
                            .absolute()
                            .inset_0()
                            .bg(rgba(0x00000088))
                            .flex()
                            .items_center()
                            .justify_center()
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                this.modal = ActiveModal::None;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .id("rename_box")
                                    .w(px(400.0))
                                    .bg(rgb(0x12131a))
                                    .border_1()
                                    .border_color(rgb(0x38bdf8))
                                    .rounded_lg()
                                    .p_4()
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .child(Icon::edit().with_size(px(14.0)).with_color(rgb(0x38bdf8)))
                                            .child(
                                                div()
                                                    .text_size(px(14.0))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(rgb(0x38bdf8))
                                                    .child(crate::t!("sftp.modal_rename"))
                                            )
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .font_family("Menlo")
                                            .text_color(rgb(0x6c7086))
                                            .child(format!("原名称: {}", old))
                                    )
                                    .child(
                                        div()
                                            .w_full()
                                            .h(px(36.0))
                                            .px_3()
                                            .bg(rgb(0x0a0b10))
                                            .border_1()
                                            .border_color(rgb(0x38bdf8))
                                            .rounded_md()
                                            .flex()
                                            .items_center()
                                            .font_family("Menlo")
                                            .text_size(px(12.0))
                                            .text_color(rgb(0xcdd6f4))
                                            .children(if !before.is_empty() {
                                                vec![div().child(before)]
                                            } else {
                                                vec![]
                                            })
                                            .child(
                                                if let Some(ch) = cursor_char {
                                                    div()
                                                        .bg(rgb(0x38bdf8))
                                                        .text_color(rgb(0x0a0b10))
                                                        .font_weight(FontWeight::BOLD)
                                                        .child(ch.to_string())
                                                } else {
                                                    div()
                                                        .w(px(7.0))
                                                        .h(px(14.0))
                                                        .bg(rgb(0x38bdf8))
                                                }
                                            )
                                            .children(if !after.is_empty() {
                                                vec![div().child(after)]
                                            } else {
                                                vec![]
                                            })
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .id("btn_confirm_rename")
                                                    .flex_1()
                                                    .h(px(32.0))
                                                    .bg(rgb(0x38bdf8))
                                                    .text_color(rgb(0x0a0b10))
                                                    .font_weight(FontWeight::BOLD)
                                                    .rounded_md()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .cursor_pointer()
                                                    .text_size(px(12.0))
                                                    .on_click(cx.listener(|this, _e: &ClickEvent, _window, cx| {
                                                        this.apply_rename(cx);
                                                    }))
                                                    .child(crate::t!("common.confirm"))
                                            )
                                            .child(
                                                div()
                                                    .id("btn_cancel_rename")
                                                    .flex_1()
                                                    .h(px(32.0))
                                                    .bg(rgb(0x232738))
                                                    .hover(|s| s.bg(rgb(0x313244)))
                                                    .rounded_md()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .cursor_pointer()
                                                    .text_size(px(11.0))
                                                    .text_color(rgb(0xcdd6f4))
                                                    .on_click(cx.listener(|this, _e: &ClickEvent, _window, cx| {
                                                        this.modal = ActiveModal::None;
                                                        cx.notify();
                                                    }))
                                                    .child(crate::t!("common.cancel"))
                                            )
                                    )
                            )
                    ]
                }
                ActiveModal::ChmodDialog { file_path, current_mode, input_mode, cursor_pos } => {
                    let path = file_path.clone();
                    let curr_mode = *current_mode;
                    let input_m = input_mode.clone();
                    let pos = *cursor_pos;

                    let chars: Vec<char> = input_m.chars().collect();
                    let clamped = pos.min(chars.len());
                    let before: String = chars[..clamped].iter().collect();
                    let (cursor_char, after): (Option<char>, String) = if clamped < chars.len() {
                        (Some(chars[clamped]), chars[clamped + 1..].iter().collect())
                    } else {
                        (None, String::new())
                    };

                    let path_p755 = path.clone();
                    let path_p644 = path.clone();
                    let path_p600 = path.clone();
                    let path_p777 = path.clone();

                    vec![
                        div()
                            .id("modal_backdrop_chmod")
                            .absolute()
                            .inset_0()
                            .bg(rgba(0x00000088))
                            .flex()
                            .items_center()
                            .justify_center()
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                this.modal = ActiveModal::None;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .id("chmod_box")
                                    .w(px(400.0))
                                    .bg(rgb(0x12131a))
                                    .border_1()
                                    .border_color(rgb(0xcba6f7))
                                    .rounded_lg()
                                    .p_4()
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .child(Icon::shield().with_size(px(14.0)).with_color(rgb(0xcba6f7)))
                                            .child(
                                                div()
                                                    .text_size(px(14.0))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(rgb(0xcba6f7))
                                                    .child(crate::t!("sftp.modal_chmod"))
                                            )
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .font_family("Menlo")
                                            .text_color(rgb(0xa6adc8))
                                            .child(format!("当前权限: {:04o}", curr_mode & 0o777))
                                    )
                                    // Octal interactive input
                                    .child(
                                        div()
                                            .w_full()
                                            .h(px(36.0))
                                            .px_3()
                                            .bg(rgb(0x0a0b10))
                                            .border_1()
                                            .border_color(rgb(0xcba6f7))
                                            .rounded_md()
                                            .flex()
                                            .items_center()
                                            .font_family("Menlo")
                                            .text_size(px(12.0))
                                            .text_color(rgb(0xcdd6f4))
                                            .children(if !before.is_empty() {
                                                vec![div().child(before)]
                                            } else {
                                                vec![]
                                            })
                                            .child(
                                                if let Some(ch) = cursor_char {
                                                    div()
                                                        .bg(rgb(0xcba6f7))
                                                        .text_color(rgb(0x0a0b10))
                                                        .font_weight(FontWeight::BOLD)
                                                        .child(ch.to_string())
                                                } else {
                                                    div()
                                                        .w(px(7.0))
                                                        .h(px(14.0))
                                                        .bg(rgb(0xcba6f7))
                                                }
                                            )
                                            .children(if !after.is_empty() {
                                                vec![div().child(after)]
                                            } else {
                                                vec![]
                                            })
                                    )
                                    // Quick Presets
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .id("btn_chmod_755")
                                                    .flex_1()
                                                    .h(px(28.0))
                                                    .bg(rgb(0x232738))
                                                    .hover(|s| s.bg(rgb(0x313244)))
                                                    .rounded_md()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .cursor_pointer()
                                                    .text_size(px(11.0))
                                                    .font_family("Menlo")
                                                    .text_color(rgb(0xcdd6f4))
                                                    .on_click(cx.listener(move |this, _e: &ClickEvent, _window, cx| {
                                                        this.apply_chmod(path_p755.clone(), 0o755, cx);
                                                    }))
                                                    .child("0755")
                                            )
                                            .child(
                                                div()
                                                    .id("btn_chmod_644")
                                                    .flex_1()
                                                    .h(px(28.0))
                                                    .bg(rgb(0x232738))
                                                    .hover(|s| s.bg(rgb(0x313244)))
                                                    .rounded_md()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .cursor_pointer()
                                                    .text_size(px(11.0))
                                                    .font_family("Menlo")
                                                    .text_color(rgb(0xcdd6f4))
                                                    .on_click(cx.listener(move |this, _e: &ClickEvent, _window, cx| {
                                                        this.apply_chmod(path_p644.clone(), 0o644, cx);
                                                    }))
                                                    .child("0644")
                                            )
                                            .child(
                                                div()
                                                    .id("btn_chmod_600")
                                                    .flex_1()
                                                    .h(px(28.0))
                                                    .bg(rgb(0x232738))
                                                    .hover(|s| s.bg(rgb(0x313244)))
                                                    .rounded_md()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .cursor_pointer()
                                                    .text_size(px(11.0))
                                                    .font_family("Menlo")
                                                    .text_color(rgb(0xcdd6f4))
                                                    .on_click(cx.listener(move |this, _e: &ClickEvent, _window, cx| {
                                                        this.apply_chmod(path_p600.clone(), 0o600, cx);
                                                    }))
                                                    .child("0600")
                                            )
                                            .child(
                                                div()
                                                    .id("btn_chmod_777")
                                                    .flex_1()
                                                    .h(px(28.0))
                                                    .bg(rgb(0x232738))
                                                    .hover(|s| s.bg(rgb(0x313244)))
                                                    .rounded_md()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .cursor_pointer()
                                                    .text_size(px(11.0))
                                                    .font_family("Menlo")
                                                    .text_color(rgb(0xcdd6f4))
                                                    .on_click(cx.listener(move |this, _e: &ClickEvent, _window, cx| {
                                                        this.apply_chmod(path_p777.clone(), 0o777, cx);
                                                    }))
                                                    .child("0777")
                                            )
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .id("btn_apply_chmod")
                                                    .flex_1()
                                                    .h(px(32.0))
                                                    .bg(rgb(0xcba6f7))
                                                    .text_color(rgb(0x0a0b10))
                                                    .font_weight(FontWeight::BOLD)
                                                    .rounded_md()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .cursor_pointer()
                                                    .text_size(px(12.0))
                                                    .on_click(cx.listener(move |this, _e: &ClickEvent, _window, cx| {
                                                        if let ActiveModal::ChmodDialog { file_path, input_mode, .. } = &this.modal
                                                            && let Ok(m) = u32::from_str_radix(input_mode, 8) {
                                                                this.apply_chmod(file_path.clone(), m, cx);
                                                            }
                                                    }))
                                                    .child(crate::t!("common.confirm"))
                                            )
                                            .child(
                                                div()
                                                    .id("btn_cancel_chmod")
                                                    .flex_1()
                                                    .h(px(32.0))
                                                    .bg(rgb(0x232738))
                                                    .hover(|s| s.bg(rgb(0x313244)))
                                                    .rounded_md()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .cursor_pointer()
                                                    .text_size(px(11.0))
                                                    .text_color(rgb(0xcdd6f4))
                                                    .on_click(cx.listener(|this, _e: &ClickEvent, _window, cx| {
                                                        this.modal = ActiveModal::None;
                                                        cx.notify();
                                                    }))
                                                    .child("取消 (Esc)")
                                            )
                                    )
                            )
                    ]
                }
                ActiveModal::NewItemDialog { is_dir, name, cursor_pos } => {
                    let is_directory = *is_dir;
                    let input_name = name.clone();
                    let pos = *cursor_pos;

                    let chars: Vec<char> = input_name.chars().collect();
                    let clamped = pos.min(chars.len());
                    let before: String = chars[..clamped].iter().collect();
                    let (cursor_char, after): (Option<char>, String) = if clamped < chars.len() {
                        (Some(chars[clamped]), chars[clamped + 1..].iter().collect())
                    } else {
                        (None, String::new())
                    };

                    let submit_name = input_name.clone();

                    vec![
                        div()
                            .id("modal_backdrop_new_item")
                            .absolute()
                            .inset_0()
                            .bg(rgba(0x00000088))
                            .flex()
                            .items_center()
                            .justify_center()
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                this.modal = ActiveModal::None;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .id("new_item_box")
                                    .w(px(400.0))
                                    .bg(rgb(0x12131a))
                                    .border_1()
                                    .border_color(rgb(0x38bdf8))
                                    .rounded_lg()
                                    .p_4()
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                if is_directory {
                                                    Icon::folder().with_size(px(14.0)).with_color(rgb(0x38bdf8))
                                                } else {
                                                    Icon::file().with_size(px(14.0)).with_color(rgb(0x38bdf8))
                                                }
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(14.0))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(rgb(0x38bdf8))
                                                    .child(if is_directory { crate::t!("sftp.modal_new_folder") } else { crate::t!("sftp.modal_new_file") })
                                            )
                                    )
                                    .child(
                                        div()
                                            .w_full()
                                            .h(px(36.0))
                                            .px_3()
                                            .bg(rgb(0x0a0b10))
                                            .border_1()
                                            .border_color(rgb(0x38bdf8))
                                            .rounded_md()
                                            .flex()
                                            .items_center()
                                            .font_family("Menlo")
                                            .text_size(px(12.0))
                                            .text_color(rgb(0xcdd6f4))
                                            .children(if !before.is_empty() {
                                                vec![div().child(before)]
                                            } else {
                                                vec![]
                                            })
                                            .child(
                                                if let Some(ch) = cursor_char {
                                                    div()
                                                        .bg(rgb(0x38bdf8))
                                                        .text_color(rgb(0x0a0b10))
                                                        .font_weight(FontWeight::BOLD)
                                                        .child(ch.to_string())
                                                } else {
                                                    div()
                                                        .w(px(7.0))
                                                        .h(px(14.0))
                                                        .bg(rgb(0x38bdf8))
                                                }
                                            )
                                            .children(if !after.is_empty() {
                                                vec![div().child(after)]
                                            } else {
                                                vec![]
                                            })
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .id("btn_confirm_create")
                                                    .flex_1()
                                                    .h(px(32.0))
                                                    .bg(rgb(0x38bdf8))
                                                    .text_color(rgb(0x0a0b10))
                                                    .font_weight(FontWeight::BOLD)
                                                    .rounded_md()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .cursor_pointer()
                                                    .text_size(px(12.0))
                                                    .on_click(cx.listener(move |this, _e: &ClickEvent, _window, cx| {
                                                        this.create_new_item(is_directory, &submit_name, cx);
                                                    }))
                                                    .child(crate::t!("common.confirm"))
                                            )
                                            .child(
                                                div()
                                                    .id("btn_cancel_create")
                                                    .flex_1()
                                                    .h(px(32.0))
                                                    .bg(rgb(0x232738))
                                                    .hover(|s| s.bg(rgb(0x313244)))
                                                    .rounded_md()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .cursor_pointer()
                                                    .text_size(px(11.0))
                                                    .text_color(rgb(0xcdd6f4))
                                                    .on_click(cx.listener(|this, _e: &ClickEvent, _window, cx| {
                                                        this.modal = ActiveModal::None;
                                                        cx.notify();
                                                    }))
                                                    .child("取消 (Esc)")
                                            )
                                    )
                            )
                    ]
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn test_sftp_pagination_and_slice() {
        let mut items = Vec::new();
        for i in 1..=125 {
            items.push(RemoteFileItem {
                name: format!("file_{:03}.txt", i),
                path: format!("/tmp/file_{:03}.txt", i),
                is_dir: false,
                is_symlink: false,
                size: i as u64 * 10,
                modified: Some(1727190000),
                permissions: 0o644,
            });
        }

        let total_pages = items.len().div_ceil(50);
        assert_eq!(total_pages, 3);

        // Page 1
        let page1 = &items[0..50];
        assert_eq!(page1.len(), 50);
        assert_eq!(page1[0].name, "file_001.txt");

        // Page 3
        let page3 = &items[100..125];
        assert_eq!(page3.len(), 25);
        assert_eq!(page3[24].name, "file_125.txt");
    }

    #[test]
    fn test_sftp_hidden_files_filtering() {
        let items = [
            RemoteFileItem {
                name: ".bashrc".into(),
                path: "/home/user/.bashrc".into(),
                is_dir: false,
                is_symlink: false,
                size: 200,
                modified: None,
                permissions: 0o644,
            },
            RemoteFileItem {
                name: ".config".into(),
                path: "/home/user/.config".into(),
                is_dir: true,
                is_symlink: false,
                size: 4096,
                modified: None,
                permissions: 0o755,
            },
            RemoteFileItem {
                name: "documents".into(),
                path: "/home/user/documents".into(),
                is_dir: true,
                is_symlink: false,
                size: 4096,
                modified: None,
                permissions: 0o755,
            },
            RemoteFileItem {
                name: "notes.txt".into(),
                path: "/home/user/notes.txt".into(),
                is_dir: false,
                is_symlink: false,
                size: 1024,
                modified: None,
                permissions: 0o644,
            },
        ];

        // show_hidden = true: shows all 4
        let filtered_all: Vec<_> = items.iter().filter(|_i| true).cloned().collect();
        assert_eq!(filtered_all.len(), 4);

        // show_hidden = false: filters out .bashrc and .config
        let filtered_visible: Vec<_> = items
            .iter()
            .filter(|i| !i.name.starts_with('.'))
            .cloned()
            .collect();
        assert_eq!(filtered_visible.len(), 2);
        assert_eq!(filtered_visible[0].name, "documents");
        assert_eq!(filtered_visible[1].name, "notes.txt");
    }
}
