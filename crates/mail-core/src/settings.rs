//! User settings, persisted as TOML in `$XDG_CONFIG_HOME/moho-mail/settings.toml`.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::notify::{NotifyMode, NotifySettings};

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid settings file: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("cannot serialize settings: {0}")]
    Serialize(#[from] toml::ser::Error),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub folders_width: f64,
    pub list_width: f64,
}

impl Default for Layout {
    fn default() -> Self {
        Self { folders_width: 240.0, list_width: 380.0 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TraySettings {
    pub close_to_tray: bool,
    pub minimize_to_tray: bool,
    pub start_minimized: bool,
}

impl Default for TraySettings {
    fn default() -> Self {
        Self { close_to_tray: true, minimize_to_tray: true, start_minimized: false }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PrivacySettings {
    pub load_remote_images: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub layout: Layout,
    pub tray: TraySettings,
    pub privacy: PrivacySettings,
    pub notifications: NotifySettings,
    /// Notification mode per account id.
    pub account_notify: HashMap<String, NotifyMode>,
}

/// Write atomically (temp file + rename) so a crash never leaves a torn file.
pub fn write_atomic(path: &Path, text: &str) -> Result<(), SettingsError> {
    let tmp = path.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

impl Settings {
    /// Load settings; a missing file yields defaults.
    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(toml::from_str(&text)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        write_atomic(path, &toml::to_string_pretty(self)?)
    }

    pub fn notify_mode(&self, account_id: &str) -> NotifyMode {
        self.account_notify.get(account_id).copied().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        assert_eq!(Settings::load(&path).unwrap(), Settings::default());

        let mut s = Settings::default();
        s.layout.list_width = 420.0;
        s.account_notify.insert("gmail".into(), NotifyMode::ImportantOnly);
        s.notifications.quiet_hours = Some((22, 7));
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), s);
    }

    #[test]
    fn partial_file_fills_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, "[tray]\nclose_to_tray = false\n").unwrap();
        let s = Settings::load(&path).unwrap();
        assert!(!s.tray.close_to_tray);
        assert!(s.tray.minimize_to_tray);
        assert_eq!(s.layout, Layout::default());
    }
}
