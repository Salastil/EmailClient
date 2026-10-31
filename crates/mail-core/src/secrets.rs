//! Secrets live only in the Secret Service (GNOME Keyring, KWallet, KeePassXC)
//! and are never written to files.
//!
//! Each secret is read from the keyring at most once per run and then kept in
//! memory: a locked keyring asks the user to unlock it on every read, and the
//! sync engine needs credentials on every reconnect.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, Once};
use std::time::{Duration, Instant};

static CACHE: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(Default::default);

const SERVICE: &str = "moho-mail";
const SECRETS_BUS_NAME: &str = "org.freedesktop.secrets";

/// Keyring daemons that provide the Secret Service but don't register for
/// D-Bus activation under its name. Plasma starts KWallet at login; other
/// desktops (Hyprland, Sway, ...) don't, so we start it ourselves.
const STARTABLE_PROVIDERS: &[&str] = &["org.kde.kwalletd6", "org.kde.kwalletd5"];

const NO_KEYRING: &str = "no keyring is running. Start one that provides the Secret Service \
     (KWallet, GNOME Keyring, or KeePassXC with Secret Service integration enabled)";

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

/// Run a keyring operation. If it fails because nothing owns the Secret
/// Service name, start a known keyring daemon (once per process) and retry.
fn with_keyring<T>(name: &str, op: impl Fn(&keyring::Entry) -> keyring::Result<T>) -> Result<T, SecretError> {
    let e = entry(name)?;
    match op(&e) {
        Err(keyring::Error::NoEntry) => Err(SecretError::Missing(name.into())),
        Err(_) if !secret_service_running() => {
            static START: Once = Once::new();
            START.call_once(start_keyring);
            if !secret_service_running() {
                return Err(SecretError::Keyring(NO_KEYRING.into()));
            }
            match op(&e) {
                Ok(v) => Ok(v),
                Err(keyring::Error::NoEntry) => Err(SecretError::Missing(name.into())),
                Err(err) => Err(SecretError::Keyring(err.to_string())),
            }
        }
        Err(err) => Err(SecretError::Keyring(err.to_string())),
        Ok(v) => Ok(v),
    }
}

fn bus_proxy(conn: &dbus::blocking::Connection) -> dbus::blocking::Proxy<'_, &dbus::blocking::Connection> {
    conn.with_proxy("org.freedesktop.DBus", "/org/freedesktop/DBus", Duration::from_secs(5))
}

fn secret_service_running() -> bool {
    let Ok(conn) = dbus::blocking::Connection::new_session() else { return false };
    bus_proxy(&conn)
        .method_call("org.freedesktop.DBus", "NameHasOwner", (SECRETS_BUS_NAME,))
        .is_ok_and(|(owned,): (bool,)| owned)
}

/// Ask the bus to start the first available provider, then wait briefly for
/// it to claim the Secret Service name.
fn start_keyring() {
    let Ok(conn) = dbus::blocking::Connection::new_session() else { return };
    let bus = bus_proxy(&conn);
    let Ok((activatable,)): Result<(Vec<String>,), _> =
        bus.method_call("org.freedesktop.DBus", "ListActivatableNames", ())
    else {
        return;
    };
    let Some(provider) = STARTABLE_PROVIDERS.iter().find(|p| activatable.iter().any(|a| a == *p)) else {
        return;
    };
    let started: Result<(u32,), _> = bus.method_call("org.freedesktop.DBus", "StartServiceByName", (*provider, 0u32));
    if started.is_err() {
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !secret_service_running() {
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub fn store(account_id: &str, kind: SecretKind, value: &str) -> Result<(), SecretError> {
    store_named(&key(account_id, kind), value)
}

pub fn load(account_id: &str, kind: SecretKind) -> Result<String, SecretError> {
    load_named(&key(account_id, kind))
}

/// Store an arbitrary named secret (used for CalDAV sources and the Proton vault key).
pub fn store_named(name: &str, value: &str) -> Result<(), SecretError> {
    with_keyring(name, |e| e.set_password(value))?;
    CACHE.lock().unwrap().insert(name.into(), value.into());
    Ok(())
}

pub fn load_named(name: &str) -> Result<String, SecretError> {
    if let Some(v) = CACHE.lock().unwrap().get(name) {
        return Ok(v.clone());
    }
    let v = with_keyring(name, |e| e.get_password())?;
    CACHE.lock().unwrap().insert(name.into(), v.clone());
    Ok(v)
}

pub fn delete_named(name: &str) {
    CACHE.lock().unwrap().remove(name);
    let _ = with_keyring(name, |e| e.delete_credential());
}

pub fn delete_account(account_id: &str) {
    for kind in [SecretKind::Password, SecretKind::RefreshToken] {
        delete_named(&key(account_id, kind));
    }
}
