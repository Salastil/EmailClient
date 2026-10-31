//! Commands invoked by the UI.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{Local, Timelike};
use mail_core::accounts::{new_account_id, AccountConfig, AuthMethod, OAuthProvider, ServerConfig, ACCOUNT_COLORS};
use mail_core::model::{Account, Address, Attachment, Folder, FolderRole, MessageSummary, Provider, ThreadNode};
use mail_core::notify::{should_notify, ImportanceContext, NotifyMode};
use mail_core::parse::{self, parse_message};
use mail_core::sanitize::{sanitize_email_html, SanitizedHtml};
use mail_core::secrets::{self, SecretKind};
use mail_core::settings::Settings;
use mail_core::threading;
use providers::autoconfig::{self, Discovered};
use providers::smtp::{self, Outgoing, OutgoingAttachment};
use providers::{imap, oauth, Credentials};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::engine::SharedEngine;
use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn msg_id(id: &str) -> CmdResult<i64> {
    id.parse().map_err(|_| format!("invalid message id {id}"))
}

/// Recompute the tray badge: unread inbox mail in accounts that are not muted.
///
/// Returns without waiting. Applying an update makes the tray thread rebuild
/// its menu, which asks the main thread whether the window is shown, so
/// blocking here from a command on the main thread would deadlock. The counts
/// are read when the update is applied, so updates finishing out of order
/// still leave the latest numbers.
pub fn refresh_tray(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Some(tray) = state.tray() else { return };
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        tray.update(|tray| {
            let Some(engine) = app.try_state::<SharedEngine>() else { return };
            tray.unread = engine
                .accounts_for_ui()
                .iter()
                .filter(|a| a.notify != NotifyMode::Muted)
                .map(|a| a.unread)
                .sum();
            tray.paused = app.state::<AppState>().notifications_paused();
        });
    });
}

// ----- reading -----------------------------------------------------------------

#[tauri::command]
pub fn list_accounts(engine: State<SharedEngine>, app: AppHandle) -> Vec<Account> {
    refresh_tray(&app);
    engine.accounts_for_ui()
}

#[tauri::command]
pub fn list_folders(account_id: String, engine: State<SharedEngine>) -> CmdResult<Vec<Folder>> {
    engine.store.lock().unwrap().folders(&account_id).map_err(err)
}

#[tauri::command]
pub fn list_threads(folder_id: String, engine: State<SharedEngine>) -> CmdResult<Vec<ThreadNode>> {
    let msgs = engine.store.lock().unwrap().messages(&folder_id, 5000).map_err(err)?;
    Ok(threading::build_threads(msgs))
}

#[tauri::command]
pub fn search(
    query: String,
    account_id: Option<String>,
    folder_id: Option<String>,
    engine: State<SharedEngine>,
) -> CmdResult<Vec<MessageSummary>> {
    engine.store.lock().unwrap().search(&query, account_id.as_deref(), folder_id.as_deref(), 200).map_err(err)
}

#[derive(Serialize)]
pub struct MessageBody {
    pub summary: MessageSummary,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub body_text: Option<String>,
    pub attachments: Vec<Attachment>,
}

#[derive(Serialize)]
pub struct MessageView {
    pub message: MessageBody,
    pub body: Option<SanitizedHtml>,
}

/// The full message, downloading and caching it on first open.
async fn raw_message(engine: &SharedEngine, id: i64) -> CmdResult<Vec<u8>> {
    if let Some(raw) = engine.store.lock().unwrap().raw(id).map_err(err)? {
        return Ok(raw);
    }
    let loc = engine.store.lock().unwrap().location(id).map_err(err)?.ok_or("message not found")?;
    let (path, uid) = (loc.folder_path.clone(), loc.uid);
    let raw = engine
        .with_session(&loc.account_id, move |s| {
            let path = path.clone();
            Box::pin(async move { imap::fetch_raw(s, &path, uid).await })
        })
        .await
        .map_err(err)?;
    let text = parse::index_text(&parse_message(&raw));
    engine.store.lock().unwrap().set_raw(id, &raw, &text).map_err(err)?;
    Ok(raw)
}

