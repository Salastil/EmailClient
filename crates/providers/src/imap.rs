//! IMAP client: connection, folder discovery, incremental sync, message
//! operations and IDLE.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Duration;

use async_imap::extensions::idle::IdleResponse;
use async_imap::types::{Fetch, Flag, NameAttribute};
use chrono::{TimeZone, Utc};
use futures::TryStreamExt;
use mail_core::accounts::{Security, ServerConfig};
use mail_core::model::FolderRole;
use mail_core::parse::parse_headers;
use mail_core::store::{folder_id, Flags, FolderRecord, NewMessage, Store};

use crate::net::{self, MailStream};
use crate::{utf7, Credentials, Error, Result};

pub type Session = async_imap::Session<MailStream>;

/// How many of the newest messages to fetch when a folder is first synced.
pub const INITIAL_SYNC_LIMIT: u32 = 1000;

const HEADER_FIELDS: &str =
    "BODY.PEEK[HEADER.FIELDS (DATE FROM TO CC SUBJECT MESSAGE-ID IN-REPLY-TO REFERENCES CONTENT-TYPE)]";

struct XOAuth2(String);

impl async_imap::Authenticator for XOAuth2 {
    type Response = String;
    fn process(&mut self, _challenge: &[u8]) -> String {
        // A second challenge carries an error description; answer empty to finish.
        std::mem::take(&mut self.0)
    }
}

pub fn xoauth2_string(user: &str, token: &str) -> String {
    format!("user={user}\x01auth=Bearer {token}\x01\x01")
}

/// Connect, secure and authenticate.
pub async fn connect(server: &ServerConfig, creds: &Credentials) -> Result<Session> {
    let stream = match server.security {
        Security::Tls => net::connect_tls(&server.host, server.port).await?,
        Security::StartTls => {
            let mut client = async_imap::Client::new(MailStream::Plain(net::tcp(&server.host, server.port).await?));
            client.read_response().await?;
            client.run_command_and_check_ok("STARTTLS", None).await?;
            match client.into_inner() {
                MailStream::Plain(tcp) => net::upgrade(tcp, &server.host).await?,
                tls => tls,
            }
        }
        Security::None if server.is_loopback() => MailStream::Plain(net::tcp(&server.host, server.port).await?),
        Security::None => return Err(Error::Insecure(server.host.clone())),
    };
    let mut client = async_imap::Client::new(stream);
    if !matches!(server.security, Security::StartTls) {
        client
            .read_response()
            .await?
            .ok_or_else(|| Error::Network("server closed the connection".into()))?;
    }
    let session = match creds {
        Credentials::Password { user, password } => client.login(user, password).await,
        Credentials::XOAuth2 { user, access_token } => {
            client.authenticate("XOAUTH2", XOAuth2(xoauth2_string(user, access_token))).await
        }
    };
    session.map_err(|(e, _)| Error::Auth(e.to_string()))
}

pub struct Capabilities {
    pub gmail: bool,
    pub mv: bool,
    pub uidplus: bool,
    pub idle: bool,
}

pub async fn capabilities(session: &mut Session) -> Result<Capabilities> {
    let caps = session.capabilities().await?;
    Ok(Capabilities {
        gmail: caps.has_str("X-GM-EXT-1"),
        mv: caps.has_str("MOVE"),
        uidplus: caps.has_str("UIDPLUS"),
        idle: caps.has_str("IDLE"),
    })
}

fn role_for(path: &str, attrs: &[NameAttribute]) -> FolderRole {
    for a in attrs {
        let r = match a {
            NameAttribute::Sent => FolderRole::Sent,
            NameAttribute::Drafts => FolderRole::Drafts,
            NameAttribute::Trash => FolderRole::Trash,
            NameAttribute::Junk => FolderRole::Spam,
            NameAttribute::Archive => FolderRole::Archive,
            NameAttribute::All => FolderRole::All,
            NameAttribute::Flagged => FolderRole::Starred,
            NameAttribute::Extension(e) if e.eq_ignore_ascii_case("\\Important") => FolderRole::Important,
            _ => continue,
        };
        return r;
    }
    let leaf = path.rsplit(['/', '.']).next().unwrap_or(path).to_lowercase();
    match leaf.as_str() {
        _ if path.eq_ignore_ascii_case("INBOX") => FolderRole::Inbox,
        "sent" | "sent items" | "sent messages" | "sent mail" => FolderRole::Sent,
        "drafts" | "draft" => FolderRole::Drafts,
        "trash" | "deleted items" | "deleted messages" | "bin" => FolderRole::Trash,
        "junk" | "spam" | "junk e-mail" | "junk email" | "bulk mail" => FolderRole::Spam,
        "archive" | "archives" => FolderRole::Archive,
        _ => FolderRole::Other,
    }
}

