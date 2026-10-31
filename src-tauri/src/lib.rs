mod commands;
mod engine;
mod folders;
mod launch;
mod notifications;
mod pim;
mod proton;
mod state;
mod tray;
mod window;

use mail_core::paths::AppPaths;
use mail_core::settings::Settings;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

use state::AppState;

fn init_logging(paths: &AppPaths) {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_env("MOHO_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let log = std::fs::OpenOptions::new().create(true).append(true).open(paths.log_dir().join("moho-mail.log"));
    match log {
        Ok(file) => {
            let _ = fmt().with_env_filter(filter).with_writer(std::sync::Mutex::new(file)).with_ansi(false).try_init();
        }
        Err(_) => {
            let _ = fmt().with_env_filter(filter).try_init();
        }
    }
}

pub fn run() {
    let paths = AppPaths::from_env().expect("HOME must be set to an absolute path");
    paths.ensure().expect("cannot create XDG directories");
    init_logging(&paths);
    let settings = Settings::load(&paths.settings_file()).unwrap_or_else(|e| {
        tracing::warn!("{e}; using default settings");
        Settings::default()
    });
    let start_hidden = settings.tray.start_minimized;
    let webview_dir = paths.webview_dir();
    let engine_paths = paths.clone();

    tauri::Builder::default()
        // A second launch just brings the running instance to the front.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| window::show(app)))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
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
                // Links in a message body open in a new window (`<base target="_blank">`).
                // WebKitGTK runs no listeners inside the script-less iframe, so the
                // reader cannot catch the click; hand the popup to the default browser.
                .on_new_window({
                    let handle = app.handle().clone();
                    move |url, _| {
                        if matches!(url.scheme(), "http" | "https" | "mailto") {
                            let handle = handle.clone();
                            tauri::async_runtime::spawn(async move {
                                if let Err(e) = launch::open_url(&handle, url.as_str()).await {
                                    tracing::warn!("{e}");
                                }
                            });
                        }
                        tauri::webview::NewWindowResponse::Deny
                    }
                })
                .build()?;

            let handle = app.handle().clone();
            let engine = engine::Engine::new(handle.clone(), engine_paths.clone())?;
            app.manage(engine.clone());
            pim::init(&handle)?;
            engine.start_all();

            let tray_ok = match tray::spawn(handle.clone()) {
                Ok(tray) => {
                    app.state::<AppState>().set_tray(tray);
                    true
                }
                Err(e) => {
                    tracing::warn!("system tray unavailable ({e}); closing will quit");
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
            commands::search,
            commands::get_message,
            commands::open_attachment,
            commands::save_attachment,
            commands::set_flag,
            commands::move_message,
            commands::archive_message,
            commands::delete_message,
            commands::junk_message,
            commands::copy_message,
            commands::save_message,
            commands::sync_now,
            folders::create_folder,
            folders::rename_folder,
            folders::move_folder,
            folders::copy_folder,
            folders::delete_folder,
            folders::mark_folder_read,
            commands::send_message,
            commands::complete_address,
            commands::pick_attachments,
            commands::discover_account,
            commands::add_account,
            commands::cancel_oauth,
            commands::reauthenticate,
            commands::update_account,
            commands::remove_account,
            commands::account_details,
            commands::get_settings,
            commands::save_settings,
            commands::set_account_notify,
            commands::test_notification,
            commands::quit,
            commands::app_info,
            launch::open_link,
            pim::pim_sources,
            pim::update_source,
            pim::remove_source,
            pim::sync_calendars,
            pim::calendar_events,
            pim::calendar_tasks,
            pim::contacts,
            pim::save_event,
            pim::delete_pim_object,
            pim::save_task,
            pim::set_task_done,
            pim::save_contact,
            pim::add_calendar_account,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Moho Mail")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(engine) = app.try_state::<engine::SharedEngine>() {
                    engine.proton.shutdown();
                }
            }
        });
}
