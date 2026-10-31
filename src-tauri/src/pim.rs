//! Calendars, tasks and contacts: keeps remote sources synced, fires event
//! reminders and serves the planner and address book.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
use mail_core::accounts::{AuthMethod, OAuthProvider};
use mail_core::model::Address;
use mail_core::secrets;
use pim::dav::{self, Kind};
use pim::ical::{self, Event, Todo};
use pim::store::{Backend, ContactItem, Instance, PimStore, Source, SourceKind, StoredObject, TaskItem};
use pim::vcard::{self, Contact};
use providers::oauth;
use providers::Credentials;
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Notify;

use crate::engine::SharedEngine;

const SYNC_EVERY: Duration = Duration::from_secs(15 * 60);

pub struct Pim {
    store: Mutex<PimStore>,
    contacts: RwLock<Vec<Contact>>,
    wake: Notify,
    graph_tokens: tokio::sync::Mutex<HashMap<String, oauth::Tokens>>,
}

pub type SharedPim = Arc<Pim>;
type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn secret_name(login: &str) -> String {
    format!("pim:{login}")
}

pub fn init(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let engine = app.state::<SharedEngine>();
    let store = PimStore::open(&engine.paths.data.join("pim.sqlite"))?;
    let pim = Arc::new(Pim {
        store: Mutex::new(store),
        contacts: RwLock::new(Vec::new()),
        wake: Notify::new(),
        graph_tokens: tokio::sync::Mutex::new(HashMap::new()),
    });
    pim.refresh_contacts();
    app.manage(Arc::clone(&pim));
    let (a, p) = (app.clone(), Arc::clone(&pim));
    tauri::async_runtime::spawn(async move {
        loop {
            p.sync_all(&a).await;
            let _ = tokio::time::timeout(SYNC_EVERY, p.wake.notified()).await;
        }
    });
    let (a, p) = (app.clone(), pim);
    tauri::async_runtime::spawn(async move { p.reminder_loop(&a).await });
    Ok(())
}

/// Lower-cased addresses from the user's address books.
pub fn contact_emails(app: &AppHandle) -> Vec<String> {
    app.try_state::<SharedPim>()
        .map(|p| p.contacts.read().unwrap().iter().flat_map(|c| c.emails.clone()).collect())
        .unwrap_or_default()
}

pub fn complete_contacts(app: &AppHandle, prefix: &str, limit: usize) -> Vec<Address> {
    let Some(p) = app.try_state::<SharedPim>() else { return vec![] };
    let q = prefix.to_lowercase();
    let contacts = p.contacts.read().unwrap();
    contacts
        .iter()
        .flat_map(|c| c.emails.iter().map(move |e| (c, e)))
        .filter(|(c, e)| e.starts_with(&q) || c.name.to_lowercase().split_whitespace().any(|w| w.starts_with(&q)))
        .take(limit)
        .map(|(c, e)| Address { name: Some(c.name.clone()).filter(|n| !n.is_empty()), email: e.clone() })
        .collect()
}

/// Drop calendar sources that belong to a removed mail account.
pub fn forget_account(app: &AppHandle, account_id: &str) {
    if let Some(p) = app.try_state::<SharedPim>() {
        let _ = p.store.lock().unwrap().remove_account_sources(account_id);
        p.refresh_contacts();
        let _ = app.emit("pim-changed", ());
    }
}

impl Pim {
    fn refresh_contacts(&self) {
        let list = self.store.lock().unwrap().contacts().map(|v| v.into_iter().map(|c| c.contact).collect()).unwrap_or_default();
        *self.contacts.write().unwrap() = list;
    }

    async fn dav_auth(&self, app: &AppHandle, s: &Source) -> Result<dav::Auth, String> {
        match s.backend {
            Backend::Dav => {
                let login = s.account_id.as_deref().ok_or("source has no login")?;
                let password = secrets::load_named(&secret_name(login)).map_err(err)?;
                Ok(dav::Auth::Basic { user: s.username.clone().unwrap_or_default(), password })
            }
            Backend::DavOAuth => {
                let engine = app.state::<SharedEngine>();
                let account = engine.account(s.account_id.as_deref().unwrap_or("")).ok_or("the mail account was removed")?;
                match engine.credentials(&account).await.map_err(err)? {
                    Credentials::XOAuth2 { access_token, .. } => Ok(dav::Auth::Bearer { token: access_token }),
                    Credentials::Password { user, password } => Ok(dav::Auth::Basic { user, password }),
                }
            }
            _ => Err("not a DAV source".into()),
        }
    }

