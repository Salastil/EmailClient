//! Folder management against a real IMAP server started by `scripts/test-servers.sh`.
//! Skipped unless MOHO_TEST_IMAP=host:port is set.

use std::sync::Mutex;

use mail_core::accounts::{Security, ServerConfig};
use mail_core::store::{folder_id, Store};
use providers::{imap, Credentials};

fn server() -> Option<ServerConfig> {
    let v = std::env::var("MOHO_TEST_IMAP").ok()?;
    let (host, port) = v.rsplit_once(':')?;
    Some(ServerConfig { host: host.into(), port: port.parse().ok()?, security: Security::None })
}

fn creds() -> Credentials {
    Credentials::Password { user: "test@example.org".into(), password: "secret".into() }
}

fn mail(id: &str, subject: &str) -> Vec<u8> {
    format!(
        "From: Alex <alex@example.com>\r\nTo: test@example.org\r\nSubject: {subject}\r\n\
         Message-ID: <{id}>\r\nDate: Tue, 27 Oct 2026 10:00:00 +0000\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\r\nHello\r\n"
    )
    .into_bytes()
}

async fn paths(s: &mut imap::Session) -> Vec<String> {
    imap::list_folders(s).await.unwrap().into_iter().map(|f| f.path).collect()
}

#[tokio::test]
async fn create_rename_move_copy_mark_read_and_delete() {
    let Some(server) = server() else {
        eprintln!("MOHO_TEST_IMAP not set; skipping");
        return;
    };
    let mut s = imap::connect(&server, &creds()).await.unwrap();
    let delim = imap::delimiter(&mut s).await.unwrap();
    // Start from a clean slate if an earlier run was interrupted.
    let mut stale: Vec<String> = imap::list_folders(&mut s)
        .await
        .unwrap()
        .into_iter()
        .filter(|f| ["Projects", "Work", "Résumés", "Projects empty"].contains(&f.name.as_str()))
        .map(|f| f.path)
        .collect();
    stale.sort_by_key(|p| std::cmp::Reverse(p.len()));
    for p in stale {
        let _ = imap::delete_folder(&mut s, &p).await;
    }

    // Create a folder with a non-ASCII name and a subfolder in it.
    let projects = imap::child_path(None, "Projects", &delim).unwrap();
    imap::create_folder(&mut s, &projects).await.unwrap();
    let resumes = imap::child_path(Some(&projects), "Résumés", &delim).unwrap();
    assert_eq!(resumes, format!("Projects{delim}R&AOk-sum&AOk-s"));
    imap::create_folder(&mut s, &resumes).await.unwrap();
    assert!(imap::child_path(None, &format!("a{delim}b"), &delim).is_err());
    assert!(imap::child_path(None, "  ", &delim).is_err());

    imap::append(&mut s, &resumes, "()", &mail("a@x", "One")).await.unwrap();
    imap::append(&mut s, &resumes, "()", &mail("b@x", "Two")).await.unwrap();

    let store = Mutex::new(Store::open_in_memory().unwrap());
    imap::sync_account(&mut s, "t", &store).await.unwrap();
    let names: Vec<String> = imap::list_folders(&mut s).await.unwrap().into_iter().map(|f| f.name).collect();
    assert!(names.contains(&"Résumés".to_string()));
    assert_eq!(store.lock().unwrap().messages(&folder_id("t", &resumes), 10).unwrap().len(), 2);

    // Rename the parent: the server and the store follow, cached mail included.
    let work = imap::child_path(None, "Work", &delim).unwrap();
    imap::rename_folder(&mut s, &projects, &work).await.unwrap();
    store.lock().unwrap().rename_folder_tree("t", &projects, &work, &delim).unwrap();
    let moved = format!("{work}{delim}{}", imap::leaf(&resumes, &delim));
    let listed = paths(&mut s).await;
    assert!(listed.contains(&moved) && !listed.contains(&resumes));
    imap::sync_account(&mut s, "t", &store).await.unwrap();
    assert_eq!(store.lock().unwrap().messages(&folder_id("t", &moved), 10).unwrap().len(), 2);

    // Mark everything in it read, on the server and locally.
    imap::mark_all_seen(&mut s, &moved).await.unwrap();
    store.lock().unwrap().mark_folder_read(&folder_id("t", &moved)).unwrap();
    imap::sync_account(&mut s, "t", &store).await.unwrap();
    assert!(store.lock().unwrap().messages(&folder_id("t", &moved), 10).unwrap().iter().all(|m| !m.unread));

    // Copy the subfolder to the top level with its messages.
    let copy = imap::leaf(&moved, &delim);
    imap::create_folder(&mut s, &copy).await.unwrap();
    imap::copy_all(&mut s, &moved, &copy).await.unwrap();
    assert_eq!(imap::server_uids(&mut s, &copy).await.unwrap().len(), 2);
    // Copying an empty folder is fine too.
    let empty = imap::child_path(None, "Projects empty", &delim).unwrap();
    imap::create_folder(&mut s, &empty).await.unwrap();
    imap::copy_all(&mut s, &empty, &copy).await.unwrap();

    // Move a folder into Trash, then delete it for good (deepest first).
    let trashed = format!("Trash{delim}{}", imap::leaf(&work, &delim));
    imap::rename_folder(&mut s, &work, &trashed).await.unwrap();
    store.lock().unwrap().rename_folder_tree("t", &work, &trashed, &delim).unwrap();
    imap::sync_account(&mut s, "t", &store).await.unwrap();
    let doomed = store.lock().unwrap().subtree_paths("t", &trashed, &delim).unwrap();
    assert_eq!(doomed, [trashed.clone(), format!("{trashed}{delim}{copy}")]);
    for p in doomed.iter().rev() {
        imap::delete_folder(&mut s, p).await.unwrap();
    }
    for p in [&copy, &empty] {
        imap::delete_folder(&mut s, p).await.unwrap();
    }
    let left = paths(&mut s).await;
    assert!(!left.iter().any(|p| p.starts_with(&trashed) || *p == copy || *p == empty), "{left:?}");
}

