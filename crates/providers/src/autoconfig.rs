//! Find IMAP/SMTP settings for an email address.
//!
//! Order: built-in presets, the domain's own autoconfig file, the Thunderbird
//! ISPDB, then the same lookups for the domain's MX host, and finally a guess.

use mail_core::accounts::{OAuthProvider, Security, ServerConfig};
use mail_core::model::Provider;
use serde::Serialize;

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Discovered {
    pub provider: Provider,
    pub imap: ServerConfig,
    pub smtp: ServerConfig,
    /// Login is the full address (otherwise the local part).
    pub username_is_email: bool,
    pub oauth: Option<OAuthProvider>,
    /// Account needs Proton's bridge rather than direct IMAP.
    pub proton: bool,
    /// Where the settings came from: "preset", "autoconfig", "ispdb", "mx", "guess".
    pub source: &'static str,
}

fn srv(host: &str, port: u16, security: Security) -> ServerConfig {
    ServerConfig { host: host.into(), port, security }
}

const COCKLI_DOMAINS: &[&str] = &[
    "cock.li", "airmail.cc", "420blaze.it", "aaathats3as.com", "cumallover.me", "firemail.cc",
    "getbackinthe.kitchen", "memeware.net", "cocaine.ninja", "waifu.club", "goat.si", "cock.email",
    "cock.lu", "tfwno.gf", "dicksinhisan.us", "dicksinmyan.us", "loves.dicksinhisan.us",
];

/// Settings for well-known providers.
pub fn preset(domain: &str) -> Option<Discovered> {
    let d = domain.to_lowercase();
    let mk = |provider, imap, smtp, oauth| Discovered {
        provider,
        imap,
        smtp,
        username_is_email: true,
        oauth,
        proton: false,
        source: "preset",
    };
    let s = match d.as_str() {
        "gmail.com" | "googlemail.com" | "google.com" => mk(
            Provider::Gmail,
            srv("imap.gmail.com", 993, Security::Tls),
            srv("smtp.gmail.com", 465, Security::Tls),
            Some(OAuthProvider::Google),
        ),
        "outlook.com" | "hotmail.com" | "live.com" | "msn.com" | "passport.com" | "outlook.office365.com"
        | "hotmail.co.uk" | "hotmail.fr" | "hotmail.de" | "hotmail.it" | "hotmail.es" | "live.co.uk"
        | "live.fr" | "live.de" | "outlook.de" | "outlook.fr" | "outlook.es" | "outlook.it" => mk(
            Provider::Outlook,
            srv("outlook.office365.com", 993, Security::Tls),
            srv("smtp-mail.outlook.com", 587, Security::StartTls),
            Some(OAuthProvider::Microsoft),
        ),
        "yahoo.com" | "ymail.com" | "rocketmail.com" | "yahoo.co.uk" | "yahoo.fr" | "yahoo.de" | "yahoo.co.jp"
        | "yahoo.ca" | "yahoo.com.au" | "yahoo.in" => mk(
            Provider::Yahoo,
            srv("imap.mail.yahoo.com", 993, Security::Tls),
            srv("smtp.mail.yahoo.com", 465, Security::Tls),
            Some(OAuthProvider::Yahoo),
        ),
        "aol.com" | "aim.com" => mk(
            Provider::Yahoo,
            srv("imap.aol.com", 993, Security::Tls),
            srv("smtp.aol.com", 465, Security::Tls),
            Some(OAuthProvider::Aol),
        ),
        "icloud.com" | "me.com" | "mac.com" => mk(
            Provider::Icloud,
            srv("imap.mail.me.com", 993, Security::Tls),
            srv("smtp.mail.me.com", 587, Security::StartTls),
            None,
        ),
        "fastmail.com" | "fastmail.fm" | "fastmail.net" | "messagingengine.com" => mk(
            Provider::Fastmail,
            srv("imap.fastmail.com", 993, Security::Tls),
            srv("smtp.fastmail.com", 465, Security::Tls),
            Some(OAuthProvider::Fastmail),
        ),
        "proton.me" | "protonmail.com" | "protonmail.ch" | "pm.me" => Discovered {
            proton: true,
            ..mk(
                Provider::Proton,
                srv("127.0.0.1", 1143, Security::None),
                srv("127.0.0.1", 1025, Security::None),
                None,
            )
        },
        "zoho.com" | "zohomail.com" => mk(
            Provider::Imap,
            srv("imap.zoho.com", 993, Security::Tls),
            srv("smtp.zoho.com", 465, Security::Tls),
            None,
        ),
        "gmx.com" | "gmx.net" | "gmx.de" | "gmx.at" | "gmx.ch" => mk(
            Provider::Imap,
            srv(if d == "gmx.com" { "imap.gmx.com" } else { "imap.gmx.net" }, 993, Security::Tls),
            srv(if d == "gmx.com" { "mail.gmx.com" } else { "mail.gmx.net" }, 587, Security::StartTls),
            None,
        ),
        "mail.com" => mk(
            Provider::Imap,
            srv("imap.mail.com", 993, Security::Tls),
            srv("smtp.mail.com", 587, Security::StartTls),
            None,
        ),
        "yandex.com" | "yandex.ru" => mk(
            Provider::Imap,
            srv("imap.yandex.com", 993, Security::Tls),
            srv("smtp.yandex.com", 465, Security::Tls),
            None,
        ),
        "posteo.de" | "posteo.net" => mk(
            Provider::Imap,
            srv("posteo.de", 993, Security::Tls),
            srv("posteo.de", 465, Security::Tls),
            None,
        ),
        "mailbox.org" => mk(
            Provider::Imap,
            srv("imap.mailbox.org", 993, Security::Tls),
            srv("smtp.mailbox.org", 465, Security::Tls),
            None,
        ),
        "disroot.org" => mk(
            Provider::Imap,
            srv("disroot.org", 993, Security::Tls),
            srv("disroot.org", 587, Security::StartTls),
            None,
        ),
        "riseup.net" => mk(
            Provider::Imap,
            srv("mail.riseup.net", 993, Security::Tls),
            srv("mail.riseup.net", 465, Security::Tls),
            None,
        ),
        _ if COCKLI_DOMAINS.contains(&d.as_str()) => mk(
            Provider::Cockli,
            srv("mail.cock.li", 993, Security::Tls),
            srv("mail.cock.li", 587, Security::StartTls),
            None,
        ),
        _ => return None,
    };
    Some(s)
}

