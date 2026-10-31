//! Composing and sending mail.

use lettre::message::header::ContentType;
use lettre::message::{Attachment as LAttachment, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::{Credentials as LCredentials, Mechanism};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use mail_core::accounts::{Security, ServerConfig};
use mail_core::model::Address;

use crate::{Credentials, Error, Result};

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct OutgoingAttachment {
    pub name: String,
    pub mime: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct Outgoing {
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub subject: String,
    pub text: String,
    pub html: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub attachments: Vec<OutgoingAttachment>,
}

fn mailbox(a: &Address) -> Result<Mailbox> {
    let email = a.email.trim().parse().map_err(|e| Error::Other(format!("invalid address {}: {e}", a.email)))?;
    Ok(Mailbox::new(a.name.clone().filter(|n| !n.trim().is_empty()), email))
}

/// Parse "Name <a@b>, c@d" into addresses.
pub fn parse_address_list(s: &str) -> Result<Vec<Address>> {
    s.split([',', ';'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mb: Mailbox = p.parse().map_err(|e| Error::Other(format!("invalid address “{p}”: {e}")))?;
            Ok(Address { name: mb.name, email: mb.email.to_string() })
        })
        .collect()
}

/// Build the RFC 5322 message. Returns it with its Message-ID.
pub fn build(o: &Outgoing) -> Result<(Message, String)> {
    let domain = o.from.email.rsplit_once('@').map(|(_, d)| d).unwrap_or("localhost");
    let message_id = format!("{}@{domain}", uuid_like());
    let mut b = Message::builder()
        .from(mailbox(&o.from)?)
        .subject(o.subject.clone())
        .message_id(Some(format!("<{message_id}>")))
        .user_agent("Moho Mail".into());
    for a in &o.to {
        b = b.to(mailbox(a)?);
    }
    for a in &o.cc {
        b = b.cc(mailbox(a)?);
    }
    for a in &o.bcc {
        b = b.bcc(mailbox(a)?);
    }
    if let Some(irt) = &o.in_reply_to {
        b = b.in_reply_to(format!("<{irt}>"));
    }
    if !o.references.is_empty() {
        b = b.references(o.references.iter().map(|r| format!("<{r}>")).collect::<Vec<_>>().join(" "));
    }

    let body = match &o.html {
        Some(html) => MultiPart::alternative_plain_html(o.text.clone(), html.clone()),
        None => MultiPart::mixed().singlepart(SinglePart::plain(o.text.clone())),
    };
    let message = if o.attachments.is_empty() {
        match &o.html {
            Some(_) => b.multipart(body),
            None => b.singlepart(SinglePart::plain(o.text.clone())),
        }
    } else {
        let mut mixed = MultiPart::mixed().multipart(body);
        for a in &o.attachments {
            let ct = ContentType::parse(&a.mime).unwrap_or(ContentType::parse("application/octet-stream").unwrap());
            mixed = mixed.singlepart(LAttachment::new(a.name.clone()).body(a.data.clone(), ct));
        }
        b.multipart(mixed)
    }
    .map_err(|e| Error::Other(format!("cannot build message: {e}")))?;
    Ok((message, message_id))
}

fn uuid_like() -> String {
    use rand::Rng;
    let mut r = rand::thread_rng();
    format!("{:016x}{:016x}", r.gen::<u64>(), r.gen::<u64>())
}

pub async fn send(server: &ServerConfig, creds: &Credentials, message: &Message) -> Result<()> {
    let builder = match server.security {
        Security::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&server.host),
        Security::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&server.host),
        Security::None if server.is_loopback() => Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&server.host)),
        Security::None => return Err(Error::Insecure(server.host.clone())),
    }
    .map_err(|e| Error::Network(e.to_string()))?;
    let (user, secret, mechanisms) = match creds {
        Credentials::Password { user, password } => (user, password, vec![Mechanism::Plain, Mechanism::Login]),
        Credentials::XOAuth2 { user, access_token } => (user, access_token, vec![Mechanism::Xoauth2]),
    };
    let transport = builder
        .port(server.port)
        .credentials(LCredentials::new(user.clone(), secret.clone()))
        .authentication(mechanisms)
        .timeout(Some(std::time::Duration::from_secs(60)))
        .build();
    transport.send(message.clone()).await.map_err(|e| {
        if e.is_permanent() && e.to_string().to_lowercase().contains("auth") {
            Error::Auth(e.to_string())
        } else if e.is_transient() || e.is_permanent() {
            Error::Protocol(e.to_string())
        } else {
            Error::Network(e.to_string())
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_address_lists() {
        let l = parse_address_list("Bob <bob@x.org>; alice@y.com,").unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].name.as_deref(), Some("Bob"));
        assert_eq!(l[1].email, "alice@y.com");
        assert!(parse_address_list("not an address").is_err());
    }

    #[test]
    fn builds_reply_with_attachment() {
        let o = Outgoing {
            from: Address { name: Some("Me".into()), email: "me@x.org".into() },
            to: vec![Address { name: None, email: "you@y.org".into() }],
            subject: "Re: Hi".into(),
            text: "hello".into(),
            in_reply_to: Some("abc@y.org".into()),
            references: vec!["root@y.org".into(), "abc@y.org".into()],
            attachments: vec![OutgoingAttachment { name: "a.txt".into(), mime: "text/plain".into(), data: b"hi".to_vec() }],
            ..Default::default()
        };
        let (m, id) = build(&o).unwrap();
        let raw = String::from_utf8(m.formatted()).unwrap();
        assert!(raw.contains("In-Reply-To: <abc@y.org>"));
        assert!(raw.contains("References: <root@y.org> <abc@y.org>"));
        assert!(raw.contains(&format!("Message-ID: <{id}>")));
        assert!(raw.contains("filename=\"a.txt\""));
    }
}
