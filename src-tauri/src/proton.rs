//! Proton Mail through the bridge built into the app (crate
//! `proton-bridge-sys`). It starts on demand, keeps its encrypted vault under
//! our XDG directories with the key in the Secret Service, and serves
//! IMAP/SMTP on loopback ports that only this process connects to.

use std::sync::Mutex;

use base64::Engine as _;
use mail_core::accounts::{AccountConfig, AuthMethod, Security, ServerConfig};
use mail_core::paths::AppPaths;
use mail_core::secrets;
use proton_bridge_sys::{self as bridge, LoginStep, Status};
use providers::{Credentials, Error};
use rand::RngCore;

const VAULT_KEY_SECRET: &str = "proton:vault-key";

pub struct ProtonBridge {
    config: String,
    data: String,
    cache: String,
    status: Mutex<Option<Status>>,
    /// Serializes startup so concurrent callers start the bridge once.
    starting: Mutex<()>,
}

fn other(e: impl std::fmt::Display) -> Error {
    Error::Other(e.to_string())
}

impl ProtonBridge {
    pub fn new(paths: &AppPaths) -> Self {
        let s = |p: std::path::PathBuf| p.to_string_lossy().into_owned();
        Self {
            config: s(paths.proton_vault_dir()),
            data: s(paths.proton_vault_dir()),
            cache: s(paths.proton_cache_dir()),
            status: Mutex::new(None),
            starting: Mutex::new(()),
        }
    }

    pub fn available(&self) -> bool {
        bridge::available()
    }

    fn vault_key() -> Result<String, Error> {
        match secrets::load_named(VAULT_KEY_SECRET) {
            Ok(k) => Ok(k),
            Err(secrets::SecretError::Missing(_)) => {
                let mut key = [0u8; 32];
                rand::rngs::OsRng.fill_bytes(&mut key);
                let k = base64::engine::general_purpose::STANDARD.encode(key);
                secrets::store_named(VAULT_KEY_SECRET, &k).map_err(other)?;
                Ok(k)
            }
            Err(e) => Err(other(e)),
        }
    }

    /// Start the bridge if it isn't running yet. Blocking; call off the UI thread.
    fn ensure_started(&self) -> Result<Status, Error> {
        let _starting = self.starting.lock().unwrap();
        if let Some(s) = self.status.lock().unwrap().clone() {
            return Ok(s);
        }
        if !self.available() {
            return Err(other(bridge::Error::NotBuilt));
        }
        let key = Self::vault_key()?;
        for d in [&self.config, &self.data, &self.cache] {
            std::fs::create_dir_all(d).map_err(other)?;
        }
        let status = bridge::start(&self.config, &self.data, &self.cache, &key).map_err(other)?;
        tracing::info!("Proton bridge {} listening on IMAP {} / SMTP {}", bridge::BRIDGE_VERSION, status.imap_port, status.smtp_port);
        *self.status.lock().unwrap() = Some(status.clone());
        Ok(status)
    }

    async fn started(&self) -> Result<Status, Error> {
        // The bridge is a process-wide singleton, so a short-lived blocking call is fine.
        tokio::task::block_in_place(|| self.ensure_started())
    }

    /// Start the bridge at launch when any Proton account exists.
    pub fn start_if_needed(&self, accounts: &[AccountConfig]) {
        if accounts.iter().any(|a| a.auth == AuthMethod::ProtonBridge) {
            if let Err(e) = self.ensure_started() {
                tracing::warn!("Proton bridge: {e}");
            }
        }
    }

    pub async fn credentials(&self, email: &str) -> Result<Credentials, Error> {
        self.started().await?;
        let info = tokio::task::block_in_place(|| bridge::user(email)).map_err(other)?;
        if !info.signed_in {
            return Err(Error::Auth("Proton signed this account out; sign in again".into()));
        }
        Ok(Credentials::Password { user: email.to_string(), password: info.bridge_pass })
    }

    /// Sign in. Errors starting with `needs:` tell the UI what to ask for next.
    pub async fn login(&self, email: &str, password: &str, totp: Option<&str>, mailbox_password: Option<&str>) -> Result<(), Error> {
        self.started().await?;
        let step = tokio::task::block_in_place(|| {
            bridge::login(email, password, totp.filter(|t| !t.is_empty()), mailbox_password.filter(|m| !m.is_empty()), None)
        })
        .map_err(|e| Error::Auth(e.to_string()))?;
        match step {
            LoginStep::Done { .. } => Ok(()),
            LoginStep::NeedTotp { error } => Err(Error::Auth(format!("needs:totp{}", error.map(|e| format!(": {e}")).unwrap_or_default()))),
            LoginStep::NeedMailboxPassword { error } => {
                Err(Error::Auth(format!("needs:mailbox_password{}", error.map(|e| format!(": {e}")).unwrap_or_default())))
            }
            LoginStep::NeedHumanVerification { url, .. } => Err(Error::Auth(format!(
                "Proton asked for a CAPTCHA. Open {url} in your browser, or sign in once at account.proton.me from this network, then try again."
            ))),
        }
    }

    pub fn logout(&self, email: &str) {
        if self.status.lock().unwrap().is_some() {
            if let Err(e) = bridge::logout(email) {
                tracing::warn!("Proton logout: {e}");
            }
        }
    }

    fn loopback(port: u16) -> ServerConfig {
        ServerConfig { host: "127.0.0.1".into(), port, security: Security::None }
    }

    pub fn imap_server(&self, cfg: &AccountConfig) -> Option<ServerConfig> {
        (cfg.auth == AuthMethod::ProtonBridge)
            .then(|| self.status.lock().unwrap().as_ref().map(|s| Self::loopback(s.imap_port)))
            .flatten()
    }

    pub fn smtp_server(&self, cfg: &AccountConfig) -> Option<ServerConfig> {
        (cfg.auth == AuthMethod::ProtonBridge)
            .then(|| self.status.lock().unwrap().as_ref().map(|s| Self::loopback(s.smtp_port)))
            .flatten()
    }

    pub fn shutdown(&self) {
        if self.status.lock().unwrap().take().is_some() {
            bridge::stop();
        }
    }
}