#[tauri::command]
pub async fn get_message(id: String, allow_remote: bool, engine: State<'_, SharedEngine>, state: State<'_, AppState>) -> CmdResult<MessageView> {
    let id = msg_id(&id)?;
    let summary = engine.store.lock().unwrap().summary(id).map_err(err)?.ok_or("message not found")?;
    let raw = raw_message(&engine, id).await?;
    let parsed = parse_message(&raw);
    let (to, cc) = engine.store.lock().unwrap().recipients(id).map_err(err)?;
    if summary.unread {
        let engine = Arc::clone(&engine);
        tauri::async_runtime::spawn(async move { let _ = set_flag_inner(&engine, id, "seen", true).await; });
    }
    let allow = allow_remote || state.settings().privacy.load_remote_images;
    Ok(MessageView {
        body: parsed.html.as_deref().map(|h| sanitize_email_html(h, allow)),
        message: MessageBody { summary, to, cc, body_text: parsed.text, attachments: parsed.attachments },
    })
}

fn safe_file_name(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if matches!(c, '/' | '\\' | '\0') { '_' } else { c }).collect();
    let trimmed = cleaned.trim_start_matches('.').trim();
    if trimmed.is_empty() { "attachment".into() } else { trimmed.into() }
}

async fn attachment_bytes(engine: &SharedEngine, id: &str, index: usize) -> CmdResult<(String, Vec<u8>)> {
    let raw = raw_message(engine, msg_id(id)?).await?;
    let (name, _mime, bytes) = parse::attachment(&raw, index).ok_or("attachment not found")?;
    Ok((safe_file_name(&name), bytes))
}

/// Open an attachment with the default application.
#[tauri::command]
pub async fn open_attachment(id: String, index: usize, engine: State<'_, SharedEngine>, app: AppHandle) -> CmdResult<()> {
    let (name, bytes) = attachment_bytes(&engine, &id, index).await?;
    let dir = engine.paths.cache.join("attachments").join(&id);
    std::fs::create_dir_all(&dir).map_err(err)?;
    let path = dir.join(name);
    std::fs::write(&path, bytes).map_err(err)?;
    crate::launch::open_path(&app, &path).await
}

