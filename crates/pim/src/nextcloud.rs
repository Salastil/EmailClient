//! Nextcloud Login Flow v2: the user approves Moho Mail in their browser and
//! Nextcloud hands back an app password.

use std::time::Duration;

use serde::Deserialize;

use crate::{Error, Result};

#[derive(Debug, Clone, Deserialize)]
pub struct Credentials {
    pub server: String,
    #[serde(rename = "loginName")]
    pub login_name: String,
    #[serde(rename = "appPassword")]
    pub app_password: String,
}

#[derive(Deserialize)]
struct Poll {
    token: String,
    endpoint: String,
}

#[derive(Deserialize)]
struct Start {
    poll: Poll,
    login: String,
}

pub struct PendingLogin {
    pub login_url: String,
    poll: Poll,
}

fn normalize(server: &str) -> String {
    let s = server.trim().trim_end_matches('/');
    if s.starts_with("http://") || s.starts_with("https://") { s.to_string() } else { format!("https://{s}") }
}

pub async fn begin(server: &str) -> Result<PendingLogin> {
    let resp = reqwest::Client::new()
        .post(format!("{}/index.php/login/v2", normalize(server)))
        .header("User-Agent", "Moho Mail")
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(Error::Protocol(format!("{} is not a Nextcloud server ({})", server, resp.status())));
    }
    let s: Start = resp.json().await.map_err(|e| Error::Protocol(e.to_string()))?;
    Ok(PendingLogin { login_url: s.login, poll: s.poll })
}

impl PendingLogin {
    /// Poll until the user approves in the browser (or `timeout` passes).
    pub async fn wait(self, timeout: Duration) -> Result<Credentials> {
        let client = reqwest::Client::new();
        let deadline = tokio::time::Instant::now() + timeout;
        while tokio::time::Instant::now() < deadline {
            let resp = client
                .post(&self.poll.endpoint)
                .form(&[("token", self.poll.token.as_str())])
                .send()
                .await
                .map_err(|e| Error::Network(e.to_string()))?;
            if resp.status().is_success() {
                return resp.json().await.map_err(|e| Error::Protocol(e.to_string()));
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        Err(Error::Auth("timed out waiting for approval in the browser".into()))
    }
}

/// CalDAV/CardDAV root of a Nextcloud server.
pub fn dav_url(server: &str) -> String {
    format!("{}/remote.php/dav/", normalize(server))
}
