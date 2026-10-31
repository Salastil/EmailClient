//! Built-in Proton Mail bridge. Filled in by the bridge integration; until an
//! account signs in, Proton accounts report that the bridge is not running.

use mail_core::accounts::{AccountConfig, ServerConfig};
use mail_core::paths::AppPaths;
use providers::{Credentials, Error};

pub struct ProtonBridge {}

impl ProtonBridge {
    pub fn new(_paths: &AppPaths) -> Self {
        Self {}
    }

    pub async fn credentials(&self, _email: &str) -> Result<Credentials, Error> {
        Err(Error::Other("the Proton bridge is not available in this build".into()))
    }

    pub async fn login(&self, _email: &str, _password: &str, _totp: Option<&str>, _mailbox_password: Option<&str>) -> Result<(), Error> {
        Err(Error::Other("the Proton bridge is not available in this build".into()))
    }

    pub fn imap_server(&self, _cfg: &AccountConfig) -> Option<ServerConfig> {
        None
    }

    pub fn smtp_server(&self, _cfg: &AccountConfig) -> Option<ServerConfig> {
        None
    }
}
