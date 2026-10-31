//! SQLite cache of folders and messages, with an FTS5 search index.
//!
//! Everything here can be re-downloaded from the server, so the database
//! lives in `$XDG_CACHE_HOME/moho-mail/mail/`.

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::model::{Address, Folder, FolderRole, MessageSummary};

pub use rusqlite::Error as StoreError;
pub type Result<T> = rusqlite::Result<T>;

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS folders (
    id          TEXT PRIMARY KEY,
    account_id  TEXT NOT NULL,
    path        TEXT NOT NULL,
    name        TEXT NOT NULL,
    parent      TEXT,
    role        TEXT NOT NULL,
    selectable  INTEGER NOT NULL DEFAULT 1,
    uidvalidity INTEGER,
    uidnext     INTEGER,
    total       INTEGER NOT NULL DEFAULT 0,
    sort        INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS folders_account ON folders(account_id);
CREATE TABLE IF NOT EXISTS messages (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id  TEXT NOT NULL,
    folder_id   TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    uid         INTEGER NOT NULL,
    message_id  TEXT NOT NULL,
    in_reply_to TEXT,
    refs        TEXT NOT NULL DEFAULT '',
    thread_hint TEXT,
    subject     TEXT NOT NULL,
    from_name   TEXT,
    from_email  TEXT NOT NULL,
    to_json     TEXT NOT NULL DEFAULT '[]',
    cc_json     TEXT NOT NULL DEFAULT '[]',
    date        INTEGER NOT NULL,
    unread      INTEGER NOT NULL,
    flagged     INTEGER NOT NULL,
    answered    INTEGER NOT NULL DEFAULT 0,
    important   INTEGER NOT NULL DEFAULT 0,
    has_attachments INTEGER NOT NULL DEFAULT 0,
    raw         BLOB,
    UNIQUE(folder_id, uid)
);
CREATE INDEX IF NOT EXISTS messages_folder_date ON messages(folder_id, date);
CREATE INDEX IF NOT EXISTS messages_msgid ON messages(account_id, message_id);
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    subject, sender, body, content='', contentless_delete=1, tokenize='unicode61 remove_diacritics 2'
);
CREATE TRIGGER IF NOT EXISTS messages_fts_del AFTER DELETE ON messages BEGIN
    DELETE FROM messages_fts WHERE rowid = old.id;
END;
CREATE TABLE IF NOT EXISTS addresses (
    email      TEXT PRIMARY KEY,
    name       TEXT,
    sent_to    INTEGER NOT NULL DEFAULT 0
);
"#;

/// A folder as reported by the server.
#[derive(Debug, Clone)]
pub struct FolderRecord {
    pub path: String,
    pub name: String,
    pub parent_path: Option<String>,
    pub role: FolderRole,
    pub selectable: bool,
}

