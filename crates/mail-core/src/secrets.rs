//! Secrets live only in the Secret Service (GNOME Keyring, KWallet, KeePassXC)
//! and are never written to files.

const SERVICE: &str = "moho-mail";

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("no secret stored for {0}")]
    Missing(String),
    #[error("system keyring unavailable: {0}")]
    Keyring(String),
}

#[derive(Debug, Clone, Copy)]
pub enum SecretKind {
    Password,
    RefreshToken,
}

fn key(account_id: &str, kind: SecretKind) -> String {
    match kind {
        SecretKind::Password => format!("{account_id}:password"),
        SecretKind::RefreshToken => format!("{account_id}:oauth-refresh"),
    }
}

fn entry(name: &str) -> Result<keyring::Entry, SecretError> {
    keyring::Entry::new(SERVICE, name).map_err(|e| SecretError::Keyring(e.to_string()))
}

pub fn store(account_id: &str, kind: SecretKind, value: &str) -> Result<(), SecretError> {
    entry(&key(account_id, kind))?
        .set_password(value)
        .map_err(|e| SecretError::Keyring(e.to_string()))
}

pub fn load(account_id: &str, kind: SecretKind) -> Result<String, SecretError> {
    let name = key(account_id, kind);
    match entry(&name)?.get_password() {
        Ok(v) => Ok(v),
        Err(keyring::Error::NoEntry) => Err(SecretError::Missing(name)),
        Err(e) => Err(SecretError::Keyring(e.to_string())),
    }
}

/// Store an arbitrary named secret (used for CalDAV sources and the Proton vault key).
pub fn store_named(name: &str, value: &str) -> Result<(), SecretError> {
    entry(name)?.set_password(value).map_err(|e| SecretError::Keyring(e.to_string()))
}

pub fn load_named(name: &str) -> Result<String, SecretError> {
    match entry(name)?.get_password() {
        Ok(v) => Ok(v),
        Err(keyring::Error::NoEntry) => Err(SecretError::Missing(name.into())),
        Err(e) => Err(SecretError::Keyring(e.to_string())),
    }
}

pub fn delete_named(name: &str) {
    if let Ok(e) = entry(name) {
        let _ = e.delete_credential();
    }
}

pub fn delete_account(account_id: &str) {
    for kind in [SecretKind::Password, SecretKind::RefreshToken] {
        delete_named(&key(account_id, kind));
    }
}