/// List every mailbox with its role, display name and parent.
pub async fn list_folders(session: &mut Session) -> Result<Vec<FolderRecord>> {
    let names: Vec<_> = session.list(Some(""), Some("*")).await?.try_collect().await?;
    let mut out: Vec<FolderRecord> = names
        .iter()
        .map(|n| {
            let path = n.name().to_string();
            let delim = n.delimiter().unwrap_or("/");
            let (parent, leaf) = match path.rsplit_once(delim) {
                Some((p, l)) if !delim.is_empty() => (Some(p.to_string()), l.to_string()),
                _ => (None, path.clone()),
            };
            let selectable = !n.attributes().iter().any(|a| matches!(a, NameAttribute::NoSelect))
                && !n.attributes().iter().any(|a| matches!(a, NameAttribute::Extension(e) if e.eq_ignore_ascii_case("\\NonExistent")));
            FolderRecord {
                role: role_for(&path, n.attributes()),
                name: if path.eq_ignore_ascii_case("INBOX") { "Inbox".into() } else { utf7::decode(&leaf) },
                parent_path: parent,
                selectable,
                path,
            }
        })
        .collect();
    // Inbox first, then special folders, then the rest alphabetically.
    let rank = |r: FolderRole| match r {
        FolderRole::Inbox => 0,
        FolderRole::Starred => 1,
        FolderRole::Drafts => 2,
        FolderRole::Sent => 3,
        FolderRole::Archive => 4,
        FolderRole::All => 5,
        FolderRole::Spam => 6,
        FolderRole::Trash => 7,
        _ => 8,
    };
    out.sort_by(|a, b| rank(a.role).cmp(&rank(b.role)).then_with(|| a.path.to_lowercase().cmp(&b.path.to_lowercase())));
    Ok(out)
}

fn flags_of(f: &Fetch) -> Flags {
    let mut out = Flags::default();
    for flag in f.flags() {
        match flag {
            Flag::Seen => out.seen = true,
            Flag::Flagged => out.flagged = true,
            Flag::Answered => out.answered = true,
            _ => {}
        }
    }
    out
}

fn to_new_message(f: &Fetch, folder: &str) -> Option<NewMessage> {
    let uid = f.uid?;
    let h = parse_headers(f.header().unwrap_or_default());
    let date = h
        .date
        .or_else(|| f.internal_date().map(|d| d.with_timezone(&Utc)))
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).unwrap());
    let important = f
        .gmail_labels()
        .is_some_and(|l| l.iter().any(|l| l.eq_ignore_ascii_case("\\Important")));
    Some(NewMessage {
        uid,
        message_id: h.message_id.unwrap_or_else(|| format!("{uid}.{folder}@moho.invalid")),
        in_reply_to: h.in_reply_to,
        references: h.references,
        thread_hint: None,
        subject: h.subject,
        from: h.from,
        to: h.to,
        cc: h.cc,
        date,
        flags: flags_of(f),
        important,
        has_attachments: h.has_attachments,
    })
}

/// Result of syncing one folder.
#[derive(Debug, Default)]
pub struct FolderSync {
    /// Messages that arrived since the previous sync (empty on first sync).
    pub new_ids: Vec<i64>,
    pub changed: bool,
}

