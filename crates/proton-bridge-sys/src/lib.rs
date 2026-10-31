//! Safe wrapper around the in-process Proton Mail Bridge.
//!
//! Built with the `bundled` feature, Proton's own bridge code runs inside the
//! app and serves IMAP/SMTP on loopback ports. Without it, every call reports
//! that the bridge is not part of this build.

use serde::Deserialize;
use serde_json::Value;

pub const BRIDGE_VERSION: &str = env!("PROTON_BRIDGE_VERSION");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    NotBuilt,
    Bridge(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotBuilt => f.write_str("this build of Moho Mail does not include the Proton bridge"),
            Error::Bridge(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Deserialize)]
pub struct Status {
    pub imap_port: u16,
    pub smtp_port: u16,
}

/// What a login attempt still needs from the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginStep {
    Done { user_id: String },
    NeedTotp { error: Option<String> },
    NeedMailboxPassword { error: Option<String> },
    /// Proton asks for a CAPTCHA; open `url`, then retry with `hv_token`.
    NeedHumanVerification { url: String, token: String },
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserInfo {
    pub signed_in: bool,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub addresses: Vec<String>,
    #[serde(default)]
    pub bridge_pass: String,
    #[serde(default)]
    pub imap_port: u16,
    #[serde(default)]
    pub smtp_port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BridgeEvent {
    pub kind: String,
    #[serde(default)]
    pub user_id: String,
}

fn check(v: Value) -> Result<Value> {
    match v.get("error").and_then(Value::as_str) {
        Some(e) if v.get("need").is_none() => Err(Error::Bridge(e.to_string())),
        _ => Ok(v),
    }
}

fn parse_login(v: Value) -> Result<LoginStep> {
    let v = check(v)?;
    let err = v.get("error").and_then(Value::as_str).map(str::to_string);
    match v.get("need").and_then(Value::as_str) {
        Some("totp") => Ok(LoginStep::NeedTotp { error: err }),
        Some("mailbox_password") => Ok(LoginStep::NeedMailboxPassword { error: err }),
        Some("human_verification") => Ok(LoginStep::NeedHumanVerification {
            url: v["url"].as_str().unwrap_or_default().into(),
            token: v["token"].as_str().unwrap_or_default().into(),
        }),
        _ => Ok(LoginStep::Done { user_id: v["user_id"].as_str().unwrap_or_default().into() }),
    }
}

#[cfg(feature = "bundled")]
mod ffi {
    use std::ffi::{c_char, CStr, CString};

    use serde_json::Value;

    extern "C" {
        fn moho_bridge_start(config: *const c_char, data: *const c_char, cache: *const c_char, key: *const c_char, version: *const c_char) -> *mut c_char;
        fn moho_bridge_events() -> *mut c_char;
        fn moho_bridge_login(email: *const c_char, password: *const c_char, totp: *const c_char, mailbox: *const c_char, hv: *const c_char) -> *mut c_char;
        fn moho_bridge_user(email: *const c_char) -> *mut c_char;
        fn moho_bridge_logout(email: *const c_char) -> *mut c_char;
        fn moho_bridge_stop();
        fn moho_bridge_free(p: *mut c_char);
    }

    fn c(s: &str) -> CString {
        CString::new(s.replace('\0', "")).expect("NULs removed")
    }

    /// Take ownership of a JSON reply from Go and free it.
    unsafe fn take(p: *mut c_char) -> Value {
        if p.is_null() {
            return Value::Null;
        }
        let s = CStr::from_ptr(p).to_string_lossy().into_owned();
        moho_bridge_free(p);
        serde_json::from_str(&s).unwrap_or(Value::Null)
    }

    pub fn start(config: &str, data: &str, cache: &str, key: &str, version: &str) -> Value {
        let (a, b, cc, k, v) = (c(config), c(data), c(cache), c(key), c(version));
        unsafe { take(moho_bridge_start(a.as_ptr(), b.as_ptr(), cc.as_ptr(), k.as_ptr(), v.as_ptr())) }
    }

    pub fn events() -> Value {
        unsafe { take(moho_bridge_events()) }
    }

    pub fn login(email: &str, password: &str, totp: &str, mailbox: &str, hv: &str) -> Value {
        let (a, b, t, m, h) = (c(email), c(password), c(totp), c(mailbox), c(hv));
        unsafe { take(moho_bridge_login(a.as_ptr(), b.as_ptr(), t.as_ptr(), m.as_ptr(), h.as_ptr())) }
    }

    pub fn user(email: &str) -> Value {
        let a = c(email);
        unsafe { take(moho_bridge_user(a.as_ptr())) }
    }

    pub fn logout(email: &str) -> Value {
        let a = c(email);
        unsafe { take(moho_bridge_logout(a.as_ptr())) }
    }

    pub fn stop() {
        unsafe { moho_bridge_stop() }
    }
}

pub const fn available() -> bool {
    cfg!(feature = "bundled")
}

/// Start the bridge (idempotent). `vault_key` is base64 of at least 32 random bytes.
#[allow(unused_variables)]
pub fn start(config_dir: &str, data_dir: &str, cache_dir: &str, vault_key: &str) -> Result<Status> {
    #[cfg(feature = "bundled")]
    {
        let v = check(ffi::start(config_dir, data_dir, cache_dir, vault_key, BRIDGE_VERSION))?;
        serde_json::from_value(v).map_err(|e| Error::Bridge(e.to_string()))
    }
    #[cfg(not(feature = "bundled"))]
    Err(Error::NotBuilt)
}

#[allow(unused_variables)]
pub fn login(email: &str, password: &str, totp: Option<&str>, mailbox_password: Option<&str>, hv_token: Option<&str>) -> Result<LoginStep> {
    #[cfg(feature = "bundled")]
    {
        parse_login(ffi::login(email, password, totp.unwrap_or(""), mailbox_password.unwrap_or(""), hv_token.unwrap_or("")))
    }
    #[cfg(not(feature = "bundled"))]
    {
        let _ = parse_login;
        Err(Error::NotBuilt)
    }
}

#[allow(unused_variables)]
pub fn user(email: &str) -> Result<UserInfo> {
    #[cfg(feature = "bundled")]
    {
        serde_json::from_value(check(ffi::user(email))?).map_err(|e| Error::Bridge(e.to_string()))
    }
    #[cfg(not(feature = "bundled"))]
    Err(Error::NotBuilt)
}

#[allow(unused_variables)]
pub fn logout(email: &str) -> Result<()> {
    #[cfg(feature = "bundled")]
    {
        check(ffi::logout(email)).map(|_| ())
    }
    #[cfg(not(feature = "bundled"))]
    Err(Error::NotBuilt)
}

pub fn events() -> Vec<BridgeEvent> {
    #[cfg(feature = "bundled")]
    {
        serde_json::from_value(ffi::events()).unwrap_or_default()
    }
    #[cfg(not(feature = "bundled"))]
    Vec::new()
}

pub fn stop() {
    #[cfg(feature = "bundled")]
    ffi::stop();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn login_steps() {
        assert_eq!(parse_login(json!({"user_id": "u1"})).unwrap(), LoginStep::Done { user_id: "u1".into() });
        assert_eq!(parse_login(json!({"need": "totp"})).unwrap(), LoginStep::NeedTotp { error: None });
        assert_eq!(
            parse_login(json!({"need": "mailbox_password", "error": "bad"})).unwrap(),
            LoginStep::NeedMailboxPassword { error: Some("bad".into()) }
        );
        assert!(matches!(
            parse_login(json!({"need": "human_verification", "url": "https://verify.proton.me/?x", "token": "t"})).unwrap(),
            LoginStep::NeedHumanVerification { .. }
        ));
        assert_eq!(parse_login(json!({"error": "nope"})).unwrap_err(), Error::Bridge("nope".into()));
    }

    #[test]
    fn unbundled_build_reports_clearly() {
        if !available() {
            assert_eq!(start("a", "b", "c", "k").unwrap_err(), Error::NotBuilt);
            assert!(events().is_empty());
        }
    }
}
