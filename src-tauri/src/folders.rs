//! Folder management from the folder pane's context menu.

use mail_core::model::{Folder, FolderRole};
use mail_core::store::folder_id;
use providers::imap;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::refresh_tray;
use crate::engine::{important_folders, SharedEngine};
use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn find(list: Vec<Folder>, id: &str) -> Option<Folder> {
    list.into_iter().find_map(|mut f| {
        if f.id == id {
            return Some(f);
        }
        find(std::mem::take(&mut f.children), id)
    })
}

fn lookup(engine: &SharedEngine, id: &str) -> CmdResult<Folder> {
    let store = engine.store.lock().unwrap();
    let (account_id, _) = store.folder_path(id).map_err(err)?.ok_or("folder not found")?;
    find(store.folders(&account_id).map_err(err)?, id).ok_or_else(|| "folder not found".into())
}

/// Only folders the user made can be renamed, moved or deleted; special
/// folders (Inbox, Sent, Trash, ...) stay where the server put them.
fn editable(f: &Folder) -> CmdResult<()> {
    match f.role {
        FolderRole::Other | FolderRole::Label => Ok(()),
        _ => Err(format!("“{}” is a special folder and can't be changed", f.name)),
    }
}

async fn delimiter(engine: &SharedEngine, account_id: &str) -> CmdResult<String> {
    engine.with_session(account_id, |s| Box::pin(imap::delimiter(s))).await.map_err(err)
}

/// Reload the account's folder list from the server, then tell the UI.
async fn finish(engine: &SharedEngine, app: &AppHandle, account_id: &str) -> CmdResult<()> {
    let store = engine.store.clone();
    let id = account_id.to_string();
    engine
        .with_session(account_id, move |s| {
            let (store, id) = (store.clone(), id.clone());
            Box::pin(async move {
                let folders = imap::list_folders(s).await?;
                store.lock().unwrap().sync_folders(&id, &folders)?;
                Ok(())
            })
        })
        .await
        .map_err(err)?;
    engine.wake(Some(account_id));
    let _ = app.emit("mail-changed", serde_json::json!({ "account_id": account_id }));
    Ok(())
}

/// Point favorites and important folders inside a moved folder at its new
/// place, or drop them when `to` is None (the folder is gone).
fn retarget_saved(app: &AppHandle, account_id: &str, from: &str, to: Option<&str>, delim: &str) {
    let state = app.state::<AppState>();
    let mut settings = state.settings();
    let (root, below) = (folder_id(account_id, from), folder_id(account_id, &format!("{from}{delim}")));
    let retarget = |list: &[String]| -> Vec<String> {
        list.iter()
            .filter_map(|id| {
                if *id != root && !id.starts_with(&below) {
                    return Some(id.clone());
                }
                to.map(|to| format!("{}{}", folder_id(account_id, to), &id[root.len()..]))
            })
            .collect()
    };
    let before = settings.clone();
    settings.favorite_folders = retarget(&settings.favorite_folders);
    if let Some(ids) = settings.important_folders.get(account_id) {
        let ids = retarget(ids);
        settings.important_folders.insert(account_id.to_string(), ids);
    }
    if settings != before {
        if let Err(e) = settings.save(&state.paths.settings_file()) {
            tracing::warn!("saving folder settings: {e}");
        }
        *state.settings.lock().unwrap() = settings;
    }
}

/// Rename a folder on the server to `to`, carrying its cached mail along.
async fn relocate(engine: &SharedEngine, app: &AppHandle, folder: &Folder, to: String, delim: &str) -> CmdResult<String> {
    let from = folder.path.clone();
    if to == from || to.starts_with(&format!("{from}{delim}")) {
        return Err("a folder can't be moved into itself".into());
    }
    let (f, t) = (from.clone(), to.clone());
    engine
        .with_session(&folder.account_id, move |s| {
            let (f, t) = (f.clone(), t.clone());
            Box::pin(async move { imap::rename_folder(s, &f, &t).await })
        })
        .await
        .map_err(err)?;
    engine.store.lock().unwrap().rename_folder_tree(&folder.account_id, &from, &to, delim).map_err(err)?;
    retarget_saved(app, &folder.account_id, &from, Some(&to), delim);
    finish(engine, app, &folder.account_id).await?;
    Ok(folder_id(&folder.account_id, &to))
}

