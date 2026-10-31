use tauri::{AppHandle, Manager, Window, WindowEvent};

use crate::state::AppState;

pub const MAIN: &str = "main";

pub fn show(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn hide(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.hide();
    }
}

pub fn is_shown(app: &AppHandle) -> bool {
    app.get_webview_window(MAIN).is_some_and(|w| {
        w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false)
    })
}

/// Tray left-click: hide when on screen, otherwise bring back.
pub fn toggle(app: &AppHandle) {
    if is_shown(app) {
        hide(app);
    } else {
        show(app);
    }
}

pub fn on_event(window: &Window, event: &WindowEvent) {
    if window.label() != MAIN {
        return;
    }
    let state = window.state::<AppState>();
    let tray = state.settings().tray;
    match event {
        WindowEvent::CloseRequested { api, .. } if tray.close_to_tray && state.has_tray() => {
            api.prevent_close();
            let _ = window.hide();
        }
        // There is no dedicated "minimized" event; a resize accompanies it.
        WindowEvent::Resized(_)
            if tray.minimize_to_tray
                && state.has_tray()
                && window.is_minimized().unwrap_or(false) =>
        {
            let _ = window.hide();
        }
        _ => {}
    }
}
