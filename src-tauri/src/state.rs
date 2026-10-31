use std::sync::Mutex;

use chrono::{DateTime, Local};
use mail_core::paths::AppPaths;
use mail_core::settings::Settings;

use crate::tray::TrayHandle;

pub struct AppState {
    pub paths: AppPaths,
    pub settings: Mutex<Settings>,
    /// Notifications are paused from the tray menu until this time.
    pub paused_until: Mutex<Option<DateTime<Local>>>,
    tray: Mutex<Option<TrayHandle>>,
    /// Cancels the browser sign-in in progress.
    pub oauth_cancel: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl AppState {
    pub fn new(paths: AppPaths, settings: Settings) -> Self {
        Self {
            paths,
            settings: Mutex::new(settings),
            paused_until: Mutex::new(None),
            tray: Mutex::new(None),
            oauth_cancel: Mutex::new(None),
        }
    }

    pub fn set_tray(&self, tray: TrayHandle) {
        *self.tray.lock().unwrap() = Some(tray);
    }

    pub fn has_tray(&self) -> bool {
        self.tray.lock().unwrap().as_ref().is_some_and(|t| !t.is_closed())
    }

    pub fn with_tray(&self, f: impl FnOnce(&TrayHandle)) {
        if let Some(t) = self.tray.lock().unwrap().as_ref() {
            f(t);
        }
    }

    pub fn notifications_paused(&self) -> bool {
        let mut paused = self.paused_until.lock().unwrap();
        match *paused {
            Some(until) if until > Local::now() => true,
            Some(_) => {
                *paused = None;
                false
            }
            None => false,
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }
}
