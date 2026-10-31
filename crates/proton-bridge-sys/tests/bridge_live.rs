//! Starts the real in-process bridge. Only built with the `bundled` feature.
#![cfg(feature = "bundled")]

use std::net::TcpStream;
use std::time::Duration;

/// Go's SQLite and ours must be the same library, with full-text search.
#[test]
fn shares_one_sqlite_with_fts5() {
    let c = rusqlite::Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE VIRTUAL TABLE t USING fts5(a, content='', contentless_delete=1)").unwrap();
}

#[test]
fn starts_serves_loopback_and_rejects_bad_login() {
    let dir = tempfile::tempdir().unwrap();
    let p = |s: &str| dir.path().join(s).to_string_lossy().into_owned();
    // 32 zero bytes, base64.
    let key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    let status = proton_bridge_sys::start(&p("config"), &p("data"), &p("cache"), key).unwrap();
    assert!(status.imap_port > 0 && status.smtp_port > 0);

    // Starting twice returns the running instance.
    let again = proton_bridge_sys::start(&p("config"), &p("data"), &p("cache"), key).unwrap();
    assert_eq!(again.imap_port, status.imap_port);

    // IMAP greets on loopback.
    let mut ok = false;
    for _ in 0..50 {
        if let Ok(s) = TcpStream::connect(("127.0.0.1", status.imap_port)) {
            use std::io::Read;
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut buf = [0u8; 64];
            let n = (&s).read(&mut buf).unwrap_or(0);
            ok = String::from_utf8_lossy(&buf[..n]).starts_with("* OK");
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(ok, "bridge IMAP server did not greet");

    let info = proton_bridge_sys::user("nobody@proton.me").unwrap();
    assert!(!info.signed_in);

    let res = proton_bridge_sys::login("moho-test-nonexistent@proton.me", "wrong password", None, None, None);
    eprintln!("bad login result: {res:?}");
    assert!(!matches!(res, Ok(proton_bridge_sys::LoginStep::Done { .. })));

    proton_bridge_sys::stop();
}
