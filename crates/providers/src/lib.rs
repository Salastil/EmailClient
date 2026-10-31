//! Network protocols: IMAP sync, SMTP, OAuth2 sign-in and account autodiscovery.

pub mod autoconfig;
pub mod imap;
pub mod net;
pub mod oauth;
pub mod smtp;
pub mod utf7;

use mail_core::store::StoreError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sign-in failed: {0}")]
    Auth(String),
    #[error("cannot reach the server: {0}")]
    Network(String),
    #[error("server error: {0}")]
    Protocol(String),
    #[error("refusing to send credentials without encryption to {0}")]
    Insecure(String),
    #[error("local cache error: {0}")]
    Store(#[from] StoreError),
    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn is_auth(&self) -> bool {
        matches!(self, Error::Auth(_))
    }
}

impl From<async_imap::error::Error> for Error {
    fn from(e: async_imap::error::Error) -> Self {
        use async_imap::error::Error as E;
        match e {
            E::Io(e) => Error::Network(e.to_string()),
            E::ConnectionLost => Error::Network("connection lost".into()),
            other => Error::Protocol(other.to_string()),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Network(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Credentials for IMAP and SMTP.
#[derive(Clone)]
pub enum Credentials {
    Password { user: String, password: String },
    XOAuth2 { user: String, access_token: String },
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Credentials::Password { user, .. } => write!(f, "Password({user})"),
            Credentials::XOAuth2 { user, .. } => write!(f, "XOAuth2({user})"),
        }
    }
}
