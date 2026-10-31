//! Opening links and files in other applications, and choosing files. Inside
//! the AppImage's bubblewrap sandbox, `xdg-open` would start the browser
//! inside the sandbox and a file dialog could only see the few folders the
//! sandbox shows, so both go through the desktop portal instead: links open
//! on the host, and the host's file chooser hands over just the chosen files
//! (through the document portal, mounted at $XDG_RUNTIME_DIR/doc).

use std::os::fd::AsFd;
use std::path::{Path, PathBuf};

use ashpd::desktop::file_chooser::{FileFilter, SelectedFiles};
use ashpd::desktop::open_uri::OpenFileRequest;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

pub fn sandboxed() -> bool {
    std::env::var("MOHO_SANDBOXED").as_deref() == Ok("1")
}

pub async fn open_url(app: &AppHandle, url: &str) -> Result<(), String> {
    if sandboxed() {
        let uri = ashpd::Uri::parse(url).map_err(|e| format!("invalid link: {e}"))?;
        OpenFileRequest::default()
            .send_uri(&uri)
            .await
            .map_err(|e| format!("cannot open the link: {e}"))?;
        return Ok(());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| format!("cannot open the browser: {e}"))
}

pub async fn open_path(app: &AppHandle, path: &Path) -> Result<(), String> {
    if sandboxed() {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        OpenFileRequest::default()
            .ask(false)
            .send_file(&file.as_fd())
            .await
            .map_err(|e| format!("cannot open the file: {e}"))?;
        return Ok(());
    }
    app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

/// Paths of the files a portal dialog returned; a cancelled dialog is no files.
fn portal_paths(result: ashpd::Result<SelectedFiles>) -> Result<Vec<PathBuf>, String> {
    match result {
        Ok(files) => Ok(files.uris().iter().filter_map(|u| url::Url::parse(u.as_str()).ok()?.to_file_path().ok()).collect()),
        Err(ashpd::Error::Response(ashpd::desktop::ResponseError::Cancelled)) => Ok(vec![]),
        Err(e) => Err(format!("cannot open the file chooser: {e}")),
    }
}

/// Ask the user for files to read. `images` limits the choice to pictures.
pub async fn pick_files(app: &AppHandle, title: &str, images: bool, multiple: bool) -> Result<Vec<PathBuf>, String> {
    if sandboxed() {
        let mut req = SelectedFiles::open_file().title(title).modal(true).multiple(multiple);
        if images {
            req = req.filter(FileFilter::new("Images").mimetype("image/*"));
        }
        return portal_paths(async { req.send().await?.response() }.await);
    }
    let app = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = app.dialog().file();
        if images {
            dialog = dialog.add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico"]);
        }
        if multiple {
            dialog.blocking_pick_files().unwrap_or_default()
        } else {
            dialog.blocking_pick_file().into_iter().collect()
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(picked.into_iter().filter_map(|p| p.into_path().ok()).collect())
}

/// Ask the user where to save a file called `name`.
pub async fn save_file(app: &AppHandle, title: &str, name: &str) -> Result<Option<PathBuf>, String> {
    if sandboxed() {
        let req = SelectedFiles::save_file().title(title).modal(true).current_name(name);
        return portal_paths(async { req.send().await?.response() }.await).map(|p| p.into_iter().next());
    }
    let (app, name) = (app.clone(), name.to_string());
    let picked = tauri::async_runtime::spawn_blocking(move || app.dialog().file().set_file_name(&name).blocking_save_file())
        .await
        .map_err(|e| e.to_string())?;
    Ok(picked.and_then(|p| p.into_path().ok()))
}

/// How to mention a saved file: files saved through the document portal live
/// under an opaque mount, so name only the file.
pub fn shown_path(path: &Path) -> String {
    let doc = std::env::var("XDG_RUNTIME_DIR").map(|r| Path::new(&r).join("doc")).ok();
    match doc {
        Some(doc) if path.starts_with(&doc) => path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
        _ => path.to_string_lossy().into_owned(),
    }
}

/// Open a web or mailto link from the reader.
#[tauri::command]
pub async fn open_link(url: String, app: AppHandle) -> Result<(), String> {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:")) {
        return Err("only web and mail links can be opened".into());
    }
    open_url(&app, &url).await
}