/// Headers and flags of a message as fetched from the server.
#[derive(Debug, Clone)]
pub struct NewMessage {
    pub uid: u32,
    pub message_id: String,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub thread_hint: Option<String>,
    pub subject: String,
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub date: DateTime<Utc>,
    pub flags: Flags,
    pub important: bool,
    pub has_attachments: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Flags {
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
}

#[derive(Debug, Clone)]
pub struct FolderState {
    pub uidvalidity: Option<u32>,
    pub uidnext: Option<u32>,
    pub max_uid: u32,
}

/// Where a cached message lives on the server.
#[derive(Debug, Clone)]
pub struct MessageLocation {
    pub id: i64,
    pub account_id: String,
    pub folder_id: String,
    pub folder_path: String,
    pub uid: u32,
}

pub struct Store {
    conn: Connection,
}

pub fn folder_id(account_id: &str, path: &str) -> String {
    format!("{account_id}:{path}")
}

fn role_str(r: FolderRole) -> String {
    serde_json::to_value(r).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

fn role_from(s: &str) -> FolderRole {
    serde_json::from_value(serde_json::Value::String(s.into())).unwrap_or(FolderRole::Other)
}

fn ts(d: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(d, 0).single().unwrap_or_default()
}

const SUMMARY_COLS: &str = "id, account_id, folder_id, message_id, in_reply_to, refs, thread_hint, subject, \
     from_name, from_email, date, unread, flagged, important, has_attachments";

fn summary_row(r: &rusqlite::Row) -> Result<MessageSummary> {
    let refs: String = r.get(5)?;
    Ok(MessageSummary {
        id: r.get::<_, i64>(0)?.to_string(),
        account_id: r.get(1)?,
        folder_id: r.get(2)?,
        message_id: r.get(3)?,
        in_reply_to: r.get(4)?,
        references: refs.split_whitespace().map(str::to_string).collect(),
        thread_hint: r.get(6)?,
        subject: r.get(7)?,
        from: Address { name: r.get(8)?, email: r.get(9)? },
        date: ts(r.get(10)?),
        unread: r.get(11)?,
        flagged: r.get(12)?,
        important: r.get(13)?,
        has_attachments: r.get(14)?,
    })
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// Replace the folder list of an account, keeping sync state of folders
    /// that still exist.
    pub fn sync_folders(&mut self, account_id: &str, folders: &[FolderRecord]) -> Result<()> {
        let tx = self.conn.transaction()?;
        let keep: Vec<String> = folders.iter().map(|f| folder_id(account_id, &f.path)).collect();
        {
            let mut existing = tx.prepare("SELECT id FROM folders WHERE account_id = ?1")?;
            let ids: Vec<String> =
                existing.query_map([account_id], |r| r.get(0))?.collect::<Result<_>>()?;
            for id in ids.iter().filter(|id| !keep.contains(id)) {
                tx.execute("DELETE FROM folders WHERE id = ?1", [id])?;
            }
            let mut up = tx.prepare(
                "INSERT INTO folders (id, account_id, path, name, parent, role, selectable, sort)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, parent = excluded.parent,
                    role = excluded.role, selectable = excluded.selectable, sort = excluded.sort",
            )?;
            for (i, f) in folders.iter().enumerate() {
                up.execute(params![
                    folder_id(account_id, &f.path),
                    account_id,
                    f.path,
                    f.name,
                    f.parent_path.as_ref().map(|p| folder_id(account_id, p)),
                    role_str(f.role),
                    f.selectable,
                    i as i64,
                ])?;
            }
        }
        tx.commit()
    }

    /// Server paths of a folder and everything below it, parents first.
    pub fn subtree_paths(&self, account_id: &str, path: &str, delim: &str) -> Result<Vec<String>> {
        Ok(self.subtree(account_id, path, delim)?.into_iter().map(|(p, _)| p).collect())
    }

    /// Like [`Self::subtree_paths`], with whether each folder can hold messages.
    pub fn subtree(&self, account_id: &str, path: &str, delim: &str) -> Result<Vec<(String, bool)>> {
        let mut st =
            self.conn.prepare("SELECT path, selectable FROM folders WHERE account_id = ?1 ORDER BY length(path), sort")?;
        let below = format!("{path}{delim}");
        let rows = st.query_map([account_id], |r| Ok((r.get::<_, String>(0)?, r.get(1)?)))?.collect::<Result<Vec<_>>>()?;
        Ok(rows.into_iter().filter(|(p, _)| p == path || p.starts_with(&below)).collect())
    }

    /// Follow a server-side rename of a folder (and its subfolders) so their
    /// cached messages and sync state carry over instead of being refetched.
    pub fn rename_folder_tree(&mut self, account_id: &str, from: &str, to: &str, delim: &str) -> Result<()> {
        let paths = self.subtree_paths(account_id, from, delim)?;
        let tx = self.conn.transaction()?;
        // Folder ids change while messages still point at them; check at commit.
        tx.execute_batch("PRAGMA defer_foreign_keys = ON")?;
        for old in &paths {
            let new = format!("{to}{}", &old[from.len()..]);
            let parent = new.rsplit_once(delim).map(|(p, _)| folder_id(account_id, p));
            tx.execute(
                "UPDATE folders SET id = ?1, path = ?2, parent = ?3 WHERE id = ?4",
                params![folder_id(account_id, &new), new, parent, folder_id(account_id, old)],
            )?;
            tx.execute(
                "UPDATE messages SET folder_id = ?1 WHERE folder_id = ?2",
                params![folder_id(account_id, &new), folder_id(account_id, old)],
            )?;
        }
        tx.commit()
    }

    pub fn mark_folder_read(&mut self, folder_id: &str) -> Result<()> {
        self.conn.execute("UPDATE messages SET unread = 0 WHERE folder_id = ?1 AND unread", [folder_id])?;
        Ok(())
    }

    pub fn folder_path(&self, folder_id: &str) -> Result<Option<(String, String)>> {
        self.conn
            .query_row("SELECT account_id, path FROM folders WHERE id = ?1", [folder_id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()
    }

    pub fn folder_by_role(&self, account_id: &str, role: FolderRole) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT path FROM folders WHERE account_id = ?1 AND role = ?2 ORDER BY sort LIMIT 1",
                params![account_id, role_str(role)],
                |r| r.get(0),
            )
            .optional()
    }

    pub fn folder_state(&self, folder_id: &str) -> Result<FolderState> {
        let (uidvalidity, uidnext) = self
            .conn
            .query_row("SELECT uidvalidity, uidnext FROM folders WHERE id = ?1", [folder_id], |r| {
                Ok((r.get::<_, Option<u32>>(0)?, r.get::<_, Option<u32>>(1)?))
            })
            .optional()?
            .unwrap_or((None, None));
        let max_uid: u32 = self.conn.query_row(
            "SELECT COALESCE(MAX(uid), 0) FROM messages WHERE folder_id = ?1",
            [folder_id],
            |r| r.get(0),
        )?;
        Ok(FolderState { uidvalidity, uidnext, max_uid })
    }

    /// Record mailbox status; a changed UIDVALIDITY drops every cached message.
    pub fn set_folder_status(&mut self, folder_id: &str, uidvalidity: Option<u32>, uidnext: Option<u32>, total: u32) -> Result<bool> {
        let tx = self.conn.transaction()?;
        let old: Option<Option<u32>> = tx
            .query_row("SELECT uidvalidity FROM folders WHERE id = ?1", [folder_id], |r| r.get(0))
            .optional()?;
        let reset = matches!((old.flatten(), uidvalidity), (Some(a), Some(b)) if a != b);
        if reset {
            tx.execute("DELETE FROM messages WHERE folder_id = ?1", [folder_id])?;
        }
        tx.execute(
            "UPDATE folders SET uidvalidity = ?2, uidnext = ?3, total = ?4 WHERE id = ?1",
            params![folder_id, uidvalidity, uidnext, total],
        )?;
        tx.commit()?;
        Ok(reset)
    }

    /// Insert newly fetched messages; returns the ids of rows actually added.
    pub fn insert_messages(&mut self, account_id: &str, folder_id: &str, msgs: &[NewMessage]) -> Result<Vec<i64>> {
        let tx = self.conn.transaction()?;
        let mut added = Vec::new();
        {
            let mut ins = tx.prepare(
                "INSERT OR IGNORE INTO messages (account_id, folder_id, uid, message_id, in_reply_to, refs,
                    thread_hint, subject, from_name, from_email, to_json, cc_json, date, unread, flagged,
                    answered, important, has_attachments)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            )?;
            let mut fts = tx.prepare("INSERT INTO messages_fts (rowid, subject, sender, body) VALUES (?1, ?2, ?3, '')")?;
            for m in msgs {
                let n = ins.execute(params![
                    account_id,
                    folder_id,
                    m.uid,
                    m.message_id,
                    m.in_reply_to,
                    m.references.join(" "),
                    m.thread_hint,
                    m.subject,
                    m.from.name,
                    m.from.email,
                    serde_json::to_string(&m.to).unwrap_or_default(),
                    serde_json::to_string(&m.cc).unwrap_or_default(),
                    m.date.timestamp(),
                    !m.flags.seen,
                    m.flags.flagged,
                    m.flags.answered,
                    m.important,
                    m.has_attachments,
                ])?;
                if n > 0 {
                    let id = tx.last_insert_rowid();
                    let sender = format!("{} {}", m.from.name.as_deref().unwrap_or(""), m.from.email);
                    fts.execute(params![id, m.subject, sender])?;
                    added.push(id);
                }
            }
        }
        tx.commit()?;
        Ok(added)
    }

    pub fn uids(&self, folder_id: &str) -> Result<Vec<u32>> {
        let mut st = self.conn.prepare("SELECT uid FROM messages WHERE folder_id = ?1 ORDER BY uid")?;
        let rows = st.query_map([folder_id], |r| r.get(0))?;
        rows.collect()
    }

    pub fn update_flags(&mut self, folder_id: &str, flags: &[(u32, Flags)]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut st = tx.prepare(
                "UPDATE messages SET unread = ?3, flagged = ?4, answered = ?5 WHERE folder_id = ?1 AND uid = ?2",
            )?;
            for (uid, f) in flags {
                st.execute(params![folder_id, uid, !f.seen, f.flagged, f.answered])?;
            }
        }
        tx.commit()
    }