    async fn graph_token(&self, app: &AppHandle, login: &str) -> Result<String, String> {
        let mut tokens = self.graph_tokens.lock().await;
        if let Some(t) = tokens.get(login).filter(|t| t.is_fresh()) {
            return Ok(t.access_token.clone());
        }
        let refresh = secrets::load_named(&format!("graph:{login}")).map_err(err)?;
        let engine = app.state::<SharedEngine>();
        let t = oauth::refresh(OAuthProvider::Microsoft, &engine.oauth_client(OAuthProvider::Microsoft), &refresh, Some(oauth::MICROSOFT_GRAPH_SCOPES))
            .await
            .map_err(err)?;
        if let Some(r) = t.refresh_token.as_deref().filter(|r| *r != refresh) {
            let _ = secrets::store_named(&format!("graph:{login}"), r);
        }
        let token = t.access_token.clone();
        tokens.insert(login.to_string(), t);
        Ok(token)
    }

    fn collection(s: &Source) -> dav::Collection {
        dav::Collection {
            url: s.url.clone(),
            name: s.name.clone(),
            color: Some(s.color.clone()),
            kind: if s.kind == SourceKind::Calendar { Kind::Calendar } else { Kind::AddressBook },
            events: s.events,
            todos: s.todos,
            writable: s.writable,
        }
    }

    fn range() -> (DateTime<Utc>, DateTime<Utc>) {
        let now = Utc::now();
        (now - chrono::Duration::days(180), now + chrono::Duration::days(540))
    }

    async fn sync_source(&self, app: &AppHandle, s: &Source) -> Result<(), String> {
        let objects: Vec<StoredObject> = match s.backend {
            Backend::Local => return Ok(()),
            Backend::Dav | Backend::DavOAuth => {
                let client = dav::Client::new(self.dav_auth(app, s).await?);
                client
                    .objects(&Self::collection(s), Some(Self::range()))
                    .await
                    .map_err(err)?
                    .into_iter()
                    .map(|o| {
                        let uid = if s.kind == SourceKind::Calendar {
                            ical::parse(&o.data).first().and_then(|c| c.children.first()).and_then(|c| c.text("UID")).unwrap_or_default()
                        } else {
                            vcard::contacts(&o.data).first().map(|c| c.uid.clone()).unwrap_or_default()
                        };
                        StoredObject { href: o.href, etag: o.etag, uid, data: o.data }
                    })
                    .collect()
            }
            Backend::Graph => {
                let token = self.graph_token(app, s.account_id.as_deref().unwrap_or("")).await?;
                let (from, to) = Self::range();
                pim::graph::Client::new(&token)
                    .events(&s.url, from, to)
                    .await
                    .map_err(err)?
                    .into_iter()
                    .map(|(id, ev)| pim::store::event_object(&ev, &id, ""))
                    .collect()
            }
            Backend::Ics => {
                let login = s.account_id.as_deref().ok_or("subscription has no address")?;
                let url = secrets::load_named(&secret_name(login)).map_err(err)?;
                let feed = pim::feed::fetch(&url).await.map_err(err)?;
                feed_objects(&feed)
            }
        };
        self.store.lock().unwrap().replace_objects(&s.id, &objects).map_err(err)
    }

    pub async fn sync_all(&self, app: &AppHandle) {
        let sources = self.store.lock().unwrap().sources().unwrap_or_default();
        for s in sources.iter().filter(|s| s.enabled && s.backend != Backend::Local) {
            if let Err(e) = self.sync_source(app, s).await {
                tracing::warn!("calendar source {} ({}): {e}", s.name, s.id);
            }
        }
        self.refresh_contacts();
        let _ = app.emit("pim-changed", ());
    }