/// Incrementally sync one folder into the store.
pub async fn sync_folder(
    session: &mut Session,
    account_id: &str,
    folder: &FolderRecord,
    store: &Mutex<Store>,
    gmail: bool,
) -> Result<FolderSync> {
    let fid = folder_id(account_id, &folder.path);
    let mbox = session.examine(&folder.path).await?;
    let reset = store
        .lock()
        .unwrap()
        .set_folder_status(&fid, mbox.uid_validity, mbox.uid_next, mbox.exists)?;
    let state = store.lock().unwrap().folder_state(&fid)?;
    let first_sync = state.max_uid == 0;
    let mut result = FolderSync { changed: reset, ..Default::default() };

    if mbox.exists == 0 {
        let known = store.lock().unwrap().uids(&fid)?;
        if !known.is_empty() {
            store.lock().unwrap().remove_uids(&fid, &known)?;
            result.changed = true;
        }
        return Ok(result);
    }

    let items = if gmail {
        format!("(UID FLAGS INTERNALDATE X-GM-LABELS {HEADER_FIELDS})")
    } else {
        format!("(UID FLAGS INTERNALDATE {HEADER_FIELDS})")
    };

    // 1. New messages.
    let fetched: Vec<Fetch> = if first_sync {
        let start = mbox.exists.saturating_sub(INITIAL_SYNC_LIMIT) + 1;
        session.fetch(format!("{start}:*"), &items).await?.try_collect().await?
    } else {
        session.uid_fetch(format!("{}:*", state.max_uid + 1), &items).await?.try_collect().await?
    };
    let new: Vec<NewMessage> = fetched
        .iter()
        .filter(|f| f.uid.is_some_and(|u| u > state.max_uid))
        .filter_map(|f| to_new_message(f, &folder.path))
        .collect();
    if !new.is_empty() {
        let added = store.lock().unwrap().insert_messages(account_id, &fid, &new)?;
        result.changed |= !added.is_empty();
        if !first_sync {
            result.new_ids = added;
        }
    }

    // 2. Flag changes and deletions among messages we already had.
    if !first_sync {
        let known = store.lock().unwrap().uids(&fid)?;
        if let (Some(&lo), Some(&hi)) = (known.first(), known.iter().filter(|&&u| u <= state.max_uid).max()) {
            let current: Vec<Fetch> =
                session.uid_fetch(format!("{lo}:{hi}"), "(UID FLAGS)").await?.try_collect().await?;
            let flags: HashMap<u32, Flags> =
                current.iter().filter_map(|f| Some((f.uid?, flags_of(f)))).collect();
            let gone: Vec<u32> =
                known.iter().copied().filter(|u| *u <= hi && !flags.contains_key(u)).collect();
            let updates: Vec<(u32, Flags)> = flags.into_iter().collect();
            let mut s = store.lock().unwrap();
            s.update_flags(&fid, &updates)?;
            if !gone.is_empty() {
                s.remove_uids(&fid, &gone)?;
                result.changed = true;
            }
            result.changed |= !updates.is_empty();
        }
    }
    Ok(result)
}

/// Folders worth keeping in sync. Gmail's "All Mail" and "Starred" only
/// duplicate messages that already live in other folders.
pub fn should_sync(folder: &FolderRecord, gmail: bool) -> bool {
    folder.selectable && !(gmail && matches!(folder.role, FolderRole::All | FolderRole::Starred))
}

#[derive(Debug, Default)]
pub struct AccountSync {
    /// (message id, folder role) of messages that arrived since the last sync.
    pub new_messages: Vec<(i64, FolderRole)>,
    pub changed: bool,
}

/// Refresh the folder list and sync every folder.
pub async fn sync_account(session: &mut Session, account_id: &str, store: &Mutex<Store>) -> Result<AccountSync> {
    let caps = capabilities(session).await?;
    let folders = list_folders(session).await?;
    store.lock().unwrap().sync_folders(account_id, &folders)?;
    let mut out = AccountSync::default();
    for f in folders.iter().filter(|f| should_sync(f, caps.gmail)) {
        match sync_folder(session, account_id, f, store, caps.gmail).await {
            Ok(r) => {
                out.changed |= r.changed;
                out.new_messages.extend(r.new_ids.into_iter().map(|id| (id, f.role)));
            }
            // A single broken mailbox must not stop the rest.
            Err(Error::Protocol(e)) => tracing::warn!("sync of {} failed: {e}", f.path),
            Err(e) => return Err(e),
        }
    }
    Ok(out)
}

/// Sync only the given folders (used after IDLE wake-ups).
pub async fn sync_paths(session: &mut Session, account_id: &str, store: &Mutex<Store>, paths: &[String]) -> Result<AccountSync> {
    let caps = capabilities(session).await?;
    let mut out = AccountSync::default();
    for path in paths {
        let role = store
            .lock()
            .unwrap()
            .folders(account_id)?
            .into_iter()
            .flat_map(flatten)
            .find(|f| &f.path == path)
            .map(|f| f.role)
            .unwrap_or(FolderRole::Other);
        let rec = FolderRecord { path: path.clone(), name: String::new(), parent_path: None, role, selectable: true };
        let r = sync_folder(session, account_id, &rec, store, caps.gmail).await?;
        out.changed |= r.changed;
        out.new_messages.extend(r.new_ids.into_iter().map(|id| (id, role)));
    }
    Ok(out)
}