    pub fn remove_uids(&mut self, folder_id: &str, uids: &[u32]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut st = tx.prepare("DELETE FROM messages WHERE folder_id = ?1 AND uid = ?2")?;
            for uid in uids {
                st.execute(params![folder_id, uid])?;
            }
        }
        tx.commit()
    }

    pub fn remove_message(&mut self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM messages WHERE id = ?1", [id]).map(|_| ())
    }

    pub fn delete_account(&mut self, account_id: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM messages WHERE account_id = ?1", [account_id])?;
        tx.execute("DELETE FROM folders WHERE account_id = ?1", [account_id])?;
        tx.commit()
    }

    /// Folder tree of an account with unread counts.
    pub fn folders(&self, account_id: &str) -> Result<Vec<Folder>> {
        let mut st = self.conn.prepare(
            "SELECT f.id, f.path, f.name, f.parent, f.role, f.total,
                    (SELECT COUNT(*) FROM messages m WHERE m.folder_id = f.id AND m.unread)
             FROM folders f WHERE f.account_id = ?1 ORDER BY f.sort",
        )?;
        let rows: Vec<(Folder, Option<String>)> = st
            .query_map([account_id], |r| {
                Ok((
                    Folder {
                        id: r.get(0)?,
                        account_id: account_id.to_string(),
                        path: r.get(1)?,
                        name: r.get(2)?,
                        role: role_from(&r.get::<_, String>(4)?),
                        total: r.get(5)?,
                        unread: r.get(6)?,
                        important: false,
                        children: vec![],
                    },
                    r.get(3)?,
                ))
            })?
            .collect::<Result<_>>()?;
        Ok(build_tree(rows))
    }

    pub fn unread_count(&self, account_id: &str, role: FolderRole) -> Result<u32> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM messages m JOIN folders f ON f.id = m.folder_id
             WHERE m.account_id = ?1 AND f.role = ?2 AND m.unread",
            params![account_id, role_str(role)],
            |r| r.get(0),
        )
    }

    /// Unread messages across the given folders, counting a message that sits
    /// in several of them (Gmail labels) once.
    pub fn unread_in(&self, account_id: &str, folder_ids: &[String]) -> Result<u32> {
        if folder_ids.is_empty() {
            return Ok(0);
        }
        let marks = vec!["?"; folder_ids.len()].join(",");
        let sql = format!(
            "SELECT COUNT(DISTINCT CASE WHEN message_id = '' THEN 'id:' || id ELSE message_id END)
             FROM messages WHERE account_id = ? AND unread AND folder_id IN ({marks})"
        );
        let args = std::iter::once(account_id).chain(folder_ids.iter().map(String::as_str));
        self.conn.query_row(&sql, rusqlite::params_from_iter(args), |r| r.get(0))
    }

    pub fn messages(&self, folder_id: &str, limit: usize) -> Result<Vec<MessageSummary>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {SUMMARY_COLS} FROM messages WHERE folder_id = ?1 ORDER BY date DESC LIMIT ?2"
        ))?;
        let rows = st.query_map(params![folder_id, limit as i64], summary_row)?;
        rows.collect()
    }

    pub fn messages_by_ids(&self, ids: &[i64]) -> Result<Vec<MessageSummary>> {
        let mut st = self.conn.prepare(&format!("SELECT {SUMMARY_COLS} FROM messages WHERE id = ?1"))?;
        let mut out = Vec::new();
        for id in ids {
            if let Some(m) = st.query_row([id], summary_row).optional()? {
                out.push(m);
            }
        }
        Ok(out)
    }

    pub fn summary(&self, id: i64) -> Result<Option<MessageSummary>> {
        Ok(self.messages_by_ids(&[id])?.pop())
    }

    pub fn recipients(&self, id: i64) -> Result<(Vec<Address>, Vec<Address>)> {
        self.conn.query_row("SELECT to_json, cc_json FROM messages WHERE id = ?1", [id], |r| {
            let to: String = r.get(0)?;
            let cc: String = r.get(1)?;
            Ok((serde_json::from_str(&to).unwrap_or_default(), serde_json::from_str(&cc).unwrap_or_default()))
        })
    }

    pub fn location(&self, id: i64) -> Result<Option<MessageLocation>> {
        self.conn
            .query_row(
                "SELECT m.id, m.account_id, m.folder_id, f.path, m.uid
                 FROM messages m JOIN folders f ON f.id = m.folder_id WHERE m.id = ?1",
                [id],
                |r| {
                    Ok(MessageLocation {
                        id: r.get(0)?,
                        account_id: r.get(1)?,
                        folder_id: r.get(2)?,
                        folder_path: r.get(3)?,
                        uid: r.get(4)?,
                    })
                },
            )
            .optional()
    }

    pub fn raw(&self, id: i64) -> Result<Option<Vec<u8>>> {
        Ok(self
            .conn
            .query_row("SELECT raw FROM messages WHERE id = ?1", [id], |r| r.get::<_, Option<Vec<u8>>>(0))
            .optional()?
            .flatten())
    }

    /// Cache the full message and index its text for search.
    pub fn set_raw(&mut self, id: i64, raw: &[u8], body_text: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("UPDATE messages SET raw = ?2 WHERE id = ?1", params![id, raw])?;
        let (subject, sender): (String, String) = tx.query_row(
            "SELECT subject, COALESCE(from_name, '') || ' ' || from_email FROM messages WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        tx.execute("DELETE FROM messages_fts WHERE rowid = ?1", [id])?;
        tx.execute(
            "INSERT INTO messages_fts (rowid, subject, sender, body) VALUES (?1, ?2, ?3, ?4)",
            params![id, subject, sender, body_text],
        )?;
        tx.commit()
    }

    pub fn set_flags(&mut self, id: i64, unread: Option<bool>, flagged: Option<bool>) -> Result<()> {
        if let Some(u) = unread {
            self.conn.execute("UPDATE messages SET unread = ?2 WHERE id = ?1", params![id, u])?;
        }
        if let Some(f) = flagged {
            self.conn.execute("UPDATE messages SET flagged = ?2 WHERE id = ?1", params![id, f])?;
        }
        Ok(())
    }

    /// Whether the user has a message in this thread in their Sent folder.
    pub fn replied_in_thread(&self, account_id: &str, refs: &[String]) -> Result<bool> {
        if refs.is_empty() {
            return Ok(false);
        }
        let mut st = self.conn.prepare(
            "SELECT 1 FROM messages m JOIN folders f ON f.id = m.folder_id
             WHERE m.account_id = ?1 AND f.role = 'sent' AND m.message_id = ?2 LIMIT 1",
        )?;
        for r in refs {
            if st.exists(params![account_id, r])? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Full-text search, optionally within one account or folder. Each word is matched as a prefix.
    pub fn search(&self, query: &str, account_id: Option<&str>, folder_id: Option<&str>, limit: usize) -> Result<Vec<MessageSummary>> {
        let terms: Vec<String> = query
            .split_whitespace()
            .map(|w| w.chars().filter(|c| c.is_alphanumeric() || *c == '@' || *c == '.').collect::<String>())
            .filter(|w| !w.is_empty())
            .map(|w| format!("\"{}\"*", w.replace('"', "")))
            .collect();
        if terms.is_empty() {
            return Ok(vec![]);
        }
        let cols = SUMMARY_COLS.split(", ").map(|c| format!("m.{c}")).collect::<Vec<_>>().join(", ");
        let mut st = self.conn.prepare(&format!(
            "SELECT {cols} FROM messages_fts JOIN messages m ON m.id = messages_fts.rowid
             WHERE messages_fts MATCH ?1 AND (?2 IS NULL OR m.account_id = ?2) AND (?3 IS NULL OR m.folder_id = ?3)
             ORDER BY m.date DESC LIMIT ?4"
        ))?;
        let rows = st.query_map(params![terms.join(" "), account_id, folder_id, limit as i64], summary_row)?;
        rows.collect()
    }

    /// Remember people the user corresponds with (for autocomplete and importance).
    pub fn remember_addresses(&mut self, addrs: &[Address], sent: bool) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut st = tx.prepare(
                "INSERT INTO addresses (email, name, sent_to) VALUES (lower(?1), ?2, ?3)
                 ON CONFLICT(email) DO UPDATE SET name = COALESCE(excluded.name, name),
                    sent_to = sent_to + excluded.sent_to",
            )?;
            for a in addrs {
                st.execute(params![a.email, a.name, sent as i64])?;
            }
        }
        tx.commit()
    }

    pub fn complete_address(&self, prefix: &str, limit: usize) -> Result<Vec<Address>> {
        let like = format!("{}%", prefix.to_lowercase().replace('%', ""));
        let mut st = self.conn.prepare(
            "SELECT name, email FROM addresses WHERE email LIKE ?1 OR lower(name) LIKE ?1
             ORDER BY sent_to DESC LIMIT ?2",
        )?;
        let rows = st.query_map(params![like, limit as i64], |r| Ok(Address { name: r.get(0)?, email: r.get(1)? }))?;
        rows.collect()
    }

    /// Addresses the user has written to; treated like contacts for importance.
    pub fn correspondents(&self) -> Result<HashMap<String, u32>> {
        let mut st = self.conn.prepare("SELECT email, sent_to FROM addresses WHERE sent_to > 0")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect()
    }
}

