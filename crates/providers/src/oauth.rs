//! OAuth 2.0 for native apps (RFC 8252): the user signs in in their own
//! browser and the authorization code comes back to a one-shot listener on
//! the loopback interface. PKCE (RFC 7636) protects the code exchange.

use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use mail_core::accounts::OAuthProvider;
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::{Error, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, Deserialize)]
pub struct ClientConfig {
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
}

pub struct Endpoints {
    pub auth_url: &'static str,
    pub token_url: &'static str,
    pub scopes: &'static [&'static str],
    pub extra: &'static [(&'static str, &'static str)],
    /// Host used in the loopback redirect URI.
    pub redirect_host: &'static str,
}

pub fn endpoints(p: OAuthProvider) -> Endpoints {
    match p {
        OAuthProvider::Google => Endpoints {
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
            token_url: "https://oauth2.googleapis.com/token",
            scopes: &[
                "https://mail.google.com/",
                "https://www.googleapis.com/auth/calendar",
                "https://www.googleapis.com/auth/carddav",
                "openid",
                "email",
            ],
            extra: &[("access_type", "offline"), ("prompt", "consent")],
            redirect_host: "127.0.0.1",
        },
        OAuthProvider::Microsoft => Endpoints {
            auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
            token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
            scopes: &[
                "https://outlook.office.com/IMAP.AccessAsUser.All",
                "https://outlook.office.com/SMTP.Send",
                "offline_access",
                "openid",
                "email",
            ],
            extra: &[("prompt", "select_account")],
            redirect_host: "localhost",
        },
        OAuthProvider::Yahoo => Endpoints {
            auth_url: "https://api.login.yahoo.com/oauth2/request_auth",
            token_url: "https://api.login.yahoo.com/oauth2/get_token",
            scopes: &["mail-w"],
            extra: &[],
            redirect_host: "127.0.0.1",
        },
        OAuthProvider::Aol => Endpoints {
            auth_url: "https://api.login.aol.com/oauth2/request_auth",
            token_url: "https://api.login.aol.com/oauth2/get_token",
            scopes: &["mail-w"],
            extra: &[],
            redirect_host: "127.0.0.1",
        },
        OAuthProvider::Fastmail => Endpoints {
            auth_url: "https://api.fastmail.com/oauth/authorize",
            token_url: "https://api.fastmail.com/oauth/refresh",
            scopes: &[
                "https://www.fastmail.com/dev/protocol-imap",
                "https://www.fastmail.com/dev/protocol-smtp",
                "https://www.fastmail.com/dev/protocol-caldav",
                "https://www.fastmail.com/dev/protocol-carddav",
                "offline_access",
            ],
            extra: &[],
            redirect_host: "127.0.0.1",
        },
    }
}

/// Microsoft Graph calendars need their own consent (a different resource).
pub const MICROSOFT_GRAPH_SCOPES: &[&str] =
    &["https://graph.microsoft.com/Calendars.ReadWrite", "offline_access"];

#[derive(Debug, Clone)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Instant,
    /// Email from the ID token, when the provider returns one.
    pub email: Option<String>,
}

