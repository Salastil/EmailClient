//! Sends through the test SMTP server and reads the message back over IMAP.
//! Skipped unless MOHO_TEST_SMTP and MOHO_TEST_IMAP are set.

use std::time::Duration;

use mail_core::accounts::{Security, ServerConfig};
use mail_core::model::Address;
use providers::smtp::{self, Outgoing, OutgoingAttachment};
use providers::{imap, Credentials};

fn server(var: &str) -> Option<ServerConfig> {
    let v = std::env::var(var).ok()?;
    let (host, port) = v.rsplit_once(':')?;
    Some(ServerConfig { host: host.into(), port: port.parse().ok()?, security: Security::None })
}

#[tokio::test]
async fn send_and_receive() {
    let (Some(smtp_srv), Some(imap_srv)) = (server("MOHO_TEST_SMTP"), server("MOHO_TEST_IMAP")) else {
        eprintln!("MOHO_TEST_SMTP/MOHO_TEST_IMAP not set; skipping");
        return;
    };
    let creds = Credentials::Password { user: "other@example.org".into(), password: "secret".into() };
    let subject = format!("smtp test {}", std::process::id());
    let out = Outgoing {
        from: Address { name: Some("Other".into()), email: "other@example.org".into() },
        to: vec![Address { name: None, email: "test@example.org".into() }],
        subject: subject.clone(),
        text: "Sent by the integration test".into(),
        attachments: vec![OutgoingAttachment { name: "note.txt".into(), mime: "text/plain".into(), data: b"hi".to_vec() }],
        ..Default::default()
    };
    let (message, message_id) = smtp::build(&out).unwrap();
    smtp::send(&smtp_srv, &creds, &message).await.unwrap();

    let reader = Credentials::Password { user: "test@example.org".into(), password: "secret".into() };
    let mut s = imap::connect(&imap_srv, &reader).await.unwrap();
    for _ in 0..20 {
        for uid in imap::server_uids(&mut s, "INBOX").await.unwrap() {
            let raw = imap::fetch_raw(&mut s, "INBOX", uid).await.unwrap();
            let h = mail_core::parse::parse_headers(&raw);
            if h.message_id.as_deref() == Some(message_id.as_str()) {
                assert_eq!(h.subject, subject);
                assert_eq!(mail_core::parse::parse_message(&raw).attachments[0].name, "note.txt");
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    panic!("sent message never arrived");
}