    async fn reminder_loop(&self, app: &AppHandle) {
        let mut fired: HashSet<(String, i64)> = HashSet::new();
        let mut last = Utc::now();
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let now = Utc::now();
            let upcoming = self.store.lock().unwrap().instances(now, now + chrono::Duration::days(2)).unwrap_or_default();
            for i in upcoming {
                let Some(m) = i.alarm_minutes else { continue };
                let at = i.start - chrono::Duration::minutes(m);
                let key = (i.uid.clone(), i.start.timestamp());
                if at > last && at <= now && fired.insert(key) {
                    let when = if m == 0 { "now".to_string() } else { format!("in {m} min") };
                    let local = i.start.with_timezone(&chrono::Local).format("%-I:%M %p");
                    crate::notifications::show_plain(app, &i.summary, &format!("Starts {when} ({local}){}", if i.location.is_empty() { String::new() } else { format!(" · {}", i.location) }));
                }
            }
            last = now;
        }
    }

    /// Write an object to wherever its source lives; returns (href, etag).
    async fn write(&self, app: &AppHandle, s: &Source, href: Option<&str>, uid: &str, data: &str) -> Result<(String, String), String> {
        if !s.writable {
            return Err(format!("“{}” is read-only", s.name));
        }
        match s.backend {
            Backend::Local => Ok((href.map(str::to_string).unwrap_or_else(|| format!("local:{uid}")), String::new())),
            Backend::Dav | Backend::DavOAuth => {
                let etag = href.and_then(|h| self.store.lock().unwrap().object(&s.id, h).ok().flatten()).map(|o| o.etag);
                let kind = if s.kind == SourceKind::Calendar { Kind::Calendar } else { Kind::AddressBook };
                dav::Client::new(self.dav_auth(app, s).await?)
                    .put(&s.url, href, uid, data, etag.as_deref(), kind)
                    .await
                    .map_err(err)
            }
            Backend::Graph => Err("use the Graph event path".into()),
            Backend::Ics => Err(format!("“{}” is a read-only subscription", s.name)),
        }
    }

    async fn remove(&self, app: &AppHandle, s: &Source, href: &str) -> Result<(), String> {
        match s.backend {
            Backend::Local => {}
            Backend::Dav | Backend::DavOAuth => {
                let etag = self.store.lock().unwrap().object(&s.id, href).ok().flatten().map(|o| o.etag);
                dav::Client::new(self.dav_auth(app, s).await?).delete(href, etag.as_deref()).await.map_err(err)?;
            }
            Backend::Graph => {
                let token = self.graph_token(app, s.account_id.as_deref().unwrap_or("")).await?;
                pim::graph::Client::new(&token).delete_event(href).await.map_err(err)?;
            }
            Backend::Ics => return Err(format!("“{}” is a read-only subscription", s.name)),
        }
        self.store.lock().unwrap().delete_object(&s.id, href).map_err(err)
    }

    fn source(&self, id: &str) -> CmdResult<Source> {
        self.store.lock().unwrap().source(id).map_err(err)?.ok_or_else(|| "calendar not found".into())
    }
}

// ----- commands -------------------------------------------------------------------

#[tauri::command]
pub fn pim_sources(pim: State<SharedPim>) -> CmdResult<Vec<Source>> {
    pim.store.lock().unwrap().sources().map_err(err)
}

#[derive(Deserialize)]
pub struct SourceUpdate {
    pub enabled: Option<bool>,
    pub color: Option<String>,
    pub name: Option<String>,
}

#[tauri::command]
pub fn update_source(id: String, update: SourceUpdate, pim: State<SharedPim>, app: AppHandle) -> CmdResult<()> {
    let mut s = pim.source(&id)?;
    if let Some(e) = update.enabled {
        s.enabled = e;
    }
    if let Some(c) = update.color.filter(|c| c.starts_with('#')) {
        s.color = c;
    }
    if let Some(n) = update.name.filter(|n| !n.trim().is_empty()) {
        s.name = n.trim().into();
    }
    pim.store.lock().unwrap().save_source(&s).map_err(err)?;
    pim.refresh_contacts();
    if update.enabled == Some(true) {
        pim.wake.notify_one();
    }
    let _ = app.emit("pim-changed", ());
    Ok(())
}

#[tauri::command]
pub fn remove_source(id: String, pim: State<SharedPim>, app: AppHandle) -> CmdResult<()> {
    let s = pim.source(&id)?;
    if s.backend == Backend::Local {
        return Err("the local calendar and address book can't be removed".into());
    }
    let store = pim.store.lock().unwrap();
    store.remove_source(&id).map_err(err)?;
    // Forget the stored login once no source uses it.
    if let (Some(login), Backend::Dav | Backend::Graph | Backend::Ics) = (&s.account_id, s.backend) {
        if !store.sources().map_err(err)?.iter().any(|o| o.account_id.as_ref() == Some(login)) {
            secrets::delete_named(&secret_name(login));
            secrets::delete_named(&format!("graph:{login}"));
        }
    }
    drop(store);
    pim.refresh_contacts();
    let _ = app.emit("pim-changed", ());
    Ok(())
}

