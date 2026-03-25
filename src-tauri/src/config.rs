use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub hotkey: HotkeyConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub whisper: WhisperConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub general: GeneralConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    /// Trigger key name
    #[serde(default = "default_trigger")]
    pub trigger: String,
    /// Minimum hold duration in ms (debounce)
    #[serde(default = "default_min_hold_ms")]
    pub min_hold_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    /// Sample rate in Hz
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    /// Maximum recording duration in seconds
    #[serde(default = "default_max_duration_sec")]
    pub max_duration_sec: u32,
    /// Minimum recording duration in ms
    #[serde(default = "default_min_duration_ms")]
    pub min_duration_ms: u64,
    /// Audio input device name ("default" for system default)
    #[serde(default = "default_device")]
    pub device: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhisperConfig {
    /// Recognition language
    #[serde(default = "default_language")]
    pub language: String,
    /// Beam search width
    #[serde(default = "default_beam_size")]
    pub beam_size: i32,
    /// Number of inference threads (0 = auto)
    #[serde(default)]
    pub threads: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// Overlay window width
    #[serde(default = "default_window_width")]
    pub window_width: u32,
    /// Overlay window height
    #[serde(default = "default_window_height")]
    pub window_height: u32,
    /// Overlay opacity (0.0 - 1.0)
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    /// Theme: auto, light, dark
    #[serde(default = "default_theme")]
    pub theme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// Auto-start on system boot
    #[serde(default)]
    pub auto_start: bool,
    /// Show tray icon
    #[serde(default = "default_true")]
    pub show_tray: bool,
    /// Log level
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// Model download mirror: "auto", "cn" (hf-mirror.com), "global" (huggingface.co)
    #[serde(default = "default_mirror")]
    pub mirror: String,
}

// Default value functions
fn default_trigger() -> String { "RControl".to_string() }
fn default_min_hold_ms() -> u64 { 300 }
fn default_sample_rate() -> u32 { 16000 }
fn default_max_duration_sec() -> u32 { 60 }
fn default_min_duration_ms() -> u64 { 500 }
fn default_device() -> String { "default".to_string() }
fn default_language() -> String { "zh".to_string() }
fn default_beam_size() -> i32 { 5 }
fn default_window_width() -> u32 { 400 }
fn default_window_height() -> u32 { 160 }
fn default_opacity() -> f64 { 0.92 }
fn default_theme() -> String { "auto".to_string() }
fn default_true() -> bool { true }
fn default_log_level() -> String { "info".to_string() }
fn default_mirror() -> String { "auto".to_string() }

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey: HotkeyConfig::default(),
            audio: AudioConfig::default(),
            whisper: WhisperConfig::default(),
            ui: UiConfig::default(),
            general: GeneralConfig::default(),
        }
    }
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            trigger: default_trigger(),
            min_hold_ms: default_min_hold_ms(),
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: default_sample_rate(),
            max_duration_sec: default_max_duration_sec(),
            min_duration_ms: default_min_duration_ms(),
            device: default_device(),
        }
    }
}

impl Default for WhisperConfig {
    fn default() -> Self {
        Self {
            language: default_language(),
            beam_size: default_beam_size(),
            threads: 0,
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            window_width: default_window_width(),
            window_height: default_window_height(),
            opacity: default_opacity(),
            theme: default_theme(),
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            auto_start: false,
            show_tray: true,
            log_level: default_log_level(),
            mirror: default_mirror(),
        }
    }
}

/// Get the config file path for the current platform
pub fn config_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("voiceinput");
    config_dir.join("config.toml")
}

/// Load config from file, creating defaults if not found
pub fn load_config() -> AppConfig {
    let path = config_path();

    if path.exists() {
        match fs::read_to_string(&path) {
            Ok(content) => match toml::from_str::<AppConfig>(&content) {
                Ok(config) => {
                    log::info!("Config loaded from: {}", path.display());
                    return config;
                }
                Err(e) => {
                    log::warn!("Failed to parse config, using defaults: {}", e);
                }
            },
            Err(e) => {
                log::warn!("Failed to read config file, using defaults: {}", e);
            }
        }
    }

    let config = AppConfig::default();
    // Save default config for user reference
    if let Err(e) = save_config(&config) {
        log::warn!("Failed to save default config: {}", e);
    }
    config
}

/// Save config to file
pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = config_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config dir: {}", e))?;
    }

    let content = toml::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    fs::write(&path, content)
        .map_err(|e| format!("Failed to write config: {}", e))?;

    log::info!("Config saved to: {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.hotkey.trigger, "RControl");
        assert_eq!(config.hotkey.min_hold_ms, 300);
        assert_eq!(config.audio.sample_rate, 16000);
        assert_eq!(config.audio.max_duration_sec, 60);
        assert_eq!(config.audio.min_duration_ms, 500);
        assert_eq!(config.whisper.language, "zh");
        assert_eq!(config.whisper.beam_size, 5);
        assert_eq!(config.whisper.threads, 0);
        assert!((config.ui.opacity - 0.92).abs() < f64::EPSILON);
        assert!(!config.general.auto_start);
        assert!(config.general.show_tray);
    }

    #[test]
    fn test_serialize_deserialize() {
        let config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.hotkey.min_hold_ms, config.hotkey.min_hold_ms);
        assert_eq!(parsed.audio.sample_rate, config.audio.sample_rate);
        assert_eq!(parsed.whisper.language, config.whisper.language);
    }

    #[test]
    fn test_partial_config_uses_defaults() {
        let partial = r#"
[hotkey]
min_hold_ms = 500

[whisper]
language = "en"
"#;
        let config: AppConfig = toml::from_str(partial).unwrap();
        assert_eq!(config.hotkey.min_hold_ms, 500);
        assert_eq!(config.whisper.language, "en");
        // Other fields should have defaults
        assert_eq!(config.audio.sample_rate, 16000);
        assert_eq!(config.whisper.beam_size, 5);
    }

    #[test]
    fn test_empty_config_uses_all_defaults() {
        let config: AppConfig = toml::from_str("").unwrap();
        assert_eq!(config.hotkey.trigger, "RControl");
        assert_eq!(config.audio.max_duration_sec, 60);
    }
}
