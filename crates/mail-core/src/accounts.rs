//! Account configuration, persisted without secrets in
//! `$XDG_CONFIG_HOME/moho-mail/accounts.toml`.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::Provider;
use crate::settings::SettingsError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Security {
    /// Implicit TLS (IMAPS 993, SMTPS 465).
    Tls,
    StartTls,
    /// Plain text; only accepted for loopback hosts.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub security: Security,
}

impl ServerConfig {
    pub fn is_loopback(&self) -> bool {
        matches!(self.host.as_str(), "127.0.0.1" | "::1" | "localhost")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OAuthProvider {
    Google,
    Microsoft,
    Yahoo,
    Aol,
    Fastmail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthMethod {
    Password,
    OAuth2 { provider: OAuthProvider },
    /// Served by the in-process Proton bridge; credentials are generated per run.
    ProtonBridge,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountConfig {
    pub id: String,
    /// Label shown in the UI ("Gmail", "Work").
    pub name: String,
    pub email: String,
    /// Name used in the From header.
    #[serde(default)]
    pub sender_name: String,
    pub provider: Provider,
    pub color: String,
    pub username: String,
    pub auth: AuthMethod,
    pub imap: ServerConfig,
    pub smtp: ServerConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AccountsFile {
    #[serde(default, rename = "account")]
    pub accounts: Vec<AccountConfig>,
}

impl AccountsFile {
    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(toml::from_str(&text)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        crate::settings::write_atomic(path, &toml::to_string_pretty(self)?)
    }

    pub fn get(&self, id: &str) -> Option<&AccountConfig> {
        self.accounts.iter().find(|a| a.id == id)
    }
}

/// Make a short, unique, filesystem- and URL-safe account id.
pub fn new_account_id(email: &str, existing: &AccountsFile) -> String {
    let base: String = email
        .split('@')
        .next()
        .unwrap_or("account")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(16)
        .collect::<String>()
        .to_lowercase();
    let base = if base.is_empty() { "account".to_string() } else { base };
    let mut id = base.clone();
    let mut n = 2;
    while existing.get(&id).is_some() {
        id = format!("{base}{n}");
        n += 1;
    }
    id
}

pub const ACCOUNT_COLORS: &[&str] =
    &["#ea4335", "#0078d4", "#8e24aa", "#6d4aff", "#2e7d32", "#f9a825", "#00838f", "#d81b60"];

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str) -> AccountConfig {
        AccountConfig {
            id: id.into(),
            name: "cock.li".into(),
            email: "me@cock.li".into(),
            sender_name: "Me".into(),
            provider: Provider::Cockli,
            color: "#8e24aa".into(),
            username: "me@cock.li".into(),
            auth: AuthMethod::Password,
            imap: ServerConfig { host: "mail.cock.li".into(), port: 993, security: Security::Tls },
            smtp: ServerConfig { host: "mail.cock.li".into(), port: 587, security: Security::StartTls },
        }
    }

    #[test]
    fn round_trip_has_no_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("accounts.toml");
        let mut file = AccountsFile::default();
        file.accounts.push(sample("me"));
        file.accounts.push(AccountConfig {
            auth: AuthMethod::OAuth2 { provider: OAuthProvider::Google },
            ..sample("g")
        });
        file.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.to_lowercase().contains("password ="));
        assert_eq!(AccountsFile::load(&path).unwrap(), file);
    }

    #[test]
    fn ids_are_unique() {
        let mut file = AccountsFile::default();
        assert_eq!(new_account_id("John.Doe@x.com", &file), "johndoe");
        file.accounts.push(sample("johndoe"));
        assert_eq!(new_account_id("john.doe@y.com", &file), "johndoe2");
        assert_eq!(new_account_id("@x", &file), "account");
    }
}