#[tauri::command]
pub fn sync_calendars(pim: State<SharedPim>) {
    pim.wake.notify_one();
}

#[tauri::command]
pub fn calendar_events(from: DateTime<Utc>, to: DateTime<Utc>, pim: State<SharedPim>) -> CmdResult<Vec<Instance>> {
    pim.store.lock().unwrap().instances(from, to).map_err(err)
}

#[tauri::command]
pub fn calendar_tasks(pim: State<SharedPim>) -> CmdResult<Vec<TaskItem>> {
    pim.store.lock().unwrap().tasks().map_err(err)
}

#[tauri::command]
pub fn contacts(pim: State<SharedPim>) -> CmdResult<Vec<ContactItem>> {
    pim.store.lock().unwrap().contacts().map_err(err)
}

#[derive(Deserialize)]
pub struct EventInput {
    pub summary: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub location: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub all_day: bool,
    pub alarm_minutes: Option<i64>,
}

/// Create (href = None) or replace an event.
#[tauri::command]
pub async fn save_event(source_id: String, href: Option<String>, uid: Option<String>, event: EventInput, pim: State<'_, SharedPim>, app: AppHandle) -> CmdResult<()> {
    let s = pim.source(&source_id)?;
    if event.summary.trim().is_empty() {
        return Err("give the event a title".into());
    }
    if event.end < event.start {
        return Err("the event ends before it starts".into());
    }
    let ev = Event {
        uid: uid.filter(|u| !u.is_empty()).unwrap_or_else(ical::new_uid),
        summary: event.summary.trim().into(),
        description: event.description,
        location: event.location,
        start: event.start,
        end: event.end,
        all_day: event.all_day,
        alarm_minutes: event.alarm_minutes,
        recurrence: None,
        recurring: false,
    };
    if s.backend == Backend::Graph {
        let token = pim.graph_token(&app, s.account_id.as_deref().unwrap_or("")).await?;
        let client = pim::graph::Client::new(&token);
        if let Some(h) = &href {
            client.delete_event(h).await.map_err(err)?;
            pim.store.lock().unwrap().delete_object(&s.id, h).map_err(err)?;
        }
        let id = client.create_event(&s.url, &ev).await.map_err(err)?;
        pim.store.lock().unwrap().put_object(&s.id, &pim::store::event_object(&ev, &id, "")).map_err(err)?;
    } else {
        // Keep the original object (recurrence, attendees) when only re-timing is not possible; replace whole.
        let data = ical::event_to_ics(&ev);
        let (new_href, etag) = pim.write(&app, &s, href.as_deref(), &ev.uid, &data).await?;
        pim.store.lock().unwrap().put_object(&s.id, &StoredObject { href: new_href, etag, uid: ev.uid.clone(), data }).map_err(err)?;
    }
    let _ = app.emit("pim-changed", ());
    pim.wake.notify_one();
    Ok(())
}

