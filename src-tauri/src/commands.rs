//! Commands invoked by the UI. Mail data currently comes from
//! `mail_core::mock` until the IMAP sync engine lands.

use chrono::{Local, Timelike};
use mail_core::model::{Account, Folder, FolderRole, Message, ThreadNode};
use mail_core::notify::{should_notify, ImportanceContext, NotifyMode};
use mail_core::sanitize::{sanitize_email_html, SanitizedHtml};
use mail_core::settings::Settings;
use mail_core::{mock, threading};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_notification::NotificationExt;

use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn accounts_with_settings(settings: &Settings) -> Vec<Account> {
    let mut accounts = mock::accounts();
    for a in &mut accounts {
        a.notify = settings.notify_mode(&a.id);
    }
    accounts
}

/// Recompute the tray badge: unread mail in accounts that are not muted.
fn refresh_tray(state: &AppState) {
    let settings = state.settings();
    let unread = accounts_with_settings(&settings)
        .iter()
        .filter(|a| a.notify != NotifyMode::Muted)
        .map(|a| a.unread)
        .sum();
    let paused = state.notifications_paused();
    state.with_tray(|t| {
        t.update(|tray| {
            tray.unread = unread;
            tray.paused = paused;
        });
    });
}

#[tauri::command]
pub fn list_accounts(state: State<AppState>) -> Vec<Account> {
    refresh_tray(&state);
    accounts_with_settings(&state.settings())
}

#[tauri::command]
pub fn list_folders(account_id: String) -> Vec<Folder> {
    mock::folders(&account_id)
}

#[tauri::command]
pub fn list_threads(folder_id: String) -> Vec<ThreadNode> {
    threading::build_threads(mock::messages(&folder_id))
}

#[derive(Serialize)]
pub struct MessageView {
    pub message: Message,
    pub body: Option<SanitizedHtml>,
}

#[tauri::command]
pub fn get_message(id: String, allow_remote: bool, state: State<AppState>) -> CmdResult<MessageView> {
    let mut message = mock::message(&id).ok_or_else(|| format!("message {id} not found"))?;
    let allow = allow_remote || state.settings().privacy.load_remote_images;
    let body = message.body_html.take().map(|html| sanitize_email_html(&html, allow));
    Ok(MessageView { message, body })
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn save_settings(settings: Settings, state: State<AppState>) -> CmdResult<()> {
    settings.save(&state.paths.settings_file()).map_err(|e| e.to_string())?;
    *state.settings.lock().unwrap() = settings;
    refresh_tray(&state);
    Ok(())
}

#[tauri::command]
pub fn set_account_notify(account_id: String, mode: NotifyMode, state: State<AppState>) -> CmdResult<()> {
    let mut settings = state.settings();
    settings.account_notify.insert(account_id, mode);
    save_settings(settings, state)
}

/// Developer aid: pretend the newest inbox message of an account just
/// arrived and run it through the real notification policy.
#[tauri::command]
pub fn simulate_incoming(account_id: String, app: AppHandle, state: State<AppState>) -> CmdResult<bool> {
    let settings = state.settings();
    let msg = mock::inbox(&account_id)
        .into_iter()
        .max_by_key(|m| m.date)
        .ok_or("account has no messages")?;
    let notify = !state.notifications_paused()
        && should_notify(
            settings.notify_mode(&account_id),
            FolderRole::Inbox,
            &msg,
            &ImportanceContext::default(),
            &settings.notifications,
            Local::now().hour() as u8,
        );
    refresh_tray(&state);
    if notify {
        let from = msg.from.name.as_deref().unwrap_or(&msg.from.email);
        app.notification()
            .builder()
            .title(from)
            .body(&msg.subject)
            .icon("moho-mail")
            .show()
            .map_err(|e| e.to_string())?;
    }
    Ok(notify)
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}
