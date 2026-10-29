//! Persisted UI preferences (tray behaviour).

use faint_signal_store::paths;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// User-facing toggles shown in the Settings window.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    pub use_system_tray: bool,
    /// Hide the main window on close instead of quitting (needs tray).
    pub close_to_system_tray: bool,
}

impl UiSettings {
    fn path() -> anyhow::Result<PathBuf> {
        let dirs = paths()?;
        std::fs::create_dir_all(&dirs.config_dir)?;
        Ok(dirs.config_dir.join("ui-settings.json"))
    }

    /// Load from disk, or defaults when the file is missing or unreadable.
    pub fn load() -> Self {
        let path = match Self::path() {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("cannot resolve settings path: {e}");
                return Self::default();
            }
        };
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|e| {
                tracing::warn!("corrupt settings {}: {e}", path.display());
                Self::default()
            }),
            Err(e) => {
                tracing::warn!("failed to read settings {}: {e}", path.display());
                Self::default()
            }
        }
    }

    pub fn save(&self) {
        let path = match Self::path() {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("cannot resolve settings path: {e}");
                return;
            }
        };
        match serde_json::to_string_pretty(self) {
            Ok(raw) => {
                if let Err(e) = std::fs::write(&path, raw) {
                    tracing::warn!("failed to write settings {}: {e}", path.display());
                }
            }
            Err(e) => tracing::warn!("failed to encode settings: {e}"),
        }
    }

    pub fn should_close_to_tray(&self) -> bool {
        self.use_system_tray && self.close_to_system_tray
    }
}
