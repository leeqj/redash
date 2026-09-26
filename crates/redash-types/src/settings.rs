use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppSettings {
    // 1. General & Monitoring Probe
    pub probe_interval_secs: u64,
    pub probe_timeout_secs: u64,
    pub history_points: usize,
    pub auto_refresh: bool,

    // 2. Terminal
    pub terminal_font_family: String,
    pub terminal_font_size: f32,
    pub terminal_cursor_style: String, // "Block", "Line", "Underline"
    pub terminal_scrollback_lines: usize,
    pub terminal_copy_on_select: bool,

    // 3. UI & Appearance
    pub theme_name: String,
    pub glow_effects_enabled: bool,
    pub compact_mode: bool,
    #[serde(default = "default_language")]
    pub language: String,

    // 4. File Management & SFTP
    pub sftp_show_hidden_files: bool,
    pub sftp_confirm_delete: bool,

    // 5. Smart Alerts & Notifications
    #[serde(default = "default_cpu_threshold")]
    pub alert_cpu_threshold: f32,
    #[serde(default = "default_mem_threshold")]
    pub alert_mem_threshold: f32,
    #[serde(default = "default_disk_threshold")]
    pub alert_disk_threshold: f32,
    #[serde(default = "default_true")]
    pub alert_notify_offline: bool,
    #[serde(default = "default_true")]
    pub alert_macos_notification: bool,
    #[serde(default)]
    pub alert_webhook_url: Option<String>,
}

fn default_language() -> String {
    "zh-CN".to_string()
}

fn default_cpu_threshold() -> f32 {
    90.0
}

fn default_mem_threshold() -> f32 {
    95.0
}

fn default_disk_threshold() -> f32 {
    90.0
}

fn default_true() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            probe_interval_secs: 2,
            probe_timeout_secs: 5,
            history_points: 30,
            auto_refresh: true,

            terminal_font_family: "Menlo".to_string(),
            terminal_font_size: 12.0,
            terminal_cursor_style: "Block".to_string(),
            terminal_scrollback_lines: 5000,
            terminal_copy_on_select: false,

            theme_name: "Minimalist Dark Tech".to_string(),
            glow_effects_enabled: true,
            compact_mode: false,
            language: default_language(),

            sftp_show_hidden_files: false,
            sftp_confirm_delete: true,

            alert_cpu_threshold: 90.0,
            alert_mem_threshold: 95.0,
            alert_disk_threshold: 90.0,
            alert_notify_offline: true,
            alert_macos_notification: true,
            alert_webhook_url: None,
        }
    }
}
