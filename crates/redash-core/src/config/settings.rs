use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

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

fn default_language() -> String {
    "zh-CN".to_string()
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
            language: "zh-CN".to_string(),

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

impl AppSettings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn default_path() -> PathBuf {
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("./.redash"));
        let redash_path = base.join("redash").join("settings.json");
        if redash_path.exists() {
            return redash_path;
        }
        let legacy_path = base.join("serverbox").join("settings.json");
        if legacy_path.exists() {
            return legacy_path;
        }
        redash_path
    }

    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read settings from {:?}", path))?;
        let settings: Self = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse settings JSON from {:?}", path))?;
        Ok(settings)
    }

    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create settings directory {:?}", parent))?;
        }
        let json = serde_json::to_string_pretty(self).context("Failed to serialize settings")?;

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("settings.json");
        let temp_file_name = format!(".{}.tmp-{}", file_name, uuid::Uuid::new_v4());
        let temp_path = path.with_file_name(temp_file_name);

        fs::write(&temp_path, json.as_bytes())
            .with_context(|| format!("Failed to write temporary settings file {:?}", temp_path))?;

        if let Err(e) = fs::rename(&temp_path, path) {
            let _ = fs::remove_file(&temp_path);
            return Err(e).with_context(|| {
                format!("Failed to atomically rename {:?} to {:?}", temp_path, path)
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_defaults() {
        let s = AppSettings::default();
        assert_eq!(s.probe_interval_secs, 2);
        assert_eq!(s.probe_timeout_secs, 5);
        assert_eq!(s.history_points, 30);
        assert!(s.auto_refresh);
        assert_eq!(s.terminal_font_family, "Menlo");
        assert_eq!(s.terminal_font_size, 12.0);
        assert_eq!(s.theme_name, "Minimalist Dark Tech");
        assert!(s.glow_effects_enabled);
    }

    #[test]
    fn test_settings_serde_and_file_io() {
        let temp_dir =
            std::env::temp_dir().join(format!("sb_test_settings_{}", uuid::Uuid::new_v4()));
        let file_path = temp_dir.join("settings.json");

        let s = AppSettings {
            probe_interval_secs: 5,
            terminal_font_family: "Fira Code".to_string(),
            terminal_font_size: 14.0,
            compact_mode: true,
            ..Default::default()
        };

        s.save_to_file(&file_path).expect("Failed to save settings");
        let loaded = AppSettings::load_from_file(&file_path).expect("Failed to load settings");

        assert_eq!(loaded.probe_interval_secs, 5);
        assert_eq!(loaded.terminal_font_family, "Fira Code");
        assert_eq!(loaded.terminal_font_size, 14.0);
        assert!(loaded.compact_mode);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
