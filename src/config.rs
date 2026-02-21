use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Application configuration persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Directory where screenshots, recordings, and bugreports are saved.
    pub output_dir: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            output_dir: ".".to_string(),
        }
    }
}

impl AppConfig {
    /// Config file path: `~/.config/adbwrenchtui/config.json`.
    fn config_path() -> Option<PathBuf> {
        dirs_path().map(|d| d.join("config.json"))
    }

    /// Load config from disk, returning defaults if not found or invalid.
    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save config to disk, creating the directory if needed.
    pub fn save(&self) {
        let Some(path) = Self::config_path() else {
            tracing::warn!("Could not determine config path");
            return;
        };
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!("Failed to create config directory: {e}");
                return;
            }
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    tracing::warn!("Failed to write config: {e}");
                }
            }
            Err(e) => {
                tracing::warn!("Failed to serialize config: {e}");
            }
        }
    }

    /// Build a full output path by joining `output_dir` with a filename.
    pub fn output_path(&self, filename: &str) -> String {
        Path::new(&self.output_dir)
            .join(filename)
            .to_string_lossy()
            .into_owned()
    }
}

/// Returns `~/.config/adbwrenchtui/`.
fn dirs_path() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".config").join("adbwrenchtui"))
}

/// Cross-platform home directory.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}