#[tokio::test]
async fn copy_a_message_and_keep_the_original() {
    let Some(server) = server() else {
        eprintln!("MOHO_TEST_IMAP not set; skipping");
        return;
    };
    let mut s = imap::connect(&server, &creds()).await.unwrap();
    let delim = imap::delimiter(&mut s).await.unwrap();
    let dest = imap::child_path(None, "Copies", &delim).unwrap();
    let _ = imap::delete_folder(&mut s, &dest).await;
    imap::create_folder(&mut s, &dest).await.unwrap();
    imap::append(&mut s, "INBOX", "()", &mail("copy-me@x", "Copy me")).await.unwrap();
    let uid = *imap::server_uids(&mut s, "INBOX").await.unwrap().iter().max().unwrap();

    imap::copy_message(&mut s, "INBOX", uid, &dest).await.unwrap();
    assert_eq!(imap::server_uids(&mut s, &dest).await.unwrap().len(), 1);
    assert!(imap::server_uids(&mut s, "INBOX").await.unwrap().contains(&uid));

    imap::expunge_message(&mut s, "INBOX", uid).await.unwrap();
    imap::delete_folder(&mut s, &dest).await.unwrap();
}

#[tokio::test]
async fn fetch_several_messages_and_skip_large_ones() {
    let Some(server) = server() else {
        eprintln!("MOHO_TEST_IMAP not set; skipping");
        return;
    };
    let mut s = imap::connect(&server, &creds()).await.unwrap();
    let before: std::collections::HashSet<u32> = imap::server_uids(&mut s, "INBOX").await.unwrap().into_iter().collect();
    imap::append(&mut s, "INBOX", "()", &mail("small-1@x", "Small one")).await.unwrap();
    imap::append(&mut s, "INBOX", "()", &mail("small-2@x", "Small two")).await.unwrap();
    let mut big = mail("big@x", "Big");
    big.extend(std::iter::repeat_n(b'x', 20_000));
    imap::append(&mut s, "INBOX", "()", &big).await.unwrap();
    let new: Vec<u32> = imap::server_uids(&mut s, "INBOX").await.unwrap().into_iter().filter(|u| !before.contains(u)).collect();
    assert_eq!(new.len(), 3);

    let got = imap::fetch_raws(&mut s, "INBOX", &new, 10_000).await.unwrap();
    let subjects: Vec<String> = got.iter().map(|(_, raw)| String::from_utf8_lossy(raw).into_owned()).collect();
    assert_eq!(got.len(), 2, "the 20 kB message is skipped");
    assert!(subjects.iter().any(|m| m.contains("Small one")) && subjects.iter().any(|m| m.contains("Small two")));
    assert_eq!(imap::fetch_raws(&mut s, "INBOX", &new, u32::MAX).await.unwrap().len(), 3);

    for uid in new {
        imap::expunge_message(&mut s, "INBOX", uid).await.unwrap();
    }
}
