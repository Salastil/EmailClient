mod commands;
mod state;
mod tray;
mod window;

use mail_core::paths::AppPaths;
use mail_core::settings::Settings;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

use state::AppState;

pub fn run() {
    let paths = AppPaths::from_env().expect("HOME must be set to an absolute path");
    paths.ensure().expect("cannot create XDG directories");
    let settings = Settings::load(&paths.settings_file()).unwrap_or_else(|e| {
        eprintln!("moho-mail: {e}; using default settings");
        Settings::default()
    });
    let start_hidden = settings.tray.start_minimized;
    let webview_dir = paths.webview_dir();

    tauri::Builder::default()
        // A second launch just brings the running instance to the front.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| window::show(app)))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new(paths, settings))
        .setup(move |app| {
            // Created here rather than in tauri.conf.json so WebKitGTK keeps its
            // data under $XDG_CACHE_HOME/moho-mail instead of a reverse-DNS dir.
            WebviewWindowBuilder::new(app, window::MAIN, WebviewUrl::default())
                .title("Moho Mail")
                .inner_size(1400.0, 900.0)
                .min_inner_size(900.0, 560.0)
                .data_directory(webview_dir.clone())
                .visible(false)
                .build()?;

            let handle = app.handle().clone();
            let tray_ok = match tray::spawn(handle.clone()) {
                Ok(tray) => {
                    app.state::<AppState>().set_tray(tray);
                    true
                }
                Err(e) => {
                    eprintln!("moho-mail: system tray unavailable ({e}); closing will quit");
                    false
                }
            };
            // Never start hidden when there is no tray to bring the window back.
            if !(start_hidden && tray_ok) {
                window::show(&handle);
            }
            Ok(())
        })
        .on_window_event(window::on_event)
        .invoke_handler(tauri::generate_handler![
            commands::list_accounts,
            commands::list_folders,
            commands::list_threads,
            commands::get_message,
            commands::get_settings,
            commands::save_settings,
            commands::set_account_notify,
            commands::simulate_incoming,
            commands::quit,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Moho Mail");
}
