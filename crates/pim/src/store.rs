//! Local PIM database in `$XDG_DATA_HOME/moho-mail/pim.sqlite`: the list of
//! calendar/address-book sources, the local "Home" calendar and address
//! book, and cached objects of remote sources.

use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::ical::{self, Event, Todo};
use crate::vcard::{self, Contact};
use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Calendar,
    AddressBook,
}

/// Where a source's data comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    /// Stored only on this computer.
    Local,
    /// CalDAV/CardDAV with a username and app password (Nextcloud, iCloud, Fastmail, Radicale...).
    Dav,
    /// CalDAV/CardDAV using a mail account's OAuth token (Google, Fastmail).
    DavOAuth,
    /// Microsoft Graph using its own OAuth grant.
    Graph,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub kind: SourceKind,
    pub backend: Backend,
    pub name: String,
    pub color: String,
    /// Collection URL (DAV) or calendar id (Graph).
    pub url: String,
    /// Mail account whose token is used (DavOAuth), or a label for the login.
    pub account_id: Option<String>,
    pub username: Option<String>,
    pub writable: bool,
    pub enabled: bool,
    pub events: bool,
    pub todos: bool,
    /// Group label shown in the UI ("you@gmail.com", "Nextcloud").
    pub group: String,
}

pub const HOME_CALENDAR: &str = "local-home";
pub const LOCAL_CONTACTS: &str = "local-contacts";

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
CREATE TABLE IF NOT EXISTS sources (
    id TEXT PRIMARY KEY,
    json TEXT NOT NULL,
    sort INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS objects (
    source_id TEXT NOT NULL,
    href      TEXT NOT NULL,
    etag      TEXT NOT NULL DEFAULT '',
    uid       TEXT NOT NULL DEFAULT '',
    data      TEXT NOT NULL,
    PRIMARY KEY (source_id, href)
);
"#;

/// An occurrence of an event, ready for the UI.
#[derive(Debug, Clone, Serialize)]
pub struct Instance {
    pub source_id: String,
    pub href: String,
    pub uid: String,
    pub summary: String,
    pub description: String,
    pub location: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub all_day: bool,
    pub alarm_minutes: Option<i64>,
    pub recurring: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskItem {
    pub source_id: String,
    pub href: String,
    #[serde(flatten)]
    pub todo: Todo,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContactItem {
    pub source_id: String,
    pub href: String,
    #[serde(flatten)]
    pub contact: Contact,
}

#[derive(Debug, Clone)]
pub struct StoredObject {
    pub href: String,
    pub etag: String,
    pub uid: String,
    pub data: String,
}

pub struct PimStore {
    conn: Connection,
}

impl PimStore {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        let s = Self { conn };
        s.ensure_local_sources()?;
        Ok(s)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        let s = Self { conn };
        s.ensure_local_sources()?;
        Ok(s)
    }

    fn ensure_local_sources(&self) -> Result<()> {
        if self.source(HOME_CALENDAR)?.is_none() {
            self.save_source(&Source {
                id: HOME_CALENDAR.into(),
                kind: SourceKind::Calendar,
                backend: Backend::Local,
                name: "Home".into(),
                color: "#9aa1a9".into(),
                url: String::new(),
                account_id: None,
                username: None,
                writable: true,
                enabled: true,
                events: true,
                todos: true,
                group: "On this computer".into(),
            })?;
        }
        if self.source(LOCAL_CONTACTS)?.is_none() {
            self.save_source(&Source {
                id: LOCAL_CONTACTS.into(),
                kind: SourceKind::AddressBook,
                backend: Backend::Local,
                name: "Personal address book".into(),
                color: "#9aa1a9".into(),
                url: String::new(),
                account_id: None,
                username: None,
                writable: true,
                enabled: true,
                events: false,
                todos: false,
                group: "On this computer".into(),
            })?;
        }
        Ok(())
    }

    pub fn save_source(&self, s: &Source) -> Result<()> {
        let json = serde_json::to_string(s).expect("source serializes");
        self.conn.execute(
            "INSERT INTO sources (id, json, sort) VALUES (?1, ?2, (SELECT COALESCE(MAX(sort), 0) + 1 FROM sources))
             ON CONFLICT(id) DO UPDATE SET json = excluded.json",
            params![s.id, json],
        )?;
        Ok(())
    }

    pub fn source(&self, id: &str) -> Result<Option<Source>> {
        Ok(self
            .conn
            .query_row("SELECT json FROM sources WHERE id = ?1", [id], |r| r.get::<_, String>(0))
            .optional()?
            .and_then(|j| serde_json::from_str(&j).ok()))
    }

    pub fn sources(&self) -> Result<Vec<Source>> {
        let mut st = self.conn.prepare("SELECT json FROM sources ORDER BY sort")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|j| j.ok().and_then(|j| serde_json::from_str(&j).ok())).collect())
    }

    pub fn remove_source(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM objects WHERE source_id = ?1", [id])?;
        self.conn.execute("DELETE FROM sources WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Remove every source tied to a mail account (when the account is removed).
    pub fn remove_account_sources(&self, account_id: &str) -> Result<Vec<String>> {
        let ids: Vec<String> = self
            .sources()?
            .into_iter()
            .filter(|s| s.account_id.as_deref() == Some(account_id))
            .map(|s| s.id)
            .collect();
        for id in &ids {
            self.remove_source(id)?;
        }
        Ok(ids)
    }

    /// Replace all cached objects of a source with a fresh server listing.
    pub fn replace_objects(&mut self, source_id: &str, objects: &[StoredObject]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM objects WHERE source_id = ?1", [source_id])?;
        {
            let mut st = tx.prepare("INSERT OR REPLACE INTO objects (source_id, href, etag, uid, data) VALUES (?1, ?2, ?3, ?4, ?5)")?;
            for o in objects {
                st.execute(params![source_id, o.href, o.etag, o.uid, o.data])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn put_object(&self, source_id: &str, o: &StoredObject) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO objects (source_id, href, etag, uid, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![source_id, o.href, o.etag, o.uid, o.data],
        )?;
        Ok(())
    }

    pub fn object(&self, source_id: &str, href: &str) -> Result<Option<StoredObject>> {
        self.conn
            .query_row(
                "SELECT href, etag, uid, data FROM objects WHERE source_id = ?1 AND href = ?2",
                params![source_id, href],
                |r| Ok(StoredObject { href: r.get(0)?, etag: r.get(1)?, uid: r.get(2)?, data: r.get(3)? }),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn delete_object(&self, source_id: &str, href: &str) -> Result<()> {
        self.conn.execute("DELETE FROM objects WHERE source_id = ?1 AND href = ?2", params![source_id, href])?;
        Ok(())
    }

    fn objects_of(&self, kind: SourceKind) -> Result<Vec<(String, StoredObject)>> {
        let enabled: Vec<String> = self.sources()?.into_iter().filter(|s| s.enabled && s.kind == kind).map(|s| s.id).collect();
        let mut st = self.conn.prepare("SELECT source_id, href, etag, uid, data FROM objects")?;
        let rows = st.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, StoredObject { href: r.get(1)?, etag: r.get(2)?, uid: r.get(3)?, data: r.get(4)? }))
        })?;
        Ok(rows.filter_map(|r| r.ok()).filter(|(s, _)| enabled.contains(s)).collect())
    }

    /// Event occurrences overlapping the range, across enabled calendars.
    pub fn instances(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<Instance>> {
        let mut out = Vec::new();
        for (source_id, o) in self.objects_of(SourceKind::Calendar)? {
            for ev in ical::events(&o.data) {
                for (start, end) in ical::occurrences(&ev, from, to) {
                    out.push(Instance {
                        source_id: source_id.clone(),
                        href: o.href.clone(),
                        uid: ev.uid.clone(),
                        summary: ev.summary.clone(),
                        description: ev.description.clone(),
                        location: ev.location.clone(),
                        start,
                        end,
                        all_day: ev.all_day,
                        alarm_minutes: ev.alarm_minutes,
                        recurring: ev.recurring,
                    });
                }
            }
        }
        out.sort_by_key(|i| i.start);
        Ok(out)
    }

    pub fn tasks(&self) -> Result<Vec<TaskItem>> {
        let mut out: Vec<TaskItem> = self
            .objects_of(SourceKind::Calendar)?
            .into_iter()
            .flat_map(|(source_id, o)| {
                ical::todos(&o.data)
                    .into_iter()
                    .map(move |todo| TaskItem { source_id: source_id.clone(), href: o.href.clone(), todo })
                    .collect::<Vec<_>>()
            })
            .collect();
        out.sort_by(|a, b| a.todo.completed.cmp(&b.todo.completed).then(a.todo.due.cmp(&b.todo.due)));
        Ok(out)
    }

    pub fn contacts(&self) -> Result<Vec<ContactItem>> {
        let mut out: Vec<ContactItem> = self
            .objects_of(SourceKind::AddressBook)?
            .into_iter()
            .flat_map(|(source_id, o)| {
                vcard::contacts(&o.data)
                    .into_iter()
                    .map(move |contact| ContactItem { source_id: source_id.clone(), href: o.href.clone(), contact })
                    .collect::<Vec<_>>()
            })
            .collect();
        out.sort_by_key(|c| c.contact.name.to_lowercase());
        Ok(out)
    }

    /// Save a new local event or task in the Home calendar.
    pub fn add_local(&self, source_id: &str, uid: &str, data: &str) -> Result<String> {
        let href = format!("local:{uid}");
        self.put_object(source_id, &StoredObject { href: href.clone(), etag: String::new(), uid: uid.into(), data: data.into() })?;
        Ok(href)
    }
}

pub fn event_object(ev: &Event, href: &str, etag: &str) -> StoredObject {
    StoredObject { href: href.into(), etag: etag.into(), uid: ev.uid.clone(), data: ical::event_to_ics(ev) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn local_calendar_tasks_and_contacts() {
        let mut s = PimStore::open_in_memory().unwrap();
        assert_eq!(s.sources().unwrap().len(), 2);
        let ev = Event {
            uid: "e1".into(),
            summary: "Nurse visit".into(),
            description: String::new(),
            location: String::new(),
            start: Utc.with_ymd_and_hms(2026, 10, 28, 12, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2026, 10, 28, 13, 0, 0).unwrap(),
            all_day: false,
            alarm_minutes: Some(15),
            recurrence: None,
            recurring: false,
        };
        s.add_local(HOME_CALENDAR, "e1", &ical::event_to_ics(&ev)).unwrap();
        let t = Todo { uid: "t1".into(), summary: "Pay rent".into(), due: None, completed: false };
        s.add_local(HOME_CALENDAR, "t1", &ical::todo_to_ics(&t)).unwrap();
        let c = Contact { uid: "c1".into(), name: "Bob".into(), emails: vec!["bob@x.org".into()], ..Default::default() };
        s.add_local(LOCAL_CONTACTS, "c1", &vcard::to_vcard(&c)).unwrap();

        let week = s.instances(Utc.with_ymd_and_hms(2026, 10, 25, 0, 0, 0).unwrap(), Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap()).unwrap();
        assert_eq!(week.len(), 1);
        assert_eq!(week[0].summary, "Nurse visit");
        assert_eq!(s.tasks().unwrap()[0].todo.summary, "Pay rent");
        assert_eq!(s.contacts().unwrap()[0].contact.emails, vec!["bob@x.org"]);

        // Disabling a source hides its data.
        let mut home = s.source(HOME_CALENDAR).unwrap().unwrap();
        home.enabled = false;
        s.save_source(&home).unwrap();
        assert!(s.tasks().unwrap().is_empty());

        s.replace_objects(LOCAL_CONTACTS, &[]).unwrap();
        assert!(s.contacts().unwrap().is_empty());
    }
}
