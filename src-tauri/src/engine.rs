//! Account workers: keep every account synced, push new mail through the
//! notification policy, and run user operations on a separate connection.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use chrono::{Local, Timelike};
use mail_core::accounts::{AccountConfig, AccountsFile, AuthMethod, OAuthProvider};
use mail_core::model::{Account, AccountStatus, FolderRole, MessageSummary};
use mail_core::notify::{should_notify, ImportanceContext, NotifyMode};
use mail_core::paths::AppPaths;
use mail_core::secrets::{self, SecretKind};
use mail_core::settings::Settings;
use mail_core::store::{folder_id, Store};
use providers::imap::{self, IdleOutcome, Session};
use providers::oauth::{self, ClientConfig, Tokens};
use providers::{Credentials, Error};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

use crate::state::AppState;

/// Re-sync every folder this often even when IDLE stays quiet.
const FULL_SYNC_EVERY: Duration = Duration::from_secs(10 * 60);
/// Servers drop IDLE after 30 minutes; renew well before.
const IDLE_TIMEOUT: Duration = Duration::from_secs(9 * 60);

#[derive(Clone, Serialize)]
pub struct MailChanged {
    pub account_id: String,
}

struct Worker {
    wake: Arc<Notify>,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

pub struct Engine {
    app: AppHandle,
    pub paths: AppPaths,
    pub store: Arc<Mutex<Store>>,
    accounts: RwLock<AccountsFile>,
    workers: Mutex<HashMap<String, Worker>>,
    statuses: Mutex<HashMap<String, AccountStatus>>,
    tokens: tokio::sync::Mutex<HashMap<String, Tokens>>,
    /// One connection per account for user actions, separate from the syncing one.
    ops: Mutex<HashMap<String, Arc<tokio::sync::Mutex<Option<Session>>>>>,
    /// Messages already notified about, so a Gmail message that turns up in
    /// several folders (labels) raises one notification.
    notified: Mutex<HashSet<String>>,
    /// Extra credential providers (the built-in Proton bridge).
    pub proton: crate::proton::ProtonBridge,
}

pub type SharedEngine = Arc<Engine>;

/// Folder ids an account monitors as important: the user's choice, or by
/// default Gmail's Important folder, otherwise the Inbox.
pub fn important_folders(store: &Store, settings: &Settings, account_id: &str) -> Vec<String> {
    if let Some(ids) = settings.important_folders.get(account_id) {
        return ids.clone();
    }
    [FolderRole::Important, FolderRole::Inbox]
        .into_iter()
        .find_map(|role| store.folder_by_role(account_id, role).ok().flatten())
        .map(|path| vec![folder_id(account_id, &path)])
        .unwrap_or_default()
}

/// Whether a message counts as being in one of the `important` folders.
/// Gmail delivers to INBOX and labels the message `\Important`, so the label
/// stands in for the Important folder until that folder is synced.
pub fn in_important_folder(store: &Store, important: &[String], msg: &MessageSummary) -> bool {
    important.contains(&msg.folder_id)
        || (msg.important
            && store
                .folder_by_role(&msg.account_id, FolderRole::Important)
                .ok()
                .flatten()
                .is_some_and(|path| important.contains(&folder_id(&msg.account_id, &path))))
}

fn builtin_client(p: OAuthProvider) -> ClientConfig {
    let (id, secret) = match p {
        OAuthProvider::Google => (option_env!("MOHO_GOOGLE_CLIENT_ID"), option_env!("MOHO_GOOGLE_CLIENT_SECRET")),
        OAuthProvider::Microsoft => (option_env!("MOHO_MICROSOFT_CLIENT_ID"), None),
        OAuthProvider::Yahoo => (option_env!("MOHO_YAHOO_CLIENT_ID"), option_env!("MOHO_YAHOO_CLIENT_SECRET")),
        OAuthProvider::Aol => (option_env!("MOHO_AOL_CLIENT_ID"), option_env!("MOHO_AOL_CLIENT_SECRET")),
        OAuthProvider::Fastmail => (option_env!("MOHO_FASTMAIL_CLIENT_ID"), None),
    };
    ClientConfig { client_id: id.unwrap_or_default().into(), client_secret: secret.map(Into::into) }
}

pub fn provider_key(p: OAuthProvider) -> &'static str {
    match p {
        OAuthProvider::Google => "google",
        OAuthProvider::Microsoft => "microsoft",
        OAuthProvider::Yahoo => "yahoo",
        OAuthProvider::Aol => "aol",
        OAuthProvider::Fastmail => "fastmail",
    }
}

