use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteFileItem {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: Option<u64>,
    pub permissions: u32,
}

impl RemoteFileItem {
    pub fn permissions_str(&self) -> String {
        format_permissions(self.permissions, self.is_dir, self.is_symlink)
    }

    pub fn category(&self) -> FileCategory {
        if self.is_dir {
            FileCategory::Directory
        } else if self.is_symlink {
            FileCategory::Symlink
        } else {
            let lower = self.name.to_lowercase();
            if lower.ends_with(".log") {
                FileCategory::Log
            } else if lower.ends_with(".json")
                || lower.ends_with(".yaml")
                || lower.ends_with(".yml")
                || lower.ends_with(".toml")
                || lower.ends_with(".conf")
                || lower.ends_with(".ini")
                || lower.ends_with(".env")
                || lower.ends_with(".xml")
                || lower.ends_with(".properties")
            {
                FileCategory::Config
            } else if lower.ends_with(".sh")
                || lower.ends_with(".bash")
                || lower.ends_with(".zsh")
                || lower.ends_with(".bin")
                || (self.permissions & 0o111 != 0)
            {
                FileCategory::ScriptOrBinary
            } else if lower.ends_with(".tar")
                || lower.ends_with(".gz")
                || lower.ends_with(".tgz")
                || lower.ends_with(".zip")
                || lower.ends_with(".7z")
                || lower.ends_with(".rar")
                || lower.ends_with(".bz2")
                || lower.ends_with(".xz")
            {
                FileCategory::Archive
            } else if lower.ends_with(".rs")
                || lower.ends_with(".py")
                || lower.ends_with(".js")
                || lower.ends_with(".ts")
                || lower.ends_with(".tsx")
                || lower.ends_with(".jsx")
                || lower.ends_with(".go")
                || lower.ends_with(".c")
                || lower.ends_with(".cpp")
                || lower.ends_with(".h")
                || lower.ends_with(".hpp")
                || lower.ends_with(".sql")
                || lower.ends_with(".html")
                || lower.ends_with(".css")
            {
                FileCategory::Code
            } else {
                FileCategory::Document
            }
        }
    }
}

pub fn format_permissions(permissions: u32, is_dir: bool, is_symlink: bool) -> String {
    let type_char = if is_dir {
        'd'
    } else if is_symlink {
        'l'
    } else {
        '-'
    };

    let user_r = if permissions & 0o400 != 0 { 'r' } else { '-' };
    let user_w = if permissions & 0o200 != 0 { 'w' } else { '-' };
    let user_x = if permissions & 0o100 != 0 { 'x' } else { '-' };

    let group_r = if permissions & 0o040 != 0 { 'r' } else { '-' };
    let group_w = if permissions & 0o020 != 0 { 'w' } else { '-' };
    let group_x = if permissions & 0o010 != 0 { 'x' } else { '-' };

    let other_r = if permissions & 0o004 != 0 { 'r' } else { '-' };
    let other_w = if permissions & 0o002 != 0 { 'w' } else { '-' };
    let other_x = if permissions & 0o001 != 0 { 'x' } else { '-' };

    format!(
        "{}{}{}{}{}{}{}{}{}{}",
        type_char, user_r, user_w, user_x, group_r, group_w, group_x, other_r, other_w, other_x
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileCategory {
    Directory,
    Symlink,
    Log,
    Config,
    ScriptOrBinary,
    Archive,
    Code,
    Document,
}

impl FileCategory {
    pub fn default_icon(&self) -> &'static str {
        match self {
            FileCategory::Directory => "📁",
            FileCategory::Symlink => "🔗",
            FileCategory::Log => "📜",
            FileCategory::Config => "⚙️",
            FileCategory::ScriptOrBinary => "⚡",
            FileCategory::Archive => "📦",
            FileCategory::Code => "💻",
            FileCategory::Document => "📄",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PagedFileResult {
    pub items: Vec<RemoteFileItem>,
    pub total_items: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

impl PagedFileResult {
    pub fn paginate(
        all_items: Vec<RemoteFileItem>,
        page: usize,
        page_size: usize,
        filter: Option<&str>,
    ) -> Self {
        let filtered: Vec<RemoteFileItem> = if let Some(query) = filter {
            let query_lower = query.trim().to_lowercase();
            if query_lower.is_empty() {
                all_items
            } else {
                all_items
                    .into_iter()
                    .filter(|item| item.name.to_lowercase().contains(&query_lower))
                    .collect()
            }
        } else {
            all_items
        };

        let total_items = filtered.len();
        let page_size = if page_size == 0 { 100 } else { page_size };
        let total_pages = if total_items == 0 {
            1
        } else {
            total_items.div_ceil(page_size)
        };
        let page = page.max(1).min(total_pages);
        let start_idx = (page - 1) * page_size;
        let paged_items = if start_idx >= total_items {
            Vec::new()
        } else {
            let end_idx = (start_idx + page_size).min(total_items);
            filtered[start_idx..end_idx].to_vec()
        };

        Self {
            items: paged_items,
            total_items,
            page,
            page_size,
            total_pages,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sftp_models_serde() {
        let item = RemoteFileItem {
            name: "config.toml".to_string(),
            path: "/etc/app/config.toml".to_string(),
            is_dir: false,
            is_symlink: false,
            size: 1024,
            modified: Some(1700000000),
            permissions: 0o644,
        };

        assert_eq!(item.category(), FileCategory::Config);
        assert_eq!(item.category().default_icon(), "⚙️");
        assert_eq!(item.permissions_str(), "-rw-r--r--");

        let json = serde_json::to_string(&item).unwrap();
        let decoded: RemoteFileItem = serde_json::from_str(&json).unwrap();
        assert_eq!(item, decoded);
    }

    #[test]
    fn test_permissions_format() {
        assert_eq!(format_permissions(0o755, true, false), "drwxr-xr-x");
        assert_eq!(format_permissions(0o777, false, true), "lrwxrwxrwx");
        assert_eq!(format_permissions(0o644, false, false), "-rw-r--r--");
        assert_eq!(format_permissions(0o600, false, false), "-rw-------");
    }

    #[test]
    fn test_paged_file_result() {
        let items: Vec<RemoteFileItem> = (0..25)
            .map(|i| RemoteFileItem {
                name: format!("file_{:02}.txt", i),
                path: format!("/tmp/file_{:02}.txt", i),
                is_dir: false,
                is_symlink: false,
                size: i * 10,
                modified: None,
                permissions: 0o644,
            })
            .collect();

        let page1 = PagedFileResult::paginate(items.clone(), 1, 10, None);
        assert_eq!(page1.items.len(), 10);
        assert_eq!(page1.total_items, 25);
        assert_eq!(page1.total_pages, 3);
        assert_eq!(page1.page, 1);

        let filtered = PagedFileResult::paginate(items, 1, 10, Some("file_1"));
        // file_10 to file_19 match (10 items)
        assert_eq!(filtered.total_items, 10);
        assert_eq!(filtered.items.len(), 10);
    }
}