fn flatten(mut f: mail_core::model::Folder) -> Vec<mail_core::model::Folder> {
    let kids = std::mem::take(&mut f.children);
    let mut out = vec![f];
    out.extend(kids.into_iter().flat_map(flatten));
    out
}

/// Download a complete message without marking it read.
pub async fn fetch_raw(session: &mut Session, path: &str, uid: u32) -> Result<Vec<u8>> {
    session.examine(path).await?;
    let msgs: Vec<Fetch> = session.uid_fetch(uid.to_string(), "(UID BODY.PEEK[])").await?.try_collect().await?;
    msgs.iter()
        .find_map(|m| m.body().map(<[u8]>::to_vec))
        .ok_or_else(|| Error::Protocol("message no longer exists on the server".into()))
}

/// Download several full messages at once, skipping any larger than
/// `max_size` bytes. Returns (uid, message) for what was fetched.
pub async fn fetch_raws(session: &mut Session, path: &str, uids: &[u32], max_size: u32) -> Result<Vec<(u32, Vec<u8>)>> {
    if uids.is_empty() {
        return Ok(vec![]);
    }
    session.examine(path).await?;
    let set = |uids: &[u32]| uids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
    let sizes: Vec<Fetch> = session.uid_fetch(set(uids), "(UID RFC822.SIZE)").await?.try_collect().await?;
    let wanted: Vec<u32> = sizes.iter().filter(|f| f.size.is_some_and(|s| s <= max_size)).filter_map(|f| f.uid).collect();
    let mut out = Vec::new();
    for chunk in wanted.chunks(25) {
        let msgs: Vec<Fetch> = session.uid_fetch(set(chunk), "(UID BODY.PEEK[])").await?.try_collect().await?;
        out.extend(msgs.iter().filter_map(|m| Some((m.uid?, m.body()?.to_vec()))));
    }
    Ok(out)
}

pub async fn store_flag(session: &mut Session, path: &str, uid: u32, flag: &str, on: bool) -> Result<()> {
    session.select(path).await?;
    let op = if on { "+FLAGS.SILENT" } else { "-FLAGS.SILENT" };
    let _: Vec<Fetch> = session.uid_store(uid.to_string(), format!("{op} ({flag})")).await?.try_collect().await?;
    Ok(())
}

/// Move a message to another mailbox, using MOVE when available.
pub async fn move_message(session: &mut Session, path: &str, uid: u32, dest: &str) -> Result<()> {
    let caps = capabilities(session).await?;
    session.select(path).await?;
    if caps.mv {
        session.uid_mv(uid.to_string(), dest).await?;
    } else {
        session.uid_copy(uid.to_string(), dest).await?;
        let _: Vec<Fetch> = session.uid_store(uid.to_string(), "+FLAGS.SILENT (\\Deleted)").await?.try_collect().await?;
        expunge(session, uid, caps.uidplus).await?;
    }
    Ok(())
}

/// Copy a message into another mailbox, leaving the original in place.
pub async fn copy_message(session: &mut Session, path: &str, uid: u32, dest: &str) -> Result<()> {
    session.select(path).await?;
    session.uid_copy(uid.to_string(), dest).await?;
    Ok(())
}

/// Permanently delete a message.
pub async fn expunge_message(session: &mut Session, path: &str, uid: u32) -> Result<()> {
    let caps = capabilities(session).await?;
    session.select(path).await?;
    let _: Vec<Fetch> = session.uid_store(uid.to_string(), "+FLAGS.SILENT (\\Deleted)").await?.try_collect().await?;
    expunge(session, uid, caps.uidplus).await
}

async fn expunge(session: &mut Session, uid: u32, uidplus: bool) -> Result<()> {
    if uidplus {
        let _: Vec<_> = session.uid_expunge(uid.to_string()).await?.try_collect().await?;
    } else {
        let _: Vec<_> = session.expunge().await?.try_collect().await?;
    }
    Ok(())
}

pub async fn append(session: &mut Session, path: &str, flags: &str, raw: &[u8]) -> Result<()> {
    session.append(path, Some(flags), None, raw).await?;
    Ok(())
}