/// Parse a Thunderbird autoconfig (config-v1.1) document.
pub fn parse_autoconfig(xml: &str, email: &str) -> Option<Discovered> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    let pick = |tag: &str, kind: &str| -> Option<(ServerConfig, bool, bool)> {
        let mut best: Option<(u8, ServerConfig, bool, bool)> = None;
        for n in doc.descendants().filter(|n| n.has_tag_name(tag) && n.attribute("type") == Some(kind)) {
            let text = |name: &str| n.children().find(|c| c.has_tag_name(name)).and_then(|c| c.text()).map(str::trim);
            let host = text("hostname")?.replace("%EMAILDOMAIN%", email.rsplit('@').next().unwrap_or(""));
            let port: u16 = text("port")?.parse().ok()?;
            let (security, rank) = match text("socketType")?.to_uppercase().as_str() {
                "SSL" => (Security::Tls, 3),
                "STARTTLS" => (Security::StartTls, 2),
                _ => continue,
            };
            let username_is_email = text("username").map(|u| u.contains("%EMAILADDRESS%")).unwrap_or(true);
            let oauth = n
                .children()
                .filter(|c| c.has_tag_name("authentication"))
                .any(|c| c.text().is_some_and(|t| t.trim().eq_ignore_ascii_case("OAuth2")));
            if best.as_ref().is_none_or(|b| rank > b.0) {
                best = Some((rank, srv(&host, port, security), username_is_email, oauth));
            }
        }
        best.map(|(_, s, u, o)| (s, u, o))
    };
    let (imap, username_is_email, oauth) = pick("incomingServer", "imap")?;
    let (smtp, _, _) = pick("outgoingServer", "smtp")?;
    let mut d = Discovered {
        provider: Provider::Imap,
        imap,
        smtp,
        username_is_email,
        oauth: None,
        proton: false,
        source: "autoconfig",
    };
    // Map well-known hosts back onto presets so OAuth is offered.
    if let Some(p) = preset_for_host(&d.imap.host) {
        d.provider = p.provider;
        d.oauth = p.oauth;
    } else if oauth {
        d.oauth = None;
    }
    Some(d)
}

fn preset_for_host(host: &str) -> Option<Discovered> {
    let h = host.to_lowercase();
    if h.ends_with("gmail.com") || h.ends_with("google.com") || h.ends_with("googlemail.com") {
        preset("gmail.com")
    } else if h.ends_with("office365.com") || h.ends_with("outlook.com") || h.ends_with("hotmail.com") {
        preset("outlook.com")
    } else if h.ends_with("yahoo.com") {
        preset("yahoo.com")
    } else if h.ends_with("fastmail.com") || h.ends_with("messagingengine.com") {
        preset("fastmail.com")
    } else if h.ends_with("cock.li") {
        preset("cock.li")
    } else {
        None
    }
}

/// Map a mail exchanger host to a provider preset (Google Workspace, Microsoft 365, ...).
pub fn preset_for_mx(mx: &str) -> Option<Discovered> {
    let h = mx.trim_end_matches('.').to_lowercase();
    if h.ends_with("google.com") || h.ends_with("googlemail.com") {
        preset("gmail.com")
    } else if h.ends_with("protection.outlook.com") || h.ends_with("outlook.com") {
        preset("outlook.com")
    } else if h.ends_with("yahoodns.net") {
        preset("yahoo.com")
    } else if h.ends_with("messagingengine.com") {
        preset("fastmail.com")
    } else if h.ends_with("protonmail.ch") {
        preset("proton.me")
    } else if h.ends_with("zoho.com") || h.ends_with("zoho.eu") {
        preset("zoho.com")
    } else if h.ends_with("cock.li") {
        preset("cock.li")
    } else {
        None
    }
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent("Moho Mail")
        .build()
        .expect("static client config")
}