fn parent_path(path: &str, delim: &str) -> Option<String> {
    path.rsplit_once(delim).map(|(p, _)| p.to_string())
}

/// Create `name` under `parent_id` (or at the top level). Returns the new folder's id.
#[tauri::command]
pub async fn create_folder(
    account_id: String,
    parent_id: Option<String>,
    name: String,
    engine: State<'_, SharedEngine>,
    app: AppHandle,
) -> CmdResult<String> {
    let parent = parent_id.map(|id| lookup(&engine, &id)).transpose()?.map(|f| f.path);
    let delim = delimiter(&engine, &account_id).await?;
    let path = imap::child_path(parent.as_deref(), &name, &delim).map_err(err)?;
    let p = path.clone();
    engine
        .with_session(&account_id, move |s| {
            let p = p.clone();
            Box::pin(async move { imap::create_folder(s, &p).await })
        })
        .await
        .map_err(err)?;
    finish(&engine, &app, &account_id).await?;
    Ok(folder_id(&account_id, &path))
}

#[tauri::command]
pub async fn rename_folder(folder_id: String, name: String, engine: State<'_, SharedEngine>, app: AppHandle) -> CmdResult<String> {
    let folder = lookup(&engine, &folder_id)?;
    editable(&folder)?;
    let delim = delimiter(&engine, &folder.account_id).await?;
    let to = imap::child_path(parent_path(&folder.path, &delim).as_deref(), &name, &delim).map_err(err)?;
    if to == folder.path {
        return Ok(folder.id);
    }
    relocate(&engine, &app, &folder, to, &delim).await
}

/// Move a folder under `dest_id`, or to the top level when it is None.
#[tauri::command]
pub async fn move_folder(
    folder_id: String,
    dest_id: Option<String>,
    engine: State<'_, SharedEngine>,
    app: AppHandle,
) -> CmdResult<String> {
    let folder = lookup(&engine, &folder_id)?;
    editable(&folder)?;
    let dest = dest_id.map(|id| lookup(&engine, &id)).transpose()?.map(|f| f.path);
    let delim = delimiter(&engine, &folder.account_id).await?;
    let leaf = imap::leaf(&folder.path, &delim);
    let to = match dest {
        Some(d) => format!("{d}{delim}{leaf}"),
        None => leaf,
    };
    relocate(&engine, &app, &folder, to, &delim).await
}