/// Save an attachment where the user chooses. Returns the saved path.
#[tauri::command]
pub async fn save_attachment(id: String, index: usize, engine: State<'_, SharedEngine>, app: AppHandle) -> CmdResult<Option<String>> {
    let (name, bytes) = attachment_bytes(&engine, &id, index).await?;
    let picked = tauri::async_runtime::spawn_blocking(move || app.dialog().file().set_file_name(&name).blocking_save_file())
        .await
        .map_err(err)?;
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    std::fs::write(&path, bytes).map_err(err)?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

// ----- message actions ---------------------------------------------------------

async fn set_flag_inner(engine: &SharedEngine, id: i64, flag: &str, on: bool) -> CmdResult<()> {
    let imap_flag = match flag {
        "seen" => "\\Seen",
        "flagged" => "\\Flagged",
        "answered" => "\\Answered",
        _ => return Err(format!("unknown flag {flag}")),
    };
    let loc = engine.store.lock().unwrap().location(id).map_err(err)?.ok_or("message not found")?;
    {
        let mut store = engine.store.lock().unwrap();
        match flag {
            "seen" => store.set_flags(id, Some(!on), None),
            "flagged" => store.set_flags(id, None, Some(on)),
            _ => Ok(()),
        }
        .map_err(err)?;
    }
    let (path, uid) = (loc.folder_path.clone(), loc.uid);
    let result = engine
        .with_session(&loc.account_id, move |s| {
            let path = path.clone();
            Box::pin(async move { imap::store_flag(s, &path, uid, imap_flag, on).await })
        })
        .await;
    engine.wake(Some(&loc.account_id));
    result.map_err(err)
}

#[tauri::command]
pub async fn set_flag(id: String, flag: String, on: bool, engine: State<'_, SharedEngine>, app: AppHandle) -> CmdResult<()> {
    set_flag_inner(&engine, msg_id(&id)?, &flag, on).await?;
    refresh_tray(&app);
    Ok(())
}

async fn move_inner(engine: &SharedEngine, id: i64, dest_path: Option<String>) -> CmdResult<()> {
    let loc = engine.store.lock().unwrap().location(id).map_err(err)?.ok_or("message not found")?;
    let (path, uid) = (loc.folder_path.clone(), loc.uid);
    let result = engine
        .with_session(&loc.account_id, move |s| {
            let (path, dest) = (path.clone(), dest_path.clone());
            Box::pin(async move {
                match dest {
                    Some(d) => imap::move_message(s, &path, uid, &d).await,
                    None => imap::expunge_message(s, &path, uid).await,
                }
            })
        })
        .await
        .map_err(err);
    if result.is_ok() {
        engine.store.lock().unwrap().remove_message(id).map_err(err)?;
    }
    engine.wake(Some(&loc.account_id));
    result
}

fn folder_path_by_role(engine: &SharedEngine, account_id: &str, role: FolderRole) -> CmdResult<Option<String>> {
    engine.store.lock().unwrap().folder_by_role(account_id, role).map_err(err)
}

#[tauri::command]
pub async fn move_message(id: String, folder_id: String, engine: State<'_, SharedEngine>, app: AppHandle) -> CmdResult<()> {
    let (_, dest) = engine.store.lock().unwrap().folder_path(&folder_id).map_err(err)?.ok_or("folder not found")?;
    move_inner(&engine, msg_id(&id)?, Some(dest)).await?;
    let _ = app.emit("mail-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn archive_message(id: String, engine: State<'_, SharedEngine>, app: AppHandle) -> CmdResult<()> {
    let id = msg_id(&id)?;
    let loc = engine.store.lock().unwrap().location(id).map_err(err)?.ok_or("message not found")?;
    let dest = match folder_path_by_role(&engine, &loc.account_id, FolderRole::Archive)? {
        Some(p) => p,
        // Gmail archives by removing the Inbox label, i.e. moving to All Mail.
        None => folder_path_by_role(&engine, &loc.account_id, FolderRole::All)?
            .ok_or("this account has no Archive folder")?,
    };
    move_inner(&engine, id, Some(dest)).await?;
    let _ = app.emit("mail-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn delete_message(id: String, engine: State<'_, SharedEngine>, app: AppHandle) -> CmdResult<()> {
    let id = msg_id(&id)?;
    let loc = engine.store.lock().unwrap().location(id).map_err(err)?.ok_or("message not found")?;
    let trash = folder_path_by_role(&engine, &loc.account_id, FolderRole::Trash)?;
    let dest = trash.filter(|t| *t != loc.folder_path);
    move_inner(&engine, id, dest).await?;
    let _ = app.emit("mail-changed", ());
    refresh_tray(&app);
    Ok(())
}

/// Move a message to the account's Spam folder.
#[tauri::command]
pub async fn junk_message(id: String, engine: State<'_, SharedEngine>, app: AppHandle) -> CmdResult<()> {
    let id = msg_id(&id)?;
    let loc = engine.store.lock().unwrap().location(id).map_err(err)?.ok_or("message not found")?;
    let spam = folder_path_by_role(&engine, &loc.account_id, FolderRole::Spam)?.ok_or("this account has no Spam folder")?;
    if spam == loc.folder_path {
        return Ok(());
    }
    move_inner(&engine, id, Some(spam)).await?;
    let _ = app.emit("mail-changed", ());
    refresh_tray(&app);
    Ok(())
}

/// Put a copy of a message in another folder of the same account.
#[tauri::command]
pub async fn copy_message(id: String, folder_id: String, engine: State<'_, SharedEngine>) -> CmdResult<()> {
    let id = msg_id(&id)?;
    let loc = engine.store.lock().unwrap().location(id).map_err(err)?.ok_or("message not found")?;
    let (dest_account, dest) = engine.store.lock().unwrap().folder_path(&folder_id).map_err(err)?.ok_or("folder not found")?;
    if dest_account != loc.account_id {
        return Err("messages can only be copied within the same account".into());
    }
    let (path, uid) = (loc.folder_path.clone(), loc.uid);
    let result = engine
        .with_session(&loc.account_id, move |s| {
            let (path, dest) = (path.clone(), dest.clone());
            Box::pin(async move { imap::copy_message(s, &path, uid, &dest).await })
        })
        .await;
    // The copy shows up once the destination folder syncs.
    engine.wake(Some(&loc.account_id));
    result.map_err(err)
}

/// Save the original message as an .eml file. Returns where it was saved.
#[tauri::command]
pub async fn save_message(id: String, engine: State<'_, SharedEngine>, app: AppHandle) -> CmdResult<Option<String>> {
    let id = msg_id(&id)?;
    let subject = engine.store.lock().unwrap().summary(id).map_err(err)?.map(|s| s.subject).unwrap_or_default();
    let raw = raw_message(&engine, id).await?;
    let mut name: String = subject.chars().map(|c| if "/\\:*?\"<>|".contains(c) || c.is_control() { '_' } else { c }).collect();
    name = name.trim().chars().take(100).collect();
    if name.is_empty() {
        name = "message".into();
    }
    let name = format!("{name}.eml");
    let picked = tauri::async_runtime::spawn_blocking(move || app.dialog().file().set_file_name(&name).blocking_save_file())
        .await
        .map_err(err)?;
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    std::fs::write(&path, raw).map_err(err)?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub fn sync_now(account_id: Option<String>, engine: State<SharedEngine>) {
    engine.wake(account_id.as_deref());
}

// ----- composing -----------------------------------------------------------------

#[derive(Deserialize)]
pub struct Draft {
    pub account_id: String,
    pub to: String,
    #[serde(default)]
    pub cc: String,
    #[serde(default)]
    pub bcc: String,
    pub subject: String,
    pub text: String,
    /// Message being replied to or forwarded.
    #[serde(default)]
    pub reply_to_id: Option<String>,
    #[serde(default)]
    pub forward_id: Option<String>,
    /// Local file paths to attach.
    #[serde(default)]
    pub attachments: Vec<String>,
}

fn guess_mime(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("txt" | "log" | "md") => "text/plain",
        Some("html" | "htm") => "text/html",
        Some("csv") => "text/csv",
        Some("ics") => "text/calendar",
        Some("vcf") => "text/vcard",
        Some("zip") => "application/zip",
        Some("gz") => "application/gzip",
        Some("doc") => "application/msword",
        Some("docx") => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Some("xlsx") => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        Some("odt") => "application/vnd.oasis.opendocument.text",
        Some("mp3") => "audio/mpeg",
        Some("mp4") => "video/mp4",
        _ => "application/octet-stream",
    }
}

/// Providers that file sent mail into Sent themselves.
fn server_saves_sent(cfg: &AccountConfig) -> bool {
    matches!(cfg.provider, Provider::Gmail | Provider::Outlook | Provider::Proton)
}

#[tauri::command]
pub async fn send_message(draft: Draft, engine: State<'_, SharedEngine>, app: AppHandle) -> CmdResult<()> {
    let cfg = engine.account(&draft.account_id).ok_or("unknown account")?;
    let mut out = Outgoing {
        from: Address {
            name: Some(if cfg.sender_name.is_empty() { cfg.name.clone() } else { cfg.sender_name.clone() }),
            email: cfg.email.clone(),
        },
        to: smtp::parse_address_list(&draft.to).map_err(err)?,
        cc: smtp::parse_address_list(&draft.cc).map_err(err)?,
        bcc: smtp::parse_address_list(&draft.bcc).map_err(err)?,
        subject: draft.subject.clone(),
        text: draft.text.clone(),
        ..Default::default()
    };
    if out.to.is_empty() && out.cc.is_empty() && out.bcc.is_empty() {
        return Err("add at least one recipient".into());
    }
    let mut replied: Option<i64> = None;
    if let Some(rid) = &draft.reply_to_id {
        let id = msg_id(rid)?;
        if let Some(orig) = engine.store.lock().unwrap().summary(id).map_err(err)? {
            out.in_reply_to = Some(orig.message_id.clone());
            out.references = orig.references.clone();
            out.references.push(orig.message_id);
            replied = Some(id);
        }
    }
    if let Some(fid) = &draft.forward_id {
        let raw = raw_message(&engine, msg_id(fid)?).await?;
        for a in parse_message(&raw).attachments {
            if let Some((name, mime, data)) = parse::attachment(&raw, a.index) {
                out.attachments.push(OutgoingAttachment { name, mime, data });
            }
        }
    }
    for p in &draft.attachments {
        let path = PathBuf::from(p);
        let data = std::fs::read(&path).map_err(|e| format!("cannot read {p}: {e}"))?;
        out.attachments.push(OutgoingAttachment {
            name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "attachment".into()),
            mime: guess_mime(&path).into(),
            data,
        });
    }

    let (message, _id) = smtp::build(&out).map_err(err)?;
    let creds = engine.credentials(&cfg).await.map_err(err)?;
    let smtp_server = engine.proton.smtp_server(&cfg).unwrap_or_else(|| cfg.smtp.clone());
    smtp::send(&smtp_server, &creds, &message).await.map_err(err)?;

    let mut everyone = out.to.clone();
    everyone.extend(out.cc.iter().cloned());
    everyone.extend(out.bcc.iter().cloned());
    let _ = engine.store.lock().unwrap().remember_addresses(&everyone, true);

    if !server_saves_sent(&cfg) {
        if let Some(sent) = folder_path_by_role(&engine, &cfg.id, FolderRole::Sent)? {
            let raw = message.formatted();
            let res = engine
                .with_session(&cfg.id, move |s| {
                    let (sent, raw) = (sent.clone(), raw.clone());
                    Box::pin(async move { imap::append(s, &sent, "(\\Seen)", &raw).await })
                })
                .await;
            if let Err(e) = res {
                tracing::warn!("message sent but not saved to Sent: {e}");
            }
        }
    }
    if let Some(id) = replied {
        let _ = set_flag_inner(&engine, id, "answered", true).await;
    }
    engine.wake(Some(&cfg.id));
    let _ = app.emit("mail-changed", ());
    Ok(())
}

#[tauri::command]
pub fn complete_address(prefix: String, engine: State<SharedEngine>, app: AppHandle) -> CmdResult<Vec<Address>> {
    if prefix.trim().len() < 2 {
        return Ok(vec![]);
    }
    let mut out = engine.store.lock().unwrap().complete_address(prefix.trim(), 8).map_err(err)?;
    for c in crate::pim::complete_contacts(&app, prefix.trim(), 8) {
        if !out.iter().any(|a| a.email.eq_ignore_ascii_case(&c.email)) {
            out.push(c);
        }
    }
    out.truncate(8);
    Ok(out)
}

#[tauri::command]
pub async fn pick_attachments(app: AppHandle) -> CmdResult<Vec<String>> {
    let picked = tauri::async_runtime::spawn_blocking(move || app.dialog().file().blocking_pick_files())
        .await
        .map_err(err)?;
    Ok(picked
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

// ----- accounts --------------------------------------------------------------------

#[tauri::command]
pub async fn discover_account(email: String) -> CmdResult<Discovered> {
    autoconfig::discover(email.trim()).await.map_err(err)
}

#[derive(Deserialize)]
pub struct NewAccount {
    pub email: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub sender_name: String,
    pub provider: Provider,
    pub username: String,
    #[serde(default)]
    pub password: Option<String>,
    pub imap: ServerConfig,
    pub smtp: ServerConfig,
    #[serde(default)]
    pub oauth: Option<OAuthProvider>,
    #[serde(default)]
    pub proton: bool,
    /// Proton two-factor code and mailbox password, when needed.
    #[serde(default)]
    pub totp: Option<String>,
    #[serde(default)]
    pub mailbox_password: Option<String>,
}

fn default_name(provider: Provider, email: &str) -> String {
    match provider {
        Provider::Gmail => "Gmail".into(),
        Provider::Outlook => "Outlook".into(),
        Provider::Yahoo => "Yahoo".into(),
        Provider::Icloud => "iCloud".into(),
        Provider::Fastmail => "Fastmail".into(),
        Provider::Cockli => "cock.li".into(),
        Provider::Proton => "Proton".into(),
        Provider::Imap => email.rsplit('@').next().unwrap_or(email).into(),
    }
}

/// Sign in through the built-in Proton bridge. When Proton asks for a CAPTCHA,
/// open it in the browser; the next attempt carries the solved challenge.
async fn proton_login(
    app: &AppHandle,
    engine: &SharedEngine,
    email: &str,
    password: Option<&str>,
    totp: Option<&str>,
    mailbox_password: Option<&str>,
) -> CmdResult<()> {
    let e = match engine.proton.login(email, password.unwrap_or(""), totp, mailbox_password).await {
        Ok(()) => return Ok(()),
        Err(e) => e.to_string(),
    };
    if let Some((_, url)) = e.split_once("needs:human_verification ") {
        if let Err(open) = crate::launch::open_url(app, url.trim()).await {
            tracing::warn!("opening the Proton CAPTCHA: {open}");
        }
        return Err(format!("needs:human_verification {}", url.trim()));
    }
    Err(e)
}

/// Run the browser sign-in and return tokens. Cancels with `cancel_oauth`.
async fn browser_sign_in(app: &AppHandle, engine: &SharedEngine, provider: OAuthProvider, hint: &str) -> CmdResult<oauth::Tokens> {
    let pending = oauth::begin(provider, &engine.oauth_client(provider), Some(hint), None).await.map_err(err)?;
    let _ = app.emit("oauth-url", pending.url.clone());
    crate::launch::open_url(app, &pending.url).await?;
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    *app.state::<AppState>().oauth_cancel.lock().unwrap() = Some(tx);
    let result = tokio::select! {
        r = pending.finish(Duration::from_secs(600)) => r.map_err(err),
        _ = rx => Err("sign-in cancelled".to_string()),
    };
    app.state::<AppState>().oauth_cancel.lock().unwrap().take();
    crate::window::show(app);
    result
}

#[tauri::command]
pub fn cancel_oauth(state: State<AppState>) {
    if let Some(tx) = state.oauth_cancel.lock().unwrap().take() {
        let _ = tx.send(());
    }
}

#[tauri::command]
pub async fn add_account(input: NewAccount, engine: State<'_, SharedEngine>, app: AppHandle) -> CmdResult<Account> {
    let email = input.email.trim().to_lowercase();
    let id = new_account_id(&email, &engine.accounts_file());
    let color = ACCOUNT_COLORS[engine.account_configs().len() % ACCOUNT_COLORS.len()].to_string();
    let mut cfg = AccountConfig {
        id: id.clone(),
        name: input.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| default_name(input.provider, &email)),
        email: email.clone(),
        sender_name: input.sender_name.trim().to_string(),
        provider: input.provider,
        color,
        username: if input.username.trim().is_empty() { email.clone() } else { input.username.trim().into() },
        auth: AuthMethod::Password,
        imap: input.imap.clone(),
        smtp: input.smtp.clone(),
    };

    if input.proton {
        cfg.auth = AuthMethod::ProtonBridge;
        cfg.provider = Provider::Proton;
        proton_login(&app, &engine, &email, input.password.as_deref(), input.totp.as_deref(), input.mailbox_password.as_deref())
            .await?;
    } else if let Some(provider) = input.oauth {
        let tokens = browser_sign_in(&app, &engine, provider, &email).await?;
        if let Some(e) = tokens.email.as_deref().filter(|e| !e.eq_ignore_ascii_case(&email)) {
            return Err(format!("you signed in as {e}, not {email}"));
        }
        let creds = Credentials::XOAuth2 { user: cfg.username.clone(), access_token: tokens.access_token.clone() };
        let mut s = imap::connect(&cfg.imap, &creds).await.map_err(err)?;
        let _ = s.logout().await;
        let refresh = tokens.refresh_token.clone().ok_or("the provider did not return a refresh token")?;
        secrets::store(&id, SecretKind::RefreshToken, &refresh).map_err(err)?;
        cfg.auth = AuthMethod::OAuth2 { provider };
        engine.remember_tokens(&id, tokens).await;
    } else {
        let password = input.password.clone().filter(|p| !p.is_empty()).ok_or("enter your password")?;
        let creds = Credentials::Password { user: cfg.username.clone(), password: password.clone() };
        let mut s = imap::connect(&cfg.imap, &creds).await.map_err(err)?;
        let _ = s.logout().await;
        secrets::store(&id, SecretKind::Password, &password).map_err(err)?;
    }

    engine.save_account(cfg)?;
    engine.start_worker(&id);
    engine.accounts_for_ui().into_iter().find(|a| a.id == id).ok_or_else(|| "account vanished".into())
}

#[tauri::command]
pub async fn reauthenticate(
    account_id: String,
    password: Option<String>,
    totp: Option<String>,
    mailbox_password: Option<String>,
    engine: State<'_, SharedEngine>,
    app: AppHandle,
) -> CmdResult<()> {
    let cfg = engine.account(&account_id).ok_or("unknown account")?;
    match cfg.auth {
        AuthMethod::Password => {
            let password = password.filter(|p| !p.is_empty()).ok_or("enter your password")?;
            let mut s = imap::connect(&cfg.imap, &Credentials::Password { user: cfg.username.clone(), password: password.clone() })
                .await
                .map_err(err)?;
            let _ = s.logout().await;
            secrets::store(&cfg.id, SecretKind::Password, &password).map_err(err)?;
        }
        AuthMethod::OAuth2 { provider } => {
            let tokens = browser_sign_in(&app, &engine, provider, &cfg.email).await?;
            if let Some(r) = &tokens.refresh_token {
                secrets::store(&cfg.id, SecretKind::RefreshToken, r).map_err(err)?;
            }
            engine.remember_tokens(&cfg.id, tokens).await;
        }
        AuthMethod::ProtonBridge => {
            proton_login(&app, &engine, &cfg.email, password.as_deref(), totp.as_deref(), mailbox_password.as_deref()).await?;
        }
    }
    engine.wake(Some(&cfg.id));
    Ok(())
}

#[derive(Deserialize)]
pub struct AccountUpdate {
    pub name: String,
    pub sender_name: String,
    pub color: String,
}

#[tauri::command]
pub fn update_account(account_id: String, update: AccountUpdate, engine: State<SharedEngine>, app: AppHandle) -> CmdResult<()> {
    let mut cfg = engine.account(&account_id).ok_or("unknown account")?;
    cfg.name = update.name.trim().to_string();
    cfg.sender_name = update.sender_name.trim().to_string();
    if update.color.starts_with('#') && update.color.len() == 7 {
        cfg.color = update.color;
    }
    engine.save_account(cfg)?;
    let _ = app.emit("account-status", ());
    Ok(())
}

#[tauri::command]
pub fn remove_account(account_id: String, engine: State<SharedEngine>, app: AppHandle) -> CmdResult<()> {
    engine.remove_account(&account_id)?;
    crate::pim::forget_account(&app, &account_id);
    Ok(())
}

#[tauri::command]
pub fn account_details(account_id: String, engine: State<SharedEngine>) -> CmdResult<AccountConfig> {
    engine.account(&account_id).ok_or_else(|| "unknown account".into())
}

// ----- settings --------------------------------------------------------------------

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Settings {
    state.settings()
}

fn sync_autostart(paths: &mail_core::paths::AppPaths, enabled: bool) -> CmdResult<()> {
    let file = paths.autostart_file();
    if enabled {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(err)?;
        }
        // From an AppImage, start that file: there is no `moho-mail` on PATH.
        let exec = std::env::var("APPIMAGE").map(|p| format!("\"{p}\"")).unwrap_or_else(|_| "moho-mail".into());
        std::fs::write(
            &file,
            format!(
                "[Desktop Entry]\nType=Application\nName=Moho Mail\nExec={exec}\nIcon=moho-mail\nX-GNOME-Autostart-enabled=true\nNoDisplay=true\n"
            ),
        )
        .map_err(err)
    } else {
        match std::fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(err(e)),
            _ => Ok(()),
        }
    }
}

#[tauri::command]
pub fn save_settings(settings: Settings, state: State<AppState>, app: AppHandle) -> CmdResult<()> {
    if settings.tray.autostart != state.settings().tray.autostart {
        sync_autostart(&state.paths, settings.tray.autostart)?;
    }
    settings.save(&state.paths.settings_file()).map_err(err)?;
    *state.settings.lock().unwrap() = settings;
    refresh_tray(&app);
    Ok(())
}

#[tauri::command]
pub fn set_account_notify(account_id: String, mode: NotifyMode, state: State<AppState>, app: AppHandle) -> CmdResult<()> {
    let mut settings = state.settings();
    settings.account_notify.insert(account_id, mode);
    save_settings(settings, state, app)
}

/// Run the newest inbox message of an account through the notification
/// policy and show it as if it had just arrived.
#[tauri::command]
pub fn test_notification(account_id: String, engine: State<SharedEngine>, state: State<AppState>, app: AppHandle) -> CmdResult<bool> {
    let settings = state.settings();
    let store = engine.store.lock().unwrap();
    let inbox = store.folder_by_role(&account_id, FolderRole::Inbox).map_err(err)?;
    let msg = inbox
        .and_then(|p| store.messages(&mail_core::store::folder_id(&account_id, &p), 1).ok())
        .and_then(|mut v| v.pop());
    drop(store);
    let Some(msg) = msg else {
        crate::notifications::show_plain(&app, "Moho Mail", "Notifications are working.");
        return Ok(true);
    };
    let notify = !state.notifications_paused()
        && should_notify(
            settings.notify_mode(&account_id),
            FolderRole::Inbox,
            &msg,
            &ImportanceContext::default(),
            &settings.notifications,
            Local::now().hour() as u8,
        );
    if notify {
        let name = engine.account(&account_id).map(|a| a.name).unwrap_or_default();
        crate::notifications::show_new_mail(&app, &name, vec![msg]);
    }
    Ok(notify)
}

#[derive(Serialize)]
pub struct AppInfo {
    pub version: &'static str,
    pub proton_bridge: Option<&'static str>,
    pub config_dir: String,
    pub cache_dir: String,
    pub data_dir: String,
}

#[tauri::command]
pub fn app_info(engine: State<SharedEngine>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        proton_bridge: proton_bridge_sys::available().then_some(proton_bridge_sys::BRIDGE_VERSION),
        config_dir: engine.paths.config.to_string_lossy().into_owned(),
        cache_dir: engine.paths.cache.to_string_lossy().into_owned(),
        data_dir: engine.paths.data.to_string_lossy().into_owned(),
    }
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}
