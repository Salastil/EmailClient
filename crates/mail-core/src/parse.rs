//! MIME parsing on top of `mail-parser`.

use chrono::{DateTime, TimeZone, Utc};
use mail_parser::{Address as MpAddress, HeaderValue, MessageParser, MimeHeaders};

use crate::model::{Address, Attachment};

#[derive(Debug, Clone)]
pub struct ParsedHeaders {
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub subject: String,
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub date: Option<DateTime<Utc>>,
    /// Best guess from the top-level Content-Type when only headers are known.
    pub has_attachments: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ParsedBody {
    pub html: Option<String>,
    pub text: Option<String>,
    pub attachments: Vec<Attachment>,
}

fn addresses(a: Option<&MpAddress>) -> Vec<Address> {
    a.map(|a| {
        a.iter()
            .filter_map(|x| {
                Some(Address {
                    name: x.name().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()),
                    email: x.address()?.to_string(),
                })
            })
            .collect()
    })
    .unwrap_or_default()
}

fn id_list(v: &HeaderValue) -> Vec<String> {
    match v {
        HeaderValue::Text(t) => vec![t.to_string()],
        HeaderValue::TextList(l) => l.iter().map(|t| t.to_string()).collect(),
        _ => vec![],
    }
}

pub fn parse_headers(raw: &[u8]) -> ParsedHeaders {
    // A header block alone parses as a message without a body.
    let mut buf = raw.to_vec();
    if !buf.ends_with(b"\r\n\r\n") && !buf.ends_with(b"\n\n") {
        buf.extend_from_slice(b"\r\n\r\n");
    }
    let parser = MessageParser::default();
    let Some(msg) = parser.parse_headers(&buf) else {
        return ParsedHeaders {
            message_id: None,
            in_reply_to: None,
            references: vec![],
            subject: String::new(),
            from: Address { name: None, email: String::new() },
            to: vec![],
            cc: vec![],
            date: None,
            has_attachments: false,
        };
    };
    let ct = msg.content_type();
    let has_attachments = ct.is_some_and(|c| {
        c.ctype().eq_ignore_ascii_case("multipart")
            && c.subtype().is_some_and(|s| s.eq_ignore_ascii_case("mixed"))
    });
    ParsedHeaders {
        message_id: msg.message_id().map(str::to_string),
        in_reply_to: id_list(msg.in_reply_to()).into_iter().next(),
        references: id_list(msg.references()),
        subject: msg.subject().unwrap_or("").trim().to_string(),
        from: addresses(msg.from()).into_iter().next().unwrap_or(Address {
            name: None,
            email: String::new(),
        }),
        to: addresses(msg.to()),
        cc: addresses(msg.cc()),
        date: msg.date().and_then(|d| Utc.timestamp_opt(d.to_timestamp(), 0).single()),
        has_attachments,
    }
}

fn mime_of(part: &mail_parser::MessagePart) -> String {
    part.content_type()
        .map(|c| match c.subtype() {
            Some(s) => format!("{}/{}", c.ctype(), s),
            None => c.ctype().to_string(),
        })
        .unwrap_or_else(|| "application/octet-stream".into())
        .to_lowercase()
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Parse a full RFC 5322 message. Inline `cid:` images are rewritten to
/// `data:` URIs so they display without network access.
pub fn parse_message(raw: &[u8]) -> ParsedBody {
    let Some(msg) = MessageParser::default().parse(raw) else {
        return ParsedBody { text: Some(String::from_utf8_lossy(raw).into_owned()), ..Default::default() };
    };
    let mut html = msg.body_html(0).map(|h| h.into_owned());
    let text = msg.body_text(0).map(|t| t.into_owned());

    let mut attachments = Vec::new();
    for (index, part) in msg.attachments().enumerate() {
        let mime = mime_of(part);
        if let (Some(cid), Some(h)) = (part.content_id(), html.as_mut()) {
            let needle = format!("cid:{}", cid.trim_matches(['<', '>']));
            if mime.starts_with("image/") && h.contains(&needle) {
                *h = h.replace(&needle, &format!("data:{mime};base64,{}", base64(part.contents())));
                continue;
            }
        }
        attachments.push(Attachment {
            index,
            name: part
                .attachment_name()
                .map(str::to_string)
                .unwrap_or_else(|| format!("attachment-{}", index + 1)),
            mime,
            size: part.contents().len() as u64,
        });
    }
    // Plain-text-only messages keep html = None; the UI renders text safely.
    ParsedBody { html, text, attachments }
}

/// Plain text used for the search index.
pub fn index_text(body: &ParsedBody) -> String {
    if let Some(t) = &body.text {
        return t.clone();
    }
    let html = body.html.as_deref().unwrap_or("");
    let stripped = ammonia::Builder::empty().clean(html).to_string();
    stripped.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn attachment(raw: &[u8], index: usize) -> Option<(String, String, Vec<u8>)> {
    let msg = MessageParser::default().parse(raw)?;
    let part = msg.attachments().nth(index)?;
    Some((
        part.attachment_name().map(str::to_string).unwrap_or_else(|| format!("attachment-{}", index + 1)),
        mime_of(part),
        part.contents().to_vec(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIL: &str = "From: \"Alex P\" <alex@example.com>\r\n\
To: me@example.org, Bob <bob@example.org>\r\n\
Subject: =?UTF-8?Q?Caf=C3=A9_plans?=\r\n\
Message-ID: <abc@example.com>\r\n\
In-Reply-To: <parent@example.com>\r\n\
References: <root@example.com> <parent@example.com>\r\n\
Date: Tue, 27 Oct 2026 14:30:00 +0000\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"b1\"\r\n\
\r\n\
--b1\r\n\
Content-Type: multipart/related; boundary=\"b2\"\r\n\
\r\n\
--b2\r\n\
Content-Type: text/html; charset=utf-8\r\n\
\r\n\
<p>Hi <img src=\"cid:logo@x\"></p>\r\n\
--b2\r\n\
Content-Type: image/png\r\n\
Content-ID: <logo@x>\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
iVBORw0KGgo=\r\n\
--b2--\r\n\
--b1\r\n\
Content-Type: application/pdf; name=\"q.pdf\"\r\n\
Content-Disposition: attachment; filename=\"q.pdf\"\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
JVBERi0=\r\n\
--b1--\r\n";

    #[test]
    fn headers() {
        let end = MAIL.find("\r\n\r\n").unwrap();
        let h = parse_headers(&MAIL.as_bytes()[..end]);
        assert_eq!(h.subject, "Café plans");
        assert_eq!(h.message_id.as_deref(), Some("abc@example.com"));
        assert_eq!(h.in_reply_to.as_deref(), Some("parent@example.com"));
        assert_eq!(h.references, vec!["root@example.com", "parent@example.com"]);
        assert_eq!(h.from.name.as_deref(), Some("Alex P"));
        assert_eq!(h.to.len(), 2);
        assert_eq!(h.date.unwrap().to_rfc3339(), "2026-10-27T14:30:00+00:00");
        assert!(h.has_attachments);
    }

    #[test]
    fn body_inlines_cid_and_lists_attachments() {
        let b = parse_message(MAIL.as_bytes());
        let html = b.html.unwrap();
        assert!(html.contains("data:image/png;base64,"), "{html}");
        assert_eq!(b.attachments.len(), 1);
        assert_eq!(b.attachments[0].name, "q.pdf");
        let (name, mime, bytes) = attachment(MAIL.as_bytes(), b.attachments[0].index).unwrap();
        assert_eq!((name.as_str(), mime.as_str()), ("q.pdf", "application/pdf"));
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn base64_encodes() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }
}