/// Copy a folder, its subfolders and all their messages under `dest_id` (or
/// to the top level). IMAP has no folder copy, so this creates each folder
/// and copies its messages over.
#[tauri::command]
pub async fn copy_folder(
    folder_id: String,
    dest_id: Option<String>,
    engine: State<'_, SharedEngine>,
    app: AppHandle,
) -> CmdResult<String> {
    let folder = lookup(&engine, &folder_id)?;
    let dest = dest_id.map(|id| lookup(&engine, &id)).transpose()?.map(|f| f.path);
    let account_id = folder.account_id.clone();
    let delim = delimiter(&engine, &account_id).await?;
    let leaf = imap::leaf(&folder.path, &delim);
    let root = match &dest {
        Some(d) => format!("{d}{delim}{leaf}"),
        None => leaf,
    };
    if root == folder.path || root.starts_with(&format!("{}{delim}", folder.path)) {
        return Err("a folder can't be copied into itself".into());
    }
    let plan: Vec<(String, String, bool)> = engine
        .store
        .lock()
        .unwrap()
        .subtree(&account_id, &folder.path, &delim)
        .map_err(err)?
        .into_iter()
        .map(|(p, selectable)| (p.clone(), format!("{root}{}", &p[folder.path.len()..]), selectable))
        .collect();
    engine
        .with_session(&account_id, move |s| {
            let plan = plan.clone();
            Box::pin(async move {
                for (from, to, selectable) in &plan {
                    imap::create_folder(s, to).await?;
                    if *selectable {
                        imap::copy_all(s, from, to).await?;
                    }
                }
                Ok(())
            })
        })
        .await
        .map_err(err)?;
    finish(&engine, &app, &account_id).await?;
    Ok(mail_core::store::folder_id(&account_id, &root))
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Deleted {
    /// Moved into the Trash folder, like Thunderbird does.
    MovedToTrash,
    /// Removed from the server (already in Trash, no Trash, or a Gmail label).
    Deleted,
}

/// Delete a folder: move it into Trash, or remove it for good when it is
/// already there, the account has no Trash, or it is a Gmail label (deleting
/// a label leaves the messages in All Mail).
#[tauri::command]
pub async fn delete_folder(folder_id: String, engine: State<'_, SharedEngine>, app: AppHandle) -> CmdResult<Deleted> {
    let folder = lookup(&engine, &folder_id)?;
    editable(&folder)?;
    let account_id = folder.account_id.clone();
    let delim = delimiter(&engine, &account_id).await?;
    let gmail = engine.with_session(&account_id, |s| Box::pin(imap::capabilities(s))).await.map_err(err)?.gmail;
    let (trash, subtree) = {
        let store = engine.store.lock().unwrap();
        let trash = store.folder_by_role(&account_id, FolderRole::Trash).map_err(err)?;
        (trash, store.subtree_paths(&account_id, &folder.path, &delim).map_err(err)?)
    };
    let in_trash = trash.as_ref().is_some_and(|t| folder.path.starts_with(&format!("{t}{delim}")));

    if let (false, false, Some(trash)) = (gmail, in_trash, trash) {
        // Pick a name that isn't taken in Trash yet.
        let leaf = imap::leaf(&folder.path, &delim);
        let taken = engine.store.lock().unwrap().subtree_paths(&account_id, &trash, &delim).map_err(err)?;
        let to = (1..)
            .map(|n| if n == 1 { format!("{trash}{delim}{leaf}") } else { format!("{trash}{delim}{leaf} {n}") })
            .find(|p| !taken.contains(p))
            .unwrap();
        relocate(&engine, &app, &folder, to.clone(), &delim).await?;
        retarget_saved(&app, &account_id, &to, None, &delim);
        return Ok(Deleted::MovedToTrash);
    }

    // Servers refuse to delete a folder that still has subfolders: go deepest first.
    engine
        .with_session(&account_id, move |s| {
            let subtree = subtree.clone();
            Box::pin(async move {
                for p in subtree.iter().rev() {
                    imap::delete_folder(s, p).await?;
                }
                Ok(())
            })
        })
        .await
        .map_err(err)?;
    retarget_saved(&app, &account_id, &folder.path, None, &delim);
    finish(&engine, &app, &account_id).await?;
    Ok(Deleted::Deleted)
}

#[tauri::command]
pub async fn mark_folder_read(folder_id: String, engine: State<'_, SharedEngine>, app: AppHandle) -> CmdResult<()> {
    let folder = lookup(&engine, &folder_id)?;
    engine.store.lock().unwrap().mark_folder_read(&folder.id).map_err(err)?;
    refresh_tray(&app);
    let _ = app.emit("mail-changed", serde_json::json!({ "account_id": folder.account_id }));
    let path = folder.path.clone();
    let result = engine
        .with_session(&folder.account_id, move |s| {
            let path = path.clone();
            Box::pin(async move { imap::mark_all_seen(s, &path).await })
        })
        .await;
    engine.wake(Some(&folder.account_id));
    result.map_err(err)
}

/// Monitor a folder as important, or stop. The account's default (Gmail's
/// Important folder, otherwise the Inbox) becomes an explicit list the first
/// time, so it can be switched off like any other folder.
#[tauri::command]
pub fn set_folder_important(
    folder_id: String,
    important: bool,
    engine: State<'_, SharedEngine>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> CmdResult<()> {
    let folder = lookup(&engine, &folder_id)?;
    let mut settings = state.settings();
    let mut ids = important_folders(&engine.store.lock().unwrap(), &settings, &folder.account_id);
    ids.retain(|id| *id != folder.id);
    if important {
        ids.push(folder.id.clone());
    }
    settings.important_folders.insert(folder.account_id.clone(), ids);
    settings.save(&state.paths.settings_file()).map_err(err)?;
    *state.settings.lock().unwrap() = settings;
    refresh_tray(&app);
    let _ = app.emit("mail-changed", serde_json::json!({ "account_id": folder.account_id }));
    Ok(())
}
