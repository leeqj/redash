use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub use redash_types::settings::AppSettings;

pub trait AppSettingsExt {
    fn new() -> Self
    where
        Self: Sized;
    fn default_path() -> PathBuf;
    fn load_from_file(path: &Path) -> Result<Self>
    where
        Self: Sized;
    fn save_to_file(&self, path: &Path) -> Result<()>;
}

impl AppSettingsExt for AppSettings {
    fn new() -> Self {
        Self::default()
    }

    fn default_path() -> PathBuf {
        if let Ok(dir) = std::env::var("REDASH_CONFIG_DIR") {
            return PathBuf::from(dir).join("settings.json");
        }
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

    fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read settings from {:?}", path))?;
        let settings: Self = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse settings JSON from {:?}", path))?;
        Ok(settings)
    }

    fn save_to_file(&self, path: &Path) -> Result<()> {
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