fn build_tree(rows: Vec<(Folder, Option<String>)>) -> Vec<Folder> {
    let ids: Vec<String> = rows.iter().map(|(f, _)| f.id.clone()).collect();
    let mut children: HashMap<String, Vec<Folder>> = HashMap::new();
    let mut roots_order = Vec::new();
    let mut all: Vec<(Folder, Option<String>)> = rows;
    // Process deepest first so children are attached before their parents move.
    all.sort_by_key(|(f, _)| std::cmp::Reverse(f.path.len()));
    let mut placed: HashMap<String, Folder> = HashMap::new();
    for (mut f, parent) in all {
        if let Some(kids) = children.remove(&f.id) {
            f.children = kids;
            f.children.sort_by_key(|c| ids.iter().position(|i| *i == c.id));
        }
        match parent.filter(|p| ids.contains(p)) {
            Some(p) => children.entry(p).or_default().push(f),
            None => {
                roots_order.push(f.id.clone());
                placed.insert(f.id.clone(), f);
            }
        }
    }
    let mut roots: Vec<Folder> = placed.into_values().collect();
    roots.sort_by_key(|f| ids.iter().position(|i| *i == f.id));
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(path: &str, role: FolderRole, parent: Option<&str>) -> FolderRecord {
        FolderRecord {
            path: path.into(),
            name: path.rsplit('/').next().unwrap().into(),
            parent_path: parent.map(Into::into),
            role,
            selectable: true,
        }
    }

    fn msg(uid: u32, subject: &str, seen: bool) -> NewMessage {
        NewMessage {
            uid,
            message_id: format!("m{uid}@x"),
            in_reply_to: None,
            references: vec![],
            thread_hint: None,
            subject: subject.into(),
            from: Address { name: Some("Alex Pérez".into()), email: "alex@example.com".into() },
            to: vec![Address { name: None, email: "me@example.org".into() }],
            cc: vec![],
            date: Utc.timestamp_opt(1_790_000_000 + uid as i64, 0).unwrap(),
            flags: Flags { seen, ..Default::default() },
            important: false,
            has_attachments: false,
        }
    }

    #[test]
    fn folders_tree_and_counts() {
        let mut s = Store::open_in_memory().unwrap();
        s.sync_folders(
            "a",
            &[
                rec("INBOX", FolderRole::Inbox, None),
                rec("Projects", FolderRole::Other, None),
                rec("Projects/Moho", FolderRole::Other, Some("Projects")),
            ],
        )
        .unwrap();
        s.insert_messages("a", "a:INBOX", &[msg(1, "hello", false), msg(2, "world", true)]).unwrap();
        let f = s.folders("a").unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].unread, 1);
        assert_eq!(f[1].children[0].name, "Moho");
        assert_eq!(s.unread_count("a", FolderRole::Inbox).unwrap(), 1);

        // A folder that disappears from the server is removed with its messages.
        s.sync_folders("a", &[rec("Projects", FolderRole::Other, None)]).unwrap();
        assert!(s.messages("a:INBOX", 10).unwrap().is_empty());
    }

    #[test]
    fn unread_across_folders_counts_each_message_once() {
        let mut s = Store::open_in_memory().unwrap();
        s.sync_folders("a", &[rec("INBOX", FolderRole::Inbox, None), rec("[Gmail]/Important", FolderRole::Important, None)])
            .unwrap();
        s.insert_messages("a", "a:INBOX", &[msg(1, "both", false), msg(2, "inbox only", false), msg(3, "read", true)])
            .unwrap();
        s.insert_messages("a", "a:[Gmail]/Important", &[msg(1, "both", false)]).unwrap();
        let both = ["a:INBOX".to_string(), "a:[Gmail]/Important".to_string()];
        assert_eq!(s.unread_in("a", &both).unwrap(), 2);
        assert_eq!(s.unread_in("a", &both[1..]).unwrap(), 1);
        assert_eq!(s.unread_in("a", &[]).unwrap(), 0);
    }

    #[test]
    fn sync_bookkeeping() {
        let mut s = Store::open_in_memory().unwrap();
        s.sync_folders("a", &[rec("INBOX", FolderRole::Inbox, None)]).unwrap();
        assert!(!s.set_folder_status("a:INBOX", Some(7), Some(10), 3).unwrap());
        let added = s.insert_messages("a", "a:INBOX", &[msg(3, "x", false), msg(5, "y", false)]).unwrap();
        assert_eq!(added.len(), 2);
        // Duplicates are ignored.
        assert!(s.insert_messages("a", "a:INBOX", &[msg(3, "x", false)]).unwrap().is_empty());
        assert_eq!(s.folder_state("a:INBOX").unwrap().max_uid, 5);
        s.update_flags("a:INBOX", &[(3, Flags { seen: true, flagged: true, answered: false })]).unwrap();
        let m = s.messages("a:INBOX", 10).unwrap();
        let three = m.iter().find(|m| m.message_id == "m3@x").unwrap();
        assert!(!three.unread && three.flagged);
        s.remove_uids("a:INBOX", &[5]).unwrap();
        assert_eq!(s.uids("a:INBOX").unwrap(), vec![3]);
        // UIDVALIDITY change wipes the folder.
        assert!(s.set_folder_status("a:INBOX", Some(8), Some(1), 0).unwrap());
        assert!(s.uids("a:INBOX").unwrap().is_empty());
    }

    #[test]
    fn search_headers_and_bodies() {
        let mut s = Store::open_in_memory().unwrap();
        s.sync_folders("a", &[rec("INBOX", FolderRole::Inbox, None)]).unwrap();
        let ids = s.insert_messages("a", "a:INBOX", &[msg(1, "Invoice October", false), msg(2, "Lunch", false)]).unwrap();
        s.set_raw(ids[1], b"raw", "let's get tacos at noon").unwrap();
        assert_eq!(s.search("invoice", None, None, 10).unwrap().len(), 1);
        assert_eq!(s.search("taco", None, None, 10).unwrap()[0].subject, "Lunch");
        assert_eq!(s.search("perez", Some("a"), None, 10).unwrap().len(), 2);
        assert!(s.search("perez", Some("b"), None, 10).unwrap().is_empty());
        s.remove_message(ids[1]).unwrap();
        assert!(s.search("taco", None, None, 10).unwrap().is_empty());
    }

    #[test]
    fn search_within_a_folder() {
        let mut s = Store::open_in_memory().unwrap();
        s.sync_folders("a", &[rec("INBOX", FolderRole::Inbox, None), rec("Work", FolderRole::Other, None)]).unwrap();
        s.insert_messages("a", "a:INBOX", &[msg(1, "Invoice inbox", false)]).unwrap();
        s.insert_messages("a", "a:Work", &[msg(1, "Invoice work", false)]).unwrap();
        assert_eq!(s.search("invoice", None, None, 10).unwrap().len(), 2);
        let hits = s.search("invoice", None, Some("a:Work"), 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].subject, "Invoice work");
    }

    #[test]
    fn renaming_a_folder_tree_keeps_its_messages() {
        let mut s = Store::open_in_memory().unwrap();
        s.sync_folders(
            "a",
            &[
                rec("INBOX", FolderRole::Inbox, None),
                rec("Projects", FolderRole::Other, None),
                rec("Projects/Moho", FolderRole::Other, Some("Projects")),
                rec("ProjectsOld", FolderRole::Other, None),
            ],
        )
        .unwrap();
        s.insert_messages("a", "a:Projects/Moho", &[msg(1, "x", false), msg(2, "y", true)]).unwrap();
        assert_eq!(s.subtree_paths("a", "Projects", "/").unwrap(), ["Projects", "Projects/Moho"]);

        s.rename_folder_tree("a", "Projects", "INBOX/Work", "/").unwrap();
        assert_eq!(s.messages("a:INBOX/Work/Moho", 10).unwrap().len(), 2);
        assert_eq!(s.folder_path("a:INBOX/Work/Moho").unwrap(), Some(("a".into(), "INBOX/Work/Moho".into())));
        // A sibling that merely shares the prefix is left alone.
        assert!(s.folder_path("a:ProjectsOld").unwrap().is_some());
        let inbox = s.folders("a").unwrap().into_iter().find(|f| f.path == "INBOX").unwrap();
        assert_eq!(inbox.children[0].path, "INBOX/Work");
        assert_eq!(inbox.children[0].children[0].unread, 1);

        s.mark_folder_read("a:INBOX/Work/Moho").unwrap();
        assert!(s.messages("a:INBOX/Work/Moho", 10).unwrap().iter().all(|m| !m.unread));
    }

    #[test]
    fn addresses_and_replies() {
        let mut s = Store::open_in_memory().unwrap();
        s.remember_addresses(&[Address { name: Some("Bob".into()), email: "Bob@X.org".into() }], true).unwrap();
        assert_eq!(s.complete_address("bo", 5).unwrap()[0].email, "bob@x.org");
        assert_eq!(s.correspondents().unwrap().get("bob@x.org"), Some(&1));

        s.sync_folders("a", &[rec("Sent", FolderRole::Sent, None)]).unwrap();
        s.insert_messages("a", "a:Sent", &[msg(9, "Re: x", true)]).unwrap();
        assert!(s.replied_in_thread("a", &["m9@x".into()]).unwrap());
        assert!(!s.replied_in_thread("a", &["nope".into()]).unwrap());
    }
}
