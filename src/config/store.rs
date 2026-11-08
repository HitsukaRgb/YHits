use crate::download::AudioFormat;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Default)]
struct Config {
    #[serde(default)]
    output_dir: String,
    #[serde(default)]
    audio_format: AudioFormat,
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("YHits").join("config.json"))
}

pub fn default_output_dir() -> PathBuf {
    dirs::audio_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("YHits")
}

fn load_config() -> Config {
    let Some(path) = config_path() else {
        return Config::default();
    };
    let Ok(text) = fs::read_to_string(&path) else {
        return Config::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn save_config(config: &Config) -> Result<(), String> {
    let Some(path) = config_path() else {
        return Err(crate::i18n::t(crate::i18n::Tr::ConfigDirMissing).into());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let text = serde_json::to_string_pretty(config).map_err(|err| err.to_string())?;
    fs::write(path, text).map_err(|err| err.to_string())
}

pub fn load_output_dir() -> String {
    let config = load_config();
    if config.output_dir.trim().is_empty() {
        default_output_dir().display().to_string()
    } else {
        config.output_dir
    }
}

pub fn save_output_dir(output_dir: &str) -> Result<(), String> {
    let mut config = load_config();
    config.output_dir = output_dir.to_string();
    save_config(&config)
}

pub fn load_audio_format() -> AudioFormat {
    load_config().audio_format
}

pub fn save_audio_format(audio_format: AudioFormat) -> Result<(), String> {
    let mut config = load_config();
    config.audio_format = audio_format;
    save_config(&config)
}

pub fn existing_ancestor(path: &str) -> Option<PathBuf> {
    let trimmed = path.trim();
    let mut current = if trimmed.is_empty() {
        default_output_dir()
    } else {
        PathBuf::from(trimmed)
    };

    if current.is_dir() {
        return Some(current);
    }

    while !current.exists() {
        if !current.pop() {
            return None;
        }
    }

    if current.is_dir() {
        Some(current)
    } else {
        current.parent().map(PathBuf::from)
    }
}