// ----- folder management --------------------------------------------------------

/// The server's hierarchy delimiter (usually "/" or ".").
pub async fn delimiter(session: &mut Session) -> Result<String> {
    let names: Vec<_> = session.list(Some(""), Some("")).await?.try_collect().await?;
    Ok(names.first().and_then(|n| n.delimiter()).filter(|d| !d.is_empty()).unwrap_or("/").to_string())
}

/// Server path for a mailbox called `name` (as typed) under `parent`, or at the top level.
pub fn child_path(parent: Option<&str>, name: &str, delim: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Other("the folder name is empty".into()));
    }
    if name.contains(delim) {
        return Err(Error::Other(format!("folder names cannot contain “{delim}”")));
    }
    let leaf = utf7::encode(name);
    Ok(match parent {
        Some(p) => format!("{p}{delim}{leaf}"),
        None => leaf,
    })
}

/// The last component of a server path.
pub fn leaf(path: &str, delim: &str) -> String {
    path.rsplit_once(delim).map_or(path, |(_, l)| l).to_string()
}

pub async fn create_folder(session: &mut Session, path: &str) -> Result<()> {
    session.create(path).await?;
    Ok(())
}

/// Make sure no folder we are about to delete or rename is the selected one:
/// servers like Dovecot drop the connection when the selected mailbox goes away.
/// EXAMINE is read-only, unlike CLOSE, which would expunge deleted messages.
async fn deselect(session: &mut Session) -> Result<()> {
    session.examine("INBOX").await?;
    Ok(())
}

pub async fn delete_folder(session: &mut Session, path: &str) -> Result<()> {
    deselect(session).await?;
    session.delete(path).await?;
    Ok(())
}

pub async fn rename_folder(session: &mut Session, from: &str, to: &str) -> Result<()> {
    deselect(session).await?;
    session.rename(from, to).await?;
    Ok(())
}

/// Mark every message in a mailbox read.
pub async fn mark_all_seen(session: &mut Session, path: &str) -> Result<()> {
    if session.select(path).await?.exists > 0 {
        let _: Vec<Fetch> = session.uid_store("1:*", "+FLAGS.SILENT (\\Seen)").await?.try_collect().await?;
    }
    Ok(())
}

/// Copy every message of mailbox `from` into the existing mailbox `to`.
pub async fn copy_all(session: &mut Session, from: &str, to: &str) -> Result<()> {
    if session.select(from).await?.exists > 0 {
        session.uid_copy("1:*", to).await?;
    }
    Ok(())
}

/// Wait on INBOX with IDLE until something changes or `timeout` passes.
/// Returns the session and whether the server reported new data.
pub async fn idle(mut session: Session, path: &str, timeout: Duration) -> Result<(Session, bool)> {
    session.examine(path).await?;
    let mut handle = session.idle();
    handle.init().await?;
    let (wait, _stop) = handle.wait_with_timeout(timeout);
    let res = wait.await?;
    let session = handle.done().await?;
    Ok((session, matches!(res, IdleResponse::NewData(_))))
}

/// Uids currently present in a mailbox (used by tests and recovery).
pub async fn server_uids(session: &mut Session, path: &str) -> Result<HashSet<u32>> {
    session.examine(path).await?;
    Ok(session.uid_search("ALL").await?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleOutcome {
    NewData,
    Timeout,
    /// `wake` was notified (user action, or shutdown).
    Woken,
}

/// IDLE on a mailbox until the server reports changes, `timeout` passes or
/// `wake` fires.
pub async fn idle_until(
    mut session: Session,
    path: &str,
    timeout: Duration,
    wake: &tokio::sync::Notify,
) -> Result<(Session, IdleOutcome)> {
    session.examine(path).await?;
    let mut handle = session.idle();
    handle.init().await?;
    let outcome = {
        let (wait, stop) = handle.wait_with_timeout(timeout);
        tokio::pin!(wait);
        tokio::select! {
            res = &mut wait => match res? {
                IdleResponse::NewData(_) => IdleOutcome::NewData,
                _ => IdleOutcome::Timeout,
            },
            _ = wake.notified() => {
                drop(stop);
                let _ = wait.await;
                IdleOutcome::Woken
            }
        }
    };
    let session = handle.done().await?;
    Ok((session, outcome))
}
