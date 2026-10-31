//! XDG Base Directory locations for Moho Mail.
//!
//! Config lives in `$XDG_CONFIG_HOME/moho-mail`, re-downloadable data in
//! `$XDG_CACHE_HOME/moho-mail`, local-only user data in
//! `$XDG_DATA_HOME/moho-mail`, logs and window state in
//! `$XDG_STATE_HOME/moho-mail`. Nothing is ever written directly into `$HOME`.

use std::io;
use std::path::{Path, PathBuf};

pub const APP_DIR: &str = "moho-mail";

#[derive(Debug, thiserror::Error)]
pub enum PathsError {
    #[error("cannot determine the home directory ($HOME is unset or relative)")]
    NoHome,
    #[error("failed to create {path}: {source}")]
    Create { path: PathBuf, source: io::Error },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub config: PathBuf,
    pub cache: PathBuf,
    pub data: PathBuf,
    pub state: PathBuf,
    /// `$XDG_RUNTIME_DIR/moho-mail`; the spec has no fallback, so this may be absent.
    pub runtime: Option<PathBuf>,
}

impl AppPaths {
    /// Resolve paths from the process environment.
    pub fn from_env() -> Result<Self, PathsError> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or(PathsError::NoHome)?;
        Ok(Self::resolve(&home, |key| std::env::var(key).ok()))
    }

    /// Resolve paths given a home directory and an environment lookup.
    ///
    /// Per the spec, a variable that is unset, empty or not an absolute path
    /// is ignored and the default is used instead.
    pub fn resolve(home: &Path, env: impl Fn(&str) -> Option<String>) -> Self {
        let base = |key: &str, default: &str| -> PathBuf {
            env(key)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(default))
                .join(APP_DIR)
        };
        Self {
            config: base("XDG_CONFIG_HOME", ".config"),
            cache: base("XDG_CACHE_HOME", ".cache"),
            data: base("XDG_DATA_HOME", ".local/share"),
            state: base("XDG_STATE_HOME", ".local/state"),
            runtime: env("XDG_RUNTIME_DIR")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .map(|p| p.join(APP_DIR)),
        }
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config.join("settings.toml")
    }

    pub fn accounts_file(&self) -> PathBuf {
        self.config.join("accounts.toml")
    }

    pub fn mail_cache_dir(&self) -> PathBuf {
        self.cache.join("mail")
    }

    /// WebKitGTK's own data/cache directory.
    pub fn webview_dir(&self) -> PathBuf {
        self.cache.join("webkit")
    }

    pub fn proton_vault_dir(&self) -> PathBuf {
        self.data.join("proton")
    }

    pub fn proton_cache_dir(&self) -> PathBuf {
        self.cache.join("proton")
    }

    pub fn log_dir(&self) -> PathBuf {
        self.state.join("logs")
    }

    /// Create every directory with user-only permissions.
    pub fn ensure(&self) -> Result<(), PathsError> {
        let mut dirs = vec![
            self.config.clone(),
            self.cache.clone(),
            self.data.clone(),
            self.state.clone(),
            self.mail_cache_dir(),
            self.log_dir(),
        ];
        dirs.extend(self.runtime.clone());
        for dir in dirs {
            create_private_dir(&dir).map_err(|source| PathsError::Create { path: dir.clone(), source })?;
        }
        Ok(())
    }
}

fn create_private_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    if dir.is_dir() {
        return Ok(());
    }
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> =
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn defaults_follow_spec() {
        let p = AppPaths::resolve(Path::new("/home/u"), env(&[]));
        assert_eq!(p.config, Path::new("/home/u/.config/moho-mail"));
        assert_eq!(p.cache, Path::new("/home/u/.cache/moho-mail"));
        assert_eq!(p.data, Path::new("/home/u/.local/share/moho-mail"));
        assert_eq!(p.state, Path::new("/home/u/.local/state/moho-mail"));
        assert_eq!(p.runtime, None);
    }

    #[test]
    fn honours_absolute_overrides() {
        let p = AppPaths::resolve(
            Path::new("/home/u"),
            env(&[
                ("XDG_CONFIG_HOME", "/cfg"),
                ("XDG_CACHE_HOME", "/tmp/cache"),
                ("XDG_DATA_HOME", "/d"),
                ("XDG_STATE_HOME", "/s"),
                ("XDG_RUNTIME_DIR", "/run/user/1000"),
            ]),
        );
        assert_eq!(p.config, Path::new("/cfg/moho-mail"));
        assert_eq!(p.cache, Path::new("/tmp/cache/moho-mail"));
        assert_eq!(p.data, Path::new("/d/moho-mail"));
        assert_eq!(p.state, Path::new("/s/moho-mail"));
        assert_eq!(p.runtime.as_deref(), Some(Path::new("/run/user/1000/moho-mail")));
    }

    #[test]
    fn ignores_empty_and_relative_values() {
        let p = AppPaths::resolve(
            Path::new("/home/u"),
            env(&[("XDG_CONFIG_HOME", ""), ("XDG_CACHE_HOME", "relative/cache")]),
        );
        assert_eq!(p.config, Path::new("/home/u/.config/moho-mail"));
        assert_eq!(p.cache, Path::new("/home/u/.cache/moho-mail"));
    }

    #[test]
    fn ensure_creates_private_dirs() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let p = AppPaths::resolve(tmp.path(), env(&[]));
        p.ensure().unwrap();
        for dir in [&p.config, &p.cache, &p.data, &p.state] {
            let mode = std::fs::metadata(dir).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{}", dir.display());
        }
    }
}