impl Engine {
    pub fn new(app: AppHandle, paths: AppPaths) -> Result<Arc<Self>, String> {
        let store = Store::open(&paths.mail_cache_dir().join("index.sqlite")).map_err(|e| e.to_string())?;
        let accounts = AccountsFile::load(&paths.accounts_file()).map_err(|e| e.to_string())?;
        let proton = crate::proton::ProtonBridge::new(&paths);
        Ok(Arc::new(Self {
            app,
            paths,
            store: Arc::new(Mutex::new(store)),
            accounts: RwLock::new(accounts),
            workers: Mutex::new(HashMap::new()),
            statuses: Mutex::new(HashMap::new()),
            tokens: tokio::sync::Mutex::new(HashMap::new()),
            ops: Mutex::new(HashMap::new()),
            notified: Mutex::new(HashSet::new()),
            proton,
        }))
    }

    // ----- account registry -------------------------------------------------

    pub fn account_configs(&self) -> Vec<AccountConfig> {
        let order = self.app.state::<AppState>().settings().account_order;
        let mut list = self.accounts.read().unwrap().accounts.clone();
        list.sort_by_key(|a| order.iter().position(|id| *id == a.id).unwrap_or(usize::MAX));
        list
    }

    pub fn account(&self, id: &str) -> Option<AccountConfig> {
        self.accounts.read().unwrap().get(id).cloned()
    }

    pub fn accounts_file(&self) -> AccountsFile {
        self.accounts.read().unwrap().clone()
    }

    pub fn save_account(&self, cfg: AccountConfig) -> Result<(), String> {
        let mut file = self.accounts.write().unwrap();
        match file.accounts.iter_mut().find(|a| a.id == cfg.id) {
            Some(existing) => *existing = cfg,
            None => file.accounts.push(cfg),
        }
        file.save(&self.paths.accounts_file()).map_err(|e| e.to_string())
    }

    pub fn remove_account(self: &Arc<Self>, id: &str) -> Result<(), String> {
        self.stop_worker(id);
        if let Some(cfg) = self.account(id).filter(|c| c.auth == AuthMethod::ProtonBridge) {
            self.proton.logout(&cfg.email);
        }
        {
            let mut file = self.accounts.write().unwrap();
            file.accounts.retain(|a| a.id != id);
            file.save(&self.paths.accounts_file()).map_err(|e| e.to_string())?;
        }
        secrets::delete_account(id);
        self.store.lock().unwrap().delete_account(id).map_err(|e| e.to_string())?;
        self.statuses.lock().unwrap().remove(id);
        self.ops.lock().unwrap().remove(id);
        self.emit_changed(id);
        Ok(())
    }

    pub fn accounts_for_ui(&self) -> Vec<Account> {
        let settings = self.app.state::<AppState>().settings();
        let store = self.store.lock().unwrap();
        let statuses = self.statuses.lock().unwrap();
        self.account_configs()
            .into_iter()
            .map(|c| Account {
                unread: store.unread_count(&c.id, FolderRole::Inbox).unwrap_or(0),
                notify: settings.notify_mode(&c.id),
                status: statuses.get(&c.id).cloned().unwrap_or_default(),
                id: c.id,
                name: c.name,
                email: c.email,
                provider: c.provider,
                color: c.color,
            })
            .collect()
    }

    fn set_status(&self, id: &str, status: AccountStatus) {
        let changed = self.statuses.lock().unwrap().insert(id.to_string(), status.clone()) != Some(status);
        if changed {
            let _ = self.app.emit("account-status", MailChanged { account_id: id.into() });
        }
    }

    fn emit_changed(&self, id: &str) {
        let _ = self.app.emit("mail-changed", MailChanged { account_id: id.into() });
        crate::commands::refresh_tray(&self.app);
    }

    // ----- credentials --------------------------------------------------------

    pub fn oauth_client(&self, p: OAuthProvider) -> ClientConfig {
        let settings = self.app.state::<AppState>().settings();
        match settings.oauth_clients.get(provider_key(p)) {
            Some(c) if !c.client_id.trim().is_empty() => {
                ClientConfig { client_id: c.client_id.trim().into(), client_secret: c.client_secret.clone().filter(|s| !s.is_empty()) }
            }
            _ => builtin_client(p),
        }
    }

    pub async fn remember_tokens(&self, account_id: &str, tokens: Tokens) {
        self.tokens.lock().await.insert(account_id.into(), tokens);
    }

