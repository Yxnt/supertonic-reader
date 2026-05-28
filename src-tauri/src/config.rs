use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use anyhow::Result;

const VALID_LANGS: &[&str] = &[
    "en", "ko", "ja", "ar", "bg", "cs", "da", "de", "el", "es", "et", "fi", "fr",
    "hi", "hr", "hu", "id", "it", "lt", "lv", "nl", "pl", "pt", "ro", "ru", "sk",
    "sl", "sv", "tr", "uk", "vi",
];

const VALID_UI_LANGS: &[&str] = &[
    "en", "zh", "ja", "ko", "es", "fr", "ms", "vi", "ru",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub initialized: bool,
    pub language: String,
    pub voice_style: String,
    pub quality: i32,
    pub speed: f32,
    pub silence: f32,
    pub shortcut: String,
    pub pause_shortcut: String,
    #[serde(rename = "uiLanguage", alias = "ui_language")]
    pub ui_language: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            initialized: false,
            language: "en".to_string(),
            voice_style: "M1".to_string(),
            quality: 8,
            speed: 1.05,
            silence: 0.3,
            shortcut: "Ctrl+`".to_string(),
            pause_shortcut: "Ctrl+1".to_string(),
            ui_language: "en".to_string(),
        }
    }
}

fn app_root_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn config_path() -> PathBuf {
    let app_dir = app_root_dir().join("supertonic-reader-data");
    std::fs::create_dir_all(&app_dir).ok();
    app_dir.join("config.json")
}

pub fn model_dir() -> PathBuf {
    let app_dir = app_root_dir().join("supertonic-reader-data").join("models");
    std::fs::create_dir_all(&app_dir).ok();
    app_dir
}

pub fn load_settings() -> Result<AppSettings> {
    let path = config_path();
    if !path.exists() {
        return Ok(AppSettings::default());
    }
    let content = std::fs::read_to_string(path)?;
    let mut settings: AppSettings = serde_json::from_str(&content)?;
    if !VALID_LANGS.contains(&settings.language.as_str()) {
        println!(
            "[config] Invalid language '{}', falling back to 'en'",
            settings.language
        );
        settings.language = "en".to_string();
    }
    // 迁移旧版 quality 范围 (0-100 / 1-10) 到新版官方范围 (2-16)
    if settings.quality > 16 || settings.quality < 2 {
        settings.quality = 8;
    }
    // 迁移旧版 speed 范围到官方推荐 (0.8-1.3)
    if settings.speed < 0.8 || settings.speed > 1.3 {
        settings.speed = 1.05;
    }
    // 迁移旧版 silence 范围到新版 0.3~1.0
    if settings.silence < 0.3 || settings.silence > 1.0 {
        settings.silence = 0.3;
    }
    // shortcut 为空时回退到默认值
    if settings.shortcut.trim().is_empty() {
        settings.shortcut = "Ctrl+`".to_string();
    }
    // 旧版本默认 pause_shortcut 是空字符串,升级为 Ctrl+1
    if settings.pause_shortcut.trim().is_empty() {
        settings.pause_shortcut = "Ctrl+1".to_string();
    }
    if !VALID_UI_LANGS.contains(&settings.ui_language.as_str()) {
        settings.ui_language = "en".to_string();
    }
    Ok(settings)
}

pub fn save_settings(settings: &AppSettings) -> Result<()> {
    let path = config_path();
    let content = serde_json::to_string_pretty(settings)?;
    std::fs::write(path, content)?;
    Ok(())
}