impl Tokens {
    pub fn is_fresh(&self) -> bool {
        self.expires_at > Instant::now() + Duration::from_secs(60)
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    id_token: Option<String>,
}

#[derive(Deserialize)]
struct TokenError {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

fn random_token(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut b);
    URL_SAFE_NO_PAD.encode(b)
}

pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn email_from_id_token(jwt: &str) -> Option<String> {
    let payload = jwt.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v.get("email").or_else(|| v.get("preferred_username"))?.as_str().map(str::to_string)
}

/// An authorization in progress: open `url` in the browser, then `finish`.
pub struct PendingAuth {
    pub url: String,
    listener: TcpListener,
    verifier: String,
    state: String,
    redirect_uri: String,
    token_url: String,
    client: ClientConfig,
}

pub async fn begin(provider: OAuthProvider, client: &ClientConfig, login_hint: Option<&str>, scopes: Option<&[&str]>) -> Result<PendingAuth> {
    begin_with(endpoints(provider), client, login_hint, scopes).await
}

pub async fn begin_with(ep: Endpoints, client: &ClientConfig, login_hint: Option<&str>, scopes: Option<&[&str]>) -> Result<PendingAuth> {
    if client.client_id.trim().is_empty() {
        return Err(Error::Other(
            "no OAuth client ID is configured for this provider; add one in Settings → Advanced".into(),
        ));
    }
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://{}:{port}/", ep.redirect_host);
    let verifier = random_token(48);
    let state = random_token(16);
    let mut url = url::Url::parse(ep.auth_url).expect("static URL");
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("response_type", "code")
            .append_pair("client_id", &client.client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("scope", &scopes.unwrap_or(ep.scopes).join(" "))
            .append_pair("state", &state)
            .append_pair("code_challenge", &pkce_challenge(&verifier))
            .append_pair("code_challenge_method", "S256");
        for (k, v) in ep.extra {
            q.append_pair(k, v);
        }
        if let Some(h) = login_hint.filter(|h| !h.is_empty()) {
            q.append_pair("login_hint", h);
        }
    }
    Ok(PendingAuth {
        url: url.into(),
        listener,
        verifier,
        state,
        redirect_uri,
        token_url: ep.token_url.into(),
        client: client.clone(),
    })
}

const DONE_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Moho Mail</title>\
<body style=\"font-family:sans-serif;background:#121417;color:#e3e5e8;display:grid;place-items:center;height:100vh;margin:0\">\
<div style=\"text-align:center\"><h2>You're signed in</h2><p>You can close this tab and return to Moho Mail.</p></div>";

const FAIL_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Moho Mail</title>\
<body style=\"font-family:sans-serif;background:#121417;color:#e3e5e8;display:grid;place-items:center;height:100vh;margin:0\">\
<div style=\"text-align:center\"><h2>Sign-in was not completed</h2><p>Return to Moho Mail to try again.</p></div>";

impl PendingAuth {
    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    /// Wait for the browser to come back, then exchange the code for tokens.
    pub async fn finish(self, timeout: Duration) -> Result<Tokens> {
        let code = tokio::time::timeout(timeout, self.wait_for_code())
            .await
            .map_err(|_| Error::Auth("timed out waiting for the browser sign-in".into()))??;
        let mut form = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code),
            ("redirect_uri", self.redirect_uri.clone()),
            ("client_id", self.client.client_id.clone()),
            ("code_verifier", self.verifier.clone()),
        ];
        if let Some(s) = &self.client.client_secret {
            form.push(("client_secret", s.clone()));
        }
        token_request(&self.token_url, &form).await
    }

    async fn wait_for_code(&self) -> Result<String> {
        loop {
            let (mut sock, _) = self.listener.accept().await?;
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let target = req.split_whitespace().nth(1).unwrap_or("/");
            let url = url::Url::parse(&format!("http://localhost{target}")).ok();
            let param = |k: &str| {
                url.as_ref()
                    .and_then(|u| u.query_pairs().find(|(name, _)| name == k).map(|(_, v)| v.into_owned()))
            };
            let (code, error) = (param("code"), param("error"));
            if code.is_none() && error.is_none() {
                // Favicon and other stray requests.
                let _ = sock.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
                continue;
            }
            let ok = code.is_some() && param("state").as_deref() == Some(self.state.as_str());
            let page = if ok { DONE_PAGE } else { FAIL_PAGE };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
                page.len()
            );
            let _ = sock.write_all(resp.as_bytes()).await;
            let _ = sock.shutdown().await;
            if let Some(e) = error {
                return Err(Error::Auth(format!("the provider reported: {e}")));
            }
            if !ok {
                return Err(Error::Auth("state mismatch in OAuth redirect".into()));
            }
            return Ok(code.unwrap());
        }
    }
}

