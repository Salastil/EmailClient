//! Opening links and files in other applications. Inside the AppImage's
//! bubblewrap sandbox, `xdg-open` would start the browser inside the sandbox,
//! so requests go through the desktop portal (org.freedesktop.portal.OpenURI)
//! instead and open on the host.

use std::os::fd::AsFd;
use std::path::Path;

use ashpd::desktop::open_uri::OpenFileRequest;
use tauri::AppHandle;
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

/// Open a web or mailto link from the reader.
#[tauri::command]
pub async fn open_link(url: String, app: AppHandle) -> Result<(), String> {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:")) {
        return Err("only web and mail links can be opened".into());
    }
    open_url(&app, &url).await
}
