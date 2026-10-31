//! Runs against a real IMAP server started by `scripts/test-servers.sh`.
//! Skipped unless MOHO_TEST_IMAP=host:port is set.

use std::sync::Mutex;
use std::time::Duration;

use mail_core::accounts::{Security, ServerConfig};
use mail_core::model::FolderRole;
use mail_core::store::{folder_id, Store};
use mail_core::threading::build_threads;
use providers::{imap, Credentials};

fn server() -> Option<ServerConfig> {
    let v = std::env::var("MOHO_TEST_IMAP").ok()?;
    let (host, port) = v.rsplit_once(':')?;
    Some(ServerConfig { host: host.into(), port: port.parse().ok()?, security: Security::None })
}

fn creds() -> Credentials {
    Credentials::Password { user: "test@example.org".into(), password: "secret".into() }
}

fn mail(id: &str, irt: Option<&str>, subject: &str) -> Vec<u8> {
    let mut s = format!(
        "From: Alex <alex@example.com>\r\nTo: test@example.org\r\nSubject: {subject}\r\n\
         Message-ID: <{id}>\r\nDate: Tue, 27 Oct 2026 10:00:00 +0000\r\n"
    );
    if let Some(p) = irt {
        s.push_str(&format!("In-Reply-To: <{p}>\r\nReferences: <{p}>\r\n"));
    }
    s.push_str("Content-Type: text/plain; charset=utf-8\r\n\r\nHello there, tacos at noon?\r\n");
    s.into_bytes()
}

async fn clear(session: &mut imap::Session, path: &str) {
    for uid in imap::server_uids(session, path).await.unwrap() {
        imap::expunge_message(session, path, uid).await.unwrap();
    }
}

#[tokio::test]
async fn sync_flags_move_fetch_and_idle() {
    let Some(server) = server() else {
        eprintln!("MOHO_TEST_IMAP not set; skipping");
        return;
    };
    let mut s = imap::connect(&server, &creds()).await.unwrap();
    for p in ["INBOX", "Trash"] {
        clear(&mut s, p).await;
    }
    imap::append(&mut s, "INBOX", "()", &mail("root@x", None, "Pricing")).await.unwrap();
    imap::append(&mut s, "INBOX", "(\\Seen)", &mail("r1@x", Some("root@x"), "Re: Pricing")).await.unwrap();
    imap::append(&mut s, "INBOX", "()", &mail("r2@x", Some("r1@x"), "Re: Pricing")).await.unwrap();

    let store = Mutex::new(Store::open_in_memory().unwrap());
    let first = imap::sync_account(&mut s, "t", &store).await.unwrap();
    assert!(first.new_messages.is_empty(), "first sync reports no 'new' mail");
    let inbox = folder_id("t", "INBOX");
    {
        let st = store.lock().unwrap();
        let folders = st.folders("t").unwrap();
        assert!(folders.iter().any(|f| f.role == FolderRole::Trash));
        assert_eq!(folders.iter().find(|f| f.role == FolderRole::Inbox).unwrap().unread, 2);
        let threads = build_threads(st.messages(&inbox, 100).unwrap());
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].reply_count, 2);
    }

    // New arrival is reported as new.
    imap::append(&mut s, "INBOX", "()", &mail("new@x", None, "Lunch")).await.unwrap();
    let r = imap::sync_paths(&mut s, "t", &store, &["INBOX".into()]).await.unwrap();
    assert_eq!(r.new_messages.len(), 1);
    assert_eq!(r.new_messages[0].1, FolderRole::Inbox);
    let new_id = r.new_messages[0].0;

    // Flags round-trip through the server.
    let loc = store.lock().unwrap().location(new_id).unwrap().unwrap();
    imap::store_flag(&mut s, "INBOX", loc.uid, "\\Flagged", true).await.unwrap();
    imap::sync_paths(&mut s, "t", &store, &["INBOX".into()]).await.unwrap();
    assert!(store.lock().unwrap().summary(new_id).unwrap().unwrap().flagged);

    // Full body download.
    let raw = imap::fetch_raw(&mut s, "INBOX", loc.uid).await.unwrap();
    let body = mail_core::parse::parse_message(&raw);
    assert!(body.text.unwrap().contains("tacos"));

    // Move to Trash.
    imap::move_message(&mut s, "INBOX", loc.uid, "Trash").await.unwrap();
    imap::sync_paths(&mut s, "t", &store, &["INBOX".into(), "Trash".into()]).await.unwrap();
    {
        let st = store.lock().unwrap();
        assert!(st.summary(new_id).unwrap().is_none());
        assert_eq!(st.messages(&folder_id("t", "Trash"), 10).unwrap().len(), 1);
    }

    // IDLE wakes up when another connection delivers mail.
    let idler = imap::connect(&server, &creds()).await.unwrap();
    let waiter = tokio::spawn(async move { imap::idle(idler, "INBOX", Duration::from_secs(20)).await });
    tokio::time::sleep(Duration::from_millis(500)).await;
    imap::append(&mut s, "INBOX", "()", &mail("idle@x", None, "Wake up")).await.unwrap();
    let (_session, changed) = tokio::time::timeout(Duration::from_secs(10), waiter).await.unwrap().unwrap().unwrap();
    assert!(changed);

    // Bad password is an auth error.
    let bad = Credentials::Password { user: "test@example.org".into(), password: "nope".into() };
    assert!(imap::connect(&server, &bad).await.unwrap_err().is_auth());
}