#[tauri::command]
pub async fn delete_pim_object(source_id: String, href: String, pim: State<'_, SharedPim>, app: AppHandle) -> CmdResult<()> {
    let s = pim.source(&source_id)?;
    pim.remove(&app, &s, &href).await?;
    pim.refresh_contacts();
    let _ = app.emit("pim-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn save_task(source_id: String, summary: String, due: Option<DateTime<Utc>>, pim: State<'_, SharedPim>, app: AppHandle) -> CmdResult<()> {
    let s = pim.source(&source_id)?;
    if !s.todos {
        return Err(format!("“{}” doesn't support tasks", s.name));
    }
    let t = Todo { uid: ical::new_uid(), summary: summary.trim().into(), due, completed: false };
    let data = ical::todo_to_ics(&t);
    let (href, etag) = pim.write(&app, &s, None, &t.uid, &data).await?;
    pim.store.lock().unwrap().put_object(&s.id, &StoredObject { href, etag, uid: t.uid, data }).map_err(err)?;
    let _ = app.emit("pim-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn set_task_done(source_id: String, href: String, done: bool, pim: State<'_, SharedPim>, app: AppHandle) -> CmdResult<()> {
    let s = pim.source(&source_id)?;
    let obj = pim.store.lock().unwrap().object(&s.id, &href).map_err(err)?.ok_or("task not found")?;
    let data = ical::set_todo_completed(&obj.data, done);
    let (href, etag) = pim.write(&app, &s, Some(&href), &obj.uid, &data).await?;
    pim.store.lock().unwrap().put_object(&s.id, &StoredObject { href, etag, uid: obj.uid, data }).map_err(err)?;
    let _ = app.emit("pim-changed", ());
    Ok(())
}

#[derive(Deserialize)]
pub struct ContactInput {
    pub name: String,
    #[serde(default)]
    pub emails: Vec<String>,
    #[serde(default)]
    pub phones: Vec<String>,
    #[serde(default)]
    pub organization: String,
    #[serde(default)]
    pub note: String,
}

#[tauri::command]
pub async fn save_contact(source_id: String, href: Option<String>, uid: Option<String>, contact: ContactInput, pim: State<'_, SharedPim>, app: AppHandle) -> CmdResult<()> {
    let s = pim.source(&source_id)?;
    if contact.name.trim().is_empty() && contact.emails.is_empty() {
        return Err("enter a name or an email address".into());
    }
    let c = Contact {
        uid: uid.filter(|u| !u.is_empty()).unwrap_or_else(uuid_like),
        name: contact.name.trim().into(),
        emails: contact.emails.into_iter().map(|e| e.trim().to_lowercase()).filter(|e| e.contains('@')).collect(),
        phones: contact.phones.into_iter().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect(),
        organization: contact.organization.trim().into(),
        note: contact.note,
    };
    let data = vcard::to_vcard(&c);
    let (href, etag) = pim.write(&app, &s, href.as_deref(), &c.uid, &data).await?;
    pim.store.lock().unwrap().put_object(&s.id, &StoredObject { href, etag, uid: c.uid, data }).map_err(err)?;
    pim.refresh_contacts();
    let _ = app.emit("pim-changed", ());
    Ok(())
}

fn uuid_like() -> String {
    ical::new_uid().replace("@moho-mail", "")
}

// ----- adding calendar accounts -------------------------------------------------------

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CalendarAccount {
    /// Use a signed-in Google or Fastmail mail account.
    MailAccount { account_id: String },
    Nextcloud { server: String },
    Icloud { username: String, password: String },
    Fastmail { username: String, password: String },
    Caldav { url: String, username: String, password: String, #[serde(default)] name: String },
    Microsoft,
    /// A read-only calendar published at a URL (Google's secret iCal address).
    Ics { url: String, #[serde(default)] name: String },
}

/// A subscribed feed as stored objects, one per event.
fn feed_objects(feed: &str) -> Vec<StoredObject> {
    pim::feed::split(feed)
        .into_iter()
        .map(|(uid, data)| StoredObject { href: format!("ics:{uid}"), etag: String::new(), uid, data })
        .collect()
}

fn palette(i: usize) -> String {
    ["#7bd148", "#4dabf7", "#f9a825", "#e91e63", "#26a69a", "#ab47bc", "#ff7043", "#8d6e63"][i % 8].into()
}

/// Who a set of discovered DAV sources belongs to.
struct DavLogin<'a> {
    auth: dav::Auth,
    backend: Backend,
    account_id: &'a str,
    username: Option<String>,
    group: &'a str,
}

async fn add_dav_sources(pim: &Pim, app: &AppHandle, login: DavLogin<'_>, cal_url: Option<&str>, card_url: Option<&str>) -> CmdResult<usize> {
    let DavLogin { auth, backend, account_id, username, group } = login;
    let client = dav::Client::new(auth);
    let mut found = Vec::new();
    if let Some(u) = cal_url {
        found.extend(client.discover(u, Kind::Calendar).await.map_err(err)?);
    }
    if let Some(u) = card_url {
        match client.discover(u, Kind::AddressBook).await {
            Ok(v) => found.extend(v),
            Err(e) => tracing::info!("no address books at {u}: {e}"),
        }
    }
    if found.is_empty() {
        return Err("no calendars or address books were found at that address".into());
    }
    let existing: HashSet<String> = pim.store.lock().unwrap().sources().map_err(err)?.into_iter().map(|s| s.url).collect();
    let mut added = 0;
    for (i, c) in found.into_iter().enumerate() {
        if existing.contains(&c.url) {
            continue;
        }
        let s = Source {
            id: format!("src-{}", uuid_like()),
            kind: if c.kind == Kind::Calendar { SourceKind::Calendar } else { SourceKind::AddressBook },
            backend,
            name: c.name.clone(),
            color: c.color.clone().unwrap_or_else(|| palette(i)),
            url: c.url.clone(),
            account_id: Some(account_id.into()),
            username: username.clone(),
            writable: c.writable,
            enabled: true,
            events: c.events,
            todos: c.todos,
            group: group.into(),
        };
        pim.store.lock().unwrap().save_source(&s).map_err(err)?;
        if let Err(e) = pim.sync_source(app, &s).await {
            tracing::warn!("first sync of {}: {e}", s.name);
        }
        added += 1;
    }
    pim.refresh_contacts();
    let _ = app.emit("pim-changed", ());
    Ok(added)
}

/// Discover and add every calendar and address book of an account. Returns how many were added.
#[tauri::command]
pub async fn add_calendar_account(input: CalendarAccount, pim: State<'_, SharedPim>, engine: State<'_, SharedEngine>, app: AppHandle) -> CmdResult<usize> {
    let login = uuid_like();
    match input {
        CalendarAccount::MailAccount { account_id } => {
            let cfg = engine.account(&account_id).ok_or("unknown account")?;
            let (cal, card) = match (&cfg.auth, cfg.provider) {
                (AuthMethod::OAuth2 { provider: OAuthProvider::Google }, _) => (
                    format!("https://apidata.googleusercontent.com/caldav/v2/{}/user", cfg.email),
                    format!("https://www.googleapis.com/carddav/v1/principals/{}/lists/default/", cfg.email),
                ),
                (AuthMethod::OAuth2 { provider: OAuthProvider::Fastmail }, _) => (
                    "https://caldav.fastmail.com/dav/".into(),
                    "https://carddav.fastmail.com/dav/".into(),
                ),
                (AuthMethod::OAuth2 { provider: OAuthProvider::Microsoft }, _) => {
                    return Err("Outlook calendars are added with “Microsoft 365 / Outlook.com”".into())
                }
                (_, mail_core::model::Provider::Gmail) => {
                    return Err("Google Calendar only accepts “Sign in with Google”, not app passwords. Add a Google \
                        OAuth client in Settings → Advanced, then remove this account and add it again with Google \
                        sign-in to use its calendar. For a read-only view without that, use “Subscribe by \
                        address (iCal)” with the calendar's secret address."
                        .into())
                }
                _ => return Err("this account can't share its sign-in with a calendar; add the calendar server directly".into()),
            };
            let auth = pim.dav_auth(&app, &Source {
                id: String::new(),
                kind: SourceKind::Calendar,
                backend: Backend::DavOAuth,
                name: String::new(),
                color: String::new(),
                url: String::new(),
                account_id: Some(account_id.clone()),
                username: None,
                writable: true,
                enabled: true,
                events: true,
                todos: true,
                group: String::new(),
            }).await?;
            add_dav_sources(&pim, &app, DavLogin { auth, backend: Backend::DavOAuth, account_id: &account_id, username: None, group: &cfg.email }, Some(&cal), Some(&card)).await
        }
        CalendarAccount::Nextcloud { server } => {
            let pending = pim::nextcloud::begin(&server).await.map_err(err)?;
            crate::launch::open_url(&app, &pending.login_url).await?;
            let creds = pending.wait(Duration::from_secs(600)).await.map_err(err)?;
            crate::window::show(&app);
            secrets::store_named(&secret_name(&login), &creds.app_password).map_err(err)?;
            let dav_root = pim::nextcloud::dav_url(&creds.server);
            let auth = dav::Auth::Basic { user: creds.login_name.clone(), password: creds.app_password };
            let group = format!("Nextcloud · {}", creds.login_name);
            add_dav_sources(&pim, &app, DavLogin { auth, backend: Backend::Dav, account_id: &login, username: Some(creds.login_name), group: &group }, Some(&dav_root), Some(&dav_root)).await
        }
        CalendarAccount::Icloud { username, password } => {
            secrets::store_named(&secret_name(&login), &password).map_err(err)?;
            let auth = dav::Auth::Basic { user: username.clone(), password };
            add_dav_sources(&pim, &app, DavLogin { auth, backend: Backend::Dav, account_id: &login, username: Some(username.clone()), group: &format!("iCloud · {username}") }, Some("https://caldav.icloud.com/"), Some("https://contacts.icloud.com/")).await
        }
        CalendarAccount::Fastmail { username, password } => {
            secrets::store_named(&secret_name(&login), &password).map_err(err)?;
            let auth = dav::Auth::Basic { user: username.clone(), password };
            add_dav_sources(&pim, &app, DavLogin { auth, backend: Backend::Dav, account_id: &login, username: Some(username.clone()), group: &format!("Fastmail · {username}") }, Some("https://caldav.fastmail.com/dav/"), Some("https://carddav.fastmail.com/dav/")).await
        }
        CalendarAccount::Caldav { url, username, password, name } => {
            secrets::store_named(&secret_name(&login), &password).map_err(err)?;
            let auth = dav::Auth::Basic { user: username.clone(), password };
            let group = if name.trim().is_empty() { url::Url::parse(&url).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or(url.clone()) } else { name };
            let res = add_dav_sources(&pim, &app, DavLogin { auth, backend: Backend::Dav, account_id: &login, username: Some(username), group: &group }, Some(&url), Some(&url)).await;
            if res.is_err() {
                secrets::delete_named(&secret_name(&login));
            }
            res
        }
        CalendarAccount::Ics { url, name } => {
            let url = pim::feed::http_url(&url).map_err(err)?;
            let feed = pim::feed::fetch(&url).await.map_err(err)?;
            let existing = pim.store.lock().unwrap().sources().map_err(err)?;
            let name = Some(name.trim().to_string())
                .filter(|n| !n.is_empty())
                .or_else(|| pim::feed::name(&feed))
                .or_else(|| url::Url::parse(&url).ok().and_then(|u| u.host_str().map(str::to_string)))
                .unwrap_or_else(|| "Subscribed calendar".into());
            secrets::store_named(&secret_name(&login), &url).map_err(err)?;
            let s = Source {
                id: format!("src-{}", uuid_like()),
                kind: SourceKind::Calendar,
                backend: Backend::Ics,
                name,
                color: palette(existing.len()),
                // The address itself is a secret; it lives in the keyring.
                url: format!("ics:{login}"),
                account_id: Some(login.clone()),
                username: None,
                writable: false,
                enabled: true,
                events: true,
                todos: false,
                group: "Subscriptions".into(),
            };
            pim.store.lock().unwrap().save_source(&s).map_err(err)?;
            pim.store.lock().unwrap().replace_objects(&s.id, &feed_objects(&feed)).map_err(err)?;
            let _ = app.emit("pim-changed", ());
            Ok(1)
        }
        CalendarAccount::Microsoft => {
            let client = engine.oauth_client(OAuthProvider::Microsoft);
            let pending = oauth::begin(OAuthProvider::Microsoft, &client, None, Some(oauth::MICROSOFT_GRAPH_SCOPES)).await.map_err(err)?;
            crate::launch::open_url(&app, &pending.url).await?;
            let tokens = pending.finish(Duration::from_secs(600)).await.map_err(err)?;
            crate::window::show(&app);
            let refresh = tokens.refresh_token.clone().ok_or("Microsoft did not return a refresh token")?;
            secrets::store_named(&format!("graph:{login}"), &refresh).map_err(err)?;
            let graph = pim::graph::Client::new(&tokens.access_token);
            pim.graph_tokens.lock().await.insert(login.clone(), tokens);
            let cals = graph.calendars().await.map_err(err)?;
            let group = "Microsoft".to_string();
            let mut added = 0;
            for (i, c) in cals.into_iter().enumerate() {
                let s = Source {
                    id: format!("src-{}", uuid_like()),
                    kind: SourceKind::Calendar,
                    backend: Backend::Graph,
                    name: c.name,
                    color: c.color.unwrap_or_else(|| palette(i)),
                    url: c.id,
                    account_id: Some(login.clone()),
                    username: None,
                    writable: c.writable,
                    enabled: true,
                    events: true,
                    todos: false,
                    group: group.clone(),
                };
                pim.store.lock().unwrap().save_source(&s).map_err(err)?;
                let _ = pim.sync_source(&app, &s).await;
                added += 1;
            }
            let _ = app.emit("pim-changed", ());
            Ok(added)
        }
    }
}