    pub async fn credentials(&self, cfg: &AccountConfig) -> Result<Credentials, Error> {
        match &cfg.auth {
            AuthMethod::Password => {
                let password = secrets::load(&cfg.id, SecretKind::Password).map_err(|e| Error::Auth(e.to_string()))?;
                Ok(Credentials::Password { user: cfg.username.clone(), password })
            }
            AuthMethod::OAuth2 { provider } => {
                let mut tokens = self.tokens.lock().await;
                if let Some(t) = tokens.get(&cfg.id).filter(|t| t.is_fresh()) {
                    return Ok(Credentials::XOAuth2 { user: cfg.username.clone(), access_token: t.access_token.clone() });
                }
                let refresh = secrets::load(&cfg.id, SecretKind::RefreshToken).map_err(|e| Error::Auth(e.to_string()))?;
                let t = oauth::refresh(*provider, &self.oauth_client(*provider), &refresh, None).await?;
                if let Some(r) = t.refresh_token.as_deref().filter(|r| *r != refresh) {
                    let _ = secrets::store(&cfg.id, SecretKind::RefreshToken, r);
                }
                let access_token = t.access_token.clone();
                tokens.insert(cfg.id.clone(), t);
                Ok(Credentials::XOAuth2 { user: cfg.username.clone(), access_token })
            }
            AuthMethod::ProtonBridge => self.proton.credentials(&cfg.email).await,
        }
    }

    async fn connect(&self, cfg: &AccountConfig) -> Result<Session, Error> {
        let creds = self.credentials(cfg).await?;
        let imap = self.proton.imap_server(cfg).unwrap_or_else(|| cfg.imap.clone());
        imap::connect(&imap, &creds).await
    }

    // ----- workers -------------------------------------------------------------

    pub fn start_all(self: &Arc<Self>) {
        let engine = Arc::clone(self);
        let configs = self.account_configs();
        // Start the Proton bridge before the workers need it, off the UI thread.
        tauri::async_runtime::spawn_blocking(move || engine.proton.start_if_needed(&configs));
        for cfg in self.account_configs() {
            self.start_worker(&cfg.id);
        }
    }

    pub fn start_worker(self: &Arc<Self>, id: &str) {
        let mut workers = self.workers.lock().unwrap();
        if workers.contains_key(id) {
            return;
        }
        let wake = Arc::new(Notify::new());
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        workers.insert(id.into(), Worker { wake: Arc::clone(&wake), stop: Arc::clone(&stop) });
        let engine = Arc::clone(self);
        let id = id.to_string();
        tauri::async_runtime::spawn(async move { engine.run_worker(id, wake, stop).await });
    }

    pub fn stop_worker(&self, id: &str) {
        if let Some(w) = self.workers.lock().unwrap().remove(id) {
            w.stop.store(true, std::sync::atomic::Ordering::SeqCst);
            w.wake.notify_waiters();
            w.wake.notify_one();
        }
    }

    /// Ask a worker (or all) to sync now.
    pub fn wake(&self, id: Option<&str>) {
        for (wid, w) in self.workers.lock().unwrap().iter() {
            if id.is_none_or(|i| i == wid) {
                w.wake.notify_one();
            }
        }
    }

    async fn run_worker(self: Arc<Self>, id: String, wake: Arc<Notify>, stop: Arc<std::sync::atomic::AtomicBool>) {
        let stopped = || stop.load(std::sync::atomic::Ordering::SeqCst);
        let mut backoff = Duration::from_secs(15);
        while !stopped() {
            let Some(cfg) = self.account(&id) else { return };
            self.set_status(&id, AccountStatus::Syncing);
            let session = match self.connect(&cfg).await {
                Ok(s) => s,
                Err(e) => {
                    self.report_error(&id, &e);
                    let wait = if e.is_auth() { Duration::from_secs(3600) } else { backoff };
                    backoff = (backoff * 2).min(Duration::from_secs(300));
                    let _ = tokio::time::timeout(wait, wake.notified()).await;
                    continue;
                }
            };
            backoff = Duration::from_secs(15);
            if let Err(e) = self.session_loop(&cfg, session, &wake, &stopped).await {
                self.report_error(&id, &e);
                let _ = tokio::time::timeout(Duration::from_secs(5), wake.notified()).await;
            }
        }
    }