async fn fetch_autoconfig(client: &reqwest::Client, domain: &str, email: &str) -> Option<Discovered> {
    let urls = [
        format!("https://autoconfig.{domain}/mail/config-v1.1.xml?emailaddress={email}"),
        format!("https://{domain}/.well-known/autoconfig/mail/config-v1.1.xml?emailaddress={email}"),
        format!("https://autoconfig.thunderbird.net/v1.1/{domain}"),
    ];
    for (i, url) in urls.iter().enumerate() {
        let Ok(resp) = client.get(url).send().await else { continue };
        if !resp.status().is_success() {
            continue;
        }
        let Ok(text) = resp.text().await else { continue };
        if let Some(mut d) = parse_autoconfig(&text, email) {
            if d.source == "autoconfig" && i == 2 {
                d.source = "ispdb";
            }
            return Some(d);
        }
    }
    None
}

async fn mx_hosts(domain: &str) -> Vec<String> {
    let Ok(resolver) = hickory_resolver::TokioAsyncResolver::tokio_from_system_conf() else {
        return vec![];
    };
    match resolver.mx_lookup(domain).await {
        Ok(mx) => {
            let mut v: Vec<(u16, String)> = mx.iter().map(|m| (m.preference(), m.exchange().to_utf8())).collect();
            v.sort();
            v.into_iter().map(|(_, h)| h.trim_end_matches('.').to_string()).collect()
        }
        Err(_) => vec![],
    }
}

/// Registrable domain of a host, approximated as its last two labels.
fn base_domain(host: &str) -> String {
    let labels: Vec<&str> = host.trim_end_matches('.').split('.').collect();
    labels[labels.len().saturating_sub(2)..].join(".")
}

pub async fn discover(email: &str) -> Result<Discovered> {
    let domain = email
        .rsplit_once('@')
        .map(|(_, d)| d.trim().to_lowercase())
        .filter(|d| d.contains('.'))
        .ok_or_else(|| Error::Other("enter a full email address".into()))?;
    if let Some(p) = preset(&domain) {
        return Ok(p);
    }
    let client = http();
    if let Some(d) = fetch_autoconfig(&client, &domain, email).await {
        return Ok(d);
    }
    if let Some(mx) = mx_hosts(&domain).await.into_iter().next() {
        if let Some(mut p) = preset_for_mx(&mx) {
            p.source = "mx";
            return Ok(p);
        }
        let mx_domain = base_domain(&mx);
        if mx_domain != domain {
            if let Some(mut d) = fetch_autoconfig(&client, &mx_domain, email).await {
                d.source = "mx";
                return Ok(d);
            }
        }
    }
    Ok(Discovered {
        provider: Provider::Imap,
        imap: srv(&format!("imap.{domain}"), 993, Security::Tls),
        smtp: srv(&format!("smtp.{domain}"), 587, Security::StartTls),
        username_is_email: true,
        oauth: None,
        proton: false,
        source: "guess",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets() {
        let g = preset("GMail.com").unwrap();
        assert_eq!(g.oauth, Some(OAuthProvider::Google));
        assert_eq!(g.imap.host, "imap.gmail.com");
        let c = preset("airmail.cc").unwrap();
        assert_eq!(c.provider, Provider::Cockli);
        assert_eq!(c.smtp, srv("mail.cock.li", 587, Security::StartTls));
        assert!(preset("pm.me").unwrap().proton);
        assert!(preset("example.org").is_none());
        assert_eq!(preset_for_mx("aspmx.l.google.com.").unwrap().provider, Provider::Gmail);
        assert_eq!(base_domain("mx1.mail.example.co."), "example.co");
    }

    #[test]
    fn parses_ispdb_xml() {
        let xml = r#"<?xml version="1.0"?>
<clientConfig version="1.1"><emailProvider id="example.org">
  <incomingServer type="pop3"><hostname>pop.example.org</hostname><port>995</port><socketType>SSL</socketType></incomingServer>
  <incomingServer type="imap"><hostname>imap.example.org</hostname><port>143</port><socketType>STARTTLS</socketType><username>%EMAILLOCALPART%</username></incomingServer>
  <incomingServer type="imap"><hostname>imap.example.org</hostname><port>993</port><socketType>SSL</socketType><username>%EMAILLOCALPART%</username><authentication>password-cleartext</authentication></incomingServer>
  <outgoingServer type="smtp"><hostname>smtp.%EMAILDOMAIN%</hostname><port>587</port><socketType>STARTTLS</socketType><username>%EMAILADDRESS%</username></outgoingServer>
</emailProvider></clientConfig>"#;
        let d = parse_autoconfig(xml, "me@example.org").unwrap();
        assert_eq!(d.imap, srv("imap.example.org", 993, Security::Tls));
        assert_eq!(d.smtp, srv("smtp.example.org", 587, Security::StartTls));
        assert!(!d.username_is_email);
    }
}