async fn token_request(token_url: &str, form: &[(&str, String)]) -> Result<Tokens> {
    let resp = reqwest::Client::new()
        .post(token_url)
        .form(form)
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    let body = resp.text().await.map_err(|e| Error::Network(e.to_string()))?;
    if !status.is_success() {
        let msg = serde_json::from_str::<TokenError>(&body)
            .map(|e| format!("{}{}", e.error, e.error_description.map(|d| format!(": {d}")).unwrap_or_default()))
            .unwrap_or(body);
        return Err(Error::Auth(msg));
    }
    let t: TokenResponse = serde_json::from_str(&body).map_err(|e| Error::Protocol(e.to_string()))?;
    Ok(Tokens {
        email: t.id_token.as_deref().and_then(email_from_id_token),
        access_token: t.access_token,
        refresh_token: t.refresh_token,
        expires_at: Instant::now() + Duration::from_secs(t.expires_in.unwrap_or(3600)),
    })
}

pub async fn refresh(provider: OAuthProvider, client: &ClientConfig, refresh_token: &str, scopes: Option<&[&str]>) -> Result<Tokens> {
    refresh_with(endpoints(provider).token_url, client, refresh_token, scopes).await
}

pub async fn refresh_with(token_url: &str, client: &ClientConfig, refresh_token: &str, scopes: Option<&[&str]>) -> Result<Tokens> {
    let mut form = vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh_token.to_string()),
        ("client_id", client.client_id.clone()),
    ];
    if let Some(s) = &client.client_secret {
        form.push(("client_secret", s.clone()));
    }
    if let Some(sc) = scopes {
        form.push(("scope", sc.join(" ")));
    }
    let mut t = token_request(token_url, &form).await?;
    // Most providers keep the old refresh token valid when they don't rotate it.
    t.refresh_token.get_or_insert_with(|| refresh_token.to_string());
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_is_base64url_sha256() {
        // Expected value computed independently with Python's hashlib.
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUZU1p1r_wW1gFWFOEjXk"),
            "WnCR6fkffTzxLg6_kghEFRg5SF9BC1_hGFad1RCLMME"
        );
    }

    #[test]
    fn reads_email_from_id_token() {
        let payload = URL_SAFE_NO_PAD.encode(br#"{"email":"me@gmail.com"}"#);
        assert_eq!(email_from_id_token(&format!("h.{payload}.s")).as_deref(), Some("me@gmail.com"));
    }

    #[tokio::test]
    async fn loopback_flow_with_fake_provider() {
        // A fake token endpoint on another loopback port.
        let token_srv = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let token_url = format!("http://{}/token", token_srv.local_addr().unwrap());
        tokio::spawn(async move {
            let (mut s, _) = token_srv.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = s.read(&mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            assert!(req.contains("code=the-code"), "{req}");
            assert!(req.contains("code_verifier="), "{req}");
            let body = r#"{"access_token":"at","refresh_token":"rt","expires_in":3600}"#;
            let resp = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len());
            s.write_all(resp.as_bytes()).await.unwrap();
        });
        let ep = Endpoints {
            auth_url: "https://auth.example/authorize",
            token_url: Box::leak(token_url.into_boxed_str()),
            scopes: &["mail"],
            extra: &[],
            redirect_host: "127.0.0.1",
        };
        let pending = begin_with(ep, &ClientConfig { client_id: "cid".into(), client_secret: None }, Some("me@x"), None)
            .await
            .unwrap();
        let url = url::Url::parse(&pending.url).unwrap();
        let state = url.query_pairs().find(|(k, _)| k == "state").unwrap().1.into_owned();
        assert!(pending.url.contains("code_challenge_method=S256"));
        let redirect = pending.redirect_uri().to_string();
        // The "browser" follows the redirect.
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let addr = redirect.trim_start_matches("http://").trim_end_matches('/').to_string();
            let mut s = tokio::net::TcpStream::connect(&addr).await.unwrap();
            s.write_all(b"GET /favicon.ico HTTP/1.1\r\n\r\n").await.unwrap();
            let mut s = tokio::net::TcpStream::connect(&addr).await.unwrap();
            s.write_all(format!("GET /?code=the-code&state={state} HTTP/1.1\r\n\r\n").as_bytes()).await.unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).await.unwrap();
            assert!(out.contains("You're signed in"));
        });
        let tokens = pending.finish(Duration::from_secs(5)).await.unwrap();
        assert_eq!(tokens.access_token, "at");
        assert_eq!(tokens.refresh_token.as_deref(), Some("rt"));
    }
}