    async fn session_loop(
        &self,
        cfg: &AccountConfig,
        mut session: Session,
        wake: &Notify,
        stopped: &impl Fn() -> bool,
    ) -> Result<(), Error> {
        loop {
            let started = Instant::now();
            // Also after "Check for new mail" wakes us, so the UI shows it working.
            self.set_status(&cfg.id, AccountStatus::Syncing);
            let report = imap::sync_account(&mut session, &cfg.id, &self.store).await?;
            self.set_status(&cfg.id, AccountStatus::Idle);
            if report.changed {
                self.emit_changed(&cfg.id);
            }
            self.handle_new(&cfg.id, &report.new_messages);

            let inbox = self
                .store
                .lock()
                .unwrap()
                .folder_by_role(&cfg.id, FolderRole::Inbox)?
                .unwrap_or_else(|| "INBOX".into());
            while started.elapsed() < FULL_SYNC_EVERY {
                if stopped() {
                    let _ = session.logout().await;
                    return Ok(());
                }
                let (s, outcome) = imap::idle_until(session, &inbox, IDLE_TIMEOUT, wake).await?;
                session = s;
                match outcome {
                    IdleOutcome::NewData => {
                        // Important folders too, so the tray catches up right away.
                        let mut paths = vec![inbox.clone()];
                        let important = {
                            let store = self.store.lock().unwrap();
                            important_folders(&store, &self.app.state::<AppState>().settings(), &cfg.id)
                        };
                        let prefix = format!("{}:", cfg.id);
                        for path in important.iter().filter_map(|id| id.strip_prefix(&prefix)) {
                            if !paths.iter().any(|p| p == path) {
                                paths.push(path.to_string());
                            }
                        }
                        let r = imap::sync_paths(&mut session, &cfg.id, &self.store, &paths).await?;
                        if r.changed {
                            self.emit_changed(&cfg.id);
                        }
                        self.handle_new(&cfg.id, &r.new_messages);
                    }
                    IdleOutcome::Woken => break,
                    IdleOutcome::Timeout => {}
                }
            }
        }
    }

    fn report_error(&self, id: &str, e: &Error) {
        tracing::warn!("account {id}: {e}");
        let status = match e {
            Error::Auth(m) => AccountStatus::NeedsAuth(m.clone()),
            Error::Network(m) => AccountStatus::Offline(m.clone()),
            other => AccountStatus::Error(other.to_string()),
        };
        self.set_status(id, status);
    }

    // ----- notifications ---------------------------------------------------------

    fn handle_new(&self, account_id: &str, new: &[(i64, FolderRole)]) {
        if new.is_empty() {
            return;
        }
        let state = self.app.state::<AppState>();
        let settings = state.settings();
        if state.notifications_paused() {
            return;
        }
        let mode = settings.notify_mode(account_id);
        if mode == NotifyMode::Muted {
            return;
        }
        let store = self.store.lock().unwrap();
        let contacts: HashSet<String> = store
            .correspondents()
            .unwrap_or_default()
            .into_keys()
            .chain(crate::pim::contact_emails(&self.app))
            .collect();
        let hour = Local::now().hour() as u8;
        let important = important_folders(&store, &settings, account_id);
        let mut notified = self.notified.lock().unwrap();
        if notified.len() > 10_000 {
            notified.clear();
        }
        let mut to_show: Vec<MessageSummary> = Vec::new();
        for (id, role) in new {
            let Ok(Some(msg)) = store.summary(*id) else { continue };
            if !msg.unread {
                continue;
            }
            let key = if msg.message_id.is_empty() { msg.id.clone() } else { msg.message_id.clone() };
            if notified.contains(&format!("{account_id}\n{key}")) {
                continue;
            }
            let mut refs = msg.references.clone();
            refs.extend(msg.in_reply_to.clone());
            let ctx = ImportanceContext {
                in_important_folder: in_important_folder(&store, &important, &msg),
                contacts: Some(&contacts),
                replied_in_thread: store.replied_in_thread(account_id, &refs).unwrap_or(false),
            };
            if should_notify(mode, *role, &msg, &ctx, &settings.notifications, hour) {
                notified.insert(format!("{account_id}\n{key}"));
                to_show.push(msg);
            }
        }
        drop(notified);
        drop(store);
        let account = self.account(account_id).map(|a| a.name).unwrap_or_default();
        crate::notifications::show_new_mail(&self.app, &account, to_show);
    }

    // ----- operations on the ops connection -------------------------------------

    fn ops_slot(&self, id: &str) -> Arc<tokio::sync::Mutex<Option<Session>>> {
        Arc::clone(self.ops.lock().unwrap().entry(id.into()).or_default())
    }

    /// Run `op` on the account's operations connection, reconnecting once if
    /// the cached connection has gone stale.
    pub async fn with_session<T, F>(&self, account_id: &str, op: F) -> Result<T, Error>
    where
        F: for<'s> Fn(&'s mut Session) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, Error>> + Send + 's>>,
    {
        let cfg = self.account(account_id).ok_or_else(|| Error::Other("unknown account".into()))?;
        let slot = self.ops_slot(account_id);
        let mut guard = slot.lock().await;
        for attempt in 0..2 {
            if guard.is_none() {
                *guard = Some(self.connect(&cfg).await?);
            }
            match op(guard.as_mut().unwrap()).await {
                Ok(v) => return Ok(v),
                Err(Error::Network(e)) if attempt == 0 => {
                    tracing::info!("ops connection for {account_id} dropped ({e}); reconnecting");
                    *guard = None;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }
}
